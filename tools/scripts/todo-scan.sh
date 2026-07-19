#!/usr/bin/env bash
# Custom tool executable for todo_scan.
# Input arrives twice: full JSON on stdin, and scalar properties as
# STELLA_INPUT_<KEY> env vars. Simple scripts can use just the env vars.
set -euo pipefail

path="${STELLA_INPUT_PATH:-.}"
marker="${STELLA_INPUT_MARKER:-TODO|FIXME|HACK}"

if command -v rg >/dev/null 2>&1; then
  rg -n --no-heading "(${marker})" "$path" || echo "no ${marker} markers under ${path}"
else
  grep -rn -E "(${marker})" "$path" || echo "no ${marker} markers under ${path}"
fi
