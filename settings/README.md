# settings.json profiles

Stella reads `settings.json` from three scopes and merges them **per provider,
per field**, ascending precedence:

1. **User** — `~/.config/stella/settings.json`
2. **Org-managed** — `/Library/Application Support/stella/settings.json`
   (macOS) or `/etc/stella/settings.json` (Linux); path overridable with
   `STELLA_MANAGED_SETTINGS`
3. **Project** — `<repo>/.stella/settings.json` (highest, trust-gated for
   sensitive fields)

A malformed file is a hard, named error — Stella never silently skips your
config. Full reference:
[Configuration → settings.json](https://stella.oxagen.sh/docs/configuration/settings).

## Profiles in this directory

| File | The idea |
| --- | --- |
| [`minimal.settings.json`](minimal.settings.json) | The smallest useful config: pick a default model, turn the shell tool on. |
| [`balanced.settings.json`](balanced.settings.json) | The daily driver — cheap capable worker, flagship cross-family judge, autos on. |
| [`max-quality.settings.json`](max-quality.settings.json) | Fable 5 worker at `xhigh` effort, GPT-5.5 judge on priority tier. When a wrong change costs more than the tokens. |
| [`dirt-cheap.settings.json`](dirt-cheap.settings.json) | DeepSeek worker with thinking off and capped output. Pennies per run. |
| [`local-ollama.settings.json`](local-ollama.settings.json) | A custom OpenAI-compatible provider pointing at Ollama. Zero API spend. |
| [`team.settings.json`](team.settings.json) | Project-scope `.stella/settings.json` a team ships in-repo: allowed models, shell on, a guard hook. |
| [`credentials.example.toml`](credentials.example.toml) | The shape of `~/.config/stella/credentials.toml` (written `0600` by the interactive prompt). |

## The one routing rule worth memorizing

`agent_engine_config.default_model` is what routes the **executing worker**.
The `pipeline_worker_model` field feeds display/selection surfaces — so keep
it and `default_model` set to the same value (every profile here does), and
use `agents.judge` / `agents.triage` (or `pipeline_judge_model` /
`pipeline_triage_model`) to pin the other roles. A `--model` flag outranks
all of it for one invocation.

## Trying a profile

```bash
cp balanced.settings.json ~/.config/stella/settings.json
stella models          # confirm what resolved
stella run "..."       # go
```

API keys never live in these files — set the provider env var
(`ZAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, …) or let the
interactive prompt write `credentials.toml`.
