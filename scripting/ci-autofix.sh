#!/usr/bin/env bash
# CI autofix: when the suite is red, give Stella one budget-capped attempt
# to fix it on a branch. The --test-command oracle is the definition of
# done — no green suite, no submitted change.
#
# Expects: a provider key in the environment (e.g. ANTHROPIC_API_KEY),
# stella on PATH, and a checkout with the failing state.
set -euo pipefail

TEST_CMD="${TEST_CMD:-cargo test --workspace}"
BUDGET="${STELLA_FIX_BUDGET:-2.00}"

if $TEST_CMD; then
  echo "suite already green; nothing to fix"
  exit 0
fi

branch="stella/autofix-$(date +%Y%m%d-%H%M%S)"
git switch -c "$branch"

stella --plain --budget "$BUDGET" --output-format json \
  run "The test suite is failing. Find the cause and make the smallest fix that turns it green without weakening any test." \
  --test-command "$TEST_CMD" \
  | tee stella-result.json

# Only publish if the oracle actually flipped.
if $TEST_CMD; then
  git add -A
  git commit -m "fix: stella autofix for failing suite"
  git push -u origin "$branch"
  echo "fix pushed to $branch — open a PR for human review"
else
  echo "stella did not get the suite green within \$$BUDGET; leaving branch local"
  exit 1
fi
