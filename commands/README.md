# Custom commands

Reusable prompt templates invoked as `/name args` from the Command Deck or
line REPL. A command is a markdown file with optional frontmatter; the body
is the prompt. Reference:
[Agent tools → Commands](https://stella.oxagen.sh/docs/agent-tools/commands).

**Where they live**

- Project: `<repo>/.stella/commands/<slug>.md` (or `<slug>/COMMAND.md`)
- User: `~/.config/stella/commands/<slug>.md`

`stella init` also **symlink-adopts** existing `.claude/commands/` and
`.agents/` content, so Claude Code commands carry over without copying.

**Frontmatter Stella parses**

| Key | Required | Meaning |
| --- | --- | --- |
| `name` | no | Slug; defaults to the filename stem |
| `description` | no | Listing line; falls back to the body's first line (≤72 chars) |

**Arguments.** `$ARGUMENTS` in the body is replaced with everything typed
after the command name. No placeholder? Non-empty args are appended as a
trailing paragraph. On name collisions: commands shadow skills, which shadow
agents.

| File | Invocation | What it shows |
| --- | --- | --- |
| [`fix-issue.md`](fix-issue.md) | `/fix-issue 142` | `$ARGUMENTS` + a definition-of-done the engine can verify |
| [`changelog.md`](changelog.md) | `/changelog` | A no-argument command that mines git history |
| [`pr-description.md`](pr-description.md) | `/pr-description` | Argument-optional: works bare or with extra reviewer notes |
