# CLAUDE.md

## Issue fields and reflection

Mac set this on 2026-09-28 for every repository. Every issue in Mac's repositories belongs on the `All issues` project board in the `macanderson` account. The board carries six fields. Keep all six correct on every issue you work on.

| Field | Values | Meaning |
|---|---|---|
| Prompt | Text | The prompt that starts an agent on the work |
| Model Tier | Ultra, Pro, Standard, Lite | The model tier the work needs |
| Size | XS, S, M, L, XL | The size of the change |
| `agent_mins_est` | Number | Agent minutes the work should take |
| `agent_mins` | Number | Agent minutes the work took |
| Resolution | Shipped, Won't ship, Duplicate | How the issue closed |

- **Add the issue to the board when you file it.** Set Prompt, Model Tier, Size, and `agent_mins_est` at the same time.
- **Stamp your minutes when your run ends.** Add the minutes your run spent on the issue to `agent_mins`. Add to the value already there, because several runs can share one issue.
- **Write a reflection when your run ends.** Post it as a comment on the issue. Say what shipped, how `agent_mins` compared with `agent_mins_est` and why, and what the next agent should know.
- **Set Resolution when the issue closes.**
- **Fix any field you find wrong** on any issue you touch.

Writing to the board needs a gh token with the `project` scope. These commands find the board and set a field:

```sh
gh project list --owner macanderson                                  # the board titled "All issues"
gh project field-list <number> --owner macanderson --format json     # field and option ids
gh project item-add <number> --owner macanderson --url <issue-url>   # prints the item id
gh project item-edit --project-id <project-id> --id <item-id> --field-id <field-id> --number 42
```
