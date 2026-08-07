//! `memorylake project document` / `proj doc` commands.
//!
//! Documents are Library files imported into a project and indexed there. The
//! import endpoint takes file ids only and works asynchronously, which is where
//! this module's two pieces of local logic come from: expanding folder ids into
//! the files underneath them, and polling until the server finishes processing.
//!
//! Both pieces are written against the [`LibrarySource`] and [`PollContext`]
//! traits rather than against [`Client`] directly, so their rules can be tested
//! without a network or a real clock.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::Client;
use memorylake_core::api::documents::{
    DOCUMENT_STATUS_ERROR, DeleteDocumentsRequest, ImportDocumentsRequest, ImportOutcome,
    ListDocumentsParams, delete_documents, get_document, import_documents, is_terminal_status,
    list_documents,
};
use memorylake_core::api::library::{Item, ItemList, ListChildrenParams, get_item, list_children};

/// Files a single `import` may cover before it refuses to run.
///
/// `MY_SPACE --recursive` is a legal way to name every file in the workspace,
/// so an accidental one is worth stopping before it becomes an irreversible
/// bulk import.
const DEFAULT_MAX_FILES: usize = 500;

/// Seconds `--wait` polls before giving up.
const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 600;

/// Gap before the second poll under `--wait`; doubles after that.
const INITIAL_POLL_DELAY: Duration = Duration::from_secs(1);

/// Longest gap between polls under `--wait`.
const MAX_POLL_DELAY: Duration = Duration::from_secs(15);

