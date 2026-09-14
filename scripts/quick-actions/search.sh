#!/bin/zsh
# Quick action: search MemoryLake for the selected text and open the results as
# a plain-text document, so any line can be selected and copied.
set -u
. "${0:A:h}/config.sh"

query="$(printf '%s' "$*" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
[ -z "$query" ] && { notify "$MSG_TITLE" "$MSG_NO_SELECTION"; exit 0; }
snd Pop

# Without --projects the server searches only the actor's personal facts, not
# the projects, so pass every project in the workspace.
projects="$("$ML" project list --workspace "$WORKSPACE" 2>>"$LOG" | json lines items id | paste -sd, -)"
scope=(); [ -n "$projects" ] && scope=(--projects "$projects")

raw="$("$ML" search --workspace "$WORKSPACE" --top-k "$TOP_K" "${scope[@]}" "$query" 2>>"$LOG")"
if [ $? -ne 0 ] || [ -z "$raw" ]; then
  snd Basso; notify "$MSG_SEARCH_FAILED" "$MSG_SEE_LOG"; exit 1
fi
echo "=== $(date '+%F %T') search q=${query:0:60}" >> "$LOG"

# One file per search in the per-user temp dir, which macOS purges on reboot and
# after three idle days; anything older than an hour goes on the next search.
dir="$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || echo /tmp/)memorylake-quick-actions"
mkdir -p "$dir"
find "$dir" -name 'search-*.txt' -mmin +60 -delete 2>/dev/null
file="$dir/search-$(date +%Y%m%d-%H%M%S).txt"
{
  printf '%s%s\n%s\n\n' "$MSG_SEARCH_TITLE" "$query" "$(date '+%F %T')"
  printf '%s' "$raw" | json search "$UI_LANG"
  printf '\n'
} > "$file"

# TextEdit: real text, selectable, ⌘W to dismiss.
open -e "$file"
exit 0
