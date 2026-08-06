# memorylake-cli

[![CI](https://github.com/memorylake-ai/memorylake-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/memorylake-ai/memorylake-cli/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/memorylake-ai/memorylake-cli/branch/main/graph/badge.svg)](https://codecov.io/gh/memorylake-ai/memorylake-cli)

Command-line interface for MemoryLake.

## Crates

| Crate | Path | Description |
| --- | --- | --- |
| `memorylake-cli` | `crates/cli` | Binary (`memorylake`) |
| `memorylake-core` | `crates/core` | Shared library logic |

## Build

```bash
cargo build -p memorylake-cli
cargo run -p memorylake-cli -- --help
```

## Test

```bash
cargo test --workspace
```

Live API tests (workspaces, actors, projects, library) require `MEMORYLAKE_API_KEY`. Put secrets in a gitignored `.env` at the repo root:

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

CLI command coverage comes from `crates/cli/tests/` (`cli_commands` harness + `auth` / `workspace` / `actor` / `project` / `library` / `meta` suites; spawns the `memorylake` binary under a temp `$HOME`). Live CLI tests also need `MEMORYLAKE_API_KEY`.

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

## Lint

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT
