//! `memorylake integration` / `im` commands: IM bots connected to agents.
//!
//! Credentials never travel on the command line — a value there lands in
//! shell history and the process list. They are read as a JSON object from
//! an environment variable, standard input, or a file, keyed by the
//! platform's own field names (`appKey`, `botToken`, …).

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use memorylake_core::api::admin::{
    CHANNELS, CreateIntegrationRequest, Integration, IntegrationCredentials, IntegrationSettings,
    ListIntegrationsParams, MAX_INTEGRATION_DATASET_ID, MAX_INTEGRATION_DATASETS,
    MAX_INTEGRATION_NAME, UpdateIntegrationRequest, create_integration, delete_integration,
    get_integration, list_integrations, update_integration,
};
use memorylake_core::config::{ResolveOverrides, resolve};
use memorylake_core::{Client, Paths};

use super::input::{read_file_or_stdin, strip_one_line_ending};
use super::{parse_non_blank, print_json, require_workspace};
use crate::interactive::prompt_secret_verbatim;

/// How long `--wait` follows a pending connection by default.
const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 120;
/// Gap between `--wait` polls.
const WAIT_INTERVAL: Duration = Duration::from_secs(2);

/// The `--help` epilogue of `create` and `update`: the credential fields
/// of each channel, which flattened arguments cannot show.
const CREDENTIALS_HELP: &str = "\
Credentials: a JSON object of the platform's own fields, read from
--credentials-env VAR, --credentials-stdin or --credentials-file PATH (never
from the command line):
  dingtalk  {\"appKey\": \"...\", \"appSecret\": \"...\"}
  feishu    {\"appId\": \"...\", \"appSecret\": \"...\"}
  slack     {\"botToken\": \"xoxb-...\", \"appToken\": \"xapp-...\"}
  wecom     {\"botId\": \"...\", \"secret\": \"...\"}
`create` needs both fields. `update` takes only the ones that change, e.g.
{\"appSecret\": \"...\"} to rotate a secret; the bot itself (appKey, appId,
botId, or the Slack workspace) cannot change.

Example:
  export SLACK_BOT='{\"botToken\":\"xoxb-...\",\"appToken\":\"xapp-...\"}'
  memorylake im create --channel slack --name Help --agent <agent-id> \\
      --read-write-project <project-id> --credentials-env SLACK_BOT --wait";

/// `--wait` and its `--timeout`.
#[derive(Debug, Clone, Args)]
pub struct WaitArgs {
    /// Poll until the status leaves `pending`, and fail if it ends in
    /// `error`, naming its `status_error_code`.
    #[arg(long)]
    wait: bool,
    /// Seconds to keep polling. Only meaningful with `--wait`. Giving up
    /// leaves the integration as it is; follow it with `integration get`.
    #[arg(long, default_value_t = DEFAULT_WAIT_TIMEOUT_SECS, value_name = "SECS", value_parser = clap::value_parser!(u64).range(1..))]
    timeout: u64,
}

/// Integration subcommands.
#[derive(Debug, Subcommand)]
pub enum IntegrationCommand {
    /// List the integrations in the workspaces you may manage, newest first.
    /// Credentials are never returned.
    List {
        /// Only this workspace's integrations. Not taken from `workspace
        /// use`: without it, every workspace you may manage is listed.
        #[arg(long, value_parser = parse_non_blank)]
        workspace: Option<String>,
        /// Only integrations answered by this agent.
        #[arg(long, value_name = "AGENT_ID", value_parser = parse_non_blank)]
        agent: Option<String>,
        /// Number of items per page (1–100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Get one integration. Its `status` goes pending → connected or error;
    /// `status_error_code` says why.
    Get {
        /// Integration id (from `integration list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
    },
    /// Connect a bot to an agent. DingTalk, Feishu and Slack credentials are
    /// verified with the platform before anything is saved; WeCom's are
    /// checked by connecting, so follow the status (or pass --wait). A bot
    /// can be connected only once.
    #[command(after_long_help = CREDENTIALS_HELP)]
    Create {
        /// IM platform.
        #[arg(long, value_parser = CHANNELS)]
        channel: String,
        /// Workspace the integration belongs to; cannot change later.
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long, value_parser = parse_non_blank)]
        workspace: Option<String>,
        /// Display name (1–255 characters).
        #[arg(long, value_parser = parse_non_blank)]
        name: String,
        /// Agent that answers messages sent to the bot (from `agent list`).
        #[arg(long, value_name = "AGENT_ID", value_parser = parse_non_blank)]
        agent: String,
        /// Project that memories extracted from IM conversations are written
        /// to (from `project list`, in the same workspace). Anyone who can
        /// message the bot can add to it.
        #[arg(long = "read-write-project", value_name = "PROJECT_ID", value_parser = parse_non_blank)]
        read_write_project: String,
        /// Additional project searched read-only. Repeat, or separate with
        /// commas, for several.
        #[arg(long = "read-only-project", value_name = "PROJECT_ID", value_delimiter = ',', value_parser = parse_non_blank)]
        read_only_projects: Vec<String>,
        /// Open-data dataset the agent may use (from `opendata list`; at most
        /// 64). Repeat, or separate with commas, for several.
        #[arg(long = "dataset", value_name = "DATASET_ID", value_delimiter = ',', value_parser = parse_non_blank)]
        datasets: Vec<String>,
        /// Messages handled per day; 0 or absent = unlimited.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        daily_message_limit: Option<i64>,
        /// DingTalk only: AI card template id, to stream replies into an
        /// interactive card.
        #[arg(long, value_name = "TEMPLATE_ID")]
        card_template_id: Option<String>,
        #[command(flatten)]
        credentials: CredentialsArgs,
        /// Retrying with the same value replays the first result instead of
        /// connecting a second time. Credential values are not compared.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
        #[command(flatten)]
        wait: WaitArgs,
    },
    /// Change an integration. Only the flags given change. Credentials not
    /// given are kept; give only the values that change to rotate a secret.
    /// The bot's identifier cannot change: delete and recreate instead.
    #[command(after_long_help = CREDENTIALS_HELP)]
    Update {
        /// Integration id (from `integration list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// New display name.
        #[arg(long, value_parser = parse_non_blank)]
        name: Option<String>,
        /// Switch the answering agent; ongoing conversations start over.
        #[arg(long, value_name = "AGENT_ID", value_parser = parse_non_blank)]
        agent: Option<String>,
        /// New project for extracted memories.
        #[arg(long = "read-write-project", value_name = "PROJECT_ID", value_parser = parse_non_blank)]
        read_write_project: Option<String>,
        /// Replace the read-only projects with these.
        #[arg(long = "read-only-project", value_name = "PROJECT_ID", value_delimiter = ',', value_parser = parse_non_blank, conflicts_with = "no_read_only_projects")]
        read_only_projects: Vec<String>,
        /// Remove every read-only project.
        #[arg(long)]
        no_read_only_projects: bool,
        /// Replace the datasets with these.
        #[arg(long = "dataset", value_name = "DATASET_ID", value_delimiter = ',', value_parser = parse_non_blank, conflicts_with = "no_datasets")]
        datasets: Vec<String>,
        /// Remove every dataset.
        #[arg(long)]
        no_datasets: bool,
        /// Messages handled per day; 0 = unlimited.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        daily_message_limit: Option<i64>,
        /// DingTalk only: AI card template id; "" goes back to plain messages.
        #[arg(long, value_name = "TEMPLATE_ID")]
        card_template_id: Option<String>,
        /// Connect the bot again after --disable. Enabling an integration
        /// that is already enabled starts no reconnection.
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        /// Disconnect the bot, keeping everything else.
        #[arg(long)]
        disable: bool,
        #[command(flatten)]
        credentials: CredentialsArgs,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
        /// Follow the reconnection that new credentials or --enable start.
        /// When the update starts none, there is nothing to wait for and the
        /// integration is printed as it is.
        #[command(flatten)]
        wait: WaitArgs,
    },
    /// Disconnect the bot and delete the integration with its conversation
    /// mappings. Conversation history is kept. There is no confirmation
    /// prompt.
    Delete {
        /// Integration id (from `integration list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
}

/// Where the bot's credentials come from; at most one source.
///
/// The value is a JSON object keyed by the platform's field names:
/// dingtalk `appKey` + `appSecret`; feishu `appId` + `appSecret`; slack
/// `botToken` + `appToken`; wecom `botId` + `secret`.
#[derive(Debug, Clone, Default, Args)]
#[group(id = "credentials_source", multiple = false)]
pub struct CredentialsArgs {
    /// Read the credentials JSON from this environment variable.
    #[arg(long, value_name = "VAR")]
    credentials_env: Option<String>,
    /// Read the credentials JSON from standard input. At a terminal, asks
    /// without echo instead.
    #[arg(long)]
    credentials_stdin: bool,
    /// Read the credentials JSON from this file.
    #[arg(long, value_name = "PATH")]
    credentials_file: Option<PathBuf>,
}

impl CredentialsArgs {
    /// Read the credentials from whichever source was named; `None` when
    /// none was.
    fn read(&self) -> Result<Option<IntegrationCredentials>> {
        let text = if let Some(var) = &self.credentials_env {
            match std::env::var(var) {
                Ok(value) => value,
                Err(std::env::VarError::NotPresent) => {
                    bail!("--credentials-env: environment variable `{var}` is not set")
                }
                Err(std::env::VarError::NotUnicode(_)) => {
                    bail!("--credentials-env: environment variable `{var}` is not valid UTF-8")
                }
            }
        } else if self.credentials_stdin {
            if std::io::stdin().is_terminal() {
                prompt_secret_verbatim("Credentials JSON").context("read credentials")?
            } else {
                read_file_or_stdin(std::path::Path::new("-"), "credentials")?
            }
        } else if let Some(path) = &self.credentials_file {
            read_file_or_stdin(path, "credentials file")?
        } else {
            return Ok(None);
        };
        IntegrationCredentials::from_json(strip_one_line_ending(&text))
            .map(Some)
            .map_err(|err| anyhow::anyhow!("invalid credentials: {err}"))
    }
}

/// Check a list of ids for the server's limits.
fn check_lists(name: &str, datasets: &[String]) -> Result<()> {
    if name.chars().count() > MAX_INTEGRATION_NAME {
        bail!("--name must be at most {MAX_INTEGRATION_NAME} characters");
    }
    if let Some(long) = datasets
        .iter()
        .find(|id| id.len() > MAX_INTEGRATION_DATASET_ID)
    {
        bail!("dataset id `{long}` is longer than {MAX_INTEGRATION_DATASET_ID} bytes");
    }
    if datasets.len() > MAX_INTEGRATION_DATASETS {
        bail!(
            "at most {MAX_INTEGRATION_DATASETS} datasets, got {}",
            datasets.len()
        );
    }
    Ok(())
}

/// Follow `status` until it leaves `pending`, then print the integration.
/// A final `error` fails the command, naming the reason code.
///
/// The write has already happened, so a failed poll says so and names the
/// id: retrying the write would only collide with what it saved.
fn wait_until_settled(
    client: &Client,
    mut integration: Integration,
    timeout: Duration,
) -> Result<()> {
    let id = integration.id.clone();
    eprintln!(
        "Waiting up to {}s for integration `{id}` to connect…",
        timeout.as_secs()
    );
    let start = Instant::now();
    while integration.status == "pending" {
        if start.elapsed() >= timeout {
            print_json(&integration)?;
            bail!(
                "integration `{id}` is still pending after {} seconds; keep polling with \
                 `integration get {id}`",
                timeout.as_secs()
            );
        }
        std::thread::sleep(WAIT_INTERVAL);
        integration = get_integration(client, &id).with_context(|| {
            format!(
                "integration `{id}` was saved, but reading its status failed; \
                 follow it with `integration get {id}`"
            )
        })?;
    }
    print_json(&integration)?;
    if integration.status == "error" {
        bail!(
            "integration `{id}` failed to connect: {} ({})",
            integration
                .status_error_code
                .as_deref()
                .unwrap_or("unknown"),
            integration
                .status_error
                .as_deref()
                .unwrap_or("no reason given")
        );
    }
    Ok(())
}

/// Execute an `integration` subcommand.
pub fn run(
    command: IntegrationCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Input is read and checked — credentials included — before credentials
    // for the API are resolved.
    let (create_credentials, update_request) = match &command {
        IntegrationCommand::Create {
            channel,
            name,
            datasets,
            credentials,
            ..
        } => {
            check_lists(name, datasets)?;
            let Some(creds) = credentials.read()? else {
                bail!(
                    "the bot's credentials are needed\n\
                     pass them as a JSON object with --credentials-env <VAR>, --credentials-stdin, or --credentials-file <PATH>\n\
                     (there is no --credentials flag: a value there would be saved in shell history)"
                );
            };
            creds
                .check_complete_for(channel)
                .map_err(|err| anyhow::anyhow!("invalid credentials: {err}"))?;
            (Some(creds), None)
        }
        IntegrationCommand::Update {
            name,
            agent,
            read_write_project,
            read_only_projects,
            no_read_only_projects,
            datasets,
            no_datasets,
            daily_message_limit,
            card_template_id,
            enable,
            disable,
            credentials,
            wait,
            ..
        } => {
            check_lists(name.as_deref().unwrap_or_default(), datasets)?;
            let credentials = credentials.read()?;
            if let Some(creds) = &credentials {
                creds
                    .check_known()
                    .map_err(|err| anyhow::anyhow!("invalid credentials: {err}"))?;
            }
            let request = UpdateIntegrationRequest {
                name: name.clone(),
                agent_id: agent.clone(),
                read_write_project_id: read_write_project.clone(),
                read_only_project_ids: if *no_read_only_projects {
                    Some(Vec::new())
                } else {
                    (!read_only_projects.is_empty()).then(|| read_only_projects.clone())
                },
                dataset_ids: if *no_datasets {
                    Some(Vec::new())
                } else {
                    (!datasets.is_empty()).then(|| datasets.clone())
                },
                daily_message_limit: *daily_message_limit,
                settings: card_template_id.as_ref().map(|id| IntegrationSettings {
                    card_template_id: Some(id.clone()),
                    ..Default::default()
                }),
                enabled: match (enable, disable) {
                    (true, _) => Some(true),
                    (_, true) => Some(false),
                    _ => None,
                },
                credentials,
            };
            if request == UpdateIntegrationRequest::default() {
                bail!("nothing to update; pass at least one flag");
            }
            if wait.wait && request.credentials.is_none() && !*enable {
                bail!(
                    "--wait follows a reconnection, which only new credentials or --enable \
                     start; this update starts none"
                );
            }
            (None, Some(request))
        }
        _ => (None, None),
    };

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        IntegrationCommand::List {
            workspace,
            agent,
            page_size,
            continuation_token,
        } => {
            let data = list_integrations(
                &client,
                &ListIntegrationsParams {
                    page_size,
                    continuation_token,
                    workspace_id: workspace,
                    agent_id: agent,
                },
            )
            .context("list integrations")?;
            print_json(&data)
        }
        IntegrationCommand::Get { id } => print_json(
            &get_integration(&client, &id).with_context(|| format!("get integration `{id}`"))?,
        ),
        IntegrationCommand::Create {
            channel,
            workspace,
            name,
            agent,
            read_write_project,
            read_only_projects,
            datasets,
            daily_message_limit,
            card_template_id,
            idempotency_key,
            wait,
            ..
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let request = CreateIntegrationRequest {
                channel,
                workspace_id: workspace,
                name,
                agent_id: agent,
                read_write_project_id: read_write_project,
                read_only_project_ids: (!read_only_projects.is_empty())
                    .then_some(read_only_projects),
                dataset_ids: (!datasets.is_empty()).then_some(datasets),
                daily_message_limit,
                settings: card_template_id.map(|id| IntegrationSettings {
                    card_template_id: Some(id),
                    ..Default::default()
                }),
                credentials: create_credentials.expect("read above"),
            };
            let created = create_integration(&client, &request, idempotency_key.as_deref())
                .context("create integration")?;
            if wait.wait {
                wait_until_settled(&client, created, Duration::from_secs(wait.timeout))
            } else {
                print_json(&created)
            }
        }
        IntegrationCommand::Update {
            id,
            idempotency_key,
            wait,
            ..
        } => {
            let request = update_request.expect("built above");
            let updated = update_integration(&client, &id, &request, idempotency_key.as_deref())
                .with_context(|| format!("update integration `{id}`"))?;
            if wait.wait && updated.status == "pending" {
                wait_until_settled(&client, updated, Duration::from_secs(wait.timeout))
            } else {
                if wait.wait {
                    eprintln!(
                        "nothing to wait for: the update started no reconnection (status `{}`)",
                        updated.status
                    );
                }
                print_json(&updated)
            }
        }
        IntegrationCommand::Delete {
            id,
            idempotency_key,
        } => {
            delete_integration(&client, &id, idempotency_key.as_deref())
                .with_context(|| format!("delete integration `{id}`"))?;
            println!("Deleted integration `{id}`");
            Ok(())
        }
    }
}
