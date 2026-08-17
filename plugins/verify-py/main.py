#!/usr/bin/env python3
"""`verify`, in Python. Standard library only — no Stella SDK, by rule.

`doc:pipeline-as-plugins` §9 rule 3: *"if a plugin CANNOT be written without an
SDK, the protocol is too complicated."* So this file imports `json`,
`subprocess`, `sys` and `time`, and nothing else — not even `os`, because since
#3498 there is no environment variable left to read. If you are reading it to
learn the protocol, the whole protocol is here.

# The wire

`crates/stella-plugin/src/wire.rs` is the contract; these are its shapes as a
Python author meets them. The host spawns `[runtime].argv` directly — no shell
(`doc:plugin-transport-spike` §5) — writes one JSON request on stdin, closes
it, and reads one JSON response from stdout:

    {"point": "after_turn", "body": {...AfterTurnRequest}}
 -> {"point": "after_turn", "body": {...AfterTurnResponse}}

Every table on that wire **denies unknown fields** — the envelope included,
since #3500 — so this program does too, at every level: the envelope, the body,
the candidate grant, the test plan and the turn outcome. A field the host does
not know, at a version the host accepts, is a typo, and a message that quietly
does nothing is worse than one that refuses. `protocol_version` rides on every
message and the contract is additive-only.

There is deliberately **no error variant** in `AfterTurnResponse`. A plugin
that cannot answer fails — non-zero exit, one line on stderr — and the host
substitutes `EvidenceSet::unobserved()`, which makes `judge` abstain instead of
blaming the worker for evidence nobody collected. So: stdout carries a valid
response or nothing at all.

# Every capability arrives in the request

The `candidate` field is a `CandidateGrant`: the handle, the canonical
workspace `root`, and — when the host has one to give — the `test` it would
run there, as a program, an argument vector, and what that same invocation
reported *before* the turn. This program reaches for nothing else: no
environment, no working directory, no git checkout, no terminal. That is what
lets it run unchanged under the CLI, under `stella-serve`, and inside an
application that embedded the loop.

# What this program may not do

It reports what it OBSERVED. It never reports a verdict, and it cannot report a
tamper finding — `ObservedEvidence` has no field for one, because snapshotting
witness-artifact identity is the host's job and a plugin vouching for its own
witness is exactly what the policy exists to prevent (#3499). `judge` is a host
function over the rule declared in `plugin.toml` and the evidence returned here
(`doc:wrapper-socket` §4), and `WrapperPoint` has no `judge` variant — so a
plugin cannot implement one in any language, which is what keeps "a
verification plugin quietly calls a model to decide done" impossible by
construction rather than by policy.

What Stella does **not** do is re-run this test or re-check this answer. The
flip and the measurements below are this plugin's own report; the host applies
the declared rule to them and owns the tamper finding beside them.
"""

import json
import subprocess
import sys
import time

# The version every message on this wire carries (`wire::PROTOCOL_VERSION`).
PROTOCOL_VERSION = 1

# The one point this plugin answers. `WrapperPoint` has exactly two —
# `before_turn` and `after_turn` — and this plugin has nothing to contribute
# before a turn runs, which `[loop].points` declares (#3501).
POINT = "after_turn"

# The fields each table on the request declares. Anything else is a typo, per
# the deny-unknown-fields rule the whole wire contract is written under.
AFTER_TURN_REQUEST_FIELDS = {
    "protocol_version",
    "wrapper",
    # Which declared stage this evidence is about. Optional, additive, and
    # deliberately not read here: this plugin declares one stage's worth of
    # behaviour, so the name changes nothing about what it observes. It is
    # listed because a field a host sends must not be refused as a typo.
    "stage",
    "round",
    "goal",
    "candidate",
    "turn",
}
CANDIDATE_GRANT_FIELDS = {"handle", "root", "test"}
TEST_PLAN_FIELDS = {"program", "args", "baseline"}
TURN_OUTCOME_FIELDS = {"completed", "answer", "tools", "changed_files"}