/// `project document` subcommands.
#[derive(Debug, Subcommand)]
pub enum DocumentCommand {
    /// Import Library files into a project.
    ///
    /// Files must already exist in the Library. A folder id is rejected unless
    /// `--recursive` is set. Importing is asynchronous: the command returns as
    /// soon as the server accepts the batch unless `--wait` is given.
    Import {
        /// Workspace id that owns the project.
        #[arg(long)]
        workspace: String,
        /// Project to import into.
        #[arg(long)]
        project: String,
        /// Library item ids. Folder ids require `--recursive`.
        #[arg(required = true, value_name = "ITEM_ID")]
        item_ids: Vec<String>,
        /// Expand folder ids into every file in their subtree.
        #[arg(long)]
        recursive: bool,
        /// Most files one invocation may import.
        ///
        /// Checked after expansion; exceeding it imports nothing.
        #[arg(long, default_value_t = DEFAULT_MAX_FILES)]
        max_files: usize,
        /// Poll until every imported document finishes processing.
        #[arg(long)]
        wait: bool,
        /// Seconds to keep polling. Only meaningful with `--wait`.
        ///
        /// Giving up does not cancel the import; it carries on server-side.
        #[arg(long, default_value_t = DEFAULT_WAIT_TIMEOUT_SECS, value_name = "SECS")]
        timeout: u64,
    },
    /// List the documents in a project.
    List {
        /// Workspace id that owns the project.
        #[arg(long)]
        workspace: String,
        /// Project whose documents to list.
        #[arg(long)]
        project: String,
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by document name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Get a single document, including its processing status.
    Get {
        /// Workspace id that owns the project.
        #[arg(long)]
        workspace: String,
        /// Project containing the document.
        #[arg(long)]
        project: String,
        /// Document id.
        document_id: String,
    },
    /// Remove documents from a project.
    ///
    /// This cannot be undone: the indexed content and every memory derived from
    /// these documents is permanently removed. There is no confirmation prompt.
    /// The Library files they came from are left untouched.
    Delete {
        /// Workspace id that owns the project.
        #[arg(long)]
        workspace: String,
        /// Project to remove the documents from.
        #[arg(long)]
        project: String,
        /// Document ids to remove.
        #[arg(required = true, value_name = "DOCUMENT_ID")]
        document_ids: Vec<String>,
    },
}

/// Execute a `project document` subcommand.
pub fn run(client: &Client, command: DocumentCommand) -> Result<()> {
    match command {
        DocumentCommand::Import {
            workspace,
            project,
            item_ids,
            recursive,
            max_files,
            wait,
            timeout,
        } => run_import(
            client,
            &workspace,
            &project,
            &item_ids,
            ImportOptions {
                recursive,
                max_files,
                wait,
                timeout: Duration::from_secs(timeout),
            },
        ),
        DocumentCommand::List {
            workspace,
            project,
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let data = list_documents(
                client,
                &workspace,
                &project,
                &ListDocumentsParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .with_context(|| format!("list documents in project `{project}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
            Ok(())
        }
        DocumentCommand::Get {
            workspace,
            project,
            document_id,
        } => {
            let data = get_document(client, &workspace, &project, &document_id)
                .with_context(|| format!("get document `{document_id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
            Ok(())
        }
        DocumentCommand::Delete {
            workspace,
            project,
            document_ids,
        } => {
            delete_documents(
                client,
                &workspace,
                &project,
                &DeleteDocumentsRequest {
                    ids: document_ids.clone(),
                },
            )
            .with_context(|| format!("delete documents from project `{project}`"))?;
            println!(
                "Deleted {} document(s) from project `{project}`: {}",
                document_ids.len(),
                document_ids.join(", ")
            );
            Ok(())
        }
    }
}

/// Everything `import` needs beyond the ids themselves.
struct ImportOptions {
    recursive: bool,
    max_files: usize,
    wait: bool,
    timeout: Duration,
}

fn run_import(
    client: &Client,
    workspace: &str,
    project: &str,
    item_ids: &[String],
    options: ImportOptions,
) -> Result<()> {
    let library = ApiLibrary { client };
    let targets = resolve_import_targets(&library, item_ids, options.recursive, options.max_files)?;
    if targets.len() != item_ids.len() {
        eprintln!(
            "Importing {} file(s) resolved from {} argument(s)",
            targets.len(),
            item_ids.len()
        );
    }

    let outcome = import_documents(
        client,
        workspace,
        project,
        &ImportDocumentsRequest {
            drive_item_ids: targets,
        },
    )
    .with_context(|| format!("import documents into project `{project}`"))?;

    // Printed before any failure is raised: the batch has already run, and the
    // caller must not have to choose between seeing the result and seeing the
    // failure.
    println!("{}", serde_json::to_string_pretty(&outcome)?);

    let waited = if options.wait {
        let document_ids = outcome.document_ids();
        eprintln!(
            "Waiting up to {}s for {} document(s) to finish processing…",
            options.timeout.as_secs(),
            document_ids.len()
        );
        let poller = ApiPoller {
            client,
            workspace,
            project,
            start: Instant::now(),
        };
        Some(wait_for_documents(&poller, &document_ids, options.timeout)?)
    } else {
        None
    };

    let failures = import_failures(&outcome, waited.as_ref());
    if !failures.is_empty() {
        bail!(
            "import finished with problems:\n  - {}",
            failures.join("\n  - ")
        );
    }

    Ok(())
}

/// The read-only slice of the Library that expansion needs.
///
/// A trait rather than a direct [`Client`] call so the expansion rules can be
/// exercised against a fake tree, including the paging behavior that is
/// otherwise only reachable with a folder big enough to span pages.
trait LibrarySource {
    /// Classify one item.
    fn item(&self, item_id: &str) -> Result<Item>;
    /// One page of a folder's children.
    fn children(&self, item_id: &str, continuation_token: Option<String>) -> Result<ItemList>;
}

struct ApiLibrary<'a> {
    client: &'a Client,
}

impl LibrarySource for ApiLibrary<'_> {
    fn item(&self, item_id: &str) -> Result<Item> {
        get_item(self.client, item_id).with_context(|| format!("look up library item `{item_id}`"))
    }

    fn children(&self, item_id: &str, continuation_token: Option<String>) -> Result<ItemList> {
        list_children(
            self.client,
            item_id,
            &ListChildrenParams {
                page_size: None,
                continuation_token,
            },
        )
        .with_context(|| format!("list children of library folder `{item_id}`"))
    }
}

/// Turn the ids the user supplied into the file ids to import.
///
/// Every id is classified first, even without `--recursive`, because a folder
/// has to be refused before the import request goes out rather than after the
/// server rejects it.
///
/// Fails — importing nothing — when the inputs cannot produce a usable batch: a
/// folder without `--recursive`, an expansion that yields no files, or more
/// files than `max_files` allows.
fn resolve_import_targets(
    source: &dyn LibrarySource,
    inputs: &[String],
    recursive: bool,
    max_files: usize,
) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut folders = Vec::new();
    let mut rejected = Vec::new();

    for input in inputs {
        let item = source.item(input)?;
        if item.is_directory() {
            if recursive {
                folders.push(item.item_id);
            } else {
                rejected.push(input.clone());
            }
        } else {
            // Canonical ids, not the strings the user typed: `MY_SPACE` and the
            // root's real id name one folder, and only the canonical form
            // dedupes them. Anything that is not a folder is offered to the
            // server, including a type this build does not recognize — dropping
            // it here would silently import less than was asked for.
            push_unique(&mut files, &mut seen, item.item_id);
        }
    }

    if rejected.len() == 1 {
        bail!(
            "`{}` is a folder; pass --recursive to import every file inside it. Nothing was imported.",
            rejected[0]
        );
    }
    if rejected.len() > 1 {
        bail!(
            "`{}` are folders; pass --recursive to import every file inside them. Nothing was imported.",
            rejected.join("`, `")
        );
    }

    // An explicit stack rather than recursion: nesting depth is decided by the
    // server, not by this process.
    while let Some(folder) = folders.pop() {
        let mut continuation_token = None;
        loop {
            let page = source.children(&folder, continuation_token)?;
            for child in page.items {
                if child.is_directory() {
                    folders.push(child.item_id);
                } else {
                    push_unique(&mut files, &mut seen, child.item_id);
                }
            }
            match page.continuation_token {
                // A folder wider than one page has to be walked to the end.
                // Stopping at the first page would import a subset that looks
                // exactly like a complete run.
                Some(next) => continuation_token = Some(next),
                None => break,
            }
        }
    }

    if files.is_empty() {
        bail!("the ids you supplied expanded to no files. Nothing was imported.");
    }
    if files.len() > max_files {
        bail!(
            "{} files to import exceeds the --max-files limit of {max_files}. \
             Narrow the selection or raise --max-files. Nothing was imported.",
            files.len()
        );
    }

    Ok(files)
}

/// Append `id` unless it is already in `files`, preserving first-seen order.
fn push_unique(files: &mut Vec<String>, seen: &mut BTreeSet<String>, id: String) {
    if seen.insert(id.clone()) {
        files.push(id);
    }
}

/// What the wait loop needs from the outside world.
///
/// Injected so the polling rules can be tested without a network and without
/// spending the real seconds the backoff schedule describes.
trait PollContext {
    /// Current processing status of one document.
    fn status(&self, document_id: &str) -> Result<String>;
    /// Pause before the next round.
    fn sleep(&self, delay: Duration);
    /// Time spent waiting so far.
    fn elapsed(&self) -> Duration;
}

struct ApiPoller<'a> {
    client: &'a Client,
    workspace: &'a str,
    project: &'a str,
    start: Instant,
}

impl PollContext for ApiPoller<'_> {
    fn status(&self, document_id: &str) -> Result<String> {
        let document = get_document(self.client, self.workspace, self.project, document_id)
            .with_context(|| format!("poll document `{document_id}`"))?;
        Ok(document.status)
    }

    fn sleep(&self, delay: Duration) {
        std::thread::sleep(delay);
    }

    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// How waiting ended.
#[derive(Debug, Default, PartialEq, Eq)]
struct WaitOutcome {
    /// Documents that settled on `error`.
    errored: Vec<String>,
    /// Documents still non-terminal when the timeout elapsed.
    timed_out: Vec<String>,
}

/// Poll `document_ids` until each reaches a terminal status or `timeout` passes.
///
/// A status this build does not recognize counts as non-terminal, so an
/// unfamiliar server state times out rather than being reported as finished.
fn wait_for_documents(
    ctx: &dyn PollContext,
    document_ids: &[String],
    timeout: Duration,
) -> Result<WaitOutcome> {
    let mut outcome = WaitOutcome::default();
    let mut pending: Vec<String> = document_ids.to_vec();
    let mut delay = INITIAL_POLL_DELAY;

    while !pending.is_empty() {
        let mut still_pending = Vec::new();
        for id in pending {
            let status = ctx.status(&id)?;
            if !is_terminal_status(&status) {
                still_pending.push(id);
            } else if status == DOCUMENT_STATUS_ERROR {
                outcome.errored.push(id);
            }
        }
        pending = still_pending;

        if pending.is_empty() {
            break;
        }
        if ctx.elapsed() >= timeout {
            outcome.timed_out = pending;
            break;
        }
        ctx.sleep(delay);
        delay = (delay * 2).min(MAX_POLL_DELAY);
    }

    Ok(outcome)
}

/// Reasons an accepted import must still fail the command.
///
/// The import endpoint answers 200 even when individual files failed, so
/// success has to be decided here rather than taken from the HTTP result. An
/// empty list means the command succeeded.
fn import_failures(outcome: &ImportOutcome, waited: Option<&WaitOutcome>) -> Vec<String> {
    let mut failures = Vec::new();

    if outcome.failure_count > 0 {
        let mut reason = format!("{} file(s) could not be imported", outcome.failure_count);
        if outcome.details_truncated {
            reason.push_str(
                "; the server truncated the per-file details, so the output above does not name all of them",
            );
        }
        failures.push(reason);
    }

    let Some(waited) = waited else {
        return failures;
    };

    if outcome.details_truncated {
        // Without the full detail list there is no way to know which documents
        // were never polled, so --wait cannot honestly claim they all finished.
        failures.push(
            "--wait could not cover every document: the server truncated the per-file details, \
             so some imported documents were never polled"
                .to_string(),
        );
    }

    if !waited.errored.is_empty() {
        failures.push(format!(
            "{} document(s) finished with status `error`: {}",
            waited.errored.len(),
            waited.errored.join(", ")
        ));
    }

    if !waited.timed_out.is_empty() {
        failures.push(format!(
            "{} document(s) were still processing when --timeout elapsed ({}); \
             the import is still running on the server and has not been cancelled",
            waited.timed_out.len(),
            waited.timed_out.join(", ")
        ));
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    use memorylake_core::api::documents::ImportDetail;
    use memorylake_core::api::library::{ITEM_TYPE_DIRECTORY, ITEM_TYPE_FILE};

    fn item(id: &str, item_type: &str) -> Item {
        Item {
            uri: format!("drive://d/{id}"),
            item_id: id.to_string(),
            name: id.to_string(),
            item_type: item_type.to_string(),
            size: None,
            etag: None,
            parent_item_id: None,
            created_at: None,
            updated_at: None,
            x_attrs: BTreeMap::new(),
        }
    }

    /// A fake Library tree.
    ///
    /// Child pages are addressed by an index-as-token so a folder can be given
    /// more than one page without inventing a token format.
    #[derive(Default)]
    struct FakeLibrary {
        items: BTreeMap<String, Item>,
        pages: BTreeMap<String, Vec<Vec<String>>>,
    }

    impl FakeLibrary {
        fn with_file(mut self, id: &str) -> Self {
            self.items.insert(id.to_string(), item(id, ITEM_TYPE_FILE));
            self
        }

        /// Add a folder whose children arrive as the given pages.
        fn with_folder(mut self, id: &str, pages: &[&[&str]]) -> Self {
            self.items
                .insert(id.to_string(), item(id, ITEM_TYPE_DIRECTORY));
            self.pages.insert(
                id.to_string(),
                pages
                    .iter()
                    .map(|page| page.iter().map(|id| id.to_string()).collect())
                    .collect(),
            );
            self
        }
    }

    impl LibrarySource for FakeLibrary {
        fn item(&self, item_id: &str) -> Result<Item> {
            self.items
                .get(item_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("fake library has no item `{item_id}`"))
        }

        fn children(&self, item_id: &str, continuation_token: Option<String>) -> Result<ItemList> {
            let pages = self.pages.get(item_id).cloned().unwrap_or_default();
            let index: usize = continuation_token
                .as_deref()
                .map(|token| token.parse().expect("fake page token"))
                .unwrap_or(0);
            let items = pages
                .get(index)
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|id| self.items[id].clone())
                .collect();
            Ok(ItemList {
                items,
                continuation_token: (index + 1 < pages.len()).then(|| (index + 1).to_string()),
            })
        }
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn plain_file_ids_pass_through_in_order() {
        let library = FakeLibrary::default().with_file("f1").with_file("f2");
        let targets =
            resolve_import_targets(&library, &ids(&["f1", "f2"]), false, 500).expect("resolve");
        assert_eq!(targets, ids(&["f1", "f2"]));
    }

    #[test]
    fn a_folder_without_recursive_is_rejected_before_importing() {
        let library = FakeLibrary::default()
            .with_file("f1")
            .with_folder("dir", &[&["f1"]]);

        let err = resolve_import_targets(&library, &ids(&["f1", "dir"]), false, 500)
            .expect_err("a folder must not reach the import request");
        let message = err.to_string();
        assert!(message.contains("`dir`"), "{message}");
        assert!(message.contains("--recursive"), "{message}");
        assert!(message.contains("Nothing was imported"), "{message}");
    }

    #[test]
    fn several_folders_without_recursive_are_all_named() {
        let library = FakeLibrary::default()
            .with_folder("a", &[])
            .with_folder("b", &[]);

        let message = resolve_import_targets(&library, &ids(&["a", "b"]), false, 500)
            .expect_err("both folders must be refused")
            .to_string();
        assert!(message.contains("`a`, `b`"), "{message}");
        assert!(message.contains("are folders"), "{message}");
    }

    #[test]
    fn recursive_expansion_walks_every_page() {
        // A folder wider than one page is the case where stopping early would
        // look exactly like a successful complete import.
        let library = FakeLibrary::default()
            .with_file("f1")
            .with_file("f2")
            .with_file("f3")
            .with_folder("dir", &[&["f1", "f2"], &["f3"]]);

        let targets = resolve_import_targets(&library, &ids(&["dir"]), true, 500).expect("resolve");
        assert_eq!(targets, ids(&["f1", "f2", "f3"]));
    }

    #[test]
    fn recursive_expansion_descends_into_subfolders() {
        let library = FakeLibrary::default()
            .with_file("top")
            .with_file("deep")
            .with_folder("inner", &[&["deep"]])
            .with_folder("outer", &[&["top", "inner"]]);

        let mut targets =
            resolve_import_targets(&library, &ids(&["outer"]), true, 500).expect("resolve");
        targets.sort();
        assert_eq!(targets, ids(&["deep", "top"]));
    }

    #[test]
    fn files_and_folders_may_be_mixed_in_one_invocation() {
        let library = FakeLibrary::default()
            .with_file("loose")
            .with_file("inside")
            .with_folder("dir", &[&["inside"]]);

        let targets =
            resolve_import_targets(&library, &ids(&["loose", "dir"]), true, 500).expect("resolve");
        assert_eq!(targets, ids(&["loose", "inside"]));
    }

    #[test]
    fn a_file_reachable_twice_is_imported_once() {
        let library = FakeLibrary::default()
            .with_file("shared")
            .with_folder("dir", &[&["shared"]]);

        let targets =
            resolve_import_targets(&library, &ids(&["shared", "dir"]), true, 500).expect("resolve");
        assert_eq!(targets, ids(&["shared"]));
    }

    #[test]
    fn an_expansion_yielding_no_files_is_rejected() {
        // Succeeding silently here would hide a mistyped or empty folder.
        let library = FakeLibrary::default().with_folder("empty", &[]);

        let message = resolve_import_targets(&library, &ids(&["empty"]), true, 500)
            .expect_err("an empty expansion must fail")
            .to_string();
        assert!(message.contains("no files"), "{message}");
        assert!(message.contains("Nothing was imported"), "{message}");
    }

    #[test]
    fn a_folder_of_only_subfolders_expands_to_nothing() {
        let library = FakeLibrary::default()
            .with_folder("inner", &[])
            .with_folder("outer", &[&["inner"]]);

        let message = resolve_import_targets(&library, &ids(&["outer"]), true, 500)
            .expect_err("no files anywhere in the subtree")
            .to_string();
        assert!(message.contains("no files"), "{message}");
    }

    #[test]
    fn exceeding_max_files_reports_both_numbers_and_imports_nothing() {
        let library = FakeLibrary::default()
            .with_file("f1")
            .with_file("f2")
            .with_file("f3")
            .with_folder("dir", &[&["f1", "f2", "f3"]]);

        let message = resolve_import_targets(&library, &ids(&["dir"]), true, 2)
            .expect_err("3 files against a cap of 2")
            .to_string();
        assert!(message.contains('3'), "actual count missing: {message}");
        assert!(message.contains('2'), "cap missing: {message}");
        assert!(message.contains("--max-files"), "{message}");
        assert!(message.contains("Nothing was imported"), "{message}");
    }

    #[test]
    fn the_cap_counts_deduplicated_files() {
        // Two arguments, one underlying file: that is one import, not two.
        let library = FakeLibrary::default()
            .with_file("shared")
            .with_folder("dir", &[&["shared"]]);

        let targets =
            resolve_import_targets(&library, &ids(&["shared", "dir"]), true, 1).expect("resolve");
        assert_eq!(targets, ids(&["shared"]));
    }

    /// Serves a scripted status sequence per document and advances a virtual
    /// clock instead of sleeping, so the wait rules run at full speed.
    struct FakePoller {
        /// document id → statuses on successive polls; the last one repeats.
        script: BTreeMap<String, Vec<String>>,
        polls: RefCell<BTreeMap<String, usize>>,
        elapsed: Cell<Duration>,
    }

    impl FakePoller {
        fn new(script: &[(&str, &[&str])]) -> Self {
            Self {
                script: script
                    .iter()
                    .map(|(id, statuses)| {
                        (
                            id.to_string(),
                            statuses.iter().map(|s| s.to_string()).collect(),
                        )
                    })
                    .collect(),
                polls: RefCell::new(BTreeMap::new()),
                elapsed: Cell::new(Duration::ZERO),
            }
        }
    }

    impl PollContext for FakePoller {
        fn status(&self, document_id: &str) -> Result<String> {
            let statuses = self
                .script
                .get(document_id)
                .unwrap_or_else(|| panic!("no script for `{document_id}`"));
            let mut polls = self.polls.borrow_mut();
            let seen = polls.entry(document_id.to_string()).or_insert(0);
            let status = statuses[(*seen).min(statuses.len() - 1)].clone();
            *seen += 1;
            Ok(status)
        }

        fn sleep(&self, delay: Duration) {
            self.elapsed.set(self.elapsed.get() + delay);
        }

        fn elapsed(&self) -> Duration {
            self.elapsed.get()
        }
    }

    #[test]
    fn waiting_ends_when_every_document_reaches_a_terminal_status() {
        let poller = FakePoller::new(&[
            ("doc-1", &["pending", "running", "okay"]),
            ("doc-2", &["running", "okay"]),
        ]);

        let outcome =
            wait_for_documents(&poller, &ids(&["doc-1", "doc-2"]), Duration::from_secs(600))
                .expect("wait");

        assert_eq!(outcome, WaitOutcome::default());
    }

    #[test]
    fn waiting_reports_a_document_that_errors() {
        let poller = FakePoller::new(&[
            ("doc-1", &["running", "okay"]),
            ("doc-2", &["running", "error"]),
        ]);

        let outcome =
            wait_for_documents(&poller, &ids(&["doc-1", "doc-2"]), Duration::from_secs(600))
                .expect("wait");

        assert_eq!(outcome.errored, ids(&["doc-2"]));
        assert!(outcome.timed_out.is_empty());
    }

    #[test]
    fn waiting_reports_documents_still_running_at_the_deadline() {
        // Never leaves `running`; the backoff schedule crosses 10s quickly.
        let poller = FakePoller::new(&[("doc-1", &["running"])]);

        let outcome =
            wait_for_documents(&poller, &ids(&["doc-1"]), Duration::from_secs(10)).expect("wait");

        assert_eq!(outcome.timed_out, ids(&["doc-1"]));
        assert!(outcome.errored.is_empty());
    }

    #[test]
    fn an_unrecognized_status_never_counts_as_finished() {
        let poller = FakePoller::new(&[("doc-1", &["reindexing"])]);

        let outcome =
            wait_for_documents(&poller, &ids(&["doc-1"]), Duration::from_secs(10)).expect("wait");

        assert_eq!(
            outcome.timed_out,
            ids(&["doc-1"]),
            "a status this build does not know must not be treated as terminal"
        );
    }

    #[test]
    fn waiting_on_nothing_finishes_immediately() {
        let poller = FakePoller::new(&[]);
        let outcome = wait_for_documents(&poller, &[], Duration::from_secs(600)).expect("wait");
        assert_eq!(outcome, WaitOutcome::default());
    }

    fn outcome(success: u32, failure: u32, duplicate: u32, truncated: bool) -> ImportOutcome {
        ImportOutcome {
            success_count: success,
            failure_count: failure,
            duplicate_count: duplicate,
            details: vec![ImportDetail {
                result: "success".to_string(),
                drive_item_id: Some("sc-a:inode-1".to_string()),
                document_id: Some("doc-1".to_string()),
            }],
            details_truncated: truncated,
        }
    }

    #[test]
    fn a_clean_import_has_no_failures() {
        assert!(import_failures(&outcome(3, 0, 0, false), None).is_empty());
    }

    #[test]
    fn duplicates_alone_are_not_a_failure() {
        // Re-importing a file the project already has is the documented
        // behavior, not an error.
        assert!(import_failures(&outcome(0, 0, 3, false), None).is_empty());
    }

    #[test]
    fn a_partial_failure_fails_the_command() {
        let failures = import_failures(&outcome(2, 1, 0, false), None);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("1 file(s) could not be imported"));
    }

    #[test]
    fn truncated_details_are_called_out_alongside_a_partial_failure() {
        let failures = import_failures(&outcome(900, 4, 0, true), None);
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0].contains("truncated"),
            "the user must be told the list is incomplete: {}",
            failures[0]
        );
    }

