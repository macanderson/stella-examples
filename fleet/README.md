# Fleet plans

`stella fleet` fans a DAG of tasks out to parallel workers, each running the
full pipeline. Plans are `.toml` or `.json` files passed with `--plan`.
Reference:
[Agent fleets](https://stella.oxagen.sh/docs/agent-fleets).

**Task fields**

| Field | Default | Meaning |
| --- | --- | --- |
| `id` | — | Unique task id (what `depends_on` points at) |
| `title` | — | Display name in the fleet monitor |
| `prompt` | — | The work itself |
| `depends_on` | `[]` | Task ids that must finish first |
| `isolation` | `"shared_tree"` | `shared_tree` = cooperative claims in one tree; `isolated` = dedicated git worktree |
| `claims` | `[]` | Workspace-relative paths this task locks while it runs (shared_tree mode) |

**Running one**

```bash
stella fleet --plan fleet/plan.toml --max-concurrency 4 --watch
stella --budget 8.00 fleet --plan fleet/plan.toml   # budget is fleet-wide
```

`--budget` is divided across the concurrency width and enforced fleet-wide —
the cap you set is the cap you spend. Results and the attempt ledger land in
`.stella/fleet.db` (see `stella stats` / `stella observe`).

| File | Scenario |
| --- | --- |
| [`plan.toml`](plan.toml) | Feature build-out: schema → API → UI, claims keeping the shared tree safe, docs isolated in a worktree |
| [`plan.json`](plan.json) | Same schema in JSON: a parallel cleanup sweep with a final integration task |

**Choosing isolation.** `shared_tree` + accurate `claims` is fastest for
tasks that touch disjoint files. Use `isolated` when a task rewrites broadly
or you want its diff reviewable as its own branch.