# What the same test invocation reported before the turn ran (`TestBaseline`).
# Four answers rather than an exit code, and the fourth is the whole reason:
# a run that timed out or could not find its toolchain never observed an
# assertion, and scoring its non-zero exit as red would let an infra failure
# satisfy a flip's precondition (#860).
BASELINE_NOT_RUN = "not-run"
BASELINE_PASSED = "passed"
BASELINE_FAILED = "failed"
BASELINE_UNOBSERVED = "unobserved"
BASELINES = (BASELINE_NOT_RUN, BASELINE_PASSED, BASELINE_FAILED, BASELINE_UNOBSERVED)

# What the plugin allows the test before killing it. Bounded well inside
# `[runtime].timeout_secs`, so the plugin reports a timeout rather than being
# killed mid-report by the host.
TEST_TIMEOUT_SECS = 240


class Refusal(Exception):
    """The plugin cannot answer. Exits non-zero; the host reports unobserved."""


def refuse(reason):
    raise Refusal(reason)


def unobserved():
    """What to report when nothing was observed at all.

    Deliberately not an empty set that reads as "nothing was wrong": an
    `unobservable` flip makes the host's `judge` abstain rather than credit or
    blame anyone for evidence that was never collected. There is no `tamper`
    key here in any language — see the module docstring.
    """
    return {"flip": "unobservable"}


def deny_unknown(table, allowed):
    """Refuse a table carrying a key the contract does not declare.

    One message shape for every table, and deliberately so: one golden refusal
    line has to cover all three languages, and Rust's `serde` reports the
    offending key without a path to the table it was found in.
    """
    unknown = sorted(set(table) - allowed)
    if unknown:
        refuse("the request denies unknown fields; got {}".format(", ".join(unknown)))
    return table


def read_request():
    """Decode `{"point": ..., "body": ...}` and return the `after_turn` body."""
    try:
        envelope = json.loads(sys.stdin.read())
    except ValueError:
        refuse("stdin was not a single JSON object")
    if not isinstance(envelope, dict):
        refuse("stdin was not a single JSON object")
    # The envelope denies unknown fields too. That used to make this plugin
    # stricter than the host's own decoder, which accepted and dropped an extra
    # key beside `point` and `body`; #3500 closed the gap on the host side.
    if set(envelope) - {"point", "body"}:
        refuse("the request envelope carried a field outside {point, body}")

    point = envelope.get("point")
    if point != POINT:
        refuse(
            "this plugin answers {} only; the host asked for {}".format(
                POINT, json.dumps(point)
            )
        )

    body = envelope.get("body")
    if not isinstance(body, dict):
        refuse("the request envelope carried no object body")
    deny_unknown(body, AFTER_TURN_REQUEST_FIELDS)

    version = body.get("protocol_version")
    if isinstance(version, bool) or version != PROTOCOL_VERSION:
        # `isinstance(True, int)` is True in Python, so a JSON `true` would
        # otherwise compare equal to version 1. It does not, and this says so.
        refuse(
            "this plugin speaks wrapper protocol_version {}; the host asked for {}".format(
                PROTOCOL_VERSION, json.dumps(version)
            )
        )

    turn = body.get("turn")
    if turn is not None:
        if not isinstance(turn, dict):
            refuse("turn must be an object")
        deny_unknown(turn, TURN_OUTCOME_FIELDS)
    return body


def grant(body):
    """The candidate workspace, or `None` when the host created none.

    `CandidateGrant` is the capability: `root` is where this plugin's own reads
    and its own test run happen, and every path it names on the way back is
    resolved against the handle by the host. A plugin that ignored the root and
    went somewhere else would have told the host nothing the host will act on.
    """
    candidate = body.get("candidate")
    if candidate is None:
        return None
    if not isinstance(candidate, dict):
        refuse("candidate must be a CandidateGrant object")
    deny_unknown(candidate, CANDIDATE_GRANT_FIELDS)
    root = candidate.get("root")
    if not isinstance(root, str) or not root:
        refuse("the candidate grant carried no root")
    return candidate


