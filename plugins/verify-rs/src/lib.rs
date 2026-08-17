//! `verify`, in Rust — the reference implementation of the wrapper socket's
//! `after_turn` point.
//!
//! # Why a library with a thin binary on top
//!
//! `doc:wrapper-socket` §1: *"a wrapper is two async calls it makes, plus two
//! pure functions the host runs on its behalf — and the two async calls are
//! defined as serialized request/response first, with the Rust trait as a
//! typed view of the same shapes."* This crate is that sentence, laid out:
//!
//! - [`AfterTurnRequest`] / [`AfterTurnResponse`] are the **wire shapes**, and
//!   they are primary. They mirror `crates/stella-plugin/src/wire.rs`
//!   field-for-field, and they are what `plugins/verify-py/main.py` and
//!   `plugins/verify-ts` build by hand out of dictionaries.
//! - [`observe`] is the **in-process path**: synchronous, injected with a
//!   [`TestRunner`] rather than reaching for one, so an embedding host can call
//!   it directly and a test can call it with a fake. Track C rule 2 lets Rust
//!   have this *in addition to* the wire path — never instead of it, and
//!   `tests/wire.rs` keeps that honest by exercising the compiled binary
//!   through stdin/stdout against the same vectors the other two run.
//!
//! # The types are re-declared here, and that is the point
//!
//! This crate does **not** depend on `stella-plugin`. A plugin author outside
//! the Stella workspace cannot, and rule 3 says a plugin must be writable with
//! a JSON parser and nothing else — so the Rust example is held to the same
//! bar as the Python one, and these structs are what a third-party author
//! writes after reading `wire.rs`. If they drift from the real contract, that
//! is a fact worth discovering here rather than in a user's plugin.
//!
//! # What this plugin may not do, in any language
//!
//! It reports evidence. It never reports a verdict. `judge` is a host function
//! over the rule declared in `plugin.toml` and the [`EvidenceSet`] returned
//! here (`doc:wrapper-socket` §4), and `WrapperPoint` has no `judge` variant —
//! so a plugin cannot implement one, which is what keeps "a verification
//! plugin quietly calls a model to decide done" impossible by construction
//! rather than by policy.
//!
//! # Two rules that look contradictory and are not
//!
//! Every table on the wire **denies unknown fields**, and the contract is
//! **additive-only**. Both hold at once because the version is what carries
//! the addition: a field this program does not know, at a version it accepts,
//! is a typo — and a message that quietly does nothing is worse than one that
//! refuses. A genuinely new field arrives with a version bump.
//!
//! # There is no error variant, deliberately
//!
//! [`AfterTurnResponse`] cannot carry a failure. A plugin that cannot answer
//! *fails*: non-zero exit, one line on stderr, nothing on stdout. The host
//! then substitutes `EvidenceSet::unobserved()`, which makes `judge` abstain
//! rather than blame the worker for evidence nobody collected. [`Refusal`] is
//! that path, and it is the only one.

use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The version every message on this wire carries (`wire::PROTOCOL_VERSION`).
pub const PROTOCOL_VERSION: u32 = 1;

/// The one point this plugin answers.
///
/// `WrapperPoint` has exactly two — `before_turn` and `after_turn` — and this
/// plugin has nothing to contribute before a turn runs.
pub const POINT: &str = "after_turn";

/// How the plugin learns which test to run.
///
/// **This is a gap, not a design.** `AfterTurnRequest` carries a
/// `CandidateHandle` but no channel to invoke `CandidateOp::RunTest` with it,
/// and no root path, so an out-of-process plugin has no in-band way to be
/// handed a test command. Both names are declared in `[runtime].env`, so they
/// are default-deny and visible at install consent — but they are out of band,
/// which is exactly the finding Track C exists to produce. See
/// `plugins/README.md` § "What the wire could not say".
pub const TEST_COMMAND_ENV: &str = "VERIFY_TEST_COMMAND";

/// The other half of that gap: the exit code the host observed before the turn.
pub const BASELINE_ENV: &str = "VERIFY_BASELINE_EXIT_CODE";

/// What the plugin allows the test command before killing it.
///
/// Bounded well inside `[runtime].timeout_secs` so the plugin reports a result
/// rather than being killed mid-report by the host.
const TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// How often the runner asks a still-running child whether it is done.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

// ---------------------------------------------------------------------------
// The wire shapes — `crates/stella-plugin/src/wire.rs`, as an outside author
// transcribes them.
// ---------------------------------------------------------------------------

