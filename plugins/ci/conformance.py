#!/usr/bin/env python3
"""Run one `verify` implementation against the shared wire vectors.

The whole point of Track C is that three programs written in three languages
are interchangeable to the host. This script is what makes that a fact rather
than a claim: it feeds *the same* `plugins/testdata/*.request.json` to whatever
program you hand it and grades the result against *the same* goldens. One
harness, one set of vectors, three languages.

    ./plugins/ci/conformance.py -- python3 plugins/verify-py/main.py
    ./plugins/ci/conformance.py -- node plugins/verify-ts/dist/main.js
    ./plugins/ci/conformance.py -- plugins/verify-rs/bin/verify-rs

Each vector `NN-name` is exactly two files:

| file | meaning |
|---|---|
| `NN-name.request.json` | written to the plugin's stdin |
| `NN-name.expected.json` | the plugin must exit 0 and print this on stdout |
| `NN-name.refusal.txt`   | the plugin must exit non-zero and print this on stderr |

A vector carries an `expected.json` or a `refusal.txt`, never both. The refusal
case is not an afterthought: `AfterTurnResponse` has no error variant on
purpose, so a plugin that cannot answer *fails*, and the host substitutes
`EvidenceSet::unobserved()`. Grading the failure path is how we check all three
fail the same way.

**Every plugin runs with `PATH` and nothing else**, which is the #3498 result
stated as a harness rule. There used to be a third file per vector — an
`env.json` naming `VERIFY_TEST_COMMAND` and `VERIFY_BASELINE_EXIT_CODE` —
because the request carried no candidate root and no test invocation, so the
only way to hand a plugin its test was out of band. The request carries both
now (`CandidateGrant`), so a vector that needed an environment would be a bug in
the plugin rather than a fixture: it would mean the plugin reached for something
the host did not send.

One value is normalized before comparison: `test-duration-ms`, which is wall
clock and cannot be golden. It is asserted to be a non-negative integer and
then replaced with the golden's value.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

TESTDATA = Path(__file__).resolve().parent.parent / "testdata"
WALL_CLOCK_MEASUREMENT = "test-duration-ms"


def normalize(response: object, golden: object) -> object:
    """Replace the one wall-clock measurement with the golden's value.

    Raises `AssertionError` if the program reported something that is not a
    non-negative integer there, so the normalization can never hide a bug in
    the field it normalizes.
    """
    if not isinstance(response, dict):
        return response
    measurements = (
        response.get("body", {}).get("evidence", {}).get("measurements")
        if isinstance(response.get("body"), dict)
        else None
    )
    if not isinstance(measurements, dict) or WALL_CLOCK_MEASUREMENT not in measurements:
        return response
    reported = measurements[WALL_CLOCK_MEASUREMENT]
    assert isinstance(reported, int) and not isinstance(reported, bool), (
        f"{WALL_CLOCK_MEASUREMENT} must be an integer, got {reported!r}"
    )
    assert reported >= 0, f"{WALL_CLOCK_MEASUREMENT} must be >= 0, got {reported}"
    golden_value = (
        golden.get("body", {}).get("evidence", {}).get("measurements", {}).get(
            WALL_CLOCK_MEASUREMENT, 0
        )
    )
    measurements[WALL_CLOCK_MEASUREMENT] = golden_value
    return response


def sibling(request_path: Path, suffix: str) -> Path:
    return request_path.with_name(request_path.name.replace(".request.json", suffix))


def run_vector(argv: list[str], request_path: Path) -> tuple[bool, str]:
    # Default-deny, exactly like `[runtime].env` — and the whole allowlist is
    # `PATH`, because that is all these manifests declare. The plugin needs it
    # to find its interpreter and the test program; everything else it acts on
    # arrives in the request. A plugin that quietly read an inherited variable
    # would pass here and fail on a host that withheld it, so nothing is
    # inherited.
    env = {"PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"}

    proc = subprocess.run(
        argv,
        input=request_path.read_bytes(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        timeout=120,
    )

    refusal_path = sibling(request_path, ".refusal.txt")
    if refusal_path.exists():
        if proc.returncode == 0:
            return False, (
                "expected a refusal (non-zero exit) and got exit 0 with stdout "
                f"{proc.stdout.decode('utf-8', 'replace').strip()!r}"
            )
        actual = proc.stderr.decode("utf-8", "replace").strip()
        expected = refusal_path.read_text().strip()
        if actual != expected:
            return False, f"stderr did not match\n  expected: {expected}\n  actual:   {actual}"
        if proc.stdout.strip():
            return False, "a refusing plugin must print nothing on stdout"
        return True, ""

    golden_path = sibling(request_path, ".expected.json")
    if not golden_path.exists():
        return False, f"vector has neither {golden_path.name} nor {refusal_path.name}"
    golden = json.loads(golden_path.read_text())

    if proc.returncode != 0:
        return False, (
            f"exit {proc.returncode}\n"
            f"  stderr: {proc.stderr.decode('utf-8', 'replace').strip()}"
        )
    try:
        actual = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return False, (
            f"stdout was not JSON ({exc})\n"
            f"  stdout: {proc.stdout.decode('utf-8', 'replace').strip()!r}"
        )
    try:
        actual = normalize(actual, golden)
    except AssertionError as exc:
        return False, str(exc)
    if actual != golden:
        return False, (
            "response did not match the golden\n"
            f"  expected: {json.dumps(golden, sort_keys=True)}\n"
            f"  actual:   {json.dumps(actual, sort_keys=True)}"
        )
    return True, ""


def main(argv: list[str]) -> int:
    if argv and argv[0] == "--":
        argv = argv[1:]
    if not argv:
        print(__doc__)
        return 2

    vectors = sorted(TESTDATA.glob("*.request.json"))
    if not vectors:
        print(f"no vectors found under {TESTDATA}", file=sys.stderr)
        return 2

    failures = 0
    for vector in vectors:
        ok, detail = run_vector(argv, vector)
        if ok:
            print(f"  ok   {vector.name}")
        else:
            failures += 1
            print(f"  FAIL {vector.name}: {detail}")

    program = " ".join(argv)
    if failures:
        print(f"\n{failures}/{len(vectors)} vectors failed for `{program}`")
        return 1
    print(f"\n{len(vectors)}/{len(vectors)} vectors passed for `{program}`")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
