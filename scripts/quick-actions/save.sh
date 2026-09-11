#!/bin/zsh
# Quick action: append the selected text to the MemoryLake clips conversation.
# The server turns the conversation into memory in the background.
# Automator passes the selection as arguments ("as arguments" input method).
set -u
. "${0:A:h}/config.sh"

text="$(printf '%s' "$*" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
[ -z "$text" ] && { notify "$MSG_TITLE" "$MSG_NO_SELECTION"; exit 0; }
snd Pop

app="$(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' 2>/dev/null)"

# Same text within the same hour → same custom-id → the server returns the
# existing message instead of storing a duplicate. Double-presses are harmless.
msg_id="clip-$(date +%Y%m%d%H)-$(printf '%s' "$text" | shasum | cut -c1-16)"

preview="${text:0:40}"; [ ${#text} -gt 40 ] && preview="${preview}…"

append() {
  "$ML" conv msg append "$CONVERSATION" \
    --actor "$ACTOR" --custom-id "$msg_id" --text "$text" \
    --metadata source=macos-quick-action --metadata app="${app:-unknown}" "$@"
}

{
  echo "=== $(date '+%F %T') save app=${app:-unknown} id=$msg_id"
  append || {
    # Concurrent appends 409; re-read the tail and retry once with --parent.
    echo "--- retry"
    parent="$("$ML" conv get "$CONVERSATION" 2>/dev/null | json get current_message_id)"
    if [ -n "$parent" ]; then append --parent "$parent"; else append; fi
  }
} >> "$LOG" 2>&1

if [ $? -eq 0 ]; then
  snd Glass; notify "$MSG_SAVED" "$preview"
else
  snd Basso; notify "$MSG_SAVE_FAILED" "$MSG_SEE_LOG"; exit 1
fi
