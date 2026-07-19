#!/usr/bin/env bash
# PreToolUse guard for the `bash` tool: veto destructive commands.
# Any non-zero exit BLOCKS the tool call; trimmed stderr (falling back to
# stdout) becomes the message the model receives instead of a result.
# Blocking is fail-closed: if this script times out or can't spawn, the
# tool call is blocked too — a broken guard never waves a call through.
set -euo pipefail

command="$(jq -r '.tool.input.command // empty')"

deny() {
  echo "blocked by guard-bash.sh: $1" >&2
  exit 2
}

case "$command" in
  *"git push --force"* | *"git push -f"*)
    deny "force-push is not allowed; use --force-with-lease after review" ;;
  *"git reset --hard"*)
    deny "hard reset discards work; stash or branch instead" ;;
  *"rm -rf /"* | *"rm -rf ~"*)
    deny "refusing catastrophic rm -rf" ;;
esac

exit 0
