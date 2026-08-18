# Contributing

## Layout

| Crate | Path | Description |
| --- | --- | --- |
| `memorylake-cli` | `crates/cli` | Binary (`memorylake`) |
| `memorylake-core` | `crates/core` | API bindings and shared logic |

## Build

```bash
cargo build -p memorylake-cli
cargo run -p memorylake-cli -- --help
```

MSRV is 1.96, edition 2024.

## Test

```bash
cargo test --workspace
```

Tests come in three layers:

- **Unit** — request bodies, URL paths, lenient decoding, and local rules such
  as the `--wait` backoff schedule (tested against a fake clock, not real
  seconds).
- **Wire** — spawn the `memorylake` binary against a loopback HTTP stub and pin
  the exact request line, body, and request *count* per endpoint.
- **Live** — run against the real API. These need `MEMORYLAKE_API_KEY`:

  ```bash
  cp .env.example .env    # then set MEMORYLAKE_API_KEY
  cargo test --workspace
  ```

  Without a key they fail rather than skip, so a missing key cannot look like a
  pass. To run everything else: `cargo test -p memorylake-core --lib` and
  `cargo test -p memorylake-cli -- --skip live`.

Live tests create real objects and clean up after themselves, several from
`Drop` so a mid-test panic still tidies up. Scratch workspaces are the exception
— the CLI has no `workspace delete` — so they accumulate; see issue #15.

CLI tests isolate state through `MEMORYLAKE_CONFIG_DIR`. Redirecting `HOME` is
not enough: on Windows `dirs::home_dir()` calls
`SHGetKnownFolderPath(FOLDERID_Profile)`, which ignores both `HOME` and
`USERPROFILE`, so a test that only sets those reads the real user's config.

Fork-based pull requests cannot pass `test` / `coverage`: GitHub does not expose
secrets to `pull_request` runs from a fork, so `MEMORYLAKE_API_KEY` is empty and
the live tests fail. Branch from this repository instead.

## Coverage

```bash
# install once: cargo install cargo-llvm-cov --locked
cargo llvm-cov --workspace --lcov --output-path lcov.info
cargo llvm-cov --workspace --html --open
```

CI uploads `lcov.info` to [Codecov](https://codecov.io/gh/memorylake-ai/memorylake-cli)
and as a workflow artifact.

## Lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Both are enforced in CI, warnings included.

## Releases

Releases are tagged by date: `vYYYYMMDD` (for example `v20260818`). A patch that
must ship the same day appends a counter: `v20260818.1`, `v20260818.2`. The
crate version in `Cargo.toml` stays semver and is independent of release tags.

Pushing a `v*` tag triggers the release workflow, which builds macOS
(arm64, x86_64), Linux (x86_64, arm64) and Windows (x86_64, arm64) binaries and
attaches them with SHA-256 checksums to the GitHub Release. Unix ships
`.tar.gz`, Windows `.zip`.

The installers in `scripts/` download whatever `latest` resolves to, so a
release that adds a command the scripts guide people through should be tagged
promptly after merging.
