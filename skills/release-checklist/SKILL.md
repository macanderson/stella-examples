---
name: release-checklist
description: Cut a release of this project, in order, with verification at each step
origin: workspace
---

# Release checklist

Execute in order. Every step has a check; a failed check stops the release.

1. **Clean tree** — `git status --porcelain` prints nothing; you are on
   `main`, up to date with origin.
2. **Gate** — format check, linter with warnings-as-errors, and the full
   test suite all pass locally.
3. **Version** — bump the version in the manifest(s); the changelog's
   `## [Unreleased]` section moves under the new version with today's date.
4. **Tag** — commit `chore: release vX.Y.Z`, then `git tag -a vX.Y.Z -m
   "vX.Y.Z"`. Tag and manifest version must match exactly.
5. **Push** — `git push && git push --tags`; watch CI to green before
   announcing anything.
6. **Verify the artifact** — install the released package/binary in a clean
   directory and run its version command; it must print X.Y.Z.

Never skip step 6: a release that isn't installable isn't released. If any
step fails after tagging, fix forward with a patch release — do not move or
delete a pushed tag.
