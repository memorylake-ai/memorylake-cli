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

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use super::all_projects;
use crate::commands::compact::{self, COMPACT_HELP};
use crate::commands::download::{SaveTo, save_download};
use crate::commands::{ensure_workspace_if_empty, require_workspace};
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::documents::{
    DOCUMENT_STATUS_ERROR, DeleteDocumentsRequest, Document, DocumentInspection, DocumentList,
    INSPECT_MAX_DOCUMENTS, ImportDocumentsRequest, ImportOutcome, InspectDocumentsRequest,
    ListDocumentsParams, ListGraphChildrenParams, delete_documents, download_document,
    get_document, get_document_graph, get_graph_node, get_graph_schema, import_documents,
    inspect_documents, is_terminal_status, list_documents, list_graph_children, reload_document,
};
use memorylake_core::api::library::{Item, ItemList, ListChildrenParams, get_item, list_children};
use memorylake_core::{Client, Paths};
use serde_json::{Value, json};

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
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
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
    ///
    /// Without `--project`, lists the documents of every project in the
    /// workspace, project by project. Every item carries its `project_id`, and
    /// `--continuation-token` pages across projects the same way.
    List {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project whose documents to list. Defaults to every project in the
        /// workspace.
        #[arg(long)]
        project: Option<String>,
        /// Number of items per page (1-100). The server defaults to 20.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by document name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
        #[arg(long, help = COMPACT_HELP)]
        compact: bool,
    },
    /// Get a single document, including its processing status.
    Get {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project containing the document.
        #[arg(long)]
        project: String,
        /// Document id.
        document_id: String,
    },
    /// Download a document's original file.
    ///
    /// Writes to the name the server reports, in the current directory, unless
    /// `--output` says otherwise. `--output -` streams to stdout, which is what
    /// to use when piping.
    ///
    /// An existing file is never overwritten without `--force`.
    Download {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project containing the document.
        #[arg(long)]
        project: String,
        /// Document id.
        document_id: String,
        /// Where to write it: a file path, a directory, or `-` for stdout.
        #[arg(long, short = 'o', value_name = "PATH")]
        output: Option<String>,
        /// Overwrite the destination if it already exists.
        #[arg(long)]
        force: bool,
    },
    /// Show low-level processing details for documents: parsed worksheets,
    /// generated artifacts, the pipeline that ran.
    ///
    /// Fields ending in `_s3_url` / `_s3_uri`, and `persist_path`, are
    /// pre-signed storage links that work for anyone holding them until they
    /// expire; treat the output as a credential. Documents in `error` status
    /// are left out of the response by the server, and the ids missing from
    /// it are named on stderr.
    Inspect {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project containing the documents.
        #[arg(long)]
        project: String,
        /// Document ids to inspect (1–100).
        #[arg(
            required = true,
            num_args = 1..=INSPECT_MAX_DOCUMENTS,
            value_name = "DOCUMENT_ID",
            value_parser = crate::commands::parse_non_blank
        )]
        document_ids: Vec<String>,
    },
    /// Process a failed document again.
    ///
    /// Only a document whose status is `error` can be reloaded. It keeps its
    /// id, returns to `pending`, and is processed in the background like an
    /// import; follow it with `document get`. A retry is not charged again.
    Reload {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project containing the document.
        #[arg(long)]
        project: String,
        /// Document id.
        document_id: String,
    },
    /// Browse a document's parsed graph: its root, nodes and relations.
    ///
    /// Only for accounts with the document graph enabled; others get 403
    /// `FEATURE_NOT_ENABLED`. Node ids change whenever the document is
    /// re-parsed. Storage-backed fields come back as download URLs valid for
    /// 5 hours; treat the output as a credential.
    #[command(subcommand)]
    Graph(GraphCommand),
    /// Remove documents from a project.
    ///
    /// This cannot be undone: the indexed content and every memory derived from
    /// these documents is permanently removed. There is no confirmation prompt.
    /// The Library files they came from are left untouched.
    Delete {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project to remove the documents from.
        #[arg(long)]
        project: String,
        /// Document ids to remove.
        #[arg(required = true, value_name = "DOCUMENT_ID")]
        document_ids: Vec<String>,
    },
}

