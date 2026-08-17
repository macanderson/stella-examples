//! The wire entrypoint: one JSON request on stdin, one JSON response on
//! stdout, or a refusal on stderr with a non-zero exit.
//!
//! This is the path CI exercises, and that is Track C rule 2
//! (`doc:pipeline-as-plugins` §9): the Rust plugin may *additionally* expose
//! the in-process path in `lib.rs`, but if the wire path were exercised only
//! by Python and TypeScript it would be second-class and would rot.
//!
//! The process model is `doc:plugin-transport-spike` §5's decision: the host
//! spawns `[runtime].argv` directly — no shell — writes the request, closes
//! stdin, and reads one response. There is no handshake, no framing and no
//! state between calls, which is what makes the same protocol writable in
//! forty lines of Python.
//!
//! This file is the only place the program touches ambient state, and it does
//! so through the one seam `lib.rs` left for it: `std::env::var` is passed in
//! as a lookup, so everything above stays pure.

use std::io::{Read, Write};

use verify_rs::{handle, ProcessRunner};

fn main() -> std::process::ExitCode {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        // Unreadable stdin is indistinguishable from unparsable stdin, and
        // both are refusals rather than crashes.
        eprintln!("verify: stdin was not a single JSON object");
        return std::process::ExitCode::FAILURE;
    }

    let response = match handle(&raw, |name| std::env::var(name).ok(), &ProcessRunner) {
        Ok(response) => response,
        Err(refusal) => {
            eprintln!("verify: {refusal}");
            return std::process::ExitCode::FAILURE;
        }
    };

    // `WrapperResponse` holds no type that can fail to serialize, so the
    // fallback is unreachable — but a plugin that panicked on its way out
    // would look to the host exactly like one that crashed mid-test, and the
    // host would then have to guess. It never has to.
    let encoded = match serde_json::to_string(&response) {
        Ok(encoded) => encoded,
        Err(err) => {
            eprintln!("verify: the response could not be encoded: {err}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut stdout = std::io::stdout();
    if writeln!(stdout, "{encoded}").is_err() || stdout.flush().is_err() {
        eprintln!("verify: the response could not be written");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
