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

    # Compare against the resolved binary rather than `command -v` alone: a
    # different memorylake earlier in PATH would otherwise look like success.
    resolved="$(command -v "$INSTALL_NAME" 2>/dev/null || true)"
    if [ "$resolved" = "$INSTALL_DIR/$INSTALL_NAME" ]; then
        say ""
        say "run '$INSTALL_NAME auth login' to get started"
    elif [ -n "$resolved" ]; then
        say ""
        say "note: '$INSTALL_NAME' currently resolves to $resolved"
        say "      put $INSTALL_DIR earlier in PATH to use the version just installed"
    else
        say ""
        say "$INSTALL_DIR is not on your PATH; add it with:"
        say "  export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
}

main "$@"
