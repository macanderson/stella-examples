#!/usr/bin/env python3
"""Track C rule 1, enforced: the three manifests differ only in their argv.

`doc:pipeline-as-plugins` §9 states the rule as *"identical manifests except
`[runtime].argv`. Diff them; they must differ in that ONE field and nothing
else."* If the Python plugin needs a manifest shape the Rust one does not, the
abstraction has leaked and that is a Track A bug, discovered here.

This script checks the rule two ways, because they fail differently:

1. **Structurally** — parse all three with `tomllib`, delete the argv the rule
   exempts, and require the remainder to be deep-equal. This catches a value
   that moved, a key that a reordering hid, and a comment-only difference is
   correctly ignored.
2. **Textually** — a line-by-line diff, so the "diff them" instruction is
   literally true and the count of differing lines is reported. This catches
   the thing the structural check cannot see: the grammar making an author
   write the same argv twice.

It also asserts the one thing that keeps the exemption honest: within a single
manifest, `[oracle].command.argv` and `[runtime].argv` must name the *same*
program. Two process declarations for one process is a finding, not a licence
to declare two different programs.
"""

from __future__ import annotations

import copy
import difflib
import sys
import tomllib
from pathlib import Path

PLUGINS = Path(__file__).resolve().parent.parent
IMPLEMENTATIONS = ["verify-rs", "verify-py", "verify-ts"]

# The one field the rule exempts, plus the redundant second copy of it that the
# grammar forces (see plugins/README.md § "What the grammar could not say").
EXEMPT = [("runtime", "argv"), ("oracle", "command", "argv")]


def strip_exempt(manifest: dict) -> dict:
    stripped = copy.deepcopy(manifest)
    for path in EXEMPT:
        node = stripped
        for key in path[:-1]:
            node = node.get(key, {})
        node.pop(path[-1], None)
    return stripped


def dig(manifest: dict, path: tuple[str, ...]):
    node = manifest
    for key in path:
        if not isinstance(node, dict) or key not in node:
            return None
        node = node[key]
    return node


def main() -> int:
    failures: list[str] = []
    parsed: dict[str, dict] = {}
    text: dict[str, list[str]] = {}

    for name in IMPLEMENTATIONS:
        path = PLUGINS / name / "plugin.toml"
        if not path.exists():
            failures.append(f"{name}: no plugin.toml at {path}")
            continue
        raw = path.read_text()
        text[name] = raw.splitlines(keepends=True)
        parsed[name] = tomllib.loads(raw)

    if failures:
        for failure in failures:
            print(f"FAIL {failure}")
        return 1

    # 1. Within each manifest, the two argv declarations name one program.
    for name, manifest in parsed.items():
        runtime_argv = dig(manifest, ("runtime", "argv"))
        oracle_argv = dig(manifest, ("oracle", "command", "argv"))
        if runtime_argv is None:
            failures.append(f"{name}: no [runtime].argv")
        elif runtime_argv != oracle_argv:
            failures.append(
                f"{name}: [runtime].argv {runtime_argv!r} != "
                f"[oracle].command.argv {oracle_argv!r} — one plugin, one program"
            )
        else:
            print(f"  ok   {name}: one program, declared twice: {runtime_argv}")

    # 2. Structural equality of everything the rule does not exempt.
    reference = IMPLEMENTATIONS[0]
    reference_stripped = strip_exempt(parsed[reference])
    for name in IMPLEMENTATIONS[1:]:
        other = strip_exempt(parsed[name])
        if other != reference_stripped:
            failures.append(
                f"{name}: manifest differs from {reference} outside [runtime].argv.\n"
                f"    {reference}: {reference_stripped}\n"
                f"    {name}: {other}"
            )
        else:
            print(f"  ok   {name}: structurally identical to {reference}")

    # 3. The literal diff, reported whether or not it fails.
    for name in IMPLEMENTATIONS[1:]:
        changed = [
            line
            for line in difflib.unified_diff(
                text[reference], text[name], reference, name, n=0
            )
            if line.startswith(("+", "-")) and not line.startswith(("+++", "---"))
        ]
        # Each exempt argv shows up as one removed and one added line.
        expected = 2 * len(EXEMPT)
        verdict = "ok  " if len(changed) == expected else "FAIL"
        print(f"  {verdict} {reference} vs {name}: {len(changed)} differing lines")
        for line in changed:
            print(f"         {line.rstrip()}")
        if len(changed) != expected:
            failures.append(
                f"{name}: {len(changed)} lines differ from {reference}, expected "
                f"exactly {expected} (the argv, written {len(EXEMPT)} times)"
            )

    if failures:
        print()
        for failure in failures:
            print(f"FAIL {failure}")
        return 1
    print("\nrule 1 holds: the manifests differ only in the argv naming the program")
    return 0


if __name__ == "__main__":
    sys.exit(main())
