# memorylake-cli

[![CI](https://github.com/memorylake-ai/memorylake-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/memorylake-ai/memorylake-cli/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/memorylake-ai/memorylake-cli/branch/main/graph/badge.svg)](https://codecov.io/gh/memorylake-ai/memorylake-cli)

Command-line interface for MemoryLake.

## Crates

| Crate | Path | Description |
| --- | --- | --- |
| `memorylake-cli` | `crates/cli` | Binary (`memorylake`) |
| `memorylake-core` | `crates/core` | Shared library logic |

## Install

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.sh | sh
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.ps1 | iex
```

Both installers resolve the latest release, download the build for your
platform, **verify it against the published SHA-256**, and install the binary.
They are configurable through the environment:

| Variable | Default | Meaning |
| --- | --- | --- |
| `MEMORYLAKE_VERSION` | `latest` | Release tag to install, e.g. `v20260818` |
| `MEMORYLAKE_INSTALL_DIR` | `~/.local/bin` (Unix), `%LOCALAPPDATA%\memorylake\bin` (Windows) | Where the binary goes |
| `MEMORYLAKE_INSTALL_NAME` | `memorylake` | Name to install it as |

```bash
# pin a version, install somewhere else
MEMORYLAKE_VERSION=v20260818 MEMORYLAKE_INSTALL_DIR=/usr/local/bin \
  curl -fsSL https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.sh | sh
```

The Unix installer prints how to add the install directory to `PATH` if it is
not already there; the PowerShell one adds it to the user `PATH` for you. Both
refuse to install if the checksum does not match. Re-running either upgrades an
existing install in place.

Prefer to do it by hand? Grab a tarball (or `.zip` on Windows) from the
[releases page](https://github.com/memorylake-ai/memorylake-cli/releases),
verify it against its `.sha256`, and put `memorylake` on your `PATH`.

## Build

```bash
cargo build -p memorylake-cli
cargo run -p memorylake-cli -- --help
```

## Test

```bash
cargo test --workspace
```

Live API tests (workspaces, actors, projects, library, agents, documents, search, conversations) require `MEMORYLAKE_API_KEY`. Put secrets in a gitignored `.env` at the repo root:

```bash
cp .env.example .env
# edit .env — set MEMORYLAKE_API_KEY (and optional MEMORYLAKE_BASE_URL)
cargo test -p memorylake-core
```

Without a key, live tests fail. CI provides the key via the `MEMORYLAKE_API_KEY` GitHub secret.

Live tests create real objects. Each one works inside its own uniquely-named
scratch folder and deletes it when it finishes; a test that fails leaves its
folder behind on purpose so the state can be inspected.

## Coverage

```bash
# install once: cargo install cargo-llvm-cov --locked
cargo llvm-cov --workspace --lcov --output-path lcov.info
cargo llvm-cov --workspace --html --open
```

CLI command coverage comes from `crates/cli/tests/` (`cli_commands` harness + `actor` / `agent` / `auth` / `conversation` / `document` / `library` / `project` / `search` / `workspace` / `meta` suites; spawns the `memorylake` binary under a temp `$HOME`). Live CLI tests also need `MEMORYLAKE_API_KEY`. The agent live test runs a full create → version → bind → unbind → delete lifecycle and deletes the agent it created, including when an assertion fails partway. The document live tests upload their own scratch files, import them into a scratch project, and remove both. The conversation live test runs a full create → append → read → delete lifecycle against a scratch project and actor, and removes both from `Drop` so a mid-test failure still cleans up.

CI uploads `lcov.info` to [Codecov](https://codecov.io/gh/memorylake-ai/memorylake-cli) and as a workflow artifact.

## Auth & workspaces

```bash
# Interactive: choose login method (API key or OAuth), then follow prompts
memorylake auth login

# Non-interactive API key login
memorylake auth login --api-key sk-... [--profile default] [--base-url URL]

memorylake auth status
memorylake auth switch <profile>
memorylake auth refresh
memorylake auth logout

memorylake workspace list
memorylake ws create --name "My Workspace" --custom-id my-ws-001
memorylake ws get ws-1234 [--by-custom-id]
```

`auth login` without `--api-key` opens an interactive picker (`api_key` / `oauth`). OAuth is listed but not implemented yet. API-key login (flag or interactive) validates against the API before writing credentials. `auth status`, `auth switch`, and `auth refresh` also validate when credentials are present.

Config and credentials live under `~/.memorylake/` (`config.toml`, `credentials.toml`).

### Credential / base URL resolution

Profile selection: CLI `--profile` → `active_profile` → not logged in. Env vars alone are not a session.

| Setting | Precedence (highest first) |
| --- | --- |
| Base URL | CLI `--base-url` → profile `base_url` in `config.toml` → `MEMORYLAKE_BASE_URL` → built-in default |
| API key (`login_method = api_key`) | profile key in `credentials.toml` → `MEMORYLAKE_API_KEY` |

`auth status` prints `Base URL source` and `API key source` (`profile` / `env` / `cli` / `default`).

## Actors

Actors are MemoryLake's identity layer — the subject every memory is attributed to. An actor exists account-wide and must be bound to a workspace before it can participate there.

```bash
memorylake actor create --custom-id user-ext-001 --display-name "Alice Chen" \
  [--type HUMAN|ASSISTANT] [--description TEXT] [--metadata '{"tier":"premium"}']

memorylake actor list [--page-size 20] [--continuation-token TOKEN] \
  [--type HUMAN|ASSISTANT] [--name "Alice"]

memorylake actor get act-a1b2c3d4e5f6
memorylake actor get user-ext-001 --by-custom-id

memorylake actor update act-a1b2c3d4e5f6 --display-name "Alice Chen (VIP)"
memorylake actor delete act-a1b2c3d4e5f6

# Workspace bindings
memorylake actor bind   --workspace ws-... --actor act-...
memorylake actor unbind --workspace ws-... --actor act-...
memorylake actor list --workspace ws-...
```

- `--custom-id` is **unique account-wide**, not per workspace.
- `--type` only accepts the exact values `HUMAN` and `ASSISTANT`; a typo is rejected before any request. Types returned by the server that this build does not know are passed through unchanged.
- `--metadata` takes a JSON object and is validated locally. On `actor update` it **replaces** the stored metadata wholesale — the server does not merge, so include every key you want to keep.
- `actor update` changes only the fields you pass, and requires at least one of `--display-name`, `--description`, `--metadata`.
- `actor list --workspace <id>` returns workspace **bindings** (`actor_id`, `bound_at`, …), a different shape from the actor objects returned without the flag.
- `actor delete` is **irreversible** and runs without a confirmation prompt. Existing memories survive but can no longer be referenced, and all workspace bindings are removed. `actor unbind` is the reversible alternative: it drops one workspace membership and keeps the actor.
- `--by-custom-id` is only available on `actor get`.

## Projects

Projects are knowledge containers inside a workspace — they organize documents, conversations, and extracted facts. Every `project` (alias `proj`) subcommand takes an explicit `--workspace`; there is no default or remembered workspace.

```bash
memorylake project list --workspace ws-1234 [--page-size 50] \
  [--continuation-token TOKEN] [--name "partial name"]

memorylake proj create --workspace ws-1234 --name "My Project" \
  --custom-id my-proj-001 [--description TEXT]

memorylake proj get --workspace ws-1234 proj-5678
memorylake proj get --workspace ws-1234 my-proj-001 --by-custom-id

memorylake proj update --workspace ws-1234 proj-5678 --name "Renamed"
memorylake proj delete --workspace ws-1234 proj-5678
```

- `--custom-id` is unique **within the workspace**, unlike an actor's account-wide custom id.
- `proj update` sends only the flags you pass; omitted fields are left unchanged. Passing no updatable flag sends an empty update, which the server accepts as a no-op.
- `proj delete` is **irreversible** and runs without a confirmation prompt. The project's documents and conversations are removed with it.
- `--by-custom-id` is only available on `project get`; `update` and `delete` address projects by their server-assigned id.
- `metadata` and `industry_ids` are accepted by the API but not yet exposed on `create` / `update`.

## Library

The Library is MemoryLake's unified file system. Items are addressed by opaque
`item_id`; the alias `MY_SPACE` stands for the workspace root and is accepted
anywhere an id is.

```bash
memorylake library get MY_SPACE
memorylake lib list                              # defaults to MY_SPACE
memorylake lib list <item-id> --page-size 50 [--continuation-token TOKEN]

memorylake lib mkdir "Reports" [--parent <item-id>] [--on-conflict deny]
memorylake lib upload ./report.pdf [--parent <item-id>] [--name report.pdf]
memorylake lib delete <item-id>
```

`library` has the visible alias `lib`. All commands print the API payload as
pretty JSON.

Paging is manual: `list` returns a `continuation_token`, which you pass back to
fetch the next page.

### Uploads

`upload` runs the chunked-upload protocol: it opens a session for the file's
exact size, `PUT`s each part to the pre-signed URL the server issued for it, and
then finalizes the file. Part sizes are chosen by the server and vary with file
size, so nothing is assumed locally. Files are streamed part by part rather than
read into memory, and the file only becomes visible once finalization succeeds.

A part that fails on a transport error, HTTP 5xx, or HTTP 429 is retried a few
times with exponential backoff. Any other rejection — including the HTTP 403 an
expired pre-signed URL returns — fails immediately, because a session's
signatures cannot be refreshed. Re-run the upload in that case.

### Name conflicts

`--on-conflict` controls what happens when the target name is taken:

| Value | Behavior |
| --- | --- |
| `rename` | Append a `_N` suffix (server default) |
| `deny` | Fail with `409 DRIVE_ITEM_CONFLICT` |
| `overwrite` | Files only: replace content, keeping the same `item_id` |
| `replace` | Files only: delete and recreate, yielding a new `item_id` |

Folders accept only `rename` and `deny`; the server rejects the others. Under
`rename` the created item's name differs from the one you asked for, so prefer
the `name` in the command's output.

### Deletes are irreversible

`library delete` removes the item immediately, with no confirmation prompt.
Deleting a folder recursively removes everything inside it. The workspace root
is protected by the server (`403 ACCESS_DENIED`).

## Documents

Documents are Library files imported into a project and indexed there for
semantic search and memory extraction. Every `document` (alias `doc`) subcommand
sits under `project` and takes an explicit `--workspace` and `--project`.

```bash
memorylake project document import --workspace ws-1234 --project proj-5678 \
  <item-id>... [--recursive] [--max-files 500] [--wait] [--timeout 600]

memorylake proj doc list --workspace ws-1234 --project proj-5678 \
  [--page-size 50] [--continuation-token TOKEN] [--name "partial name"]

memorylake proj doc get    --workspace ws-1234 --project proj-5678 doc-3m4n5o6p
memorylake proj doc delete --workspace ws-1234 --project proj-5678 doc-3m4n5o6p ...
```

Files must already be in the Library — upload them with `lib upload` first.
Documents are addressed by their server-assigned `doc-...` id, which is **not**
the Library `item_id` they came from. There is no `--by-custom-id`.

### Importing folders

`import` takes file ids. A folder id fails and imports nothing unless
`--recursive` is set, which expands the folder into every file in its subtree.
File ids and folder ids can be mixed in one invocation, and a file reachable
through more than one argument is imported once.

`--max-files` (default 500) caps the expanded set; exceeding it fails before any
import request goes out. `MY_SPACE --recursive` names every file in the
workspace, so the cap is what stands between a typo and a bulk import that
cannot be undone.

### Importing is asynchronous

`import` returns as soon as the server accepts the batch. Each document then
moves through `pending` / `running` to `okay` or `error`; `list` and `get` show
the current `status`.

`--wait` polls until every imported document reaches a terminal status, backing
off from 1s to 15s between rounds and giving up after `--timeout` seconds
(default 600). **Giving up does not cancel the import** — it carries on
server-side, and `get` will show the outcome later.

### Partial failure and exit status

The import endpoint answers `200` even when individual files failed, so the CLI
decides success itself. It prints the full API payload to stdout first, then
exits **non-zero** when any of these hold:

| Condition | Meaning |
| --- | --- |
| `failure_count > 0` | one or more files could not be imported |
| `--wait` and a document ends in `error` | processing failed |
| `--wait` and the timeout elapses | some documents never finished |
| `--wait` and `details_truncated` is true | the server omitted per-file entries, so not every document could be polled |
| a folder id without `--recursive` | nothing was imported |
| more files than `--max-files` | nothing was imported |
| the ids expand to no files | nothing was imported |

Files already in the project come back as duplicates rather than failures and do
not affect the exit status.

On a large batch the server may set `details_truncated` and drop entries from
`details`. The counts stay accurate; the per-file list does not.

### Deletes are irreversible

`document delete` removes the documents immediately, with no confirmation prompt
and no `--yes` flag. The indexed content and every memory derived from them is
destroyed. The Library files they were imported from are untouched, so the same
files can be imported again afterwards.
## Facts

Facts are single remembered statements. Every fact is owned by exactly one
scope — an actor or a project — and `add` / `delete` name that scope with a
required, mutually exclusive `--actor` / `--project` pair.

```bash
memorylake fact add --workspace ws-1234 (--actor actor-… | --project proj-…) \
  "fact text" ["another fact" ...]

memorylake fact delete --workspace ws-1234 (--actor actor-… | --project proj-…) \
  <fact-id> [<fact-id> ...]

memorylake fact list --workspace ws-1234 [--actors a1,a2] [--projects p1,p2] \
  [--page-size 50] [--continuation-token TOKEN]
```

- Facts are stored **verbatim** and are searchable immediately; there is no
  asynchronous indexing step. Facts are immutable — to update one, add the
  new statement; the server resolves semantic conflicts between facts itself.
- The fact text lives under the wire key `fact` in every response, not
  `content` (that was the v2 name).
- `delete` sends **one request per id** (the API's `forget` endpoint) and
  prints per-id outcomes (`{"forgotten": [...], "not_found": [...]}`). The
  API's batch forget endpoint is deliberately not used: it is not atomic —
  given a mix of valid and invalid ids it deletes the valid ones and then
  fails the whole call. An id the server has already forgotten succeeds again
  (idempotent); only an id that never existed in the scope lands in
  `not_found`. Like `project document import`, the outcomes print first and
  the command then exits **non-zero** when `not_found` is non-empty, so
  scripts can trust the exit code.
- `list` requires **at least one** of `--actors` / `--projects`: with neither
  filter the API answers an empty page rather than every fact in the workspace,
  and relaying that as output would misread as "no facts exist". Both flags
  take one comma-separated value, like `search`. Listed facts carry an
  `owner: {type, id}` naming the scope they live in, and the payload's `total`
  is the exact cross-page count when the server provides it.
- Page size is capped at 50 by the server.

## Conversations

A conversation is an ordered log of messages that the server turns into memory
in the background. It belongs to a workspace and writes what it learns into
exactly one project, named at creation time.

```bash
memorylake conversation create --workspace ws-1234 --custom-id session-42 \
  --project proj-… --actors a1[,a2] [--name "Q3 Planning"] [--kind DIRECT|GROUP] \
  [--metadata key=value ...]

memorylake conversation list --workspace ws-1234 [--page-size N] [--continuation-token TOKEN]
memorylake conversation get --workspace ws-1234 <id> [--by-custom-id]
memorylake conversation cook-status --workspace ws-1234 <id> [--by-custom-id]
memorylake conversation delete --workspace ws-1234 <id>

memorylake conversation message append <conversation-id> --actor actor-… --custom-id msg-42 \
  (--text "hello" [--text ...] | --content-json '<blocks>' | --content-file blocks.json) \
  [--parent <message-id>] [--timestamp 2026-08-13T00:00:00Z] [--metadata key=value ...] \
  [--wait --workspace ws-1234 [--timeout 600]]

memorylake conversation message list <conversation-id> [--page-size N] [--continuation-token TOKEN]
```

- `conv` is an alias for `conversation`, `msg` for `message`.
- **The message subcommands take no `--workspace`.** The API addresses
  conversations under their workspace
  (`workspaces/{id}/memories/conversations/…`) but messages by conversation id
  alone (`conversations/{id}/messages`), and the CLI mirrors that split.
- `--project` is the conversation's read-write project — the scope it may read
  context from and write into — and the caller needs `project:mem_add` and
  `project:doc_add` on it. It bounds the conversation's access; it does not
  decide where extracted facts end up (see below). The API takes a
  `rw_project_ids` list but accepts exactly one entry today.
- `--actors` is **required**: the API rejects a conversation created without
  participants, so the CLI asks for them up front rather than letting the
  server turn it down. Pass several comma-separated for a `GROUP`.
- **There is no project-scoped conversation listing.** `list` takes no filter
  at all — not by project, not by actor — because the API offers none; filter
  the workspace listing client-side (each item carries `rw_project_ids`) if you
  need to.
- Message content is a list of typed blocks. Each `--text` becomes one `TEXT`
  block, in the order given; for the other five types (`FILE`, `IMAGE`,
  `THINKING`, `TOOL_USE`, `TOOL_RESULT`) pass the block array directly with
  `--content-json` / `--content-file`. The two ways are **mutually exclusive**
  — mixing them would leave block order up to an unwritten rule. Blocks are
  forwarded verbatim, so a block type this CLI predates still works; only a
  missing or non-string `block_type` is rejected locally.
- Appends to one conversation are **serialized server-side**: two at once leave
  one caller with a 409 Conflict. Recovering is safe because `--custom-id` is
  required and makes a retry idempotent — the same id returns the message
  created the first time instead of duplicating it. After a 409, re-read
  `message list` and retry with `--parent` set to the current last message.
  Omitting `--parent` means "append after the latest message".
- **`message append` does not echo everything it stored.** Its response leaves
  `metadata`, `timestamp` and `actor_type` null even when the request set them
  (measured 2026-08-13 against production). The values *are* stored —
  `message list` reports them — so confirm a write by reading the listing, not
  the append output.
- **Memory lags messages.** An appended message is stored immediately but is
  not searchable until the server has processed it. `cook-status` reports
  `cook_finished` for that.
- `message append --wait` does that polling for you, backing off from 1s to 15s
  between rounds and giving up after `--timeout` seconds (default 600). Three
  things worth knowing:
  - It **requires `--workspace`**, the one place a message subcommand does:
    `cook-status` is a workspace-scoped endpoint while the append itself is not.
  - It waits on the **whole conversation**, not just the message you appended —
    that is all the endpoint reports — so a concurrent writer can keep it
    unfinished.
  - **Giving up does not undo the append or stop the processing**; both carry on
    server-side. The message is printed before the wait begins and the command
    then exits non-zero, so a timeout never costs you the output. Bound the wait
    yourself: the API does not guarantee every conversation reaches a finished
    state.
- A conversation with no messages reports `cook_finished: true` — the flag means
  "nothing left in flight", not "memory has been built". Measured against
  production 2026-08-13, one message took ~9s to come back finished and three
  took ~19s.
- **Which scope an extracted fact lands in is the server's decision, not
  yours.** Nothing on `create` or `message append` selects it: the backend
  attributes each fact to an actor or a project based on what the fact is
  about. `--project` scopes what the conversation may read and write; it does
  not route facts. So after a wait returns, look in both places — a
  single-actor conversation of three first-person messages put all six facts
  under `fact list --actors <actor>` and none under
  `fact list --projects <project>` (measured 2026-08-13).
- `delete` removes the conversation **and every message in it**, immediately,
  with no confirmation prompt. Unlike `get` and `cook-status` it has no
  `--by-custom-id` lookup; resolve a `custom_id` with
  `conversation get --by-custom-id` first.

## Agents

```bash
memorylake agent list [--page-size N] [--continuation-token TOKEN] [--name FUZZY]
memorylake agent create --name "Support" --custom-id support-1 [--description D] \
                        [--model M] [--system-prompt P] [--config agent.json]
memorylake agent get <id> [--by-custom-id]
memorylake agent update <id> [--name NAME] [--description D] [--config identity.json]
memorylake agent delete <id>

memorylake agent version create <id> [--model M] [--system-prompt P] \
                                     [--config version.json] [--from-version latest|N]
memorylake agent version list <id> [--page-size N] [--continuation-token TOKEN]
memorylake agent version get <id> <version>

memorylake agent bind <agent-id> --workspace <workspace-id>
memorylake agent unbind <agent-id> --workspace <workspace-id>
memorylake agent bindings --workspace <workspace-id> [--page-size N] [--name FUZZY]
```

Creating an agent also generates an Actor identity for it, returned as `actor_id`.
An agent can only operate inside the workspaces it is bound to.

### Identity vs. configuration

The API splits agent changes in two, and the CLI follows it:

| Change | Command | Effect |
| --- | --- | --- |
| `name`, `description`, `metadata` | `agent update` | Updated in place; `metadata` is **replaced**, not merged |
| `model`, `capabilities`, `policies`, `output`, `subagents`, `skills`, `system_prompt`, `model_settings`, `runtime_bindings` | `agent version create` | Creates a new immutable version |

`agent update` rejects configuration fields up front — from flags or from
`--config` — and points you at `agent version create` rather than letting the
server return an opaque error.

### Supplying nested configuration

Scalar fields (`name`, `custom_id`, `description`, `model`, `system_prompt`)
have dedicated flags. Everything with structure — `metadata`, `capabilities`,
`policies`, `output`, `subagents`, `skills`, `model_settings`,
`runtime_bindings` — is supplied through `--config <FILE>`, a JSON object used
as the request body:

```bash
cat > agent.json <<'JSON'
{
  "name": "Support",
  "custom_id": "support-1",
  "model": "claude-sonnet-4-20250514",
  "system_prompt": "Answer support questions from memory.",
  "policies": { "max_turns": 8, "deny_tools": ["shell"] },
  "output": { "mode": "json", "json_schema": { "type": "object" } }
}
JSON

memorylake agent create --config agent.json            # everything from the file
memorylake agent create --config agent.json --model X  # flag overrides the file
```

Scalar flags override same-named keys in the file. Top-level keys this CLI does
not know about are forwarded to the API unchanged, so a newer server field works
without a CLI upgrade.

`agent version create --from-version latest|<N>` fetches that version and applies
your overrides on top, so bumping one setting does not mean re-sending the whole
configuration:

```bash
memorylake agent version create <id> --from-version latest --model claude-sonnet-4-20250514
```

Overrides replace whole top-level keys; nested objects are not deep-merged.
Without `--from-version`, only the values you supply are sent and version
inheritance is left to the server.

### Destructive commands

`agent delete` and `agent unbind` run immediately — there is no confirmation
prompt and no `--yes` flag. `agent delete` **cannot be undone**: it removes the
agent, every one of its versions, and all of its workspace bindings. `agent
unbind` removes only the binding; the agent definition survives.

## Search

Natural-language retrieval across one workspace. Search returns two independent result sets — matched `documents` and matched `facts` — rather than one ranked list.

```bash
memorylake search --workspace ws-1234 "what were the quarterly revenue figures"

memorylake search --workspace ws-1234 \
  --projects proj-1,proj-2 \
  --actors act-a1b2c3 \
  --types document,fact \
  --top-k 10 \
  "quarterly revenue"
```

- The filter flags each take **one comma-separated value** (`--projects a,b`), not a repeated flag. Surrounding spaces are trimmed, so `--projects "a, b"` works; an empty entry such as `a,,b` or a trailing comma is rejected before any request.
- `--types` accepts only `document` and `fact`, lowercase. Anything else is rejected locally with the accepted values listed.
- Omitting a filter searches everything in that dimension — the CLI sends no key at all rather than an empty list.
- `--top-k` caps results **per source type**. Left unset, the server picks its own default; the CLI does not impose one.
- There is no pagination: the endpoint has no continuation token.
- A search that matches nothing succeeds and prints empty collections.

Automated live tests can only prove that a search is accepted and decodes, because this CLI cannot ingest memories — a freshly created workspace has nothing to match. Verify relevance by searching a workspace that already holds content.

## Releases

Releases are tagged by date: `vYYYYMMDD` (for example `v20260810`). A patch
that must ship the same day appends a counter: `v20260810.1`, `v20260810.2`,
and so on. Pushing the tag triggers the release workflow, which builds macOS
(arm64, x86_64) and Linux (x86_64, arm64) binaries and attaches tarballs with
SHA-256 checksums to the GitHub Release. The crate version in `Cargo.toml`
stays semver and is independent of release tags.

## Lint

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT
