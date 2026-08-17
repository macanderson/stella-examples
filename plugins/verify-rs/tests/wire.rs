//! Track C rule 2, as a test: **the Rust plugin is judged on the wire path.**
//!
//! `lib.rs` exposes an in-process path and that is allowed — but if the wire
//! path were exercised only by Python and TypeScript it would become
//! second-class and rot (`doc:pipeline-as-plugins` §5.2). So this file spawns
//! the *compiled binary*, writes a request on its stdin, and grades stdout (or
//! stderr, for the refusal vectors) against the exact same
//! `plugins/testdata/` goldens `plugins/ci/conformance.py` runs the other two
//! implementations against.
//!
//! Nothing here imports `verify_rs`. That is deliberate: this test knows only
//! what a host knows.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The one value that is wall clock and cannot be golden.
const WALL_CLOCK_MEASUREMENT: &str = "test-duration-ms";

/// The whole environment the vectors run with. Default-deny like
/// `[runtime].env`, whose entire allowlist is now `PATH`: the child sees this
/// and nothing of the ambient environment, because everything it acts on
/// arrives in the request (#3498).
const VECTOR_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

fn testdata_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("plugins/verify-rs has a parent")
        .join("testdata")
}

struct Answer {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Feed one request to the compiled binary with `PATH` and nothing else.
fn ask(request: &[u8]) -> Answer {
    let mut command = Command::new(env!("CARGO_BIN_EXE_verify-rs"));
    command.env_clear().env("PATH", VECTOR_PATH);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the plugin binary spawns");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(request)
        .expect("the host can write a request");
    let output = child.wait_with_output().expect("the plugin answers");
    Answer {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Replace the wall-clock measurement with the golden's value, after checking
/// it is a non-negative integer — so the normalization can never hide a bug in
/// the field it normalizes.
fn normalize(actual: &mut serde_json::Value, golden: &serde_json::Value) {
    let pointer = format!("/body/evidence/measurements/{WALL_CLOCK_MEASUREMENT}");
    let Some(reported) = actual.pointer_mut(&pointer) else {
        return;
    };
    let elapsed = reported
        .as_u64()
        .unwrap_or_else(|| panic!("{WALL_CLOCK_MEASUREMENT} must be a non-negative integer"));
    assert!(elapsed < u64::MAX, "{WALL_CLOCK_MEASUREMENT} overflowed");
    *reported = golden
        .pointer(&pointer)
        .cloned()
        .unwrap_or_else(|| serde_json::json!(0));
}

fn read_json(path: &Path) -> serde_json::Value {
    let bytes =
        std::fs::read(path).unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("{} is JSON: {err}", path.display()))
}

#[test]
fn the_compiled_binary_answers_every_shared_vector_exactly() {
    let dir = testdata_dir();
    let mut vectors: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("{} is readable: {err}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".request.json"))
        })
        .collect();
    vectors.sort();

    assert!(
        vectors.len() >= 17,
        "expected the shared vectors at {}, found {}",
        dir.display(),
        vectors.len()
    );

    for vector in vectors {
        let name = vector
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a vector has a name")
            .to_string();
        let sibling = |suffix: &str| dir.join(name.replace(".request.json", suffix));

        let request = std::fs::read(&vector).expect("the vector is readable");
        let answer = ask(&request);

        let refusal_path = sibling(".refusal.txt");
        if refusal_path.exists() {
            let expected = std::fs::read_to_string(&refusal_path).expect("readable");
            assert!(
                !answer.success,
                "{name}: expected a refusal, got exit 0 with stdout {}",
                answer.stdout
            );
            assert_eq!(answer.stderr.trim(), expected.trim(), "{name}: stderr");
            assert!(
                answer.stdout.trim().is_empty(),
                "{name}: a refusing plugin must print nothing on stdout"
            );
            continue;
        }

        assert!(
            answer.success,
            "{name}: expected an answer, got a refusal: {}",
            answer.stderr
        );
        let golden = read_json(&sibling(".expected.json"));
        let mut actual: serde_json::Value = serde_json::from_str(&answer.stdout)
            .unwrap_or_else(|err| panic!("{name}: stdout was not JSON ({err}): {}", answer.stdout));
        normalize(&mut actual, &golden);
        assert_eq!(actual, golden, "{name} did not match its golden");
    }
}

#[test]
fn a_plugin_that_is_asked_nothing_refuses_rather_than_answering() {
    let answer = ask(b"");
    assert!(!answer.success);
    assert_eq!(
        answer.stderr.trim(),
        "verify: stdin was not a single JSON object"
    );
}

/// **The witness for #3498**, and the reason this file no longer has an
/// environment parameter: the plugin observes a flip with `PATH` as its whole
/// environment, because the candidate root and the test invocation arrived in
/// the request. The same request used to answer `unobservable` here unless the
/// harness also set `VERIFY_TEST_COMMAND` and `VERIFY_BASELINE_EXIT_CODE`.
///
/// The falsifier is the second half: strip the grant's `test` out of the very
/// same request and the flip collapses to `unobservable`, which is what proves
/// the answer above came from the request rather than from anything ambient.
#[test]
fn the_plugin_needs_no_environment_beyond_path() {
    let path = testdata_dir().join("01-flip.request.json");
    let request = std::fs::read(&path).expect("readable");
    let answer = ask(&request);
    assert!(answer.success, "{}", answer.stderr);
    assert!(
        answer.stdout.contains(r#""flip":"achieved""#),
        "the grant carried the test and the baseline: {}",
        answer.stdout
    );

    let mut stripped = read_json(&path);
    stripped
        .pointer_mut("/body/candidate")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the vector carries a grant")
        .remove("test");
    let answer = ask(stripped.to_string().as_bytes());
    assert!(answer.success, "{}", answer.stderr);
    assert!(
        answer.stdout.contains(r#""flip":"unobservable""#),
        "with no test in the grant there is nothing to observe: {}",
        answer.stdout
    );
}
