---
name: changelog
description: Draft a CHANGELOG entry from commits since the last tag
---

Draft a changelog entry for the next release.

1. Find the last tag (`git describe --tags --abbrev=0`) and list commits
   since it (`git log <tag>..HEAD --oneline --no-merges`).
2. Group the changes under **Added / Changed / Fixed / Removed** — describe
   user-visible behavior, not commit messages. Fold related commits into one
   line; drop pure chores.
3. Append the entry to `CHANGELOG.md` under an `## [Unreleased]` heading,
   matching the file's existing format. Create the file in Keep a Changelog
   style if it doesn't exist.
