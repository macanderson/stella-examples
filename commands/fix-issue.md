---
name: fix-issue
description: Fix a GitHub issue end-to-end with a witness test
---

Fix GitHub issue #$ARGUMENTS.

1. Read the issue: `gh issue view $ARGUMENTS --comments`.
2. Reproduce the reported behavior before changing anything.
3. Write a witness test that fails on the current code because of this bug.
4. Make the smallest change that turns that test green. Follow the
   surrounding code's style; do not refactor unrelated code.
5. Run the affected test suite and `verify_done` — the witness must fail on
   HEAD and pass on the working tree.
6. Summarize: root cause, the fix, and the witness test path.
