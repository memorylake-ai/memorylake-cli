#!/bin/sh
# Install the MemoryLake quick actions on macOS: keyboard shortcuts that save or
# search the selected text in any app, and a Finder right-click item that uploads
# files into a project. Requires the memorylake CLI, already logged in.
#
#   curl -fsSL https://raw.githubusercontent.com/memorylake-ai/memorylake-cli/main/scripts/install-macos-quick-actions.sh | sh
#
# Nothing is asked. The script finds the actor behind your API key, the
# workspace the CLI is set to, and a project + conversation to clip into
# (created on first run, found again on every later run). Flags via `sh -s --`:
#
#   --project <id|custom-id|name>  project to clip into (default: one named
#                                  "MemoryLake Quick Actions" is created)
#   --save-key <keys>              shortcut for "save", default ^~@s  (⌃⌥⌘S)
#   --search-key <keys>            shortcut for "search", default ^~@l (⌃⌥⌘L)
#                                  ^ control  ~ option  @ command  $ shift
#   --lang zh|en                   menu and message language (default: system)
#   --source <url|dir>             where the quick-action scripts come from
#                                  (default: this script's GitHub directory)
#   --uninstall                    remove the quick actions; server data stays
#
# Every flag has an environment variable: MEMORYLAKE_QA_PROJECT, _SAVE_KEY,
# _SEARCH_KEY, _LANG, _SOURCE. A flag wins over its variable.
#
# What it writes:  ~/.memorylake/quick-actions/   the scripts and config
#                  ~/Library/Services/*.workflow  three Automator quick actions
#                  the `pbs` preferences           shortcuts + Finder menu entry
#
# Only tools shipped with macOS are used: zsh, osascript, PlistBuddy, shasum.
# POSIX sh on purpose — it runs before anything is known about the user's shell.

set -eu

REPO="memorylake-ai/memorylake-cli"
DEFAULT_SOURCE="https://raw.githubusercontent.com/$REPO/main/scripts/quick-actions"
QA_DIR="$HOME/.memorylake/quick-actions"
SERVICES_DIR="$HOME/Library/Services"
LOG_FILE="$HOME/Library/Logs/memorylake-quick-actions.log"
PROJECT_CUSTOM_ID="macos-quick-actions"
CONVERSATION_CUSTOM_ID="macos-quick-actions-clips"

PROJECT="${MEMORYLAKE_QA_PROJECT:-}"
SAVE_KEY="${MEMORYLAKE_QA_SAVE_KEY:-^~@s}"
SEARCH_KEY="${MEMORYLAKE_QA_SEARCH_KEY:-^~@l}"
LANG_CHOICE="${MEMORYLAKE_QA_LANG:-}"
SOURCE="${MEMORYLAKE_QA_SOURCE:-}"
UNINSTALL=0

say() { printf '%s\n' "$*"; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --project)    [ $# -ge 2 ] || err "--project needs a value";    PROJECT="$2";     shift 2 ;;
        --save-key)   [ $# -ge 2 ] || err "--save-key needs a value";   SAVE_KEY="$2";    shift 2 ;;
        --search-key) [ $# -ge 2 ] || err "--search-key needs a value"; SEARCH_KEY="$2";  shift 2 ;;
        --lang)       [ $# -ge 2 ] || err "--lang needs a value";       LANG_CHOICE="$2"; shift 2 ;;
        --source)     [ $# -ge 2 ] || err "--source needs a value";     SOURCE="$2";      shift 2 ;;
        --uninstall)  UNINSTALL=1; shift ;;
        -h|--help)    sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) err "unknown flag: $1" ;;
    esac
done

[ "$(uname -s)" = "Darwin" ] || err "this installer is for macOS only"

# ---------------------------------------------------------------- language

if [ -z "$LANG_CHOICE" ]; then
    case "$(defaults read -g AppleLanguages 2>/dev/null | sed -n '2p')" in
        *zh*) LANG_CHOICE=zh ;;
        *)    LANG_CHOICE=en ;;
    esac
