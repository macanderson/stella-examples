# Custom script tools

Give the agent a new tool by dropping a TOML manifest + an executable into
place — no plugin API, no recompiling. Reference:
[Agent tools → Custom tools](https://stella.oxagen.sh/docs/agent-tools/custom-tools).

**Where they live**

- Project: `<repo>/.stella/tools/<name>.toml`
- User: `~/.config/stella/tools/<name>.toml`

**Manifest schema**

| Key | Required | Meaning |
| --- | --- | --- |
| `name` | yes | `^[a-z][a-z0-9_]{1,63}$`; `verify_done` is reserved |
| `description` | yes | What the model reads to decide when to call it |
| `command` | yes | **Argv array**, spawned directly — no shell interpolation |
| `timeout_ms` | no | Default `30000`, hard cap `600000` |
| `[env]` | no | Extra environment for the child process |
| `[input_schema]` | no | JSON Schema (as TOML) for the tool's input |

**How input arrives.** The model's JSON input is piped to the command's
**stdin**, and every scalar property is also exported as
`STELLA_INPUT_<KEY>` (upper-cased) in the environment — so simple scripts
can skip JSON parsing entirely.

**Validate before trusting:**

```bash
stella tools --validate         # checks every manifest it can discover
```

| Manifest | Script | What it shows |
| --- | --- | --- |
| [`todo-scan.toml`](todo-scan.toml) | [`scripts/todo-scan.sh`](scripts/todo-scan.sh) | Input schema + `STELLA_INPUT_*` env consumption |
| [`loc-report.toml`](loc-report.toml) | [`scripts/loc-report.sh`](scripts/loc-report.sh) | Minimal manifest: no input schema, `[env]` table |

Copy both the manifest and its script, keep the script executable
(`chmod +x`), and adjust the `command` path to where the script lands in
your repo.
