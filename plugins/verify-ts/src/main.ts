/**
 * `verify`, in TypeScript. No dependencies — not even `@types/node`.
 *
 * `doc:pipeline-as-plugins` §9 rule 3 asks for the standard library and a JSON
 * parser. Node's `JSON` is the parser and `node:fs` / `node:child_process` are
 * the standard library, so `package.json` has an empty `dependencies` and one
 * `devDependencies` entry: the TypeScript compiler, which is a build tool and
 * not an SDK. The handful of ambient declarations this file needs are
 * hand-written in `src/node-min.d.ts`. If you would rather install nothing at
 * all, this file is close enough to plain JavaScript that stripping the types
 * is a five-minute job.
 *
 * The build step, shown rather than assumed:
 *
 *     npm install          # fetches tsc, nothing else
 *     npm run build        # tsc -> dist/main.js, which [runtime].argv names
 *
 * # The wire
 *
 * `crates/stella-plugin/src/wire.rs` is the contract. The host spawns
 * `[runtime].argv` directly — no shell — writes one JSON request on stdin,
 * closes it, and reads one JSON response from stdout:
 *
 *     {"point": "after_turn", "body": {...AfterTurnRequest}}
 *  -> {"point": "after_turn", "body": {...AfterTurnResponse}}
 *
 * Every table on that wire denies unknown fields — the envelope included,
 * since #3500 — so this program does too, at every level: the envelope, the
 * body, the candidate grant, the test plan and the turn outcome. And
 * `AfterTurnResponse` deliberately has no error variant: a plugin that cannot
 * answer *fails* — non-zero exit, one line on stderr — and the host
 * substitutes `EvidenceSet::unobserved()`, which makes `judge` abstain rather
 * than blame the worker for evidence nobody collected.
 *
 * # Every capability arrives in the request
 *
 * `candidate` is a `CandidateGrant`: the handle, the canonical workspace
 * `root`, and — when the host has one to give — the `test` it would run there,
 * as a program, an argument vector, and what that same invocation reported
 * *before* the turn. This program reads no environment variable at all. It used
 * to read two, because the request carried neither a root nor a test (#3498).
 *
 * # What this program may not do
 *
 * It reports what it observed. It never reports a verdict, and it cannot report
 * a tamper finding — `ObservedEvidence` has no field for one, because
 * snapshotting witness-artifact identity is the host's job (#3499). What Stella
 * does not do is re-run this test or re-check this answer: it applies the rule
 * `plugin.toml` declares to what this plugin reported.
 *
 * Everything semantic here is identical to `plugins/verify-py/main.py` and
 * `plugins/verify-rs/src/lib.rs`. Diff them: the differences are three
 * languages, not three protocols.
 */

const fs = require("node:fs");
const childProcess = require("node:child_process");

/** The version every message on this wire carries (`wire::PROTOCOL_VERSION`). */
const PROTOCOL_VERSION = 1;

/**
 * The one point this plugin answers. `WrapperPoint` has exactly two —
 * `before_turn` and `after_turn` — and this plugin has nothing to contribute
 * before a turn runs, which `[loop].points` declares (#3501).
 */
const POINT = "after_turn";

/** The fields each table on the request declares. Anything else is a typo. */
const AFTER_TURN_REQUEST_FIELDS = [
  "protocol_version",
  "wrapper",
  // Which declared stage this evidence is about. Optional, additive, and
  // deliberately not read here: this plugin declares one stage's worth of
  // behaviour, so the name changes nothing about what it observes. It is
  // listed because a field a host sends must not be refused as a typo.
  "stage",
  "round",
  "goal",
  "candidate",
  "turn",
];
const CANDIDATE_GRANT_FIELDS = ["handle", "root", "test"];
const TEST_PLAN_FIELDS = ["program", "args", "baseline"];
const TURN_OUTCOME_FIELDS = ["completed", "answer", "tools", "changed_files"];

/**
 * What the same test invocation reported before the turn ran (`TestBaseline`).
 * Four answers rather than an exit code, and the fourth is the whole reason: a
 * run that timed out or could not find its toolchain never observed an
 * assertion, and scoring its non-zero exit as red would let an infra failure
 * satisfy a flip's precondition (#860).
 */
const BASELINES = ["not-run", "passed", "failed", "unobserved"];

/**
 * What the plugin allows the test before killing it. Bounded well inside
 * `[runtime].timeout_secs`, so the plugin reports a result rather than being
 * killed mid-report by the host.
 */