def test_plan(candidate):
    """The invocation to run and its pre-turn baseline, or `None`.

    `None` is "the host has no test invocation to give", never "run whatever
    you like": with no plan this plugin reports `unobservable` rather than
    guessing at a command.
    """
    if candidate is None:
        return None
    plan = candidate.get("test")
    if plan is None:
        return None
    if not isinstance(plan, dict):
        refuse("candidate.test must be a TestPlan object")
    deny_unknown(plan, TEST_PLAN_FIELDS)

    program = plan.get("program")
    if not isinstance(program, str) or not program:
        refuse("the test plan named no program")
    args = plan.get("args", [])
    if not isinstance(args, list) or not all(isinstance(part, str) for part in args):
        # argv, never a shell string — the host's own strict parser ran before
        # the grant was minted, so a plugin receives a program and its
        # arguments and never a line to hand to a shell.
        refuse("the test plan's args must be an array of strings")
    baseline = plan.get("baseline", BASELINE_NOT_RUN)
    if baseline not in BASELINES:
        refuse(
            "TestBaseline is a closed set {{{}}}; got {}".format(
                ", ".join(BASELINES), json.dumps(baseline)
            )
        )
    return {"argv": [program] + args, "baseline": baseline}


def flip_from(baseline, exit_code):
    """What the run says about the fail->pass flip, and nothing more.

    Three answers, and the third is the one #860 is about:

    - `failed` before and green after is the flip the witness contract asks
      for. Red before and still red after is `not-achieved`.
    - `passed` before means the test ran on both sides and did not flip.
    - `not-run` and `unobserved` observed **no assertion** before the turn, so
      neither answer above is available. Reporting `not-achieved` would blame
      the worker for an instrument that never ran; `unobservable` makes the
      host abstain, which is the honest half of what was seen.

    The plugin states the observation. The host's `FlipPolicy` decides what it
    is worth.
    """
    if baseline == BASELINE_FAILED:
        return "achieved" if exit_code == 0 else "not-achieved"
    if baseline == BASELINE_PASSED:
        return "not-achieved"
    return "unobservable"


def observe(plan, root):
    """Run the test in the candidate root and report what was seen."""
    started = time.monotonic()
    try:
        completed = subprocess.run(
            plan["argv"],
            cwd=root,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=TEST_TIMEOUT_SECS,
        )
    except (subprocess.TimeoutExpired, OSError):
        # Could not run it at all — an unreadable root, a program that is not
        # there, a run that outlived its budget. That is a claim about the
        # instrument, not about the work, so the flip is `unobservable` and the
        # host abstains.
        return unobserved()
    elapsed_ms = int((time.monotonic() - started) * 1000)

    # `measurements` is a map of non-negative integers, so a signal-killed test
    # (a negative returncode here, a null status in Node, `None` from
    # `ExitStatus::code` in Rust) reports as 1 — not a pass, and one number all
    # three languages can produce.
    exit_code = completed.returncode if completed.returncode >= 0 else 1
    return {
        "flip": flip_from(plan["baseline"], exit_code),
        # The after side WAS observed even when the baseline says nothing, so
        # the numbers are reported either way: `tests-pass` is decided by the
        # exit code and does not need the flip.
        "measurements": {
            "test-command-exit-code": exit_code,
            "test-duration-ms": elapsed_ms,
        },
    }


def main():
    try:
        body = read_request()
        candidate = grant(body)
        plan = test_plan(candidate)
        evidence = unobserved() if plan is None else observe(plan, candidate["root"])
    except Refusal as refusal:
        sys.stderr.write("verify: {}\n".format(refusal))
        return 1

    response = {
        "point": POINT,
        "body": {"protocol_version": PROTOCOL_VERSION, "evidence": evidence},
    }
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
