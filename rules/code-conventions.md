---
name: code-conventions
description: House style the agent follows in every session
---

Conventions for this workspace, in priority order:

1. Match the file you are in — its naming, comment density, and error
   handling — before any global preference.
2. No new dependencies without being asked. Prefer the standard library and
   what's already in the lockfile.
3. Errors are handled or propagated, never swallowed. No bare `catch {}` /
   `let _ =` on fallible operations.
4. Public functions that gain behavior gain a test in the same change.
5. Comments state constraints the code can't ("must run before X",
   "ordering matters because Y") — not narration of the next line.