/// Everything a wrapper is given once the turn's completion lands.
///
/// It receives the outcome; it does **not** hold a channel into the turn.
///
/// It arrives inside the envelope `{"point": "after_turn", "body": {…}}` —
/// `WrapperRequest` is adjacently tagged, which is what lets this body keep
/// `deny_unknown_fields`: an internally tagged enum hands the tag down into the
/// variant, where a denying struct would reject it. [`read_request`] takes the
/// envelope apart by hand rather than mirroring the whole enum, because this
/// plugin implements one of the two points and a `BeforeTurn` variant it can
/// only refuse would be a shape with no reader.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AfterTurnRequest {
    /// The version this message is written at.
    pub protocol_version: u32,
    /// The variant id of the wrapper being asked.
    #[serde(default)]
    pub wrapper: String,
    /// Which round of the wrapper's loop just ran.
    #[serde(default)]
    pub round: u32,
    /// The goal, as the user stated it.
    #[serde(default)]
    pub goal: String,
    /// The candidate workspace the turn ran against, when there was one.
    ///
    /// A name the host resolves, never a path to be trusted — and, today,
    /// never a name this plugin can *do* anything with: see
    /// [`TEST_COMMAND_ENV`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<String>,
    /// What the turn did.
    #[serde(default)]
    pub turn: TurnOutcome,
}

/// The read-only report of a turn that finished.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnOutcome {
    /// Whether the engine reported the turn complete, as opposed to aborted.
    #[serde(default)]
    pub completed: bool,
    /// The final assistant text.
    #[serde(default)]
    pub answer: String,
    /// The tools the turn dispatched, in call order, by name.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Workspace-relative paths the turn changed.
    #[serde(default)]
    pub changed_files: Vec<String>,
}

/// One response envelope, in the same framing as the request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "point", content = "body", rename_all = "snake_case")]
pub enum WrapperResponse {
    /// The evidence the wrapper gathered about the turn that ran.
    AfterTurn(AfterTurnResponse),
}

/// The evidence a wrapper gathered. No error variant, deliberately — see the
/// module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AfterTurnResponse {
    /// The version this message is written at.
    pub protocol_version: u32,
    /// What the wrapper observed.
    pub evidence: EvidenceSet,
}

/// What a wrapper observed about the turn — the whole evidence vocabulary.
///
/// A struct of closed fields rather than an open list of facts, because
/// `judge` must be **total** over it. Nothing here is a verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceSet {
    /// What the wrapper saw of the fail→pass flip.
    pub flip: FlipObservation,
    /// What the wrapper's tamper check found.
    pub tamper: TamperFinding,
    /// The numbers the oracle reported, by declared measurement name. A name
    /// absent here is *missing*, never a satisfied budget. A `BTreeMap` so the
    /// serialized order is deterministic.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub measurements: BTreeMap<String, u64>,
}

impl EvidenceSet {
    /// The set to report when nothing could be gathered.
    ///
    /// Deliberately *not* an empty set that reads as "nothing was wrong":
    /// [`FlipObservation::Unobservable`] makes `judge` abstain rather than
    /// blame the worker for evidence nobody collected.
    #[must_use]
    pub fn unobserved() -> Self {
        Self {
            flip: FlipObservation::Unobservable,
            tamper: TamperFinding::NotChecked,
            measurements: BTreeMap::new(),
        }
    }
}

/// What a wrapper saw of the fail→pass flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlipObservation {
    /// Red before the work, green after it: the flip the witness contract asks
    /// for.
    Achieved,
    /// The test ran on both sides and did not flip.
    NotAchieved,
    /// The test could not be run at all.
    Unobservable,
}

/// What a wrapper's tamper check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TamperFinding {
    /// No snapshot was taken, so there is nothing to compare.
    ///
    /// The only value this plugin can honestly report: the snapshotting is
    /// host-side, and an out-of-process plugin has nothing to compare. Saying
    /// `clean` would be vouching for its own work.
    NotChecked,
}

// ---------------------------------------------------------------------------
// Refusal — the only failure path
// ---------------------------------------------------------------------------

/// The plugin cannot answer.
///
/// Rendered as one line on stderr with a non-zero exit; the host reports
/// `unobserved` on the plugin's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refusal {}

