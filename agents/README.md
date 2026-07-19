# Custom agents

A custom agent is a persona with its own system prompt and a **scoped
toolbelt**, invoked as `/agent-name <task>`. Like commands, agents are
markdown + frontmatter. Reference:
[Agent tools → Custom agents](https://stella.oxagen.sh/docs/agent-tools/custom-agents).

**Where they live**

- Project: `<repo>/.stella/agents/<slug>.md` (or `<slug>/AGENT.md`)
- User: `~/.config/stella/agents/<slug>.md`

`stella init` symlink-adopts `.claude/agents/` too, and edits are versioned
under `<dir>/.versions/<slug>/`.

**Frontmatter Stella parses** — exactly these keys; anything else is
silently ignored:

| Key | Required | Meaning |
| --- | --- | --- |
| `name` | no | Slug; defaults to the file/dir name |
| `description` | no | One-liner for the `/agents` listing; falls back to the body's first line |
| `tools` | no | Comma list or array of tool names the agent may use. **Omitted, `*`, or `all` grants every tool.** |

The body (required) is the agent's persona / system prompt. Built-in tool
names you can grant: `bash`, `read_file`, `write_file`, `edit_file`,
`grep`, `glob`, `graph_query`, `verify_done`, `save_memory`, `cite_memory` —
plus any [custom tools](../tools/) and MCP tools you've configured.

> **Porting from Claude Code?** Keys like `model`, `skills`, or
> `memory_dir` in agent frontmatter are not part of Stella's schema and are
> ignored. Route models with
> [`agent_engine_config`](../settings/) instead.

| File | Toolbelt | The idea |
| --- | --- | --- |
| [`code-reviewer.md`](code-reviewer.md) | read-only + `bash` | Evidence-first review; can run tests but never edits |
| [`test-writer.md`](test-writer.md) | write + `verify_done` | Turns a bug report into a failing witness test |
| [`docs-writer.md`](docs-writer.md) | read + write, no shell | Keeps docs in sync with the code it reads |
