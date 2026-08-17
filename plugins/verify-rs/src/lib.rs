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
//! # Every capability arrives in the request
//!
//! [`AfterTurnRequest::candidate`] is a [`CandidateGrant`]: the handle, the
//! canonical workspace [`root`](CandidateGrant::root), and — when the host has
//! one to give — the [`test`](CandidateGrant::test) it would run there, as a
//! program, an argument vector, and what that same invocation reported
//! **before** the turn. Nothing else is reached for: this program takes no
//! environment lookup, has no working directory of its own, and touches no git
//! checkout. It used to read two environment variables, because the request
//! carried neither a root nor a test invocation (#3498).
//!
//! # What this plugin may not do, in any language
//!
//! It reports what it observed. It never reports a verdict, and it cannot
//! report a tamper finding: [`ObservedEvidence`] has no field for one, because
//! snapshotting witness-artifact identity is the host's job and a plugin
//! vouching for its own witness is what the policy exists to prevent (#3499).
//! `judge` is a host function over the rule declared in `plugin.toml` and the
//! evidence returned here (`doc:wrapper-socket` §4), and `WrapperPoint` has no
//! `judge` variant — so a plugin cannot implement one, which is what keeps "a
//! verification plugin quietly calls a model to decide done" impossible by
//! construction rather than by policy.
//!
//! What Stella does **not** do is re-run this test or re-check this answer. The
//! flip and the measurements are this plugin's own report; the host applies the
//! declared rule to them and owns the tamper finding beside them.
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
/// plugin has nothing to contribute before a turn runs, which `[loop].points`
/// declares in the manifest (#3501) so a host learns it without asking.
pub const POINT: &str = "after_turn";

/// What the plugin allows the test before killing it.
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
/// `WrapperRequest` is adjacently framed, which is what lets this body keep
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
    /// Which declared stage this evidence is about — absent when the host runs
    /// no stage program, which is a different answer from "some default stage".
    ///
    /// The contract types it as an optional `StageName`, a closed twelve-value
    /// enum. It is a `String` here because this plugin never reads it: it
    /// declares one stage's worth of behaviour, so mirroring twelve variants in
    /// three languages would buy nothing. It is declared at all because a field
    /// a host sends must not be refused as a typo — the additive half of the
    /// deny-unknown-fields rule.
    #[serde(default)]
    pub stage: Option<String>,
    /// Which round of the wrapper's loop just ran.
    #[serde(default)]
    pub round: u32,
    /// The goal, as the user stated it.
    #[serde(default)]
    pub goal: String,
    /// The candidate workspace the turn ran against, when there was one.
    #[serde(default)]
    pub candidate: Option<CandidateGrant>,
    /// What the turn did.
    #[serde(default)]
    pub turn: TurnOutcome,
}

/// The candidate workspace, as a plugin receives it.
///
/// A capability the host resolves and bounds, never a path a plugin is trusted
/// to stay inside: [`Self::root`] is where this plugin's own reads and its own
/// test run happen, and every path it might name on the way back is resolved
/// against the handle by the host, on the host's filesystem, after symlinks. A
/// plugin that ignored the root and went somewhere else would have told the
/// host nothing the host will act on.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateGrant {
    /// The name the host minted for this workspace, and the only thing that
    /// re-addresses it.
    ///
    /// Defaulted rather than required because this program never spends it:
    /// the one-shot exchange gives it nothing to address, so refusing a message
    /// over a field it does not read would be strictness for its own sake. It
    /// is declared so that the real host's message — which always carries one —
    /// is not refused as an unknown field.
    #[serde(default)]
    pub handle: String,
    /// The workspace's canonical absolute root on the host's filesystem.
    ///
    /// Defaulted for one reason: an absent root and an empty one are the same
    /// defect, and [`grant`] reports both with the same sentence the Python and
    /// TypeScript plugins print, so one golden line covers three languages.
    #[serde(default)]
    pub root: String,
    /// The test the host would run in this workspace, when it has one to give.
    ///
    /// `None` is "the host has no test invocation", never "run whatever you
    /// like": with no plan this plugin reports [`FlipObservation::Unobservable`]
    /// rather than guessing at a command.
    #[serde(default)]
    pub test: Option<TestPlan>,
}

/// One test invocation, as the host already parsed it.
///
/// **argv, never a shell string** — the #1400 rule every spawned thing in the
/// Stella workspace follows. The host's own strict test-command parser runs
/// before the grant is minted, so a plugin receives a program and its
/// arguments and never a line to hand to a shell.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestPlan {
    /// The test runner executable.
    #[serde(default)]
    pub program: String,
    /// Its exact argument vector.
    #[serde(default)]
    pub args: Vec<String>,
    /// What this same invocation reported *before* the turn ran.
    #[serde(default)]
    pub baseline: TestBaseline,
}

