# stella-examples

Example ways to use **[Stella](https://github.com/macanderson/stella)** — the
open-source, BYOK, no-phone-home coding agent that runs in your terminal.

This repo is a working cookbook for **everything configurable in Stella**:
settings profiles, lifecycle hooks, custom agents, custom commands, custom
script tools, skills, permission rules, MCP servers, fleet plans, memory
taxonomies, headless scripting, and turn-loop plugins. Every file matches the
schemas Stella actually parses, and every directory README says where the file
goes on disk and what it does once it is there.

- **Website / docs:** [stella.oxagen.sh](https://stella.oxagen.sh)
- **Showcase:** these examples are indexed in the
  [Stella showcase](https://stella.oxagen.sh/docs/showcase)
- **Install Stella:**
  `curl -fsSL https://raw.githubusercontent.com/macanderson/stella/main/install.sh | sh`

## What's here

| Directory | Surface | Goes in |
| --- | --- | --- |
| [`settings/`](settings/) | `settings.json` profiles — model lineups, providers, local models, team config | `~/.config/stella/settings.json` or `<repo>/.stella/settings.json` |
| [`hooks/`](hooks/) | Lifecycle hooks — `SessionStart`, `PreToolUse`, `PostToolUse` + guard scripts | `hooks` key in `settings.json` |
| [`commands/`](commands/) | Custom slash commands (`/fix-issue 42`) | `.stella/commands/` or `~/.config/stella/commands/` |
| [`agents/`](agents/) | Custom agents with scoped toolbelts (`/code-reviewer …`) | `.stella/agents/` or `~/.config/stella/agents/` |
| [`skills/`](skills/) | Reusable skill documents | `.stella/skills/<slug>/SKILL.md` |
| [`rules/`](rules/) | Workspace rules — soft conventions and hard permission guards | `.stella/rules/` or `~/.config/stella/rules/` |
| [`tools/`](tools/) | Custom script tools (TOML manifest + executable) | `.stella/tools/*.toml` or `~/.config/stella/tools/*.toml` |
| [`mcp/`](mcp/) | MCP server config — stdio and HTTP transports | `.stella/mcp.toml` |
| [`fleet/`](fleet/) | Fleet plan files — task DAGs fanned out to parallel workers | anywhere; `stella fleet --plan <file>` |
| [`memory/`](memory/) | Memory & domain taxonomy — `domains.toml`, memories, promotion loop | `.stella/` |
| [`scripting/`](scripting/) | Headless & CI usage — budgets, JSON output, test-command oracles | your CI / shell |
| [`plugins/`](plugins/) | Turn-loop plugins — one verification plugin written three times, in Rust, Python and TypeScript | `.stella/plugins/` or `~/.stella/plugins/` |

## How to use these examples

Most files copy straight into place. The two locations that matter:

- **User scope** — `~/.config/stella/` applies to every project on your
  machine (`settings.json`, `commands/`, `agents/`, `skills/`, `rules/`,
  `tools/`).
- **Project scope** — `<repo>/.stella/` ships with a repository and applies
  there (highest precedence in the merge).

One deliberate speed bump: **project-scope hooks and credential-routing
fields don't load from a cloned repo** until you set `STELLA_TRUST_PROJECT=1`.
That's Stella refusing to let a repo you just cloned run arbitrary commands
on your machine. Everything cosmetic (display names, default models, custom
commands/agents) applies without it.

**Plugins are the one surface with real code in it.**
[`plugins/`](plugins/) holds the same verification plugin implemented three
times — Rust, Python and TypeScript — with manifests that are identical except
for the argv naming each program, and one CI job that runs all three against
one set of wire vectors. Start with [`plugins/README.md`](plugins/README.md)
for what a plugin is, the participation grades, and the four points of the
turn loop.

```bash
# try a profile
cp settings/balanced.settings.json ~/.config/stella/settings.json

# give this repo a custom command
mkdir -p .stella/commands && cp commands/fix-issue.md .stella/commands/

# validate custom tools after copying them
stella tools --validate
```

## Contributing

Have a hook, agent, plan, or profile worth showcasing? PRs welcome — keep
each example small, self-contained, and true to the schema Stella parses
(cite the docs page it demonstrates). Real-world workflows beat toys.

## License

[MIT](LICENSE). Stella itself is dual-licensed MIT OR Apache-2.0.