/// Render a value exactly as the Python and TypeScript implementations do, so
/// one golden refusal line covers all three.
fn as_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// Decode the envelope and return the `after_turn` body.
///
/// # Errors
///
/// [`Refusal`] for anything this plugin will not answer: unparsable stdin, an
/// envelope or body field the contract does not declare, the wrong point, or a
/// protocol version this program does not speak.
pub fn read_request(raw: &str) -> Result<AfterTurnRequest, Refusal> {
    // Decoded twice on purpose: once loosely, so a refusal can quote what the
    // host actually asked for the way the other two implementations do, and
    // once strictly, so `deny_unknown_fields` is serde's job and not a
    // hand-maintained list.
    let loose: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| Refusal("stdin was not a single JSON object".to_string()))?;
    if !loose.is_object() {
        return Err(Refusal("stdin was not a single JSON object".to_string()));
    }
    let Some(object) = loose.as_object() else {
        return Err(Refusal("stdin was not a single JSON object".to_string()));
    };
    if object.keys().any(|key| key != "point" && key != "body") {
        return Err(Refusal(
            "the request envelope carried a field outside {point, body}".to_string(),
        ));
    }
    let point = object
        .get("point")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if point.as_str() != Some(POINT) {
        return Err(Refusal(format!(
            "this plugin answers {POINT} only; the host asked for {}",
            as_json(&point)
        )));
    }

    let body = object
        .get("body")
        .filter(|body| body.is_object())
        .ok_or_else(|| Refusal("the request envelope carried no object body".to_string()))?;
    let request: AfterTurnRequest = serde_json::from_value(body.clone()).map_err(|err| {
        // serde's message names the offending key, which is the half a plugin
        // author needs; the prefix keeps the three languages' wording aligned.
        let text = err.to_string();
        match text.split_once("unknown field `") {
            Some((_, rest)) => {
                let field = rest.split('`').next().unwrap_or("");
                Refusal(format!(
                    "AfterTurnRequest denies unknown fields; got {field}"
                ))
            }
            None => Refusal(format!("AfterTurnRequest could not be read: {text}")),
        }
    })?;

    if request.protocol_version != PROTOCOL_VERSION {
        return Err(Refusal(format!(
            "this plugin speaks wrapper protocol_version {PROTOCOL_VERSION}; \
             the host asked for {}",
            request.protocol_version
        )));
    }
    Ok(request)
}

// ---------------------------------------------------------------------------
// The injected capability
// ---------------------------------------------------------------------------

/// The test command and the exit code the host observed before the turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCommand {
    /// Program and arguments. A list, never a shell string.
    pub argv: Vec<String>,
    /// What the command exited with before this turn ran.
    pub baseline: i64,
}

/// Read [`TEST_COMMAND_ENV`] and [`BASELINE_ENV`] from an environment lookup.
///
/// Takes the lookup rather than calling `std::env` so the function stays pure
/// and testable — the same reason [`observe`] takes a [`TestRunner`].
///
/// # Errors
///
/// [`Refusal`] when a name is present but unreadable. Both absent is `Ok(None)`
/// — the host granted nothing, and the honest answer is
/// [`EvidenceSet::unobserved`], not a failure.
pub fn test_command<F>(mut lookup: F) -> Result<Option<TestCommand>, Refusal>
where
    F: FnMut(&str) -> Option<String>,
{
    let (Some(raw_argv), Some(raw_baseline)) = (lookup(TEST_COMMAND_ENV), lookup(BASELINE_ENV))
    else {
        return Ok(None);
    };
    let argv: Vec<String> = serde_json::from_str(&raw_argv).map_err(|_| {
        Refusal(format!(
            "{TEST_COMMAND_ENV} is not JSON; it must be an argv array"
        ))
    })?;
    if argv.is_empty() {
        return Err(Refusal(format!(
            "{TEST_COMMAND_ENV} must be a non-empty array of strings"
        )));
    }
    let baseline = raw_baseline
        .trim()
        .parse::<i64>()
        .map_err(|_| Refusal(format!("{BASELINE_ENV} is not an integer exit code")))?;
    Ok(Some(TestCommand { argv, baseline }))
}

/// What running the test command produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestOutcome {
    /// The child's exit status. A signal-killed test reports 1 — not a pass,
    /// and the one number all three languages can produce.
    pub exit_code: u64,
    /// Wall clock, milliseconds.
    pub elapsed_ms: u64,
}