fi
case "$LANG_CHOICE" in zh|en) ;; *) err "--lang must be zh or en" ;; esac

if [ "$LANG_CHOICE" = zh ]; then
    NAME_SAVE="存到 MemoryLake"; NAME_SEARCH="搜索 MemoryLake"; NAME_UPLOAD="上传到 MemoryLake"
else
    NAME_SAVE="Save to MemoryLake"; NAME_SEARCH="Search MemoryLake"; NAME_UPLOAD="Upload to MemoryLake"
fi

# `pbs` keys a service by "(null) - <menu title> - <selector>".
pbs_key() { printf '(null) - %s - runWorkflowAsService' "$1"; }

# ------------------------------------------------------- pbs preferences
# `defaults write` cannot take these keys (they start with a parenthesis, which
# it reads as an old-style array), so edit an exported copy with PlistBuddy and
# import it back. One flush at the end; running apps pick the change up live.

PBS_TMP="$(mktemp -t memorylake-pbs)"
trap 'rm -f "$PBS_TMP"' EXIT
defaults export pbs "$PBS_TMP"
pb() { /usr/libexec/PlistBuddy -c "$1" "$PBS_TMP" >/dev/null 2>&1; }

pbs_remove() { pb "Delete ':NSServicesStatus:$(pbs_key "$1")'" || true
               pb "Delete ':FinderActive:$(pbs_key "$1")'" || true; }
pbs_add_shortcut() {
    pb "Add ':NSServicesStatus' dict" || true
    pb "Add ':NSServicesStatus:$(pbs_key "$1")' dict"
    pb "Add ':NSServicesStatus:$(pbs_key "$1"):key_equivalent' string '$2'"
    pb "Add ':NSServicesStatus:$(pbs_key "$1"):presentation_modes' dict"
    for m in ContextMenu ServicesMenu TouchBar; do
        pb "Add ':NSServicesStatus:$(pbs_key "$1"):presentation_modes:$m' bool true"
    done
}
pbs_enable_in_finder() {
    pb "Add ':FinderActive' dict" || true
    pb "Add ':FinderActive:$(pbs_key "$1")' bool true"
    pb "Add ':NSServicesStatus:$(pbs_key "$1")' dict"
    pb "Add ':NSServicesStatus:$(pbs_key "$1"):presentation_modes' dict"
    for m in ContextMenu ServicesMenu TouchBar FinderPreview; do
        pb "Add ':NSServicesStatus:$(pbs_key "$1"):presentation_modes:$m' bool true"
    done
}
pbs_commit() { defaults import pbs "$PBS_TMP"; /System/Library/CoreServices/pbs -flush; }

# All names, both languages, so an uninstall or a language switch removes the
# services installed under the other language too.
pbs_remove_all() {
    for n in "存到 MemoryLake" "搜索 MemoryLake" "上传到 MemoryLake" \
             "Save to MemoryLake" "Search MemoryLake" "Upload to MemoryLake"; do
        pbs_remove "$n"
        rm -rf "$SERVICES_DIR/$n.workflow"
    done
}

if [ "$UNINSTALL" = 1 ]; then
    pbs_remove_all
    pbs_commit
    rm -rf "$QA_DIR"
    say "Removed the MemoryLake quick actions. Nothing on the server was touched."
    exit 0
fi

# ---------------------------------------------------------------- the CLI

if command -v memorylake >/dev/null 2>&1; then
    ML="$(command -v memorylake)"
elif [ -x "$HOME/.local/bin/memorylake" ]; then
    ML="$HOME/.local/bin/memorylake"
else
    err "memorylake CLI not found; install it first: curl -fsSL https://raw.githubusercontent.com/$REPO/main/scripts/install.sh | sh"
fi
json() { osascript -l JavaScript "$QA_DIR/json.js" "$@"; }

