# Memory & domains

Stella's memory is a local self-improvement loop, and most of it is
user-inspectable plain text under `.stella/`. Reference:
[stella memory](https://stella.oxagen.sh/docs/commands/memory).

**The loop**

1. After turns, Stella reflects and records lessons — appended to
   `.stella/reflections.jsonl` (one JSON object per line: `lesson`,
   `domains`, `occurred_at`) and indexed in the local context DB.
2. Lessons that keep recurring are **auto-promoted to skills**
   (`.stella/skills/<slug>/SKILL.md`) — see [`../skills/`](../skills/).
3. You can promote a lesson to a hard [rule](../rules/) yourself:
   `stella memory promote` writes `.stella/rules/<slug>.md`.
4. Hand-written notes live in `.stella/memories/*.md`; the agent cites them
   with `cite_memory` when they steer a decision.

```bash
stella memory list        # what Stella has learned here
stella memory promote     # lesson → enforced rule
stella memory validate    # prune stale/contradicted lessons
```

**Domains** — `.stella/domains.toml` names the areas of your codebase so
lessons and telemetry tag correctly. Stella infers one on `stella init`
(stamped `inferred_by = "heuristic"`); [`domains.toml`](domains.toml) here
shows a hand-tuned taxonomy for a typical web app. Everything stays on your
machine — memory, reflections, and telemetry never leave `.stella/`.
