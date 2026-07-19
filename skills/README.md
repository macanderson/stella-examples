# Skills

A skill is a reusable how-to document the agent loads when the task calls
for it — invoked directly as `/skill-name`, or cited by the engine. Stella
also **auto-promotes** recurring lessons from its reflection loop into
skills, so this directory grows on its own over time. Reference:
[Agent tools → Skills](https://stella.oxagen.sh/docs/agent-tools/skills).

**Where they live**

- Project: `<repo>/.stella/skills/<slug>/SKILL.md` (flat `<slug>.md` works too)
- User: `~/.config/stella/skills/`

**Frontmatter**: `name`, `description`, and an `origin` marker
(`workspace` for hand-written skills like these; Stella stamps `auto` on
promoted ones and `installed` on fetched ones).

| Skill | The idea |
| --- | --- |
| [`conventional-commits/`](conventional-commits/SKILL.md) | House rules for commit messages, in a form the agent applies verbatim |
| [`release-checklist/`](release-checklist/SKILL.md) | An ordered, verifiable release procedure the agent can execute |

On name collisions, commands shadow skills, and skills shadow agents.
