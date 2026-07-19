# Headless & CI scripting

Stella is scriptable: structured output, hard budgets, and a deterministic
definition of done make it safe to run where nobody is watching. Reference:
[Scripting](https://stella.oxagen.sh/docs/scripting).

**The flags that matter unattended**

| Flag / env | Effect |
| --- | --- |
| `--output-format json` (or `stream-json`) | Machine-readable results on stdout (`STELLA_OUTPUT_FORMAT`) |
| `--budget 2.00` | Hard USD cap, enforced between steps — aborts cleanly, never mid-tool (`STELLA_BUDGET`) |
| `--test-command "…"` | Arms the fail→pass flip oracle: the witness test must fail on HEAD and pass on the change |
| `--model provider/model` | Pin the model for this run, outranking settings (`STELLA_MODEL`) |
| `--plain` / `--no-anim` | Line-oriented output, no TUI — right for CI logs (`STELLA_PLAIN`, `STELLA_NO_ANIM`) |

Provider keys come from the environment (`ZAI_API_KEY`,
`ANTHROPIC_API_KEY`, …) — in CI, from your secret store.

| Script | What it shows |
| --- | --- |
| [`ci-autofix.sh`](ci-autofix.sh) | A CI job that lets Stella fix a red test suite, budget-capped, oracle-armed |
| [`nightly-goal.sh`](nightly-goal.sh) | A cron-able goal run: judged rounds toward "clippy clean, tests green" |

Afterwards, spend is queryable: `stella stats --format json` for dashboards,
`stella observe` for the local Observatory UI.
