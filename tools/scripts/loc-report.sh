#!/usr/bin/env bash
# Custom tool executable for loc_report: line counts by extension,
# git-tracked files only. No input needed; stdin is ignored.
set -euo pipefail

git ls-files \
  | awk -F. 'NF>1 {print $NF}' \
  | sort | uniq -c | sort -rn | head -15 \
  | awk '{printf "%-10s %s files\n", "."$2, $1}'

echo "---"
echo "total lines in tracked files:"
git ls-files -z | xargs -0 cat 2>/dev/null | wc -l