/// The one thing this plugin reaches outside itself to do, behind a seam.
///
/// A trait rather than a direct `Command` call so [`observe`] stays a pure
/// function over owned data with its I/O injected.
pub trait TestRunner {
    /// Run `argv`, or answer `None` if it could not be run at all — which is a
    /// claim about the instrument, not about the work.
    fn run(&self, argv: &[String]) -> Option<TestOutcome>;
}

/// The real runner: spawns the argv directly, never through a shell.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessRunner;

impl TestRunner for ProcessRunner {
    fn run(&self, argv: &[String]) -> Option<TestOutcome> {
        let (program, args) = argv.split_first()?;
        let started = Instant::now();
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Some(TestOutcome {
                        exit_code: status
                            .code()
                            .and_then(|code| u64::try_from(code).ok())
                            .unwrap_or(1),
                        elapsed_ms: u64::try_from(started.elapsed().as_millis())
                            .unwrap_or(u64::MAX),
                    });
                }
                Ok(None) => {
                    if started.elapsed() >= TEST_TIMEOUT {
                        // Best effort: the answer is "unobservable" either way,
                        // and a reap failure must not mask it.
                        let _ = child.kill();
                        let _ = child.wait();
                        return None;
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(_) => return None,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The point itself
// ---------------------------------------------------------------------------

/// Run the test command and report what was seen, never what it means.
///
/// The in-process path: synchronous and total, with the capability injected.
#[must_use]
pub fn observe(command: &TestCommand, runner: &dyn TestRunner) -> EvidenceSet {
    let Some(outcome) = runner.run(&command.argv) else {
        return EvidenceSet::unobserved();
    };
    let mut measurements = BTreeMap::new();
    measurements.insert("test-command-exit-code".to_string(), outcome.exit_code);
    measurements.insert("test-duration-ms".to_string(), outcome.elapsed_ms);
    EvidenceSet {
        // Red before the work and green after it is the flip the witness
        // contract asks for. Anything else is `not-achieved`; the plugin states
        // the observation and the host's FlipPolicy decides what it is worth.
        flip: if command.baseline != 0 && outcome.exit_code == 0 {
            FlipObservation::Achieved
        } else {
            FlipObservation::NotAchieved
        },
        tamper: TamperFinding::NotChecked,
        measurements,
    }
}

/// The whole wire path: raw stdin and an environment in, a response out.
///
/// # Errors
///
/// [`Refusal`], which the binary renders on stderr before exiting non-zero.
pub fn handle<F>(raw: &str, lookup: F, runner: &dyn TestRunner) -> Result<WrapperResponse, Refusal>
where
    F: FnMut(&str) -> Option<String>,
{
    read_request(raw)?;
    let evidence = match test_command(lookup)? {
        Some(command) => observe(&command, runner),
        None => EvidenceSet::unobserved(),
    };
    Ok(WrapperResponse::AfterTurn(AfterTurnResponse {
        protocol_version: PROTOCOL_VERSION,
        evidence,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A runner that answers without spawning anything — the reason [`observe`]
    /// takes the capability rather than reaching for it.
    struct FakeRunner(Option<TestOutcome>);

    impl TestRunner for FakeRunner {
        fn run(&self, _argv: &[String]) -> Option<TestOutcome> {
            self.0
        }
    }

    fn envelope(body: &str) -> String {
        format!(r#"{{"point":"after_turn","body":{body}}}"#)
    }

    fn full_body() -> String {
        r#"{
            "protocol_version": 1,
            "wrapper": "verify-v1",
            "round": 0,
            "goal": "make the failing test pass",
            "candidate": "candidate-1",
            "turn": { "completed": true, "answer": "done", "changed_files": ["src/lib.rs"] }
        }"#
        .to_string()
    }

    fn granted(argv: &[&str], baseline: &str) -> impl FnMut(&str) -> Option<String> {
        let argv = serde_json::to_string(argv).expect("argv serializes");
        let baseline = baseline.to_string();
        move |name: &str| match name {
            TEST_COMMAND_ENV => Some(argv.clone()),
            BASELINE_ENV => Some(baseline.clone()),
            _ => None,
        }
    }

    #[test]
    fn red_before_and_green_after_is_the_flip() {
        let runner = FakeRunner(Some(TestOutcome {
            exit_code: 0,
            elapsed_ms: 7,
        }));
        let response = handle(&envelope(&full_body()), granted(&["true"], "1"), &runner)
            .expect("a well-formed request is answered");
        let WrapperResponse::AfterTurn(body) = response;
        assert_eq!(body.evidence.flip, FlipObservation::Achieved);
        assert_eq!(body.evidence.measurements["test-command-exit-code"], 0);
        assert_eq!(body.evidence.measurements["test-duration-ms"], 7);
    }

    #[test]
    fn a_baseline_that_already_passed_is_not_a_flip() {
        let runner = FakeRunner(Some(TestOutcome {
            exit_code: 0,
            elapsed_ms: 0,
        }));
        let response =
            handle(&envelope(&full_body()), granted(&["true"], "0"), &runner).expect("answered");
        let WrapperResponse::AfterTurn(body) = response;
        assert_eq!(body.evidence.flip, FlipObservation::NotAchieved);
    }

    #[test]
    fn nothing_granted_is_unobserved_rather_than_a_pass() {
        let runner = FakeRunner(Some(TestOutcome {
            exit_code: 0,
            elapsed_ms: 0,
        }));
        let response = handle(&envelope(&full_body()), |_| None, &runner).expect("answered");
        let WrapperResponse::AfterTurn(body) = response;
        assert_eq!(body.evidence, EvidenceSet::unobserved());
        assert!(body.evidence.measurements.is_empty());
    }

    #[test]
    fn a_command_that_cannot_be_run_is_unobservable() {
        let runner = FakeRunner(None);
        let response =
            handle(&envelope(&full_body()), granted(&["nope"], "1"), &runner).expect("answered");
        let WrapperResponse::AfterTurn(body) = response;
        assert_eq!(body.evidence.flip, FlipObservation::Unobservable);
    }

    #[test]
    fn the_plugin_can_say_no_verdict_because_the_type_has_nowhere_to_put_one() {
        let runner = FakeRunner(Some(TestOutcome {
            exit_code: 0,
            elapsed_ms: 0,
        }));
        let response =
            handle(&envelope(&full_body()), granted(&["true"], "1"), &runner).expect("answered");
        let json = serde_json::to_string(&response).expect("serializes");
        // `judge` is the host's. There is nowhere in the wire shape for a
        // plugin to put an answer, and this test is what says so out loud.
        for forbidden in ["verdict", "done", "satisfied", "requirement", "unmet"] {
            assert!(!json.contains(forbidden), "{json} leaked `{forbidden}`");
        }
    }

    #[test]
    fn an_unknown_body_field_is_refused_because_the_wire_denies_them() {
        let body = r#"{"protocol_version":1,"a_field_a_newer_host_added":1}"#;
        let err = read_request(&envelope(body)).expect_err("refused");
        assert_eq!(
            err.0,
            "AfterTurnRequest denies unknown fields; got a_field_a_newer_host_added"
        );
    }

    #[test]
    fn a_version_this_program_does_not_speak_is_refused() {
        let err = read_request(&envelope(r#"{"protocol_version":2}"#)).expect_err("refused");
        assert_eq!(
            err.0,
            "this plugin speaks wrapper protocol_version 1; the host asked for 2"
        );
    }

    #[test]
    fn the_other_socket_point_is_refused_rather_than_answered_emptily() {
        let raw = r#"{"point":"before_turn","body":{"protocol_version":1}}"#;
        let err = read_request(raw).expect_err("refused");
        assert_eq!(
            err.0,
            "this plugin answers after_turn only; the host asked for \"before_turn\""
        );
    }

    #[test]
    fn malformed_stdin_is_refused_rather_than_panicked_on() {
        let err = read_request("{ this is not JSON").expect_err("refused");
        assert_eq!(err.0, "stdin was not a single JSON object");
    }

    #[test]
    fn the_real_runner_reports_a_non_zero_exit_rather_than_failing() {
        let outcome = ProcessRunner
            .run(&["false".to_string()])
            .expect("a failing command still ran");
        assert_ne!(outcome.exit_code, 0);
    }

    #[test]
    fn the_real_runner_answers_none_for_a_command_it_cannot_start() {
        assert!(ProcessRunner
            .run(&["stella-no-such-program-exists".to_string()])
            .is_none());
    }

    #[test]
    fn the_envelope_serializes_in_the_shape_the_host_reads() {
        let response = WrapperResponse::AfterTurn(AfterTurnResponse {
            protocol_version: PROTOCOL_VERSION,
            evidence: EvidenceSet::unobserved(),
        });
        assert_eq!(
            serde_json::to_string(&response).expect("serializes"),
            r#"{"point":"after_turn","body":{"protocol_version":1,"evidence":{"flip":"unobservable","tamper":"not-checked"}}}"#
        );
    }
}
