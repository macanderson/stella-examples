#!/usr/bin/env python3
"""The Python plugin's own test. Stdlib `unittest`, no framework to install.

Two layers, because they fail differently:

- The **shared** layer delegates to `plugins/ci/conformance.py`, which grades
  this plugin against the same vectors and the same goldens the Rust and
  TypeScript plugins are graded against. That is the layer Track C cares about:
  one harness, three languages, one set of answers.
- The **in-process** layer imports `main` and calls its functions directly, to
  cover what a golden cannot reach cheaply — a test command that outlives its
  budget, a malformed grant, and the property that no verdict word ever appears
  in a response.

Run it:

    python3 plugins/verify-py/test_wire.py
"""

import json
import os
import subprocess
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
PLUGINS = os.path.dirname(HERE)
CONFORMANCE = os.path.join(PLUGINS, "ci", "conformance.py")
MAIN = os.path.join(HERE, "main.py")

sys.path.insert(0, HERE)
import main as plugin  # noqa: E402  (the path has to be set up first)


class SharedVectors(unittest.TestCase):
    """The same harness that grades the other two languages."""

    def test_the_shared_conformance_suite_passes(self):
        proc = subprocess.run(
            [sys.executable, CONFORMANCE, "--", sys.executable, MAIN],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=300,
        )
        self.assertEqual(
            proc.returncode, 0, proc.stdout.decode("utf-8", "replace")
        )


class InProcess(unittest.TestCase):
    """What a golden cannot reach cheaply."""

    ENVELOPE = json.dumps(
        {
            "point": "after_turn",
            "body": {
                "protocol_version": 1,
                "wrapper": "verify-v1",
                "round": 0,
                "goal": "make the failing test pass",
                "candidate": "candidate-1",
                "turn": {"completed": True},
            },
        }
    )

    def test_the_plugin_reports_no_verdict_field_at_all(self):
        # `judge` is the host's. `WrapperPoint` has no `judge` variant and
        # `AfterTurnResponse` has nowhere to put an answer, so this test is
        # what says so out loud in Python too.
        encoded = json.dumps(plugin.observe(["true"], 1))
        for forbidden in ("verdict", "done", "satisfied", "requirement", "unmet"):
            self.assertNotIn(forbidden, encoded)

    def test_a_command_that_outlives_its_budget_is_unobservable(self):
        original = plugin.TEST_TIMEOUT_SECS
        plugin.TEST_TIMEOUT_SECS = 1
        try:
            self.assertEqual(
                plugin.observe(["sleep", "5"], 1), plugin.unobserved()
            )
        finally:
            plugin.TEST_TIMEOUT_SECS = original

    def test_a_command_that_cannot_be_started_is_unobservable(self):
        self.assertEqual(
            plugin.observe(["stella-no-such-program-exists"], 1),
            plugin.unobserved(),
        )

    def test_a_signal_killed_test_reports_a_non_zero_code_not_a_negative_one(self):
        # `EvidenceSet.measurements` is a map of NON-NEGATIVE integers, and
        # Python's `returncode` is negative for a signal. Reporting -9 would be
        # rejected by the host's decoder; reporting 0 would be a false pass.
        evidence = plugin.observe(["sh", "-c", "kill -9 $$"], 1)
        self.assertGreater(evidence["measurements"]["test-command-exit-code"], 0)
        self.assertEqual(evidence["flip"], "not-achieved")

    def test_a_grant_that_is_not_an_argv_array_is_refused(self):
        os.environ[plugin.TEST_COMMAND_ENV] = '"cargo test"'
        os.environ[plugin.BASELINE_ENV] = "1"
        try:
            with self.assertRaises(plugin.Refusal):
                plugin.test_command()
        finally:
            del os.environ[plugin.TEST_COMMAND_ENV]
            del os.environ[plugin.BASELINE_ENV]

    def test_half_a_grant_is_nothing_granted_rather_than_a_refusal(self):
        # A host that set one name and not the other has granted nothing
        # usable; the honest answer is `unobserved`, not a crash.
        os.environ[plugin.TEST_COMMAND_ENV] = '["true"]'
        os.environ.pop(plugin.BASELINE_ENV, None)
        try:
            self.assertIsNone(plugin.test_command())
        finally:
            del os.environ[plugin.TEST_COMMAND_ENV]


if __name__ == "__main__":
    unittest.main(verbosity=2)
