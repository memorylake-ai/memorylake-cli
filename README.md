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

Live API tests (workspaces, actors) require `MEMORYLAKE_API_KEY`. Put secrets in a gitignored `.env` at the repo root:

```bash
cp .env.example .env
# edit .env — set MEMORYLAKE_API_KEY (and optional MEMORYLAKE_BASE_URL)
cargo test -p memorylake-core
```

Without a key, live tests fail. CI provides the key via the `MEMORYLAKE_API_KEY` GitHub secret.

## Coverage

```bash
# install once: cargo install cargo-llvm-cov --locked
cargo llvm-cov --workspace --lcov --output-path lcov.info
cargo llvm-cov --workspace --html --open
```

CLI command coverage comes from `crates/cli/tests/` (`cli_commands` harness + `auth` / `workspace` / `actor` / `meta` suites; spawns the `memorylake` binary under a temp `$HOME`). Live CLI tests also need `MEMORYLAKE_API_KEY`.

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

## Lint

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT
