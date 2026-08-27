# memorylake-cli

[![CI](https://github.com/memorylake-ai/memorylake-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/memorylake-ai/memorylake-cli/actions/workflows/ci.yml)

Command-line interface for [MemoryLake](https://app.memorylake.ai). Upload files,
store and search memories, and manage the workspaces, projects, actors and agents
they belong to.

Every command prints the API response as pretty JSON, so anything here pipes into
`jq`.

## Install

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.sh | sh
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.ps1 | iex
```

Both verify the download against its published SHA-256 and refuse to install on a
mismatch. Re-running upgrades in place. On a first install they walk you through
logging in and picking a workspace; `MEMORYLAKE_VERSION`,
`MEMORYLAKE_INSTALL_DIR`, `MEMORYLAKE_INSTALL_NAME` and `MEMORYLAKE_NO_SETUP`
override the details — see the comments at the top of either script.

### Installing without prompts

Supply the credentials and nothing is asked — useful for a link handed out by a
console, and for CI, where there is no one to prompt:

```bash
curl -fsSL https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.sh \
  | sh -s -- --api-key sk-… --workspace ws-… [--base-url URL]
```

```powershell
# `irm | iex` cannot pass parameters, so Windows uses the environment
$env:MEMORYLAKE_API_KEY='sk-…'; $env:MEMORYLAKE_WORKSPACE='ws-…'
irm https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.ps1 | iex
```

Every flag has an environment variable (`MEMORYLAKE_API_KEY`,
`MEMORYLAKE_WORKSPACE`, `MEMORYLAKE_BASE_URL`); a flag wins over its variable.
**Supplied credentials replace whatever is already stored** — that is the point
when a console hands out a key. The key is validated before anything is written,
so one that will not work leaves your existing configuration untouched.

A key on the command line is recorded in your shell's history. Prefer a
short-lived key where that matters.

Prefer not to pipe a script into your shell? Download an archive from the
[releases page](https://github.com/memorylake-ai/memorylake-cli/releases), check
it against its `.sha256`, and put `memorylake` on your `PATH`.

### Upgrading

Re-run the same install command. It replaces the binary in place and leaves your
credentials and workspace alone — an install that is already logged in is not
asked to set anything up again.

```bash
memorylake version    # v20260818.1 — which release this is
```

A build that was not produced by the release workflow says so (`0.1.0 (dev
build)`), so it cannot be mistaken for one.

## Getting started

```bash
memorylake auth login          # pick an endpoint, then paste your API key
memorylake workspace use       # pick a default workspace from a list

memorylake project create --name "Research" --custom-id research-1
memorylake lib upload ./report.pdf
memorylake project document import --project proj-… <item-id> --wait

memorylake search "what were the quarterly revenue figures"
```

`workspace use` remembers a workspace, so `--workspace` can be omitted everywhere
after it. Pass it explicitly to override for a single command.

## Commands

Aliases: `ws` = `workspace`, `proj` = `project`, `lib` = `library`,
`doc` = `document`, `conv` = `conversation`, `msg` = `message`,
`key` = `api-key`, `invite` = `invitation`.

### Auth

```bash
memorylake auth login                          # interactive: endpoint, then API key
memorylake auth login --api-key sk-… [--base-url URL] [--profile NAME]
memorylake auth status                         # who am I, and where did it come from
memorylake auth switch <profile>
memorylake auth refresh
memorylake auth logout
```

Interactive login offers the **Global** (`app.memorylake.ai`) and **China**
(`app.memorylake.cn`) endpoints, or a URL you type. They are separate
deployments: an account on one does not exist on the other. Choose one
non-interactively with `--base-url`.

### Workspaces

```bash
memorylake ws list [--name FUZZY] [--page-size N] [--continuation-token TOKEN]
memorylake ws create --name "My Workspace" --custom-id my-ws-001
memorylake ws get <id> [--by-custom-id]

memorylake ws use              # pick a default from a list
memorylake ws use <id>         # or name one
memorylake ws current          # which one is in effect, and why
memorylake ws use --clear
```

### Actors

An actor is who a memory is attributed to. Actors exist account-wide and must be
bound to a workspace to participate there.

```bash
memorylake actor create --custom-id user-001 --display-name "Alice Chen" \
  [--type HUMAN|ASSISTANT] [--description TEXT] [--tags vip,cn] \
  [--metadata '{"tier":"premium"}']

memorylake actor list [--type HUMAN|ASSISTANT] [--name FUZZY] [--tags vip,cn] [--page-size N]
memorylake actor get <id> [--by-custom-id]
memorylake actor update <id> [--display-name NAME] [--description D] \
  [--tags vip,cn | --clear-tags] [--metadata JSON]
memorylake actor delete <id>

memorylake actor bind   --actor <id> [--workspace <id>]
memorylake actor unbind --actor <id> [--workspace <id>]
memorylake actor list   --workspace <id>        # bindings, not actors
```

`--custom-id` is unique account-wide. On update, `--metadata` **replaces** the
stored value rather than merging into it.

Tags are short labels for grouping and filtering: up to 20 per actor, each 1-64
characters, no commas. Matching is exact and case-sensitive — `VIP` and `vip` are
two different tags. Prefer them over `--metadata` for anything you want to filter
on, because metadata is not filterable.

```bash
memorylake actor list --tags vip          # actors tagged vip
memorylake actor list --tags vip,cn       # tagged BOTH vip and cn, not either
```

`--tags` on update replaces the whole list, like `--metadata`; `--clear-tags`
removes every tag. Leaving both out keeps the actor's tags as they are.

### Projects

A project holds documents, conversations and the facts extracted from them.

```bash
memorylake proj list [--name FUZZY] [--page-size N] [--continuation-token TOKEN]
memorylake proj create --name "My Project" --custom-id my-proj-001 [--description D]
memorylake proj get <id> [--by-custom-id]
memorylake proj update <id> [--name NAME] [--description D]
memorylake proj delete <id>
```

`--custom-id` is unique within the workspace.

### Library

The Library is MemoryLake's file system. `MY_SPACE` is the workspace root, and is
accepted anywhere an item id is.

```bash
memorylake lib list [<item-id>] [--page-size N] [--continuation-token TOKEN]
memorylake lib get <item-id>
memorylake lib mkdir "Reports" [--parent <item-id>] [--on-conflict rename|deny]
memorylake lib upload ./report.pdf [--parent <item-id>] [--name NAME] \
  [--on-conflict rename|deny|overwrite|replace]
memorylake lib delete <item-id>
```

`upload` streams the file in parts and retries transient failures; the file
appears only once it completes. `--on-conflict` decides what happens when the
name is taken — the default `rename` appends `_N`, so read the `name` in the
output rather than assuming the one you asked for. `overwrite` and `replace`
apply to files only.

### Documents

Documents are Library files imported into a project and indexed for search.

```bash
memorylake proj doc import --project <id> <item-id>... \
  [--recursive] [--max-files 500] [--wait] [--timeout 600]

memorylake proj doc list   --project <id> [--name FUZZY] [--page-size N]
memorylake proj doc get      --project <id> <doc-id>
memorylake proj doc download --project <id> <doc-id> [-o PATH] [--force]
memorylake proj doc delete   --project <id> <doc-id>...
```

Upload files with `lib upload` first. A folder id needs `--recursive`, and
`--max-files` caps how many files one command may import.

Importing is asynchronous: the command returns once the server accepts the batch,
and each document moves through `pending` / `running` to `okay` or `error`.
`--wait` polls until they settle. Giving up does not cancel anything — the import
carries on server-side.

The API answers `200` even when individual files fail, so the CLI prints the full
payload and then exits non-zero if anything went wrong. Files already in the
project count as duplicates, not failures.

`download` writes the original file under the name the server reports, in the
current directory. `-o` takes a file path or a directory, and `-o -` streams to
stdout for piping. An existing file is never replaced without `--force`.

### Facts

A fact is one remembered statement, owned by exactly one actor or project.

```bash
memorylake fact add (--actor <id> | --project <id>) "fact text" ["another" ...]
memorylake fact list (--actors a,b | --projects a,b) [--page-size N]
memorylake fact delete (--actor <id> | --project <id>) <fact-id>...
```

Facts are stored verbatim and are searchable immediately. They are immutable — to
change one, add the new statement and let the server resolve the conflict.
`fact list` needs at least one of `--actors` / `--projects`.

### Conversations

A conversation is a log of messages that the server turns into memory in the
background.

```bash
memorylake conv create --custom-id session-42 --project <id> --actors a1[,a2] \
  [--name "Q3 Planning"] [--kind DIRECT|GROUP] [--metadata k=v ...]

memorylake conv list [--page-size N] [--continuation-token TOKEN]
memorylake conv get <id> [--by-custom-id]
memorylake conv cook-status <id> [--by-custom-id]
memorylake conv delete <id>

memorylake conv msg append <conv-id> --actor <id> --custom-id msg-42 \
  (--text "hello" ... | --content-json '<blocks>' | --content-file blocks.json) \
  [--parent <msg-id>] [--timestamp ISO8601] [--metadata k=v ...] [--wait [--timeout 600]]

memorylake conv msg list <conv-id> [--page-size N] [--continuation-token TOKEN]
```

Message content is a list of typed blocks. Each `--text` becomes one `TEXT`
block; use `--content-json` / `--content-file` for `FILE`, `IMAGE`, `THINKING`,
`TOOL_USE` and `TOOL_RESULT`.

Every message names the one it follows. Without `--parent` the command looks the
conversation's latest message up for you, which is why it needs a workspace then
— pass `--parent <id>` to skip the lookup.

Appends to one conversation are serialized, so two at once leave one caller with
a `409`. Retrying is safe because `--custom-id` makes it idempotent — the same id
returns the message created the first time. After a `409`, re-read `msg list` and
retry with `--parent` set to the current last message.

Memory lags messages: an appended message is stored immediately but is not
searchable until the server has processed it. `cook-status` reports when that is
done, and `--wait` polls for you. Facts drawn from a conversation are attributed
by the server to an actor or a project as it sees fit, so look under both
`fact list --actors` and `fact list --projects`.

### Agents

```bash
memorylake agent list [--name FUZZY] [--page-size N]
memorylake agent create --name "Support" --custom-id support-1 \
  [--model M] [--system-prompt P] [--description D] [--config agent.json]
memorylake agent get <id> [--by-custom-id]
memorylake agent update <id> [--name NAME] [--description D] [--config identity.json]
memorylake agent delete <id>

memorylake agent version create <id> [--model M] [--system-prompt P] \
  [--config version.json] [--from-version latest|N]
memorylake agent version list <id>
memorylake agent version get <id> <version>

memorylake agent bind   <id> --workspace <id>
memorylake agent unbind <id> --workspace <id>
memorylake agent bindings [--workspace <id>] [--name FUZZY]
```

Creating an agent also creates an actor identity for it, returned as `actor_id`.
An agent works only in the workspaces it is bound to.

Changes split in two. `agent update` changes identity — `name`, `description`,
`metadata` — in place. Anything about behaviour (`model`, `system_prompt`,
`policies`, `capabilities`, `output`, `subagents`, `skills`, …) creates a new
immutable version through `agent version create`; passing one of those to
`update` is rejected up front.

Structured fields come from a JSON file:

```bash
cat > agent.json <<'JSON'
{
  "name": "Support",
  "custom_id": "support-1",
  "model": "claude-sonnet-4-20250514",
  "policies": { "max_turns": 8, "deny_tools": ["shell"] }
}
JSON

memorylake agent create --config agent.json
memorylake agent create --config agent.json --model X   # flags win over the file
```

Unknown top-level keys are forwarded to the API unchanged, so a newer server
field works without upgrading the CLI. `--from-version latest|N` starts from an
existing version and applies your overrides on top, replacing whole top-level
keys rather than deep-merging.

### Search

```bash
memorylake search "what were the quarterly revenue figures"

memorylake search "quarterly revenue" \
  --projects proj-1,proj-2 --actors act-1 --types document,fact --top-k 10
```

Returns matched `documents` and `facts` as two separate sets rather than one
ranked list. Filters take one comma-separated value each (`--projects a,b`), and
omitting a filter searches everything in that dimension. `--top-k` caps results
per type. There is no pagination.

### Team management

The team your API key belongs to — its API keys, members, invitations and
usage — is managed with the same key and endpoint as everything above. The team
is fixed by the key: nothing here takes a team parameter, and each command is
authorized by what the key's creator may do in the console.

```bash
memorylake team get
memorylake team rename --name "New Name"        # owner only

memorylake key list [--name FUZZY] [--page-size N] [--continuation-token TOKEN]
memorylake key get <id>
memorylake key create --name ci [--member <principal-id>] [--expires-at UNIX_SECONDS]
memorylake key rotate <id>
memorylake key revoke <id>

memorylake member list [--name FUZZY] [--page-size N]
memorylake member create --name "CI Bot" --role tenant_member   # virtual member
memorylake member set-role <principal-id> --role tenant_admin
memorylake member remove <principal-id>

memorylake invite create --email person@example.com --role tenant_member
memorylake invite list [--status pending|accepted|rejected|expired|revoked]
memorylake invite revoke <id>

memorylake usage [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
```

`key create` and `key rotate` print the full key **exactly once** — list and get
only ever return its prefix, and an idempotent replay omits it too, so capture
it from the first response.

A *virtual member* is a login-less identity for automations: create one with
`member create`, then issue it a key with `key create --member <principal-id>`.
That key acts with the virtual member's role instead of yours, so a CI job can
hold exactly the permissions it needs.

Every write takes `--idempotency-key VALUE`. Retrying with the same value
replays the first result instead of repeating the write — no duplicate key,
member, or invitation email.

## Configuration

Credentials and settings live in `~/.memorylake/` (`credentials.toml`,
`config.toml`), or in `MEMORYLAKE_CONFIG_DIR` if that is set.

Profiles keep several accounts or endpoints side by side: `--profile` selects one
for a single command, `auth switch` changes the default.

| Setting | Resolution order |
| --- | --- |
| API key | profile in `credentials.toml` → `MEMORYLAKE_API_KEY` |
| Base URL | `--base-url` → profile → `MEMORYLAKE_BASE_URL` → `app.memorylake.ai` |
| Workspace | `--workspace` → `ws use` → `MEMORYLAKE_WORKSPACE` |
| Config location | `MEMORYLAKE_CONFIG_DIR` → `~/.memorylake` |

`auth status` and `ws current` both report which source won. There is no built-in
default workspace: with none remembered and none passed, a command that needs one
fails and says how to supply it.

## Things to know

- **Deletes are immediate and irreversible.** No confirmation prompt and no
  `--yes` flag anywhere. Deleting a project takes its documents and conversations
  with it; deleting a Library folder takes everything inside it.
- **Paging is manual.** List commands return a `continuation_token`; pass it back
  to fetch the next page.
- **Exit codes are meaningful.** Commands that can partially fail — document
  import, fact delete — print the full result first and then exit non-zero, so
  scripts can trust the status without parsing output.
- `-v` / `-vv` raise log verbosity, and `RUST_LOG` is honoured.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for building, testing and releasing.

## License

MIT