    #[test]
    fn truncated_details_alone_are_not_a_failure_without_wait() {
        // Nothing was claimed about processing, so nothing is unverified.
        assert!(import_failures(&outcome(900, 0, 0, true), None).is_empty());
    }

    #[test]
    fn truncated_details_fail_the_command_under_wait() {
        let failures = import_failures(&outcome(900, 0, 0, true), Some(&WaitOutcome::default()));
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0].contains("--wait could not cover every document"),
            "{}",
            failures[0]
        );
    }

    #[test]
    fn an_errored_document_fails_the_command() {
        let waited = WaitOutcome {
            errored: ids(&["doc-9"]),
            timed_out: Vec::new(),
        };
        let failures = import_failures(&outcome(1, 0, 0, false), Some(&waited));
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("doc-9"), "{}", failures[0]);
        assert!(failures[0].contains("error"), "{}", failures[0]);
    }

    #[test]
    fn a_timeout_fails_the_command_and_says_the_import_continues() {
        let waited = WaitOutcome {
            errored: Vec::new(),
            timed_out: ids(&["doc-9"]),
        };
        let failures = import_failures(&outcome(1, 0, 0, false), Some(&waited));
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0].contains("has not been cancelled"),
            "a timeout must not read as a cancelled import: {}",
            failures[0]
        );
    }

    #[test]
    fn every_distinct_problem_is_reported_together() {
        let waited = WaitOutcome {
            errored: ids(&["doc-8"]),
            timed_out: ids(&["doc-9"]),
        };
        let failures = import_failures(&outcome(3, 2, 0, true), Some(&waited));
        assert_eq!(
            failures.len(),
            4,
            "partial failure, truncation, error, and timeout are separate problems: {failures:?}"
        );
    }
}
