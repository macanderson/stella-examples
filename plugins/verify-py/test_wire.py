#!/usr/bin/env python3
"""The Python plugin's own test. Stdlib `unittest`, no framework to install.

Two layers, because they fail differently:

- The **shared** layer delegates to `plugins/ci/conformance.py`, which grades
  this plugin against the same vectors and the same goldens the Rust and
  TypeScript plugins are graded against. That is the layer Track C cares about:
  one harness, three languages, one set of answers.
- The **in-process** layer imports `main` and calls its functions directly, to
  cover what a golden cannot reach cheaply — a test that outlives its budget, a
  signal-killed test, a malformed grant, and the two properties the wire itself
  is supposed to make impossible: no verdict, and no tamper finding.

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


def plan(program="test", args=("1", "=", "1"), baseline="failed"):
    """A test plan in the shape `main.test_plan` hands to `main.observe`."""
    return {"argv": [program] + list(args), "baseline": baseline}


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

    def test_the_plugin_reports_no_verdict_and_no_tamper_finding(self):
        # `judge` is the host's, and so is the tamper finding: `WrapperPoint`
        # has no `judge` variant, and `ObservedEvidence` has no `tamper` field
        # in any language (#3499). This test says both out loud in Python too.
        encoded = json.dumps(plugin.observe(plan(), "/tmp"))
        for forbidden in (
            "verdict",
            "done",
            "satisfied",
            "requirement",
            "unmet",
            "tamper",
        ):
            self.assertNotIn(forbidden, encoded)

    def test_a_command_that_outlives_its_budget_is_unobservable(self):
        original = plugin.TEST_TIMEOUT_SECS
        plugin.TEST_TIMEOUT_SECS = 1
        try:
            self.assertEqual(
                plugin.observe(plan("sleep", ("5",)), "/tmp"), plugin.unobserved()
            )
        finally:
            plugin.TEST_TIMEOUT_SECS = original

    def test_a_command_that_cannot_be_started_is_unobservable(self):
        self.assertEqual(
            plugin.observe(plan("stella-no-such-program-exists", ()), "/tmp"),
            plugin.unobserved(),
        )

    def test_a_root_that_does_not_exist_is_unobservable_not_a_run_elsewhere(self):
        # The root is where the test runs. A root that does not resolve is a
        # failure to run the test, never a licence to run it in this process's
        # own working directory — which would report a flip about the wrong tree.
        self.assertEqual(
            plugin.observe(plan(), "/tmp/stella-no-such-candidate-root"),
            plugin.unobserved(),
        )

    def test_a_signal_killed_test_reports_a_non_zero_code_not_a_negative_one(self):
        # `measurements` is a map of NON-NEGATIVE integers, and Python's
        # `returncode` is negative for a signal. Reporting -9 would be rejected
        # by the host's decoder; reporting 0 would be a false pass.
        evidence = plugin.observe(plan("sh", ("-c", "kill -9 $$")), "/tmp")
        self.assertGreater(evidence["measurements"]["test-command-exit-code"], 0)
        self.assertEqual(evidence["flip"], "not-achieved")

    def test_a_baseline_that_observed_nothing_is_neither_a_flip_nor_a_failure(self):
        # #860 arriving on the wire: `unobserved` and `not-run` never watched an
        # assertion, so a green run after them is not a flip — and it is not the
        # worker's failure either. Both wrong answers are excluded here.
        for baseline in ("unobserved", "not-run"):
            evidence = plugin.observe(plan(baseline=baseline), "/tmp")
            self.assertEqual(evidence["flip"], "unobservable", baseline)
            self.assertEqual(
                evidence["measurements"]["test-command-exit-code"],
                0,
                "the after side WAS observed, so its numbers are still reported",
            )

    def test_a_grant_whose_test_is_not_a_plan_is_refused(self):
        candidate = {"handle": "c", "root": "/tmp", "test": "pytest -q"}
        with self.assertRaises(plugin.Refusal):
            plugin.test_plan(candidate)

    def test_a_grant_with_no_test_is_nothing_granted_rather_than_a_refusal(self):
        # A host that created a candidate but has no test invocation to give has
        # granted nothing usable; the honest answer is `unobservable`, not a
        # crash and not a guess at a command.
        self.assertIsNone(plugin.test_plan({"handle": "c", "root": "/tmp"}))

    def test_a_baseline_outside_the_closed_set_is_refused(self):
        candidate = {
            "handle": "c",
            "root": "/tmp",
            "test": {"program": "true", "baseline": "flaky"},
        }
        with self.assertRaises(plugin.Refusal):
            plugin.test_plan(candidate)

    def test_the_plugin_reads_no_environment_variable_at_all(self):
        # The witness for #3498 in Python: the module's own source names no
        # environment access. It used to read VERIFY_TEST_COMMAND and
        # VERIFY_BASELINE_EXIT_CODE because the request carried neither the
        # candidate root nor the test invocation.
        with open(MAIN, encoding="utf-8") as handle:
            source = handle.read()
        for forbidden in ("os.environ", "getenv", "VERIFY_TEST_COMMAND"):
            self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
