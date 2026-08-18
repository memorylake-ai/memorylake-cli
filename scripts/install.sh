#!/bin/sh
# Install the memorylake CLI on macOS or Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install.sh | sh
#
# Environment overrides:
#   MEMORYLAKE_VERSION       release tag to install (default: latest)
#   MEMORYLAKE_INSTALL_DIR   where to put the binary (default: $HOME/.local/bin)
#   MEMORYLAKE_INSTALL_NAME  name to install it as (default: memorylake)
#
# POSIX sh on purpose: this runs before anything is installed, so it may not
# assume bash. `set -eu` without pipefail — pipefail is not POSIX, so every
# pipeline that matters is checked through its own exit status instead.

set -eu

REPO="memorylake-ai/memorylake-cli"
BIN_NAME="memorylake"
VERSION="${MEMORYLAKE_VERSION:-latest}"
INSTALL_DIR="${MEMORYLAKE_INSTALL_DIR:-$HOME/.local/bin}"
INSTALL_NAME="${MEMORYLAKE_INSTALL_NAME:-$BIN_NAME}"

say() {
    printf '%s\n' "$*"
}

err() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1
}

# Map `uname` output onto the Rust target triple the release is built for.
#
# The release names its assets by target triple rather than by a friendlier
# os-arch pair, so this resolves straight to the triple instead of composing
# one from two lookups.
detect_target() {
    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Darwin) ;;
        Linux) ;;
        MINGW* | MSYS* | CYGWIN*)
            err "detected Windows ($_os); use the PowerShell installer instead:
  irm https://raw.githubusercontent.com/$REPO/main/scripts/install.ps1 | iex"
            ;;
        *) err "unsupported operating system: $_os" ;;
    esac

    case "$_os-$_arch" in
        Darwin-arm64) echo "aarch64-apple-darwin" ;;
        Darwin-x86_64) echo "x86_64-apple-darwin" ;;
        Linux-x86_64 | Linux-amd64) echo "x86_64-unknown-linux-gnu" ;;
        Linux-aarch64 | Linux-arm64) echo "aarch64-unknown-linux-gnu" ;;
        *) err "unsupported platform: $_os $_arch" ;;
    esac
}

# Resolve `latest` to a concrete tag.
#
# Reads the redirect GitHub serves from /releases/latest rather than calling the
# JSON API, which is rate-limited far more aggressively for unauthenticated
# callers — a plain `curl | sh` has no token to offer.
resolve_version() {
    [ "$VERSION" = "latest" ] || return 0

    if need_cmd curl; then
        VERSION="$(curl -fsSI "https://github.com/$REPO/releases/latest" 2>/dev/null |
            tr -d '\r' | awk 'tolower($1) == "location:" { sub(".*/tag/", "", $2); print $2 }' | tail -1)"
    else
        VERSION="$(wget --spider --max-redirect=0 "https://github.com/$REPO/releases/latest" 2>&1 |
            tr -d '\r' | awk 'tolower($1) == "location:" { sub(".*/tag/", "", $2); print $2 }' | tail -1)"
    fi

    [ -n "$VERSION" ] ||
        err "could not resolve the latest release; set MEMORYLAKE_VERSION to a tag such as v20260818"
}

# Every function prefixes its variables with `_`. POSIX sh has no local scope,
# so a bare `archive=` inside a function would clobber the caller's — which is
# exactly the kind of bug that only shows up at runtime.
download() {
    _url="$1"
    _dest="$2"
    if need_cmd curl; then
        curl -fsSL "$_url" -o "$_dest"
    else
        wget -q "$_url" -O "$_dest"
    fi
}

# Verify the tarball against the .sha256 published beside it.
#
# Skipped only when the machine has neither checksum tool, and loudly — a silent
# skip would make an unverified install look identical to a verified one.
verify_checksum() {
    _archive="$1"
    # The .sha256 file names the archive as it was built, so verification runs
    # from the directory holding both rather than rewriting the file.
    _dir="$(dirname "$_archive")"
    _sums="$(basename "$2")"

    # Each branch spells out its own command instead of storing one in a
    # variable: an unquoted `$cmd` relies on word splitting, which zsh does not
    # do by default, so the check would silently break for anyone running this
    # under `zsh install.sh` rather than through `sh`.
    if need_cmd shasum; then
        (cd "$_dir" && shasum -a 256 -c "$_sums" >/dev/null 2>&1) ||
            err "checksum verification failed for $(basename "$_archive"); refusing to install"
    elif need_cmd sha256sum; then
        (cd "$_dir" && sha256sum -c "$_sums" >/dev/null 2>&1) ||
            err "checksum verification failed for $(basename "$_archive"); refusing to install"
    else
        say "warning: neither shasum nor sha256sum found; skipping checksum verification"
    fi
}

# Set by `setup` when the install already has usable credentials, so the closing
# message does not tell someone to log in seconds after they just did.
SETUP_DONE=0

