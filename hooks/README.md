# Lifecycle hooks

Shell commands that fire on Stella's agent lifecycle events, declared under
the `hooks` key in `settings.json`. Claude Code parity — existing hook
knowledge carries over. Full reference:
[Agent tools → Hooks](https://stella.oxagen.sh/docs/agent-tools/hooks).

| Event | When | Blocking? | Matcher |
| --- | --- | --- | --- |
| `SessionStart` | Once before the turn | No — **stdout is appended to the system prompt** | Ignored |
| `PreToolUse` | Before a tool runs | **Yes — non-zero exit blocks the tool** (fail-closed: timeout/spawn-fail also block) | Glob over tool name |
| `PostToolUse` | After a tool runs | No — side effects only | Glob over tool name |

Every hook gets one JSON document on **stdin**:

```json
{
  "event": "PostToolUse",
  "cwd": "/path/to/workspace",
  "tool": { "name": "bash", "input": { "command": "git status" } },
  "toolResult": "On branch main…"
}
```

Absent fields are omitted entirely (a `SessionStart` payload is just
`event` + `cwd`). Commands run as `bash -c <command>` with cwd = workspace
root. `timeoutMs` defaults to `60000`, hard cap `600000`.

## Files here

| File | Event | What it shows |
| --- | --- | --- |
| [`settings.json`](settings.json) | all three | Complete `hooks` block wiring the three scripts below |
| [`scripts/session-context.sh`](scripts/session-context.sh) | `SessionStart` | Inject branch, recent commits, and dirty files into the system prompt |
| [`scripts/guard-bash.sh`](scripts/guard-bash.sh) | `PreToolUse` | Veto destructive shell commands (force-push, hard reset, `rm -rf`) |
| [`scripts/log-tool-use.sh`](scripts/log-tool-use.sh) | `PostToolUse` | Append a JSONL audit line per tool call to `.stella/tool-log.jsonl` |

## Install

Merge [`settings.json`](settings.json) into `~/.config/stella/settings.json`
and copy `scripts/` into your repo (the commands use paths relative to the
workspace root). Useful tool names for matchers: `bash`, `read_file`,
`write_file`, `edit_file`, `grep`, `glob`, `graph_query`, `verify_done` —
globs like `*_file` match the whole file family.

> **Trust boundary:** hooks declared in a repo's `.stella/settings.json` do
> not load until you set `STELLA_TRUST_PROJECT=1` (legacy:
> `STELLA_PROJECT_HOOKS=1`) — a cloned repo can't run commands on your
> machine just because you opened it. User-scope hooks always load.
