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

Live API tests (workspaces) require `MEMORYLAKE_API_KEY`. Put secrets in a gitignored `.env` at the repo root:

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

CLI command coverage comes from `crates/cli/tests/` (`cli_commands` harness + `auth` / `workspace` / `meta` suites; spawns the `memorylake` binary under a temp `$HOME`). Live CLI tests also need `MEMORYLAKE_API_KEY`.

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

## Lint

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT
