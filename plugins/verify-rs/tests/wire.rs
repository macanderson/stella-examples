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

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The one value that is wall clock and cannot be golden.
const WALL_CLOCK_MEASUREMENT: &str = "test-duration-ms";

/// The `PATH` the vectors run with. Default-deny like `[runtime].env`: the
/// child sees this and whatever the vector's `.env.json` adds, and nothing of
/// the ambient environment.
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

/// Feed one request to the compiled binary under one exact environment.
fn ask(request: &[u8], env: &BTreeMap<String, String>) -> Answer {
    let mut command = Command::new(env!("CARGO_BIN_EXE_verify-rs"));
    command.env_clear().env("PATH", VECTOR_PATH);
    for (name, value) in env {
        command.env(name, value);
    }
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
        vectors.len() >= 8,
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

        let env_path = sibling(".env.json");
        let env: BTreeMap<String, String> = if env_path.exists() {
            serde_json::from_value(read_json(&env_path)).expect("the env map is strings")
        } else {
            BTreeMap::new()
        };

        let request = std::fs::read(&vector).expect("the vector is readable");
        let answer = ask(&request, &env);

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
    let answer = ask(b"", &BTreeMap::new());
    assert!(!answer.success);
    assert_eq!(
        answer.stderr.trim(),
        "verify: stdin was not a single JSON object"
    );
}

#[test]
fn the_plugin_sees_only_the_environment_it_was_given() {
    // `[runtime].env` is default-deny, and the vectors run that way. A plugin
    // that quietly read an inherited variable would pass its own tests and
    // fail on a host that withheld it, so the harness withholds everything.
    let mut env = BTreeMap::new();
    env.insert("VERIFY_TEST_COMMAND".to_string(), r#"["true"]"#.to_string());
    env.insert("VERIFY_BASELINE_EXIT_CODE".to_string(), "1".to_string());
    let request = std::fs::read(testdata_dir().join("01-flip.request.json")).expect("readable");
    let answer = ask(&request, &env);
    assert!(answer.success, "{}", answer.stderr);
    assert!(answer.stdout.contains(r#""flip":"achieved""#));
}
