#!/bin/zsh
# Finder quick action: upload the selected files to the Library and import them
# into a project the user picks. Indexing happens server-side afterwards.
set -u
. "${0:A:h}/config.sh"

files=(); skipped=0
for f in "$@"; do
  if [ -f "$f" ]; then files+=("$f"); else skipped=$((skipped+1)); fi
done
[ ${#files[@]} -eq 0 ] && { notify "$MSG_TITLE" "$MSG_NO_FILES"; exit 0; }
snd Pop

projects_json="$("$ML" project list --workspace "$WORKSPACE" 2>>"$LOG")" || {
  snd Basso; notify "$MSG_TITLE" "$MSG_PROJECTS_FAILED"; exit 1; }
names="$(printf '%s' "$projects_json" | json lines items name)"
[ -z "$names" ] && { snd Basso; notify "$MSG_TITLE" "$MSG_NO_PROJECTS"; exit 1; }

n=${#files[@]}
prompt="${MSG_UPLOAD_PROMPT//\{n\}/$n}"
chosen="$(osascript <<APPLESCRIPT
tell application "System Events" to activate
set opts to paragraphs of "$names"
set def to {item 1 of opts}
if opts contains "$DEFAULT_PROJECT_NAME" then set def to {"$DEFAULT_PROJECT_NAME"}
set r to choose from list opts with title "$MSG_UPLOAD_TITLE" with prompt "$prompt" default items def OK button name "$MSG_UPLOAD" cancel button name "$MSG_CANCEL"
if r is false then return ""
return item 1 of r
APPLESCRIPT
)"
[ -z "$chosen" ] && exit 0
project_id="$(printf '%s' "$projects_json" | json find items name "$chosen" id)"
[ -z "$project_id" ] && { snd Basso; notify "$MSG_TITLE" "$MSG_PROJECT_NOT_FOUND $chosen"; exit 1; }

notify "$MSG_TITLE" "${MSG_UPLOADING//\{n\}/$n} $chosen"
echo "=== $(date '+%F %T') upload n=$n project=$chosen($project_id)" >> "$LOG"

item_ids=(); failed=()
for f in "${files[@]}"; do
  out="$("$ML" lib upload "$f" 2>>"$LOG")"
  if [ $? -eq 0 ]; then
    id="$(printf '%s' "$out" | json get item_id)"
    echo "  ok  $f -> $id" >> "$LOG"
    [ -n "$id" ] && item_ids+=("$id")
  else
    echo "  FAIL $f" >> "$LOG"; failed+=("${f:t}")
  fi
done

import_msg=""
if [ ${#item_ids[@]} -gt 0 ]; then
  out="$("$ML" proj doc import --project "$project_id" "${item_ids[@]}" 2>>"$LOG")"
  rc=$?; echo "$out" >> "$LOG"
  import_msg="$(printf '%s' "$out" | json import "$UI_LANG")"
  [ $rc -ne 0 ] && [ -z "$import_msg" ] && import_msg="$MSG_IMPORT_FAILED"
fi

summary="${#item_ids[@]}/$n $MSG_UPLOADED, $import_msg"
[ ${#failed[@]} -gt 0 ] && summary="$summary; $MSG_FAILED: ${(j:, :)failed}"
[ $skipped -gt 0 ] && summary="$summary; ${MSG_SKIPPED_FOLDERS//\{n\}/$skipped}"
if [ ${#failed[@]} -eq 0 ]; then
  snd Glass; notify "$MSG_UPLOADED_TO $chosen" "$summary"
else
  snd Basso; notify "$MSG_UPLOAD_PARTIAL" "$summary"; exit 1
fi
