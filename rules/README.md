# Workspace rules & permission guards

Rules are markdown files that shape what the agent may do. They come in two
tiers, and the difference is the whole point:

- **Tier 1 — soft rules.** No guard keys. The rule's text is injected into
  the prompt: strong guidance, but the model is doing the obeying.
- **Tier 2 — hard guards.** Any `guard-*` key in the frontmatter makes the
  rule **enforced at the tool boundary** — the blocked call never executes,
  regardless of what the model decides.

Reference:
[Agent tools → Permissions](https://stella.oxagen.sh/docs/agent-tools/permissions).

**Where they live** (later wins on conflicts): `~/.config/stella/rules/`,
then `<repo>/.claude/rules/`, then `<repo>/.stella/rules/`.

**Frontmatter**

| Key | Tier | Meaning |
| --- | --- | --- |
| `name` | — | Slug; defaults to filename |
| `description` | — | One-liner for listings |
| `guard-tool` | 2 | Deny a tool outright, by name |
| `guard-deny-path` | 2 | Deny file operations on paths matching a glob |
| `guard-deny-command` | 2 | Deny `bash` commands matching a glob |

| File | Tier | What it shows |
| --- | --- | --- |
| [`no-force-push.md`](no-force-push.md) | 2 | `guard-deny-command` — force-pushes can't execute |
| [`protect-migrations.md`](protect-migrations.md) | 2 | `guard-deny-path` — applied migrations are immutable |
| [`no-shell.md`](no-shell.md) | 2 | `guard-tool` — a workspace where `bash` is off the table |
| [`code-conventions.md`](code-conventions.md) | 1 | A soft rule: prompt-injected house style |

Guards complement [`PreToolUse` hooks](../hooks/): a guard is declarative
and can't be forgotten; a hook can express arbitrary logic in script. Use
guards for bright lines, hooks for judgment calls.
