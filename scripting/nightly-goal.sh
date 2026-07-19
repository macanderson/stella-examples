#!/usr/bin/env bash
# Nightly hygiene run: goal mode works in judged rounds until the judge
# signs off or the budget stops it. Safe to cron — the budget is a hard
# cap and all telemetry stays in .stella/store.db.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
git fetch origin
git switch -c "stella/nightly-$(date +%Y%m%d)" origin/main

stella --plain --budget 5.00 \
  goal "cargo clippy --workspace --all-targets -- -D warnings is clean and cargo test --workspace passes"

# Publish for review only if there is something to show.
if ! git diff --quiet || ! git diff --quiet --staged; then
  git add -A
  git commit -m "chore: stella nightly hygiene run"
  git push -u origin HEAD
fi

# What did tonight cost?
stella stats --format json | tail -1