/// `project document graph` subcommands.
#[derive(Debug, Subcommand)]
pub enum GraphCommand {
    /// Show a document's graph overview: root node and node counts per type.
    Overview {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project containing the document.
        #[arg(long)]
        project: String,
        /// Document id.
        document_id: String,
        /// Also return the first N children (0-100) of each of the root's list relations.
        #[arg(long, value_parser = clap::value_parser!(u32).range(0..=100))]
        preview: Option<u32>,
    },
    /// Show one node: its fields, parent, links and child counts.
    ///
    /// A block id from a search hit (`paragraph_id`, `table_id`,
    /// `figure_entity_id`) is a valid node id.
    Node {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project containing the document.
        #[arg(long)]
        project: String,
        /// Document id.
        document_id: String,
        /// Node id.
        node_id: String,
        /// Comma-separated one-to-one links to return beside the node (e.g. `summary`).
        #[arg(long)]
        expand: Option<String>,
    },
    /// List one page of the children under a node's list relation.
    Children {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project containing the document.
        #[arg(long)]
        project: String,
        /// Document id.
        document_id: String,
        /// Node id.
        node_id: String,
        /// Relation name, one of the node's `edges` (e.g. `paragraphs`, `chunks`).
        edge: String,
        /// Most children in this page (1-500).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=500))]
        count: Option<u32>,
        /// Continuation token from a previous page.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Leave out each child's own child counts (cheaper for leaf nodes).
        #[arg(long)]
        no_edge_counts: bool,
        /// Comma-separated one-to-one links of the children to return beside the page.
        #[arg(long)]
        expand: Option<String>,
    },
    /// Show the catalog of every node type: fields, links and relations.
    ///
    /// Platform-wide, not tied to a document.
    Schema,
}

/// Page size of a workspace-wide listing when `--page-size` is left out; the
/// same as the server's default for one project.
const DEFAULT_DOCUMENT_PAGE_SIZE: u32 = 20;

/// Marks a continuation token that pages across projects, so a token from a
/// single-project listing is refused instead of misread.
const WORKSPACE_TOKEN_PREFIX: &str = "ws1:";

/// A document as the API returns it, plus the project it belongs to.
fn with_project_id(document: &Document, project: &str) -> Result<Value> {
    let mut value = serde_json::to_value(document).context("serialize document")?;
    if let Value::Object(map) = &mut value {
        map.insert("project_id".into(), project.into());
    }
    Ok(value)
}

/// Token resuming a workspace-wide listing at `project`, inside it at `inner`
/// (that project's own token), or at its start when `inner` is `None`.
fn workspace_token(project: &str, inner: Option<&str>) -> String {
    format!("{WORKSPACE_TOKEN_PREFIX}{project}:{}", inner.unwrap_or(""))
}

/// Split a token made by [`workspace_token`] into its project and inner token.
fn parse_workspace_token(token: &str) -> Result<(&str, Option<&str>)> {
    let Some((project, inner)) = token
        .strip_prefix(WORKSPACE_TOKEN_PREFIX)
        .and_then(|rest| rest.split_once(':'))
        .filter(|(project, _)| !project.is_empty())
    else {
        bail!(
            "--continuation-token `{token}` did not come from a listing without --project; \
             pass the token printed by the previous page of this same listing"
        );
    };
    Ok((project, (!inner.is_empty()).then_some(inner)))
}

/// Documents paired with their project id, and the token for the next page.
type DocumentPage = (Vec<(String, Document)>, Option<String>);

