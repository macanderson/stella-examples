#!/usr/bin/env bash
# SessionStart hook: whatever this prints to stdout is appended to the
# system prompt as extra context for the whole session.
# Payload on stdin is just {"event":"SessionStart","cwd":"…"} — the cwd is
# also this script's working directory, so plain git commands work.
set -euo pipefail

echo "## Workspace snapshot"
echo "- branch: $(git branch --show-current 2>/dev/null || echo 'not a git repo')"

dirty="$(git status --porcelain 2>/dev/null | head -10)"
if [ -n "$dirty" ]; then
  echo "- uncommitted changes:"
  echo "$dirty" | sed 's/^/    /'
else
  echo "- working tree clean"
fi

echo "- last 3 commits:"
git log --oneline -3 2>/dev/null | sed 's/^/    /' || true

exit 0
