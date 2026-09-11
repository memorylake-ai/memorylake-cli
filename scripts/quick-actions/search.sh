#!/bin/zsh
# Quick action: search MemoryLake for the selected text and show the results.
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

body="$(printf '%s' "$raw" | json search "$UI_LANG")"
esc() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }
q="$(esc "${query:0:60}")"; b="$(esc "$body")"

# Activate System Events first, or the dialog opens behind the current window.
choice="$(osascript <<APPLESCRIPT
tell application "System Events" to activate
set r to display dialog "$b" with title "$MSG_SEARCH_TITLE$q" buttons {"$MSG_COPY", "$MSG_CLOSE"} default button "$MSG_CLOSE" giving up after 120
return button returned of r
APPLESCRIPT
)"
[ "$choice" = "$MSG_COPY" ] && printf '%s' "$body" | pbcopy
exit 0
