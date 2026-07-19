---
name: protect-migrations
description: Applied database migrations are immutable
guard-deny-path: "migrations/**"
---

Files under `migrations/` are already applied to real databases. Editing or
deleting one desynchronizes every environment that ran it. Schema changes
are always a **new** migration file created outside this directory's history
— generate it with the project's migration tool, then it may be added.