const TEST_TIMEOUT_SECS = 240;

interface EvidenceSet {
  flip: string;
  measurements?: Record<string, number>;
}

interface TestPlan {
  argv: string[];
  baseline: string;
}

/** The candidate workspace, as this plugin needs it: a root, and maybe a test. */
interface Grant {
  root: string;
  test: unknown;
}

/** The plugin cannot answer. Exits non-zero; the host reports unobserved. */
class Refusal extends Error {}

function refuse(reason: string): never {
  throw new Refusal(reason);
}

/**
 * What to report when nothing was observed at all.
 *
 * Deliberately not an empty set that reads as "nothing was wrong": an
 * `unobservable` flip makes the host's `judge` abstain rather than credit or
 * blame anyone for evidence that was never collected. There is no `tamper` key
 * here in any language — see the module comment.
 */
function unobserved(): EvidenceSet {
  return { flip: "unobservable" };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Refuse a table carrying a key the contract does not declare.
 *
 * One message shape for every table, and deliberately so: one golden refusal
 * line has to cover all three languages, and Rust's `serde` reports the
 * offending key without a path to the table it was found in.
 */
function denyUnknown(table: Record<string, unknown>, allowed: string[]): void {
  const unknown = Object.keys(table)
    .filter((key) => !allowed.includes(key))
    .sort();
  if (unknown.length > 0) {
    refuse(`the request denies unknown fields; got ${unknown.join(", ")}`);
  }
}

/**
 * Render a value exactly as the Rust and Python implementations do, so one
 * golden refusal line covers all three. `undefined` is spelled `null`, because
 * that is what an absent field renders as in the other two.
 */
function asJson(value: unknown): string {
  return value === undefined ? "null" : JSON.stringify(value);
}

/** Decode `{"point": ..., "body": ...}` and return the `after_turn` body. */
function readRequest(raw: string): Record<string, unknown> {
  let envelope: unknown;
  try {
    envelope = JSON.parse(raw);
  } catch {
    refuse("stdin was not a single JSON object");
  }
  if (!isPlainObject(envelope)) {
    refuse("stdin was not a single JSON object");
  }
  // The envelope denies unknown fields too. That used to make this plugin
  // stricter than the host's own decoder, which accepted and dropped an extra
  // key beside `point` and `body`; #3500 closed the gap on the host side.
  if (Object.keys(envelope).some((key) => key !== "point" && key !== "body")) {
    refuse("the request envelope carried a field outside {point, body}");
  }
  if (envelope["point"] !== POINT) {
    refuse(
      `this plugin answers ${POINT} only; ` +
        `the host asked for ${asJson(envelope["point"])}`,
    );
  }
  const body = envelope["body"];
  if (!isPlainObject(body)) {
    refuse("the request envelope carried no object body");
  }
  denyUnknown(body, AFTER_TURN_REQUEST_FIELDS);
  const version = body["protocol_version"];
  if (version !== PROTOCOL_VERSION) {
    refuse(
      `this plugin speaks wrapper protocol_version ${PROTOCOL_VERSION}; ` +
        `the host asked for ${asJson(version)}`,
    );
  }
  const turn = body["turn"];
  if (turn !== undefined) {
    if (!isPlainObject(turn)) {
      refuse("turn must be an object");
    }
    denyUnknown(turn, TURN_OUTCOME_FIELDS);
  }
  return body;
}

/**
 * The candidate workspace, or `null` when the host created none.
 *
 * `CandidateGrant` is the capability: `root` is where this plugin's own reads
 * and its own test run happen, and every path it names on the way back is
 * resolved against the handle by the host. A plugin that ignored the root and
 * went somewhere else would have told the host nothing the host will act on.
 */
function grant(body: Record<string, unknown>): Grant | null {
  const candidate = body["candidate"];
  if (candidate === undefined || candidate === null) {
    return null;
  }
  if (!isPlainObject(candidate)) {
    refuse("candidate must be a CandidateGrant object");
  }
  denyUnknown(candidate, CANDIDATE_GRANT_FIELDS);
  const root = candidate["root"];
  if (typeof root !== "string" || root.length === 0) {
    refuse("the candidate grant carried no root");
  }
  return { root, test: candidate["test"] };
}

/**
 * The invocation to run and its pre-turn baseline, or `null`.
 *
 * `null` is "the host has no test invocation to give", never "run whatever you
 * like": with no plan this plugin reports `unobservable` rather than guessing
 * at a command.
 */
function testPlan(candidate: Grant | null): TestPlan | null {
  if (candidate === null) {
    return null;
  }
  const plan = candidate.test;
  if (plan === undefined || plan === null) {
    return null;
  }
  if (!isPlainObject(plan)) {
    refuse("candidate.test must be a TestPlan object");
  }
  denyUnknown(plan, TEST_PLAN_FIELDS);
  const program = plan["program"];
  if (typeof program !== "string" || program.length === 0) {
    refuse("the test plan named no program");
  }
  // argv, never a shell string — the host's own strict parser ran before the
  // grant was minted, so a plugin receives a program and its arguments and
  // never a line to hand to a shell.
  const args = plan["args"] === undefined ? [] : plan["args"];
  if (!Array.isArray(args) || !args.every((part) => typeof part === "string")) {
    refuse("the test plan's args must be an array of strings");
  }
  const baseline = plan["baseline"] === undefined ? "not-run" : plan["baseline"];
  if (typeof baseline !== "string" || !BASELINES.includes(baseline)) {
    refuse(
      `TestBaseline is a closed set {${BASELINES.join(", ")}}; ` +
        `got ${asJson(baseline)}`,
    );
  }
  return { argv: [program, ...(args as string[])], baseline };
}

/**
 * What the run says about the fail->pass flip, and nothing more.
 *
 * Three answers, and the third is the one #860 is about:
 *
 * - `failed` before and green after is the flip the witness contract asks for.
 *   Red before and still red after is `not-achieved`.
 * - `passed` before means the test ran on both sides and did not flip.
 * - `not-run` and `unobserved` observed **no assertion** before the turn, so
 *   neither answer above is available. Reporting `not-achieved` would blame the
 *   worker for an instrument that never ran; `unobservable` makes the host
 *   abstain, which is the honest half of what was seen.
 */
function flipFrom(baseline: string, exitCode: number): string {
  if (baseline === "failed") {
    return exitCode === 0 ? "achieved" : "not-achieved";
  }
  if (baseline === "passed") {
    return "not-achieved";
  }
  return "unobservable";
}

/** Run the test in the candidate root and report what was seen. */
function observe(plan: TestPlan, root: string): EvidenceSet {
  const started = Date.now();
  const result = childProcess.spawnSync(plan.argv[0], plan.argv.slice(1), {
    cwd: root,
    stdio: "ignore",
    timeout: TEST_TIMEOUT_SECS * 1000,
  });
  const elapsedMs = Date.now() - started;

  if (result.error) {
    // Could not run it at all — an unreadable root, a program that is not
    // there, a run that outlived its budget. A claim about the instrument, not
    // about the work, so the flip is `unobservable` and the host abstains.
    return unobserved();
  }
  // `measurements` is a map of non-negative integers, so a signal-killed test
  // (a null status here, a negative returncode in Python, `None` from
  // `ExitStatus::code` in Rust) reports as 1 — not a pass, and one number all
  // three languages can produce.
  const exitCode =
    typeof result.status === "number" && result.status >= 0 ? result.status : 1;

  return {
    flip: flipFrom(plan.baseline, exitCode),
    // The after side WAS observed even when the baseline says nothing, so the
    // numbers are reported either way: `tests-pass` is decided by the exit code
    // and does not need the flip.
    measurements: {
      "test-command-exit-code": exitCode,
      "test-duration-ms": elapsedMs,
    },
  };
}

function main(): void {
  let evidence: EvidenceSet;
  try {
    let raw: string;
    try {
      // Descriptor 0 is stdin; the host writes one request and closes it.
      raw = fs.readFileSync(0, "utf8");
    } catch {
      raw = "";
    }
    const body = readRequest(raw);
    const candidate = grant(body);
    const plan = testPlan(candidate);
    evidence =
      plan === null || candidate === null
        ? unobserved()
        : observe(plan, candidate.root);
  } catch (err) {
    if (err instanceof Refusal) {
      process.stderr.write(`verify: ${err.message}\n`);
      process.exitCode = 1;
      return;
    }
    throw err;
  }

  const response = {
    point: POINT,
    body: { protocol_version: PROTOCOL_VERSION, evidence },
  };
  process.stdout.write(JSON.stringify(response) + "\n");
}

main();