/// One page of documents across `projects`, in order, at most `page_size`.
///
/// Each project's own pages are read through `fetch(project, page_size,
/// token)`, asking only for what still fits on this page. The returned token
/// names the project to resume in and, when the page ended inside one, that
/// project's token. Kept apart from the client so the paging rules are tested
/// without a network.
fn page_across_projects(
    projects: &[String],
    page_size: u32,
    token: Option<&str>,
    mut fetch: impl FnMut(&str, u32, Option<String>) -> Result<DocumentList>,
) -> Result<DocumentPage> {
    let (mut index, mut inner) = match token {
        None => (0, None),
        Some(token) => {
            let (project, inner) = parse_workspace_token(token)?;
            let Some(index) = projects.iter().position(|id| id == project) else {
                bail!(
                    "project `{project}` from --continuation-token is no longer in the workspace; \
                     list again without --continuation-token"
                );
            };
            (index, inner.map(str::to_string))
        }
    };
    let page_size = page_size.max(1) as usize;
    let mut items = Vec::new();
    while index < projects.len() {
        let project = &projects[index];
        let remaining = (page_size - items.len()) as u32;
        let page = fetch(project, remaining, inner.take())?;
        let got_any = !page.items.is_empty();
        items.extend(
            page.items
                .into_iter()
                .map(|document| (project.clone(), document)),
        );
        let next = page.continuation_token.filter(|next| !next.is_empty());
        match next {
            // More in this project, and an empty page with a token is not
            // trusted to ever end.
            Some(next) if got_any => {
                if items.len() >= page_size {
                    return Ok((items, Some(workspace_token(project, Some(&next)))));
                }
                inner = Some(next);
            }
            _ => {
                index += 1;
                if items.len() >= page_size {
                    let token = projects
                        .get(index)
                        .map(|project| workspace_token(project, None));
                    return Ok((items, token));
                }
            }
        }
    }
    Ok((items, None))
}

