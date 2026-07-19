#!/usr/bin/env bash
# PostToolUse audit log: append one JSONL line per tool call.
# PostToolUse never blocks — exit status is ignored, side effects only.
set -euo pipefail

mkdir -p .stella
jq -c '{ts: now | todate, event, tool: .tool.name, input: .tool.input}' \
  >> .stella/tool-log.jsonl

exit 0