# Walk a fresh install through logging in and picking a workspace.
#
# Piped through `sh`, stdin is the script itself, so the CLI's prompts would
# read from it and see EOF. Everything interactive is therefore redirected from
# /dev/tty, and when there is no terminal to read from — CI, a Dockerfile — the
# setup is skipped with the commands printed instead of hanging or half-running.
#
# Skipped entirely when MEMORYLAKE_NO_SETUP is set, for anyone scripting the
# install itself.
setup() {
    _bin="$1"

    [ -z "${MEMORYLAKE_NO_SETUP:-}" ] || return 0

    # Already logged in? Then this is an upgrade, not a first install.
    #
    # Checked before the terminal test, because it needs no terminal: an
    # upgrade run from CI or a script would otherwise be told that setup was
    # skipped and to go log in, when it is already configured.
    #
    # Read the reported state rather than the exit status: `auth status`
    # succeeds either way, because answering "not logged in" is a successful
    # query. A CLI test pins this output so the check cannot rot silently.
    if "$_bin" auth status 2>/dev/null | grep -q 'Logged in: yes'; then
        say ""
        say "already logged in; leaving your credentials and workspace as they are"
        SETUP_DONE=1
        return 0
    fi

    # Actually open /dev/tty rather than testing it with `-r`/`-w`: the device
    # node can exist and pass those tests while opening it fails with "Device
    # not configured", which is what happens with no controlling terminal — cron,
    # some containers, a detached session. Done in a subshell so the descriptor
    # does not leak into this one.
    if ! (exec 3<>/dev/tty) 2>/dev/null; then
        say ""
        say "no terminal available, so setup was skipped. To finish:"
        say "  $INSTALL_NAME auth login       # store your API key"
        say "  $INSTALL_NAME workspace use    # pick a default workspace"
        return 0
    fi

    say ""
    say "Let's get you set up. Ctrl-C to skip — you can run these later."
    say ""

    # `auth login` prompts for the key and validates it against the API before
    # storing anything, so a typo is caught here rather than on first use.
    if ! "$_bin" auth login </dev/tty >/dev/tty 2>&1; then
        say ""
        say "login did not complete. Run '$INSTALL_NAME auth login' when ready."
        return 0
    fi

    # `workspace use` with no argument lists the account's workspaces and lets
    # the user choose. Nobody has a workspace id memorised on day one, so the
    # setup must never ask them to type one.
    say ""
    if ! "$_bin" workspace use </dev/tty >/dev/tty 2>&1; then
        say ""
        say "no workspace selected. Run '$INSTALL_NAME workspace use' to pick one,"
        say "or pass --workspace <id> to each command."
    fi

    # Logged in either way: a skipped workspace is a preference, not a failure,
    # and the line above already says how to set one.
    SETUP_DONE=1
}

main() {
    need_cmd curl || need_cmd wget || err "need curl or wget to download the release"
    need_cmd tar || err "need tar to unpack the release"

    target="$(detect_target)"
    resolve_version

    stem="$BIN_NAME-$VERSION-$target"
    archive="$stem.tar.gz"
    base_url="https://github.com/$REPO/releases/download/$VERSION"

    say "installing $BIN_NAME $VERSION ($target)"

    tmp="$(mktemp -d)"
    # Runs on error and on success, so a failed download leaves nothing behind.
    trap 'rm -rf "$tmp"' EXIT INT TERM

    download "$base_url/$archive" "$tmp/$archive" ||
        err "could not download $archive
  check that $VERSION exists and publishes a build for $target:
  https://github.com/$REPO/releases"
    download "$base_url/$archive.sha256" "$tmp/$archive.sha256" ||
        err "could not download the checksum for $archive"

    verify_checksum "$tmp/$archive" "$tmp/$archive.sha256"
    tar -xzf "$tmp/$archive" -C "$tmp"

    unpacked="$tmp/$stem/$BIN_NAME"
    [ -f "$unpacked" ] || err "the archive did not contain $BIN_NAME as expected"

    mkdir -p "$INSTALL_DIR"
    # Install through a temporary name in the destination directory, then
    # rename: `cp` onto a running binary fails with ETXTBSY, while a rename is
    # atomic and leaves any running process on the old inode.
    staged="$INSTALL_DIR/.$INSTALL_NAME.$$"
    cp "$unpacked" "$staged"
    chmod +x "$staged"
    mv -f "$staged" "$INSTALL_DIR/$INSTALL_NAME"

    say "installed $INSTALL_DIR/$INSTALL_NAME"

    setup "$INSTALL_DIR/$INSTALL_NAME"

    # Compare against the resolved binary rather than `command -v` alone: a
    # different memorylake earlier in PATH would otherwise look like success.
    resolved="$(command -v "$INSTALL_NAME" 2>/dev/null || true)"
    if [ -z "$resolved" ]; then
        say ""
        say "$INSTALL_DIR is not on your PATH; add it with:"
        say "  export PATH=\"$INSTALL_DIR:\$PATH\""
    elif [ "$resolved" != "$INSTALL_DIR/$INSTALL_NAME" ]; then
        say ""
        say "note: '$INSTALL_NAME' currently resolves to $resolved"
        say "      put $INSTALL_DIR earlier in PATH to use the version just installed"
    fi

    # Kept separate from the PATH notes above: whether the binary is reachable
    # and whether it is configured are different questions, and telling someone
    # to log in seconds after they did reads like the setup failed.
    say ""
    if [ "$SETUP_DONE" = "1" ]; then
        say "you're set. Try '$INSTALL_NAME workspace current' or '$INSTALL_NAME project list'."
    else
        say "run '$INSTALL_NAME auth login' to get started"
    fi
}

main "$@"
