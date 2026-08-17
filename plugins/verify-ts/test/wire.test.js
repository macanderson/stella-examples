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
//   cannot reach cheaply — a signal-killed test, a malformed grant, and the two
//   properties the wire is supposed to make impossible: no verdict, and no
//   tamper finding.
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

/**
 * One `after_turn` request, built the way the host builds it: everything the
 * plugin acts on rides in the message.
 */
function request({ root = "/tmp", test: plan = {} } = {}) {
  const grant = { handle: "candidate-1", root };
  if (plan !== null) {
    grant.test = {
      program: "test",
      args: ["1", "=", "1"],
      baseline: "failed",
      ...plan,
    };
  }
  return JSON.stringify({
    point: "after_turn",
    body: {
      protocol_version: 1,
      wrapper: "verify-v1",
      round: 0,
      goal: "make the failing test pass",
      candidate: grant,
      turn: { completed: true },
    },
  });
}

/**
 * Spawn the plugin the way a host does: argv, stdin, and an environment holding
 * `PATH` and nothing else — which is the whole `[runtime].env` allowlist now
 * that the grant carries the root and the test invocation (#3498).
 */
function ask(input) {
  const result = spawnSync(process.execPath, [MAIN], {
    input,
    timeout: 60_000,
    env: { PATH: VECTOR_PATH },
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

test("the plugin reports no verdict and no tamper finding", () => {
  // `judge` is the host's, and so is the tamper finding: `WrapperPoint` has no
  // `judge` variant, and `ObservedEvidence` has no `tamper` field in any
  // language (#3499).
  const answer = ask(request());
  assert.strictEqual(answer.status, 0, answer.stderr);
  for (const forbidden of [
    "verdict",
    "done",
    "satisfied",
    "requirement",
    "unmet",
    "tamper",
  ]) {
    assert.ok(!answer.stdout.includes(forbidden), `${answer.stdout} leaked ${forbidden}`);
  }
});

test("a signal-killed test reports a non-zero code, never a null one", () => {
  // `measurements` is a map of NON-NEGATIVE integers, and Node's `status` is
  // null for a signal. Reporting null would be rejected by the host's decoder;
  // reporting 0 would be a false pass.
  const evidence = evidenceFrom(
    ask(request({ test: { program: "sh", args: ["-c", "kill -9 $$"] } })),
  );
  assert.ok(evidence.measurements["test-command-exit-code"] > 0);
  assert.strictEqual(evidence.flip, "not-achieved");
});

test("a command that cannot be started is unobservable, not a pass", () => {
  const evidence = evidenceFrom(
    ask(request({ test: { program: "stella-no-such-program-exists", args: [] } })),
  );
  assert.deepStrictEqual(evidence, { flip: "unobservable" });
});

test("a root that does not exist is unobservable, not a run somewhere else", () => {
  // The root is where the test runs. A root that does not resolve is a failure
  // to run the test, never a licence to run it in this process's own working
  // directory — which would report a flip about the wrong tree.
  const evidence = evidenceFrom(
    ask(request({ root: "/tmp/stella-no-such-candidate-root" })),
  );
  assert.deepStrictEqual(evidence, { flip: "unobservable" });
});

test("a baseline that observed nothing is neither a flip nor a failure", () => {
  // #860 arriving on the wire: `unobserved` and `not-run` never watched an
  // assertion, so a green run after them is not a flip — and it is not the
  // worker's failure either.
  for (const baseline of ["unobserved", "not-run"]) {
    const evidence = evidenceFrom(ask(request({ test: { baseline } })));
    assert.strictEqual(evidence.flip, "unobservable", baseline);
    assert.strictEqual(
      evidence.measurements["test-command-exit-code"],
      0,
      "the after side WAS observed, so its numbers are still reported",
    );
  }
});

test("a grant whose test is not a plan is refused", () => {
  const answer = ask(
    JSON.stringify({
      point: "after_turn",
      body: {
        protocol_version: 1,
        candidate: { handle: "c", root: "/tmp", test: "npm test" },
      },
    }),
  );
  assert.notStrictEqual(answer.status, 0);
  assert.match(answer.stderr, /candidate\.test must be a TestPlan object/);
});

test("a grant with no test is nothing granted rather than a refusal", () => {
  // A host that created a candidate but has no test invocation to give has
  // granted nothing usable; the honest answer is `unobservable`, not a crash
  // and not a guess at a command.
  const evidence = evidenceFrom(ask(request({ test: null })));
  assert.deepStrictEqual(evidence, { flip: "unobservable" });
});