/// Execute a `project document` subcommand.
pub fn run(client: &Client, paths: &Paths, profile: &str, command: DocumentCommand) -> Result<()> {
    match command {
        DocumentCommand::Import {
            workspace,
            project,
            item_ids,
            recursive,
            max_files,
            wait,
            timeout,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            run_import(
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
            )
        }
        DocumentCommand::List {
            workspace,
            project,
            page_size,
            continuation_token,
            name_fuzzy,
            compact,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            let (items, continuation_token) = match project {
                Some(project) => {
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
                    let items = data
                        .items
                        .into_iter()
                        .map(|document| (project.clone(), document))
                        .collect();
                    (items, data.continuation_token)
                }
                None => {
                    let projects: Vec<String> = all_projects(client, &workspace)?
                        .into_iter()
                        .map(|project| project.id)
                        .collect();
                    ensure_workspace_if_empty(client, &workspace, projects.is_empty())
                        .context("list documents")?;
                    page_across_projects(
                        &projects,
                        page_size.unwrap_or(DEFAULT_DOCUMENT_PAGE_SIZE),
                        continuation_token.as_deref(),
                        |project, page_size, token| {
                            list_documents(
                                client,
                                &workspace,
                                project,
                                &ListDocumentsParams {
                                    page_size: Some(page_size),
                                    continuation_token: token,
                                    name_fuzzy: name_fuzzy.clone(),
                                },
                            )
                            .with_context(|| format!("list documents in project `{project}`"))
                        },
                    )?
                }
            };
            let items: Vec<Value> = items
                .iter()
                .map(|(project, document)| {
                    if compact {
                        Ok(compact::document(project, document))
                    } else {
                        with_project_id(document, project)
                    }
                })
                .collect::<Result<_>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "items": items,
                    "continuation_token": continuation_token,
                }))?
            );
            Ok(())
        }
        DocumentCommand::Get {
            workspace,
            project,
            document_id,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            let data = get_document(client, &workspace, &project, &document_id)
                .with_context(|| format!("get document `{document_id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
            Ok(())
        }
        DocumentCommand::Download {
            workspace,
            project,
            document_id,
            output,
            force,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            save_download(
                SaveTo {
                    output: output.as_deref(),
                    force,
                    default_name: None,
                },
                &format!("document `{document_id}`"),
                &document_id,
                |mut out| download_document(client, &workspace, &project, &document_id, &mut out),
            )
        }
        DocumentCommand::Inspect {
            workspace,
            project,
            document_ids,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            let data = inspect_documents(
                client,
                &workspace,
                &project,
                &InspectDocumentsRequest {
                    document_ids: document_ids.clone(),
                },
            )
            .with_context(|| format!("inspect documents in project `{project}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);

            let missing = uninspected_ids(&document_ids, &data);
            if !missing.is_empty() {
                eprintln!(
                    "no details returned for {} document(s): {} \
                     (the server omits documents whose status is `error`; check with `document get`)",
                    missing.len(),
                    missing.join(", ")
                );
            }
            Ok(())
        }
        DocumentCommand::Reload {
            workspace,
            project,
            document_id,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            reload_document(client, &workspace, &project, &document_id)
                .with_context(|| format!("reload document `{document_id}`"))?;
            println!(
                "Queued document `{document_id}` for processing again; follow it with `project document get`"
            );
            Ok(())
        }
        DocumentCommand::Graph(command) => run_graph(client, paths, profile, command),
        DocumentCommand::Delete {
            workspace,
            project,
            document_ids,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
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

fn run_graph(client: &Client, paths: &Paths, profile: &str, command: GraphCommand) -> Result<()> {
    let data = match command {
        GraphCommand::Overview {
            workspace,
            project,
            document_id,
            preview,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            get_document_graph(client, &workspace, &project, &document_id, preview)
                .with_context(|| format!("get graph of document `{document_id}`"))?
        }
        GraphCommand::Node {
            workspace,
            project,
            document_id,
            node_id,
            expand,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            get_graph_node(
                client,
                &workspace,
                &project,
                &document_id,
                &node_id,
                expand.as_deref(),
            )
            .with_context(|| format!("get graph node `{node_id}`"))?
        }
        GraphCommand::Children {
            workspace,
            project,
            document_id,
            node_id,
            edge,
            count,
            continuation_token,
            no_edge_counts,
            expand,
        } => {
            let workspace = require_workspace(paths, profile, workspace)?;
            list_graph_children(
                client,
                &workspace,
                &project,
                &document_id,
                &node_id,
                &edge,
                &ListGraphChildrenParams {
                    count,
                    continuation_token,
                    with_edge_counts: no_edge_counts.then_some(false),
                    expand,
                },
            )
            .with_context(|| format!("list `{edge}` of graph node `{node_id}`"))?
        }
        GraphCommand::Schema => {
            get_graph_schema(client).context("get the document graph schema")?
        }
    };
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

/// Requested ids the inspect response said nothing about, in request order,
/// each named once.
fn uninspected_ids(requested: &[String], inspection: &DocumentInspection) -> Vec<String> {
    let returned: BTreeSet<&str> = inspection
        .items
        .iter()
        .filter_map(|item| item.document_id.as_deref())
        .collect();
    let mut seen = BTreeSet::new();
    requested
        .iter()
        .filter(|id| !returned.contains(id.as_str()) && seen.insert(id.as_str()))
        .cloned()
        .collect()
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

    let (waited, polled) = if options.wait {
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
        let polled = document_ids.len();
        let waited = match wait_for_documents(&poller, &document_ids, options.timeout) {
            Ok(waited) => waited,
            Err(err) => {
                // The batch was accepted before polling broke; say what was
                // accepted rather than leave stdout empty.
                println!("{}", serde_json::to_string_pretty(&outcome)?);
                return Err(err);
            }
        };
        (Some(waited), polled)
    } else {
        (None, 0)
    };

    // Printed once, after any waiting, and before any failure is raised: the
    // batch has already run, and the caller must not have to choose between
    // seeing the result and seeing the failure. Under --wait the payload
    // carries each document's final status, so stdout alone tells an import
    // that failed processing from one that succeeded.
    match &waited {
        Some(waited) => println!(
            "{}",
            serde_json::to_string_pretty(&import_report(&outcome, waited)?)?
        ),
        None => println!("{}", serde_json::to_string_pretty(&outcome)?),
    }

    let failures = import_failures(&outcome, waited.as_ref(), polled);
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
                with_xattr_keys: None,
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
    /// Current processing state of one document.
    fn status(&self, document_id: &str) -> Result<DocumentState>;
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
    fn status(&self, document_id: &str) -> Result<DocumentState> {
        let document = get_document(self.client, self.workspace, self.project, document_id)
            .with_context(|| format!("poll document `{document_id}`"))?;
        Ok(DocumentState {
            status: document.status,
            error: document.error,
        })
    }

    fn sleep(&self, delay: Duration) {
        std::thread::sleep(delay);
    }

    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// One poll's reading of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentState {
    /// Processing status as the server reported it.
    status: String,
    /// Error detail, set when `status` is `error`.
    error: Option<Value>,
}

/// How waiting ended.
#[derive(Debug, Default, PartialEq, Eq)]
struct WaitOutcome {
    /// Documents that settled on `error`.
    errored: Vec<String>,
    /// Documents still non-terminal when the timeout elapsed.
    timed_out: Vec<String>,
    /// Last state observed for every polled document: terminal for settled
    /// ones, whatever it was at the deadline for timed-out ones.
    last_seen: BTreeMap<String, DocumentState>,
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
            let state = ctx.status(&id)?;
            if !is_terminal_status(&state.status) {
                still_pending.push(id.clone());
            } else if state.status == DOCUMENT_STATUS_ERROR {
                outcome.errored.push(id.clone());
            }
            outcome.last_seen.insert(id, state);
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

/// The payload `import --wait` prints.
///
/// The server's acceptance summary, where each `details` entry also gets the document's final `status` (and `error`,
/// when there is one), and a top-level `processing` tally counts how the
/// polled documents ended. `success_count` and `result` keep describing
/// acceptance, so existing parsers see the same fields; the added ones are
/// what say whether processing worked.
fn import_report(outcome: &ImportOutcome, waited: &WaitOutcome) -> Result<Value> {
    let mut report = serde_json::to_value(outcome)?;

    if let Some(details) = report.get_mut("details").and_then(Value::as_array_mut) {
        for detail in details {
            let Some(state) = detail
                .get("document_id")
                .and_then(Value::as_str)
                .and_then(|id| waited.last_seen.get(id))
            else {
                continue;
            };
            if let Some(entry) = detail.as_object_mut() {
                entry.insert("status".to_string(), json!(state.status));
                if let Some(error) = &state.error {
                    entry.insert("error".to_string(), error.clone());
                }
            }
        }
    }

    let errored = waited.errored.len();
    let timed_out = waited.timed_out.len();
    let okay = waited.last_seen.len().saturating_sub(errored + timed_out);
    if let Some(top) = report.as_object_mut() {
        top.insert(
            "processing".to_string(),
            json!({ "okay": okay, "error": errored, "timed_out": timed_out }),
        );
    }
    Ok(report)
}

/// Reasons an accepted import must still fail the command.
///
/// The import endpoint answers 200 even when individual files failed, so
/// success has to be decided here rather than taken from the HTTP result. An
/// empty list means the command succeeded.
fn import_failures(
    outcome: &ImportOutcome,
    waited: Option<&WaitOutcome>,
    polled: usize,
) -> Vec<String> {
    let mut failures = Vec::new();

    if outcome.failure_count > 0 {
        let mut reason = format!("{} file(s) could not be imported", outcome.failure_count);
        if outcome.details_truncated {
            reason.push_str(
                "; the server truncated the per-file details, so the output above does not name all of them",
            );
        }
        if outcome.quota_exceeded {
            reason.push_str(
                "; the import stopped part-way because the remaining quota could not cover the rest of the files",
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

    // Same blind spot without truncation: polling covers only the document
    // ids named in `details`. If the server accepted more files than it
    // named ids for, the difference was never polled, and exiting cleanly
    // would claim a verification that never happened.
    let accepted = u64::from(outcome.success_count) + u64::from(outcome.duplicate_count);
    if !outcome.details_truncated && (polled as u64) < accepted {
        failures.push(format!(
            "--wait could not cover every document: the server accepted {accepted} file(s) \
             but named only {polled} document id(s) in `details`, so the rest were never polled"
        ));
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

    /// A fake document store: each project's documents, served `per_page` at
    /// a time with the offset as the token, and every call recorded.
    struct FakeProjects {
        docs: BTreeMap<String, Vec<String>>,
        per_page: usize,
        calls: RefCell<Vec<(String, u32, Option<String>)>>,
    }

    impl FakeProjects {
        fn new(per_page: usize, docs: &[(&str, usize)]) -> Self {
            Self {
                docs: docs
                    .iter()
                    .map(|(project, count)| {
                        let ids = (0..*count).map(|n| format!("{project}-d{n}")).collect();
                        (project.to_string(), ids)
                    })
                    .collect(),
                per_page,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn fetch(
            &self,
            project: &str,
            page_size: u32,
            token: Option<String>,
        ) -> Result<DocumentList> {
            self.calls
                .borrow_mut()
                .push((project.to_string(), page_size, token.clone()));
            let all = &self.docs[project];
            let start: usize = token.map_or(0, |token| token.parse().expect("offset token"));
            let end = (start + self.per_page.min(page_size as usize)).min(all.len());
            let items = all[start..end]
                .iter()
                .map(|id| {
                    serde_json::from_value(json!({"id": id, "name": id, "status": "okay"}))
                        .expect("document")
                })
                .collect();
            let continuation_token = (end < all.len()).then(|| end.to_string());
            Ok(DocumentList {
                items,
                continuation_token,
            })
        }

        fn page(&self, page_size: u32, token: Option<&str>) -> (Vec<String>, Option<String>) {
            let projects: Vec<String> = self.docs.keys().cloned().collect();
            let (items, token) =
                page_across_projects(&projects, page_size, token, |p, n, t| self.fetch(p, n, t))
                    .expect("page");
            let ids = items
                .into_iter()
                .map(|(project, document)| {
                    assert!(
                        document.id.starts_with(&project),
                        "{project} owns {}",
                        document.id
                    );
                    document.id
                })
                .collect();
            (ids, token)
        }
    }

    #[test]
    fn workspace_listing_fills_a_page_from_several_projects() {
        let fake = FakeProjects::new(100, &[("p1", 2), ("p2", 1), ("p3", 3)]);
        let (ids, token) = fake.page(5, None);
        assert_eq!(ids, ["p1-d0", "p1-d1", "p2-d0", "p3-d0", "p3-d1"]);
        assert_eq!(token.as_deref(), Some("ws1:p3:2"));
        // Each request asks only for what still fits on the page.
        let sizes: Vec<u32> = fake.calls.borrow().iter().map(|call| call.1).collect();
        assert_eq!(sizes, [5, 3, 2]);

        let (ids, token) = fake.page(5, token.as_deref());
        assert_eq!(ids, ["p3-d2"]);
        assert_eq!(token, None);
    }

    #[test]
    fn workspace_listing_visits_every_document_once_whatever_the_page_size() {
        for page_size in 1..=7 {
            for per_page in 1..=4 {
                let fake = FakeProjects::new(per_page, &[("p1", 3), ("p2", 0), ("p3", 4)]);
                let mut seen = Vec::new();
                let mut token = None;
                loop {
                    let (ids, next) = fake.page(page_size, token.as_deref());
                    assert!(ids.len() <= page_size as usize);
                    seen.extend(ids);
                    match next {
                        Some(next) => token = Some(next),
                        None => break,
                    }
                }
                let expected: Vec<String> = [
                    "p1-d0", "p1-d1", "p1-d2", "p3-d0", "p3-d1", "p3-d2", "p3-d3",
                ]
                .map(String::from)
                .to_vec();
                assert_eq!(seen, expected, "page_size {page_size}, per_page {per_page}");
            }
        }
    }

    #[test]
    fn workspace_listing_ending_on_a_project_boundary_resumes_at_the_next_project() {
        let fake = FakeProjects::new(100, &[("p1", 2), ("p2", 1)]);
        let (ids, token) = fake.page(2, None);
        assert_eq!(ids, ["p1-d0", "p1-d1"]);
        assert_eq!(token.as_deref(), Some("ws1:p2:"));
        // The last project ending the page exactly leaves nothing to resume.
        let (_, token) = fake.page(1, token.as_deref());
        assert_eq!(token, None);
    }

    #[test]
    fn workspace_listing_refuses_a_foreign_or_stale_token() {
        let fake = FakeProjects::new(100, &[("p1", 1)]);
        let projects = vec!["p1".to_string()];
        let fetch = |p: &str, n: u32, t: Option<String>| fake.fetch(p, n, t);
        let err = page_across_projects(&projects, 5, Some("opaque-server-token"), fetch)
            .expect_err("a single-project token is refused");
        assert!(err.to_string().contains("without --project"), "{err}");
        let fetch = |p: &str, n: u32, t: Option<String>| fake.fetch(p, n, t);
        let err = page_across_projects(&projects, 5, Some("ws1:p-gone:"), fetch)
            .expect_err("a deleted project is refused");
        assert!(
            err.to_string().contains("no longer in the workspace"),
            "{err}"
        );
        assert!(
            fake.calls.borrow().is_empty(),
            "nothing is fetched for a bad token"
        );
    }

    #[test]
    fn ids_left_out_of_an_inspection_are_named_once_in_request_order() {
        let inspection: DocumentInspection =
            serde_json::from_str(r#"{"items":[{"document_id":"doc-2"}]}"#).unwrap();
        let requested = vec![
            "doc-3".to_string(),
            "doc-2".to_string(),
            "doc-1".to_string(),
            "doc-3".to_string(),
        ];
        assert_eq!(
            uninspected_ids(&requested, &inspection),
            vec!["doc-3".to_string(), "doc-1".to_string()]
        );
        assert!(uninspected_ids(&["doc-2".to_string()], &inspection).is_empty());
    }

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
        fn status(&self, document_id: &str) -> Result<DocumentState> {
            let statuses = self
                .script
                .get(document_id)
                .unwrap_or_else(|| panic!("no script for `{document_id}`"));
            let mut polls = self.polls.borrow_mut();
            let seen = polls.entry(document_id.to_string()).or_insert(0);
            let status = statuses[(*seen).min(statuses.len() - 1)].clone();
            *seen += 1;
            let error = (status == DOCUMENT_STATUS_ERROR)
                .then(|| json!({"code": "ERROR_FILE_CORRUPTED", "msg": "unreadable"}));
            Ok(DocumentState { status, error })
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

        assert!(outcome.errored.is_empty());
        assert!(outcome.timed_out.is_empty());
        assert_eq!(outcome.last_seen["doc-1"].status, "okay");
        assert_eq!(outcome.last_seen["doc-2"].status, "okay");
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
            quota_exceeded: false,
        }
    }

    #[test]
    fn a_quota_stop_is_named_in_the_failure() {
        let mut stopped = outcome(1, 2, 0, false);
        stopped.quota_exceeded = true;
        let failures = import_failures(&stopped, None, 0);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("quota"), "{failures:?}");
    }

    #[test]
    fn a_clean_import_has_no_failures() {
        assert!(import_failures(&outcome(3, 0, 0, false), None, 0).is_empty());
    }

    #[test]
    fn waiting_on_fewer_ids_than_accepted_is_reported() {
        // The server said it accepted three files but details named only one
        // document id (and nothing was truncated): two documents were never
        // polled, and --wait must not exit cleanly pretending they were.
        let failures = import_failures(&outcome(3, 0, 0, false), Some(&WaitOutcome::default()), 1);
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0].contains("--wait could not cover every document"),
            "{failures:?}"
        );
        assert!(failures[0].contains("accepted 3"), "{failures:?}");
        assert!(failures[0].contains("only 1"), "{failures:?}");
    }

    #[test]
    fn duplicates_count_toward_wait_coverage() {
        // A duplicate detail carries the existing document id, so it is
        // pollable and belongs in the accepted-vs-polled reconciliation.
        let failures = import_failures(&outcome(0, 0, 2, false), Some(&WaitOutcome::default()), 1);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("accepted 2"), "{failures:?}");
    }

    #[test]
    fn full_coverage_under_wait_is_clean() {
        assert!(
            import_failures(&outcome(1, 0, 0, false), Some(&WaitOutcome::default()), 1).is_empty()
        );
    }

    #[test]
    fn duplicates_alone_are_not_a_failure() {
        // Re-importing a file the project already has is the documented
        // behavior, not an error.
        assert!(import_failures(&outcome(0, 0, 3, false), None, 0).is_empty());
    }

    #[test]
    fn a_partial_failure_fails_the_command() {
        let failures = import_failures(&outcome(2, 1, 0, false), None, 0);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("1 file(s) could not be imported"));
    }

    #[test]
    fn truncated_details_are_called_out_alongside_a_partial_failure() {
        let failures = import_failures(&outcome(900, 4, 0, true), None, 0);
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
        assert!(import_failures(&outcome(900, 0, 0, true), None, 0).is_empty());
    }

    #[test]
    fn truncated_details_fail_the_command_under_wait() {
        let failures = import_failures(&outcome(900, 0, 0, true), Some(&WaitOutcome::default()), 1);
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0].contains("--wait could not cover every document"),
            "{}",
            failures[0]
        );
    }

    #[test]
    fn under_wait_the_report_says_a_document_failed_processing() {
        // The import was accepted (`result: success`) but the document then
        // errored; stdout must say so rather than read as a success.
        let poller = FakePoller::new(&[("doc-1", &["running", "error"])]);
        let waited =
            wait_for_documents(&poller, &ids(&["doc-1"]), Duration::from_secs(600)).expect("wait");

        let report = import_report(&outcome(1, 0, 0, false), &waited).expect("report");

        assert_eq!(report["success_count"], 1, "acceptance fields are kept");
        assert_eq!(report["details"][0]["result"], "success");
        assert_eq!(report["details"][0]["status"], "error");
        assert_eq!(
            report["details"][0]["error"]["code"],
            "ERROR_FILE_CORRUPTED"
        );
        assert_eq!(
            report["processing"],
            json!({"okay": 0, "error": 1, "timed_out": 0})
        );
    }

    #[test]
    fn under_wait_the_report_tallies_okay_and_timed_out_documents() {
        let mut accepted = outcome(2, 0, 0, false);
        accepted.details.push(ImportDetail {
            result: "success".to_string(),
            drive_item_id: Some("sc-a:inode-2".to_string()),
            document_id: Some("doc-2".to_string()),
        });
        let poller = FakePoller::new(&[("doc-1", &["okay"]), ("doc-2", &["running"])]);
        let waited =
            wait_for_documents(&poller, &ids(&["doc-1", "doc-2"]), Duration::from_secs(10))
                .expect("wait");

        let report = import_report(&accepted, &waited).expect("report");

        assert_eq!(report["details"][0]["status"], "okay");
        assert!(report["details"][0].get("error").is_none());
        assert_eq!(report["details"][1]["status"], "running");
        assert_eq!(
            report["processing"],
            json!({"okay": 1, "error": 0, "timed_out": 1})
        );
    }

    #[test]
    fn an_errored_document_fails_the_command() {
        let waited = WaitOutcome {
            errored: ids(&["doc-9"]),
            timed_out: Vec::new(),
            ..WaitOutcome::default()
        };
        let failures = import_failures(&outcome(1, 0, 0, false), Some(&waited), 1);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("doc-9"), "{}", failures[0]);
        assert!(failures[0].contains("error"), "{}", failures[0]);
    }

    #[test]
    fn a_timeout_fails_the_command_and_says_the_import_continues() {
        let waited = WaitOutcome {
            errored: Vec::new(),
            timed_out: ids(&["doc-9"]),
            ..WaitOutcome::default()
        };
        let failures = import_failures(&outcome(1, 0, 0, false), Some(&waited), 1);
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
            ..WaitOutcome::default()
        };
        let failures = import_failures(&outcome(3, 2, 0, true), Some(&waited), 1);
        assert_eq!(
            failures.len(),
            4,
            "partial failure, truncation, error, and timeout are separate problems: {failures:?}"
        );
    }
}