/// What a [`TestPlan`]'s invocation reported before the turn ran.
///
/// Four answers rather than an exit code, and the fourth is why: a run that
/// timed out or could not find its toolchain never observed an assertion, and
/// scoring its non-zero exit as "red" would let an infra failure satisfy a
/// flip's precondition, so the next clean run would be credited as a fix
/// (#860). This plugin's `[runtime].env` used to carry that exit code, and it
/// could not tell those two apart at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestBaseline {
    /// The host did not run the invocation before the turn.
    #[default]
    NotRun,
    /// It ran and its assertions passed.
    Passed,
    /// It ran and its assertions genuinely failed — the red a flip needs.
    Failed,
    /// It was attempted and did not complete, so it says nothing about
    /// assertions either way.
    Unobserved,
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
    pub evidence: ObservedEvidence,
}

/// What a wrapper observed about the turn — the plugin-owned half of the
/// evidence, and the whole of what it may say.
///
/// There is no `tamper` field, in this program or on the wire it speaks
/// (#3499). Snapshotting witness-artifact identity is host-side by design: the
/// host owns the candidate worktree and the authoring-time snapshot, the plugin
/// sees neither, and a plugin vouching for its own witness is exactly what the
/// policy exists to prevent. The host merges its own finding in before `judge`
/// runs. Nothing here is a verdict either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedEvidence {
    /// What the wrapper saw of the fail→pass flip.
    pub flip: FlipObservation,
    /// The numbers this plugin reported, by declared measurement name. A name
    /// absent here is *missing*, never a satisfied budget. A `BTreeMap` so the
    /// serialized order is deterministic.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub measurements: BTreeMap<String, u64>,
}

