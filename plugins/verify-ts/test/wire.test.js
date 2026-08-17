// The TypeScript plugin's own test, in plain JavaScript against the built
// output — `node --test`, no test framework to install.
//
// Two layers, because they fail differently:
//
// - The **shared** layer delegates to `plugins/ci/conformance.py`, which grades
//   this plugin against the same vectors and the same goldens the Rust and
//   Python plugins are graded against. One harness, three languages, one set of
//   answers.
// - The **process** layer drives `dist/main.js` directly for what a golden
//   cannot reach cheaply — a signal-killed test, a malformed grant, and the
//   property that no verdict word ever appears in a response.
//
//     npm test        # builds first (see `pretest`), then runs this

const test = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const HERE = __dirname;
const MAIN = path.join(HERE, "..", "dist", "main.js");
const CONFORMANCE = path.join(HERE, "..", "..", "ci", "conformance.py");
const VECTOR_PATH = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

const ENVELOPE = JSON.stringify({
  point: "after_turn",
  body: {
    protocol_version: 1,
    wrapper: "verify-v1",
    round: 0,
    goal: "make the failing test pass",
    candidate: "candidate-1",
    turn: { completed: true },
  },
});

/**
 * Spawn the plugin the way a host does: argv, stdin, and a default-deny
 * environment holding exactly what the caller granted.
 */
function ask(granted) {
  const result = spawnSync(process.execPath, [MAIN], {
    input: ENVELOPE,
    timeout: 60_000,
    env: { PATH: VECTOR_PATH, ...granted },
  });
  return {
    status: result.status,
    stdout: result.stdout.toString("utf8"),
    stderr: result.stderr.toString("utf8"),
  };
}

function evidenceFrom(answer) {
  assert.strictEqual(answer.status, 0, answer.stderr);
  return JSON.parse(answer.stdout).body.evidence;
}

test("the shared conformance suite passes", () => {
  const result = spawnSync("python3", [CONFORMANCE, "--", process.execPath, MAIN], {
    timeout: 300_000,
  });
  assert.strictEqual(
    result.status,
    0,
    `${result.stdout?.toString("utf8")}${result.stderr?.toString("utf8")}`,
  );
});

test("the plugin reports no verdict field at all", () => {
  // `judge` is the host's. `WrapperPoint` has no `judge` variant and
  // `AfterTurnResponse` has nowhere to put an answer.
  const answer = ask({
    VERIFY_TEST_COMMAND: '["true"]',
    VERIFY_BASELINE_EXIT_CODE: "1",
  });
  assert.strictEqual(answer.status, 0, answer.stderr);
  for (const forbidden of ["verdict", "done", "satisfied", "requirement", "unmet"]) {
    assert.ok(!answer.stdout.includes(forbidden), `${answer.stdout} leaked ${forbidden}`);
  }
});

test("a signal-killed test reports a non-zero code, never a null one", () => {
  // `EvidenceSet.measurements` is a map of NON-NEGATIVE integers, and Node's
  // `status` is null for a signal. Reporting null would be rejected by the
  // host's decoder; reporting 0 would be a false pass.
  const evidence = evidenceFrom(
    ask({
      VERIFY_TEST_COMMAND: '["sh","-c","kill -9 $$"]',
      VERIFY_BASELINE_EXIT_CODE: "1",
    }),
  );
  assert.ok(evidence.measurements["test-command-exit-code"] > 0);
  assert.strictEqual(evidence.flip, "not-achieved");
});

test("a command that cannot be started is unobservable, not a pass", () => {
  const evidence = evidenceFrom(
    ask({
      VERIFY_TEST_COMMAND: '["stella-no-such-program-exists"]',
      VERIFY_BASELINE_EXIT_CODE: "1",
    }),
  );
  assert.deepStrictEqual(evidence, { flip: "unobservable", tamper: "not-checked" });
});

test("a grant that is not an argv array is refused", () => {
  const answer = ask({
    VERIFY_TEST_COMMAND: '"npm test"',
    VERIFY_BASELINE_EXIT_CODE: "1",
  });
  assert.notStrictEqual(answer.status, 0);
  assert.match(answer.stderr, /VERIFY_TEST_COMMAND must be a non-empty array of strings/);
});

test("half a grant is nothing granted rather than a refusal", () => {
  // A host that set one name and not the other has granted nothing usable;
  // the honest answer is `unobserved`, not a crash.
  const evidence = evidenceFrom(ask({ VERIFY_TEST_COMMAND: '["true"]' }));
  assert.deepStrictEqual(evidence, { flip: "unobservable", tamper: "not-checked" });
});
