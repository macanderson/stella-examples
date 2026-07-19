---
name: no-force-push
description: Force-pushes are blocked at the tool boundary
guard-deny-command: "*git push*--force*"
---

Force-pushing rewrites shared history. It is never part of an agent task in
this workspace. If a branch needs rewriting, stop and hand the decision back
to a human — with the exact commands you would have run and why.
