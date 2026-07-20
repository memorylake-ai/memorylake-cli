# memorylake-cli

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

Live API tests (workspaces) run when `MEMORYLAKE_API_KEY` is set. Put secrets in a gitignored `.env` at the repo root:

```bash
cp .env.example .env
# edit .env — set MEMORYLAKE_API_KEY (and optional MEMORYLAKE_BASE_URL)
cargo test -p memorylake-core
```

Without a key, live tests skip so CI still passes.

## Auth & workspaces

```bash
memorylake auth login api_key --api-key sk_... [--profile default] [--base-url URL]
memorylake auth status
memorylake auth switch <profile>
memorylake auth refresh
memorylake auth logout

memorylake workspace list
memorylake ws create --name "My Workspace" --custom-id my-ws-001
```

Config and credentials live under `~/.memorylake/` (`config.toml`, `credentials.toml`).

## Lint

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT
