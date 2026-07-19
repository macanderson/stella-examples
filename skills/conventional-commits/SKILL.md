---
name: conventional-commits
description: Write commit messages in this repo's Conventional Commits dialect
origin: workspace
---

# Conventional commits, house dialect

Every commit message follows `type(scope): subject`.

- **Types**: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`,
  `ci`. Nothing else.
- **Scope** is the top-level directory or crate touched (`cli`, `core`,
  `docs`). Omit it only for repo-wide changes.
- **Subject**: imperative mood, lower-case, no trailing period, ≤72 chars.
  "add retry to fetch", not "Added retries".
- Body (optional, blank line after subject): the *why*, wrapped at 72.
  The diff already shows the what.
- Breaking changes: add a `BREAKING CHANGE:` footer describing the
  migration, and `!` after the type — `feat(core)!: …`.
- One logical change per commit. If the message needs "and", split it.

Before committing: re-read the staged diff (`git diff --staged --stat`) and
make sure the type matches what actually changed — a `fix` that adds a
feature is mislabeled.
