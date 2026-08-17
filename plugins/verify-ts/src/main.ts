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
 * Every table on that wire denies unknown fields, so this program does too.
 * And `AfterTurnResponse` deliberately has no error variant: a plugin that
 * cannot answer *fails* — non-zero exit, one line on stderr — and the host
 * substitutes `EvidenceSet::unobserved()`, which makes `judge` abstain rather
 * than blame the worker for evidence nobody collected.
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
 * before a turn runs.
 */
const POINT = "after_turn";

/** The fields `AfterTurnRequest` declares. Anything else is a typo. */
const AFTER_TURN_REQUEST_FIELDS = [
  "protocol_version",
  "wrapper",
  "round",
  "goal",
  "candidate",
  "turn",
];

/**
 * How the plugin learns which test to run. THIS IS A GAP, NOT A DESIGN:
 * `AfterTurnRequest` carries a `CandidateHandle` but no channel to invoke
 * `CandidateOp::RunTest` with it and no root path, so an out-of-process plugin
 * has no in-band way to be handed a test command. Both names are declared in
 * `[runtime].env`, so they are default-deny and visible at install consent —
 * but they are out of band, which is exactly the finding Track C exists to
 * produce. See plugins/README.md § "What the wire could not say".
 */
const TEST_COMMAND_ENV = "VERIFY_TEST_COMMAND";
const BASELINE_ENV = "VERIFY_BASELINE_EXIT_CODE";

/**
 * What the plugin allows the test command before killing it. Bounded well
 * inside `[runtime].timeout_secs`, so the plugin reports a result rather than
 * being killed mid-report by the host.
 */
const TEST_TIMEOUT_SECS = 240;

interface EvidenceSet {
  flip: string;
  tamper: string;
  measurements?: Record<string, number>;
}

/** The plugin cannot answer. Exits non-zero; the host reports unobserved. */
class Refusal extends Error {}

function refuse(reason: string): never {
  throw new Refusal(reason);
}

/**
 * `EvidenceSet::unobserved()` — the honest answer when nothing was seen.
 *
 * Deliberately not an empty set that reads as "nothing was wrong": an
 * `unobservable` flip makes the host's `judge` abstain rather than credit or
 * blame anyone for evidence that was never collected.
 */
function unobserved(): EvidenceSet {
  return { flip: "unobservable", tamper: "not-checked" };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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
  const unknown = Object.keys(body)
    .filter((key) => !AFTER_TURN_REQUEST_FIELDS.includes(key))
    .sort();
  if (unknown.length > 0) {
    refuse(`AfterTurnRequest denies unknown fields; got ${unknown.join(", ")}`);
  }
  const version = body["protocol_version"];
  if (version !== PROTOCOL_VERSION) {
    refuse(
      `this plugin speaks wrapper protocol_version ${PROTOCOL_VERSION}; ` +
        `the host asked for ${asJson(version)}`,
    );
  }
  return body;
}

/** The argv and the baseline, or `null` when the host supplied neither. */
function testCommand(): { argv: string[]; baseline: number } | null {
  const rawArgv = process.env[TEST_COMMAND_ENV];
  const rawBaseline = process.env[BASELINE_ENV];
  if (rawArgv === undefined || rawBaseline === undefined) {
    return null;
  }
  let argv: unknown;
  try {
    argv = JSON.parse(rawArgv);
  } catch {
    refuse(`${TEST_COMMAND_ENV} is not JSON; it must be an argv array`);
  }
  if (
    !Array.isArray(argv) ||
    argv.length === 0 ||
    !argv.every((part) => typeof part === "string")
  ) {
    refuse(`${TEST_COMMAND_ENV} must be a non-empty array of strings`);
  }
  if (!/^[+-]?\d+$/.test(rawBaseline.trim())) {
    refuse(`${BASELINE_ENV} is not an integer exit code`);
  }
  return { argv: argv as string[], baseline: Number.parseInt(rawBaseline, 10) };
}

/** Run the test command and report what was seen, never what it means. */
function observe(argv: string[], baseline: number): EvidenceSet {
  const started = Date.now();
  const result = childProcess.spawnSync(argv[0], argv.slice(1), {
    stdio: "ignore",
    timeout: TEST_TIMEOUT_SECS * 1000,
  });
  const elapsedMs = Date.now() - started;

  if (result.error) {
    // Could not run it at all — a claim about the instrument, not about the
    // work, so the flip is `unobservable` and the host abstains.
    return unobserved();
  }
  // `EvidenceSet.measurements` is a map of non-negative integers, so a
  // signal-killed test (a null status here, a negative returncode in Python,
  // `None` from `ExitStatus::code` in Rust) reports as 1 — not a pass, and one
  // number all three languages can produce.
  const exitCode =
    typeof result.status === "number" && result.status >= 0 ? result.status : 1;

  return {
    // Red before the work and green after it is the flip the witness contract
    // asks for. Anything else is `not-achieved`; the plugin states the
    // observation and the host's FlipPolicy decides what it is worth.
    flip: baseline !== 0 && exitCode === 0 ? "achieved" : "not-achieved",
    // The host snapshots witness-artifact identity, not the plugin — an
    // out-of-process plugin has nothing to compare, and saying `clean` would
    // be vouching for its own work. See plugins/README.md.
    tamper: "not-checked",
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
    readRequest(raw);
    const granted = testCommand();
    evidence =
      granted === null ? unobserved() : observe(granted.argv, granted.baseline);
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
