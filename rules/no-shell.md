---
name: no-shell
description: The bash tool is disabled in this workspace
guard-tool: bash
---

This workspace runs Stella without shell access: file tools, search, and
the code graph only. Anything that needs a command run (installs, builds,
deploys) is a request to surface to the human, not an action to take.

(Belt and suspenders with `"tools": { "bash": "on" }` absent from
settings.json — the guard also covers setups where a broader user-scope
settings file turned the shell on.)