# ------------------------------------------------------------- the scripts
# Fetched first because the installer itself parses JSON with json.js.

mkdir -p "$QA_DIR" "$SERVICES_DIR" "$(dirname "$LOG_FILE")"
here="$(cd "$(dirname "$0")" 2>/dev/null && pwd)" || here=""
if [ -z "$SOURCE" ]; then
    if [ -n "$here" ] && [ -f "$here/quick-actions/json.js" ]; then
        SOURCE="$here/quick-actions"          # running from a checkout
    else
        SOURCE="$DEFAULT_SOURCE"
    fi
fi
for f in json.js save.sh search.sh upload.sh; do
    case "$SOURCE" in
        http://*|https://*) curl -fsSL "$SOURCE/$f" -o "$QA_DIR/$f" || err "download failed: $SOURCE/$f" ;;
        *) cp "$SOURCE/$f" "$QA_DIR/$f" || err "not found: $SOURCE/$f" ;;
    esac
done
chmod 755 "$QA_DIR"/*.sh

# ---------------------------------------------------------- server objects

say "Checking the memorylake CLI…"
actor_json="$("$ML" actor me 2>&1)" || err "the CLI is not logged in (memorylake actor me failed): $actor_json"
ACTOR="$(printf '%s' "$actor_json" | json get id)"
[ -n "$ACTOR" ] || err "could not read the actor id from: $actor_json"

WORKSPACE="$("$ML" workspace current 2>/dev/null | awk '{print $1}')"
if [ -z "$WORKSPACE" ]; then
    ws_json="$("$ML" workspace list 2>&1)" || err "cannot list workspaces: $ws_json"
    ws_count="$(printf '%s' "$ws_json" | json lines items id | grep -c .)"
    [ "$ws_count" = 1 ] || err "several workspaces and none remembered; run: memorylake workspace use <id>"
    WORKSPACE="$(printf '%s' "$ws_json" | json get items.0.id)"
fi

resolve_project() {
    # Accept an id, a custom-id, or a name.
    p="$("$ML" project get "$1" --workspace "$WORKSPACE" 2>/dev/null | json get id)" && [ -n "$p" ] && { printf '%s' "$p"; return; }
    p="$("$ML" project get "$1" --by-custom-id --workspace "$WORKSPACE" 2>/dev/null | json get id)" && [ -n "$p" ] && { printf '%s' "$p"; return; }
    "$ML" project list --workspace "$WORKSPACE" 2>/dev/null | json find items name "$1" id
}

if [ -n "$PROJECT" ]; then
    PROJECT_ID="$(resolve_project "$PROJECT")"
    [ -n "$PROJECT_ID" ] || err "project not found: $PROJECT"
else
    PROJECT_ID="$("$ML" project get "$PROJECT_CUSTOM_ID" --by-custom-id --workspace "$WORKSPACE" 2>/dev/null | json get id || true)"
    if [ -z "$PROJECT_ID" ]; then
        say "Creating project \"MemoryLake Quick Actions\"…"
        PROJECT_ID="$("$ML" project create --workspace "$WORKSPACE" --name "MemoryLake Quick Actions" \
            --custom-id "$PROJECT_CUSTOM_ID" \
            --description "Text and files saved from macOS quick actions" | json get id)"
    fi
fi
PROJECT_NAME="$("$ML" project get "$PROJECT_ID" --workspace "$WORKSPACE" | json get name)"

CONVERSATION="$("$ML" conv get "$CONVERSATION_CUSTOM_ID" --by-custom-id 2>/dev/null | json get id || true)"
if [ -z "$CONVERSATION" ]; then
    say "Creating the clips conversation…"
    CONVERSATION="$("$ML" conv create --custom-id "$CONVERSATION_CUSTOM_ID" --project "$PROJECT_ID" \
        --actors "$ACTOR" --name "macOS clips" --kind DIRECT | json get id)"
fi
[ -n "$CONVERSATION" ] || err "could not create the clips conversation"

# ------------------------------------------------------------------ config

if [ "$LANG_CHOICE" = zh ]; then
    cat > "$QA_DIR/messages.sh" <<'EOF'
MSG_TITLE="MemoryLake"
MSG_NO_SELECTION="没有选中文本"
MSG_SAVED="已存入 MemoryLake"
MSG_SAVE_FAILED="MemoryLake 保存失败"
MSG_SEE_LOG="详情见 ~/Library/Logs/memorylake-quick-actions.log"
MSG_SEARCH_FAILED="MemoryLake 搜索失败"
MSG_SEARCH_TITLE="MemoryLake 搜索："
MSG_NO_FILES="没有可上传的文件（不支持文件夹）"
MSG_PROJECTS_FAILED="获取 project 列表失败"
MSG_NO_PROJECTS="workspace 里还没有 project"
MSG_UPLOAD_TITLE="上传到 MemoryLake"
MSG_UPLOAD_PROMPT="把 {n} 个文件导入哪个 project？"
MSG_UPLOAD="上传"
MSG_CANCEL="取消"
MSG_PROJECT_NOT_FOUND="找不到 project："
MSG_UPLOADING="正在上传 {n} 个文件到"
MSG_UPLOADED="个已上传"
MSG_UPLOADED_TO="已上传到"
MSG_UPLOAD_PARTIAL="MemoryLake 部分失败"
MSG_IMPORT_FAILED="导入请求失败"
MSG_FAILED="失败"
MSG_SKIPPED_FOLDERS="跳过 {n} 个文件夹"
EOF
else
    cat > "$QA_DIR/messages.sh" <<'EOF'
MSG_TITLE="MemoryLake"
MSG_NO_SELECTION="Nothing selected"
MSG_SAVED="Saved to MemoryLake"
MSG_SAVE_FAILED="MemoryLake save failed"
MSG_SEE_LOG="See ~/Library/Logs/memorylake-quick-actions.log"
MSG_SEARCH_FAILED="MemoryLake search failed"
MSG_SEARCH_TITLE="MemoryLake: "
MSG_NO_FILES="No files to upload (folders are not supported)"
MSG_PROJECTS_FAILED="Could not list projects"
MSG_NO_PROJECTS="The workspace has no projects yet"
MSG_UPLOAD_TITLE="Upload to MemoryLake"
MSG_UPLOAD_PROMPT="Import {n} file(s) into which project?"
MSG_UPLOAD="Upload"
MSG_CANCEL="Cancel"
MSG_PROJECT_NOT_FOUND="Project not found:"
MSG_UPLOADING="Uploading {n} file(s) to"
MSG_UPLOADED="uploaded"
MSG_UPLOADED_TO="Uploaded to"
MSG_UPLOAD_PARTIAL="MemoryLake: some uploads failed"
MSG_IMPORT_FAILED="import request failed"
MSG_FAILED="failed"
MSG_SKIPPED_FOLDERS="skipped {n} folder(s)"
EOF
fi

cat > "$QA_DIR/config.sh" <<EOF
# Generated by install-macos-quick-actions.sh on $(date '+%F %T'). Re-run it to update.
ML="$ML"
WORKSPACE="$WORKSPACE"
ACTOR="$ACTOR"
PROJECT="$PROJECT_ID"
DEFAULT_PROJECT_NAME="$PROJECT_NAME"
CONVERSATION="$CONVERSATION"
LOG="$LOG_FILE"
UI_LANG="$LANG_CHOICE"
TOP_K=5
QA_DIR="$QA_DIR"
. "\$QA_DIR/messages.sh"

# Services run with no locale at all. Without one zsh slices strings by byte,
# a character cut in half makes osascript decode the whole dialog script in the
# system legacy encoding (GB18030 on a Chinese Mac), and everything shows as mojibake.
export LANG="\${LANG:-en_US.UTF-8}"

json() { osascript -l JavaScript "\$QA_DIR/json.js" "\$@"; }
# Sounds are immediate and ignore Focus mode, unlike notifications: Pop when the
# shortcut is received, Glass on success, Basso on failure.
snd() { afplay "/System/Library/Sounds/\$1.aiff" >/dev/null 2>&1 & }
notify() {
  t="\$(printf '%s' "\$1" | sed -e 's/\\\\/\\\\\\\\/g' -e 's/"/\\\\"/g')"
  b="\$(printf '%s' "\$2" | sed -e 's/\\\\/\\\\\\\\/g' -e 's/"/\\\\"/g')"
  osascript -e "display notification \\"\$b\\" with title \\"\$t\\"" >/dev/null 2>&1
}
EOF

# --------------------------------------------------------------- workflows
# Automator bundles written by hand: a "Run Shell Script" action fed the
# selection as arguments. The text and the Finder variants differ only in the
# input type and where the service is offered.

write_workflow() {   # name  command  kind(text|files)
    name="$1"; cmd="$2"; kind="$3"
    bundle="$SERVICES_DIR/$name.workflow"
    rm -rf "$bundle"; mkdir -p "$bundle/Contents"
    if [ "$kind" = text ]; then
        accepts="com.apple.cocoa.string"; input="com.apple.Automator.text"; finder=""
        services='<key>NSSendTypes</key><array><string>public.utf8-plain-text</string></array>'
    else
        accepts="com.apple.cocoa.path"; input="com.apple.Automator.fileSystemObject"
        finder='<key>serviceApplicationBundleID</key><string>com.apple.finder</string>
		<key>serviceApplicationPath</key><string>/System/Library/CoreServices/Finder.app</string>'
        services='<key>NSSendFileTypes</key><array><string>public.item</string></array>
			<key>NSRequiredContext</key><dict><key>NSApplicationIdentifier</key><string>com.apple.finder</string></dict>'
    fi
    cat > "$bundle/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSBackgroundColorName</key><string>background</string>
			<key>NSIconName</key><string>NSActionTemplate</string>
			<key>NSMenuItem</key><dict><key>default</key><string>$name</string></dict>
			<key>NSMessage</key><string>runWorkflowAsService</string>
			$services
		</dict>
	</array>
</dict>
</plist>
EOF
    cat > "$bundle/Contents/document.wflow" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key><string>528</string>
	<key>AMApplicationVersion</key><string>2.10</string>
	<key>AMDocumentVersion</key><string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Optional</key><true/>
					<key>Types</key><array><string>$accepts</string></array>
				</dict>
				<key>AMActionVersion</key><string>2.0.3</string>
				<key>AMApplication</key><array><string>Automator</string></array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key><dict/>
					<key>CheckedForUserDefaultShell</key><dict/>
					<key>inputMethod</key><dict/>
					<key>shell</key><dict/>
					<key>source</key><dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Types</key><array><string>com.apple.cocoa.string</string></array>
				</dict>
				<key>ActionBundlePath</key><string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key><string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key><string>$cmd</string>
					<key>CheckedForUserDefaultShell</key><true/>
					<key>inputMethod</key><integer>1</integer>
					<key>shell</key><string>/bin/zsh</string>
					<key>source</key><string></string>
				</dict>
				<key>BundleIdentifier</key><string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key><string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key><false/>
				<key>CanShowWhenRun</key><true/>
				<key>Category</key><array><string>AMCategoryUtilities</string></array>
				<key>Class Name</key><string>RunShellScriptAction</string>
				<key>InputUUID</key><string>DDF8FE78-CFC4-4F00-BF46-13409E7560E3</string>
				<key>Keywords</key><array><string>Shell</string><string>Script</string></array>
				<key>OutputUUID</key><string>44C42D40-DA79-4F0A-8639-878A7B15D159</string>
				<key>UUID</key><string>3EB3A69B-DD71-4E64-85D1-FD709CDB03AE</string>
				<key>UnlocalizedApplications</key><array><string>Automator</string></array>
				<key>arguments</key>
				<dict>
					<key>0</key><dict><key>default value</key><integer>0</integer><key>name</key><string>inputMethod</string><key>required</key><string>0</string><key>type</key><string>0</string><key>uuid</key><string>0</string></dict>
					<key>1</key><dict><key>default value</key><false/><key>name</key><string>CheckedForUserDefaultShell</string><key>required</key><string>0</string><key>type</key><string>0</string><key>uuid</key><string>1</string></dict>
					<key>2</key><dict><key>default value</key><string></string><key>name</key><string>source</string><key>required</key><string>0</string><key>type</key><string>0</string><key>uuid</key><string>2</string></dict>
					<key>3</key><dict><key>default value</key><string></string><key>name</key><string>COMMAND_STRING</string><key>required</key><string>0</string><key>type</key><string>0</string><key>uuid</key><string>3</string></dict>
					<key>4</key><dict><key>default value</key><string>/bin/sh</string><key>name</key><string>shell</string><key>required</key><string>0</string><key>type</key><string>0</string><key>uuid</key><string>4</string></dict>
				</dict>
				<key>conversionLabel</key><integer>0</integer>
				<key>isViewVisible</key><integer>1</integer>
				<key>location</key><string>309.000000:305.000000</string>
				<key>nibPath</key><string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
			</dict>
			<key>isViewVisible</key><integer>1</integer>
		</dict>
	</array>
	<key>connectors</key><dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key><dict/>
		<key>applicationPaths</key><array/>
		<key>inputTypeIdentifier</key><string>$input</string>
		<key>outputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>presentationMode</key><integer>11</integer>
		<key>processesInput</key><false/>
		$finder
		<key>serviceInputTypeIdentifier</key><string>$input</string>
		<key>serviceOutputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key><false/>
		<key>systemImageName</key><string>NSActionTemplate</string>
		<key>useAutomaticInputType</key><false/>
		<key>workflowTypeIdentifier</key><string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
EOF
    plutil -lint -s "$bundle/Contents/Info.plist" "$bundle/Contents/document.wflow" >/dev/null || err "generated an invalid workflow for $name"
}

pbs_remove_all      # drop any earlier install, whichever language it used
write_workflow "$NAME_SAVE"   "exec \"\$HOME/.memorylake/quick-actions/save.sh\" \"\$@\""   text
write_workflow "$NAME_SEARCH" "exec \"\$HOME/.memorylake/quick-actions/search.sh\" \"\$@\"" text
write_workflow "$NAME_UPLOAD" "exec \"\$HOME/.memorylake/quick-actions/upload.sh\" \"\$@\"" files
pbs_add_shortcut "$NAME_SAVE" "$SAVE_KEY"
pbs_add_shortcut "$NAME_SEARCH" "$SEARCH_KEY"
pbs_enable_in_finder "$NAME_UPLOAD"
pbs_commit

# ----------------------------------------------------------------- summary

pretty_key() {   # ^~@s -> ⌃⌥⌘S
    printf '%s' "$1" | sed -e 's/\^/⌃/g' -e 's/~/⌥/g' -e 's/@/⌘/g' -e 's/\$/⇧/g' | tr '[:lower:]' '[:upper:]'
}
say ""
say "Installed the MemoryLake quick actions."
say "  $(pretty_key "$SAVE_KEY")   $NAME_SAVE      selected text → conversation \"macOS clips\" in project \"$PROJECT_NAME\""
say "  $(pretty_key "$SEARCH_KEY")   $NAME_SEARCH    selected text → search results in a dialog"
say "  right-click → Quick Actions → $NAME_UPLOAD   (Finder, files only)"
say ""
say "You will hear a pop when a shortcut is received, a chime on success."
say "The first run asks for permission to control System Events and to send notifications; allow both."
say "Log: $LOG_FILE"
say "Remove: rerun this script with --uninstall"
