#!/usr/bin/env python3
"""`verify`, in Python. Standard library only — no Stella SDK, by rule.

`doc:pipeline-as-plugins` §9 rule 3: *"if a plugin CANNOT be written without an
SDK, the protocol is too complicated."* So this file imports `json`, `os`,
`subprocess`, `sys` and `time`, and nothing else. If you are reading it to
learn the protocol, the whole protocol is here.

# The wire

`crates/stella-plugin/src/wire.rs` is the contract; these are its shapes as a
Python author meets them. The host spawns `[runtime].argv` directly — no shell
(`doc:plugin-transport-spike` §5) — writes one JSON request on stdin, closes
it, and reads one JSON response from stdout:

    {"point": "after_turn", "body": {...AfterTurnRequest}}
 -> {"point": "after_turn", "body": {...AfterTurnResponse}}

Every table on that wire **denies unknown fields**, so this program does too:
a field the host does not know, at a version the host accepts, is a typo, and
a manifest or a message that quietly does nothing is worse than one that
refuses. `protocol_version` rides on every message and the contract is
additive-only.

There is deliberately **no error variant** in `AfterTurnResponse`. A plugin
that cannot answer fails — non-zero exit, one line on stderr — and the host
substitutes `EvidenceSet::unobserved()`, which makes `judge` abstain instead of
blaming the worker for evidence nobody collected. So: stdout carries a valid
response or nothing at all.

# What this program may not do

It reports evidence. It never reports a verdict. `judge` is a host function
over the rule declared in `plugin.toml` and the `EvidenceSet` returned here
(`doc:wrapper-socket` §4), and `WrapperPoint` has no `judge` variant — so a
plugin cannot implement one in any language, which is what keeps "a
verification plugin quietly calls a model to decide done" impossible by
construction rather than by policy.
"""

import json
import os
import subprocess
import sys
import time

# The version every message on this wire carries (`wire::PROTOCOL_VERSION`).
PROTOCOL_VERSION = 1

# The one point this plugin answers. `WrapperPoint` has exactly two —
# `before_turn` and `after_turn` — and this plugin has nothing to contribute
# before a turn runs.
POINT = "after_turn"

# The fields `AfterTurnRequest` declares. Anything else is a typo, per the
# deny-unknown-fields rule the whole wire contract is written under.
AFTER_TURN_REQUEST_FIELDS = {
    "protocol_version",
    "wrapper",
    "round",
    "goal",
    "candidate",
    "turn",
}

# How the plugin learns which test to run. THIS IS A GAP, NOT A DESIGN:
# `AfterTurnRequest` carries a `CandidateHandle` but no channel to invoke
# `CandidateOp::RunTest` with it and no root path, so an out-of-process plugin
# has no in-band way to be handed a test command. Both names are declared in
# `[runtime].env`, so they are default-deny and visible at install consent —
# but they are out-of-band, which is exactly the finding Track C exists to
# produce. See plugins/README.md § "What the wire could not say".
TEST_COMMAND_ENV = "VERIFY_TEST_COMMAND"
BASELINE_ENV = "VERIFY_BASELINE_EXIT_CODE"

# What the plugin allows the test command before killing it. Bounded well
# inside `[runtime].timeout_secs`, so the plugin reports a timeout rather than
# being killed mid-report by the host.
TEST_TIMEOUT_SECS = 240


class Refusal(Exception):
    """The plugin cannot answer. Exits non-zero; the host reports unobserved."""


def refuse(reason):
    raise Refusal(reason)


def unobserved():
    """`EvidenceSet::unobserved()` — the honest answer when nothing was seen.

    Deliberately not an empty set that reads as "nothing was wrong": an
    `unobservable` flip makes the host's `judge` abstain rather than credit or
    blame anyone for evidence that was never collected.
    """
    return {"flip": "unobservable", "tamper": "not-checked"}


def read_request():
    """Decode `{"point": ..., "body": ...}` and return the `after_turn` body."""
    try:
        envelope = json.loads(sys.stdin.read())
    except ValueError:
        refuse("stdin was not a single JSON object")
    if not isinstance(envelope, dict):
        refuse("stdin was not a single JSON object")
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
    unknown = sorted(set(body) - AFTER_TURN_REQUEST_FIELDS)
    if unknown:
        refuse("AfterTurnRequest denies unknown fields; got {}".format(", ".join(unknown)))

    version = body.get("protocol_version")
    if isinstance(version, bool) or version != PROTOCOL_VERSION:
        # `isinstance(True, int)` is True in Python, so a JSON `true` would
        # otherwise compare equal to version 1. It does not, and this says so.
        refuse(
            "this plugin speaks wrapper protocol_version {}; the host asked for {}".format(
                PROTOCOL_VERSION, json.dumps(version)
            )
        )
    return body


def test_command():
    """The argv and the baseline, or `None` when the host supplied neither."""
    raw_argv = os.environ.get(TEST_COMMAND_ENV)
    raw_baseline = os.environ.get(BASELINE_ENV)
    if raw_argv is None or raw_baseline is None:
        return None
    try:
        argv = json.loads(raw_argv)
    except ValueError:
        refuse("{} is not JSON; it must be an argv array".format(TEST_COMMAND_ENV))
    if not isinstance(argv, list) or not argv or not all(
        isinstance(part, str) for part in argv
    ):
        refuse("{} must be a non-empty array of strings".format(TEST_COMMAND_ENV))
    try:
        baseline = int(raw_baseline)
    except ValueError:
        refuse("{} is not an integer exit code".format(BASELINE_ENV))
    return argv, baseline


def observe(argv, baseline):
    """Run the test command and report what was seen, never what it means."""
    started = time.monotonic()
    try:
        completed = subprocess.run(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=TEST_TIMEOUT_SECS,
        )
    except (subprocess.TimeoutExpired, OSError):
        # Could not run it at all — which is a claim about the instrument, not
        # about the work, so the flip is `unobservable` and the host abstains.
        return unobserved()
    elapsed_ms = int((time.monotonic() - started) * 1000)

    return {
        # Red before the work and green after it is the flip the witness
        # contract asks for. Anything else is `not-achieved`; the plugin
        # states the observation and the host's FlipPolicy decides what it
        # is worth.
        "flip": "achieved" if baseline != 0 and completed.returncode == 0 else "not-achieved",
        # The host snapshots witness-artifact identity, not the plugin — an
        # out-of-process plugin has nothing to compare, and saying `clean`
        # would be vouching for its own work. See plugins/README.md.
        "tamper": "not-checked",
        "measurements": {
            # `EvidenceSet.measurements` is a map of non-negative integers, so
            # a signal-killed test (a negative returncode here, a null status
            # in Node, `None` from `ExitStatus::code` in Rust) reports as 1 —
            # not a pass, and one number all three languages can produce.
            "test-command-exit-code": max(completed.returncode, 0)
            if completed.returncode >= 0
            else 1,
            "test-duration-ms": elapsed_ms,
        },
    }


def main():
    try:
        read_request()
        granted = test_command()
        evidence = unobserved() if granted is None else observe(*granted)
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
