---
name: pr-description
description: Write a PR title and description for the current branch
---

Write a pull-request title and description for the current branch.

Diff against the default branch (`git diff origin/HEAD...HEAD` and
`git log origin/HEAD..HEAD --oneline`), then produce:

- A one-line imperative title (≤72 chars).
- **What & why** — the problem and the approach, two short paragraphs max.
- **How it was verified** — tests added/run, with the witness test called
  out if there is one.
- **Risk notes** — anything a reviewer should look at twice.

If the repository has a PR template (`.github/PULL_REQUEST_TEMPLATE.md`),
fill that structure instead.

$ARGUMENTS