impl ObservedEvidence {
    /// What to report when nothing was observed at all.
    ///
    /// Deliberately *not* an empty set that reads as "nothing was wrong":
    /// [`FlipObservation::Unobservable`] makes `judge` abstain rather than
    /// blame the worker for evidence nobody collected.
    #[must_use]
    pub fn nothing() -> Self {
        Self {
            flip: FlipObservation::Unobservable,
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
    /// The flip could not be observed — the test could not be run, or the
    /// baseline never observed an assertion to be red about.
    Unobservable,
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

/// Turn serde's decoding failure into the sentence the other two print.
///
/// Two of serde's messages are graded by a shared golden and so are translated
/// exactly: an unknown field anywhere in the request, and an unknown
/// [`TestBaseline`]. Serde names the offending key but not the table it was
/// found in, which is why the unknown-field message names neither — one wording
/// that three languages can produce is worth more here than a path Python and
/// TypeScript could report and Rust could not. Everything else keeps serde's
/// own words behind a prefix; those paths are type errors no vector grades.
fn read_failure(err: &serde_json::Error) -> Refusal {
    let text = err.to_string();
    if let Some((_, rest)) = text.split_once("unknown field `") {
        let field = rest.split('`').next().unwrap_or("");
        return Refusal(format!("the request denies unknown fields; got {field}"));
    }
    if let Some((_, rest)) = text.split_once("unknown variant `") {
        // `TestBaseline` is the only closed set on this request, so an unknown
        // variant can be nothing else.
        let variant = rest.split('`').next().unwrap_or("");
        return Refusal(format!(
            "TestBaseline is a closed set {{not-run, passed, failed, unobserved}}; \
             got {}",
            as_json(&serde_json::Value::String(variant.to_string()))
        ));
    }
    Refusal(format!("the request could not be read: {text}"))
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
    let Some(object) = loose.as_object() else {
        return Err(Refusal("stdin was not a single JSON object".to_string()));
    };
    // The envelope denies unknown fields too. That used to make this plugin
    // stricter than the host's own decoder, which accepted and dropped an extra
    // key beside `point` and `body`; #3500 closed the gap on the host side.
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
    let request: AfterTurnRequest =
        serde_json::from_value(body.clone()).map_err(|err| read_failure(&err))?;

    if request.protocol_version != PROTOCOL_VERSION {
        return Err(Refusal(format!(
            "this plugin speaks wrapper protocol_version {PROTOCOL_VERSION}; \
             the host asked for {}",
            request.protocol_version
        )));
    }
    Ok(request)
}

/// The candidate workspace this plugin was granted, validated.
///
/// # Errors
///
/// [`Refusal`] when a grant is present but carries no root — the one field
/// this program cannot proceed without. No grant at all is `Ok(None)`: the host
/// created no candidate, and the honest answer is
/// [`ObservedEvidence::nothing`], not a failure.
pub fn grant(request: &AfterTurnRequest) -> Result<Option<&CandidateGrant>, Refusal> {
    let Some(candidate) = request.candidate.as_ref() else {
        return Ok(None);
    };
    if candidate.root.is_empty() {
        return Err(Refusal("the candidate grant carried no root".to_string()));
    }
    Ok(Some(candidate))
}

/// The test invocation the grant names, validated.
///
/// # Errors
///
/// [`Refusal`] when a plan is present but names no program. No plan is
/// `Ok(None)` — see [`CandidateGrant::test`].
pub fn test_plan(candidate: Option<&CandidateGrant>) -> Result<Option<&TestPlan>, Refusal> {
    let Some(plan) = candidate.and_then(|candidate| candidate.test.as_ref()) else {
        return Ok(None);
    };
    if plan.program.is_empty() {
        return Err(Refusal("the test plan named no program".to_string()));
    }
    Ok(Some(plan))
}

// ---------------------------------------------------------------------------
// The injected capability
// ---------------------------------------------------------------------------

/// What running the test produced.
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
    /// Run `argv` inside `root`, or answer `None` if it could not be run at
    /// all — which is a claim about the instrument, not about the work.
    fn run(&self, argv: &[String], root: &str) -> Option<TestOutcome>;
}

/// The real runner: spawns the argv directly in the granted root, never
/// through a shell.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessRunner;

impl TestRunner for ProcessRunner {
    fn run(&self, argv: &[String], root: &str) -> Option<TestOutcome> {
        let (program, args) = argv.split_first()?;
        let started = Instant::now();
        let mut child = Command::new(program)
            .args(args)
            // The grant's root, which is where the host says this workspace
            // is. A root that does not resolve is a failure to run the test,
            // not a licence to run it somewhere else.
            .current_dir(root)
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

/// What the run says about the fail→pass flip, and nothing more.
///
/// Three answers, and the third is the one #860 is about:
///
/// - [`TestBaseline::Failed`] before and green after is the flip the witness
///   contract asks for. Red before and still red after is `not-achieved`.
/// - [`TestBaseline::Passed`] before means the test ran on both sides and did
///   not flip.
/// - [`TestBaseline::NotRun`] and [`TestBaseline::Unobserved`] observed **no
///   assertion** before the turn, so neither answer above is available.
///   Reporting `not-achieved` would blame the worker for an instrument that
///   never ran; `unobservable` makes the host abstain, which is the honest half
///   of what was seen.
#[must_use]
pub fn flip_from(baseline: TestBaseline, exit_code: u64) -> FlipObservation {
    match baseline {
        TestBaseline::Failed if exit_code == 0 => FlipObservation::Achieved,
        TestBaseline::Failed | TestBaseline::Passed => FlipObservation::NotAchieved,
        TestBaseline::NotRun | TestBaseline::Unobserved => FlipObservation::Unobservable,
    }
}

/// Run the test in the granted root and report what was seen, never what it
/// means.
///
/// The in-process path: synchronous and total, with the capability injected.
#[must_use]
pub fn observe(plan: &TestPlan, root: &str, runner: &dyn TestRunner) -> ObservedEvidence {
    let mut argv = Vec::with_capacity(plan.args.len() + 1);
    argv.push(plan.program.clone());
    argv.extend(plan.args.iter().cloned());

    let Some(outcome) = runner.run(&argv, root) else {
        // Could not run it at all — an unreadable root, a program that is not
        // there, a run that outlived its budget.
        return ObservedEvidence::nothing();
    };
    let mut measurements = BTreeMap::new();
    measurements.insert("test-command-exit-code".to_string(), outcome.exit_code);
    measurements.insert("test-duration-ms".to_string(), outcome.elapsed_ms);
    ObservedEvidence {
        flip: flip_from(plan.baseline, outcome.exit_code),
        // The after side WAS observed even when the baseline says nothing, so
        // the numbers are reported either way: `tests-pass` is decided by the
        // exit code and does not need the flip.
        measurements,
    }
}

/// The whole wire path: raw stdin in, a response out.
///
/// There is no environment lookup in this signature, and that is the shape of
/// #3498: everything the plugin acts on arrived in `raw`.
///
/// # Errors
///
/// [`Refusal`], which the binary renders on stderr before exiting non-zero.
pub fn handle(raw: &str, runner: &dyn TestRunner) -> Result<WrapperResponse, Refusal> {
    let request = read_request(raw)?;
    let candidate = grant(&request)?;
    let evidence = match test_plan(candidate)? {
        // `candidate` is `Some` whenever a plan is, so the root is available;
        // `unwrap_or_default` keeps that fact from needing a panic to state.
        Some(plan) => observe(
            plan,
            candidate
                .map(|candidate| candidate.root.as_str())
                .unwrap_or_default(),
            runner,
        ),
        None => ObservedEvidence::nothing(),
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
        fn run(&self, _argv: &[String], _root: &str) -> Option<TestOutcome> {
            self.0
        }
    }

    fn envelope(body: &str) -> String {
        format!(r#"{{"point":"after_turn","body":{body}}}"#)
    }

    /// A full request whose grant carries a test with the given baseline.
    fn body_with(baseline: &str) -> String {
        format!(
            r#"{{
            "protocol_version": 1,
            "wrapper": "verify-v1",
            "round": 0,
            "goal": "make the failing test pass",
            "candidate": {{
                "handle": "candidate-1",
                "root": "/tmp",
                "test": {{"program": "test", "args": ["1", "=", "1"], "baseline": "{baseline}"}}
            }},
            "turn": {{ "completed": true, "answer": "done", "changed_files": ["src/lib.rs"] }}
        }}"#
        )
    }

    fn ran(exit_code: u64) -> FakeRunner {
        FakeRunner(Some(TestOutcome {
            exit_code,
            elapsed_ms: 7,
        }))
    }

    fn evidence_for(baseline: &str, runner: &dyn TestRunner) -> ObservedEvidence {
        let response = handle(&envelope(&body_with(baseline)), runner)
            .expect("a well-formed request is answered");
        let WrapperResponse::AfterTurn(body) = response;
        body.evidence
    }

    #[test]
    fn red_before_and_green_after_is_the_flip() {
        let evidence = evidence_for("failed", &ran(0));
        assert_eq!(evidence.flip, FlipObservation::Achieved);
        assert_eq!(evidence.measurements["test-command-exit-code"], 0);
        assert_eq!(evidence.measurements["test-duration-ms"], 7);
    }

    #[test]
    fn a_baseline_that_already_passed_is_not_a_flip() {
        assert_eq!(
            evidence_for("passed", &ran(0)).flip,
            FlipObservation::NotAchieved
        );
    }

    #[test]
    fn red_before_and_still_red_after_is_not_a_flip() {
        assert_eq!(
            evidence_for("failed", &ran(1)).flip,
            FlipObservation::NotAchieved
        );
    }

    /// #860 on the wire: a baseline that never observed an assertion is not
    /// red, so a green run after it is not a flip — and it is not the worker's
    /// failure either. Both wrong answers are excluded here, in one test.
    #[test]
    fn a_baseline_that_observed_nothing_is_neither_a_flip_nor_a_failure() {
        for baseline in ["unobserved", "not-run"] {
            let evidence = evidence_for(baseline, &ran(0));
            assert_eq!(
                evidence.flip,
                FlipObservation::Unobservable,
                "baseline {baseline} must not credit a flip, and must not blame the worker"
            );
            assert_eq!(
                evidence.measurements["test-command-exit-code"], 0,
                "the after side WAS observed, so its numbers are still reported"
            );
        }
    }

    #[test]
    fn a_grant_with_no_test_is_unobserved_rather_than_a_pass() {
        let body = r#"{"protocol_version":1,"candidate":{"handle":"c","root":"/tmp"}}"#;
        let response = handle(&envelope(body), &ran(0)).expect("answered");
        let WrapperResponse::AfterTurn(body) = response;
        assert_eq!(body.evidence, ObservedEvidence::nothing());
        assert!(body.evidence.measurements.is_empty());
    }

    #[test]
    fn no_grant_at_all_is_unobserved_rather_than_a_pass() {
        let response = handle(&envelope(r#"{"protocol_version":1}"#), &ran(0)).expect("answered");
        let WrapperResponse::AfterTurn(body) = response;
        assert_eq!(body.evidence, ObservedEvidence::nothing());
    }

    #[test]
    fn a_command_that_cannot_be_run_is_unobservable() {
        assert_eq!(
            evidence_for("failed", &FakeRunner(None)).flip,
            FlipObservation::Unobservable
        );
    }

    #[test]
    fn a_grant_with_no_root_is_refused_rather_than_run_somewhere_else() {
        let body = r#"{"protocol_version":1,"candidate":{"handle":"c","test":{"program":"true"}}}"#;
        let err = handle(&envelope(body), &ran(0)).expect_err("refused");
        assert_eq!(err.0, "the candidate grant carried no root");
    }

    /// The root is not decoration: the test runs *there*, and this is the
    /// falsifier that says so — the same shape as
    /// `a_plugin_acts_on_the_candidate_using_only_what_the_request_carried` in
    /// `crates/stella-runtime/tests/wrapper_socket.rs`.
    #[test]
    fn the_test_is_run_in_the_root_the_grant_named() {
        struct RootRecorder(std::cell::RefCell<String>);
        impl TestRunner for RootRecorder {
            fn run(&self, _argv: &[String], root: &str) -> Option<TestOutcome> {
                *self.0.borrow_mut() = root.to_string();
                Some(TestOutcome {
                    exit_code: 0,
                    elapsed_ms: 0,
                })
            }
        }
        let recorder = RootRecorder(std::cell::RefCell::new(String::new()));
        let _ = evidence_for("failed", &recorder);
        assert_eq!(*recorder.0.borrow(), "/tmp");
    }

    #[test]
    fn the_plugin_can_say_no_verdict_because_the_type_has_nowhere_to_put_one() {
        let response = handle(&envelope(&body_with("failed")), &ran(0)).expect("answered");
        let json = serde_json::to_string(&response).expect("serializes");
        // `judge` is the host's. There is nowhere in the wire shape for a
        // plugin to put an answer, and this test is what says so out loud.
        for forbidden in ["verdict", "done", "satisfied", "requirement", "unmet"] {
            assert!(!json.contains(forbidden), "{json} leaked `{forbidden}`");
        }
    }

    /// #3499, structurally: `tamper` is not a word this plugin can say. The
    /// host owns the artifact-identity finding and merges its own in.
    #[test]
    fn the_plugin_cannot_report_a_tamper_finding() {
        let response = handle(&envelope(&body_with("failed")), &ran(0)).expect("answered");
        let json = serde_json::to_string(&response).expect("serializes");
        assert!(
            !json.contains("tamper"),
            "{json} claimed the host's finding"
        );
    }

    #[test]
    fn an_unknown_body_field_is_refused_because_the_wire_denies_them() {
        let body = r#"{"protocol_version":1,"a_field_a_newer_host_added":1}"#;
        let err = read_request(&envelope(body)).expect_err("refused");
        assert_eq!(
            err.0,
            "the request denies unknown fields; got a_field_a_newer_host_added"
        );
    }

    /// The denial reaches the nested tables, which is where a host that grew a
    /// field would actually put it.
    #[test]
    fn an_unknown_field_inside_the_grant_is_refused_too() {
        let body = r#"{"protocol_version":1,"candidate":{"handle":"c","root":"/tmp","extra":1}}"#;
        let err = read_request(&envelope(body)).expect_err("refused");
        assert_eq!(err.0, "the request denies unknown fields; got extra");
    }

    #[test]
    fn a_baseline_outside_the_closed_set_is_refused() {
        let body = r#"{"protocol_version":1,"candidate":{"handle":"c","root":"/tmp","test":{"program":"true","baseline":"flaky"}}}"#;
        let err = read_request(&envelope(body)).expect_err("refused");
        assert_eq!(
            err.0,
            "TestBaseline is a closed set {not-run, passed, failed, unobserved}; got \"flaky\""
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
            .run(&["false".to_string()], "/tmp")
            .expect("a failing command still ran");
        assert_ne!(outcome.exit_code, 0);
    }

    #[test]
    fn the_real_runner_answers_none_for_a_command_it_cannot_start() {
        assert!(ProcessRunner
            .run(&["stella-no-such-program-exists".to_string()], "/tmp")
            .is_none());
    }

    #[test]
    fn the_real_runner_answers_none_for_a_root_that_does_not_exist() {
        assert!(ProcessRunner
            .run(&["true".to_string()], "/tmp/stella-no-such-candidate-root")
            .is_none());
    }

    #[test]
    fn the_envelope_serializes_in_the_shape_the_host_reads() {
        let response = WrapperResponse::AfterTurn(AfterTurnResponse {
            protocol_version: PROTOCOL_VERSION,
            evidence: ObservedEvidence::nothing(),
        });
        assert_eq!(
            serde_json::to_string(&response).expect("serializes"),
            r#"{"point":"after_turn","body":{"protocol_version":1,"evidence":{"flip":"unobservable"}}}"#
        );
    }
}
