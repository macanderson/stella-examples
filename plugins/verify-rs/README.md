# `verify`, in Rust — the reference implementation

The `after_turn` wrapper-socket point, twice: as the **wire path** every host
uses, and as an **in-process path** an embedding host can call directly.

Track C rule 2 is why both exist and why the wire one is the one CI grades:
*"the Rust example uses the wire path. It may additionally have an in-process
path, but if only Rust can reach a capability, the wire contract is
second-class and will rot."*

## What it does

Once a turn completes, Stella sends this plugin an `after_turn` request
carrying the **candidate grant**: the workspace root, and the test invocation to
run in it as a program, an argument vector, and what that same invocation
reported before the turn. The plugin runs that test and answers with what it
observed — the fail→pass flip and the numbers it measured. It reports **no
verdict**, and it cannot report a tamper finding: that one is the host's own
(#3499).

**Stella does not run this oracle and does not re-check what comes back.** The
flip and every measurement are what this plugin's process said it saw; Stella
applies the rule [`plugin.toml`](plugin.toml) declares to those reported claims
and will not credit a requirement they leave undecided, but it cannot tell an
earned result from a typed one. That is what the install prompt says, and it is
the architecture rather than an apology: verification is delivered by the
plugin.

## It does not depend on `stella-plugin`

Deliberately. A plugin author outside the Stella workspace could not, and rule 3
says a plugin must be writable with a JSON parser and nothing else — so the Rust
example is held to the same bar as the Python one. [`src/lib.rs`](src/lib.rs)
re-declares the wire shapes as a third-party author would after reading
`crates/stella-plugin/src/wire.rs`. If they ever drift from the real contract,
that is a fact worth discovering here rather than in a user's plugin.

The two dependencies are `serde` and `serde_json`: a JSON parser, which rule 3
allows in as many words.

## Layout

| File | What it is |
| --- | --- |
| [`src/lib.rs`](src/lib.rs) | the wire shapes, plus `observe()` — the in-process path, synchronous, with the test runner injected through a `TestRunner` seam so it spawns nothing in a unit test |
| [`src/main.rs`](src/main.rs) | the wire entrypoint: stdin → response on stdout, or a refusal on stderr with a non-zero exit. It touches no ambient state at all — it used to pass `std::env::var` in as a lookup, and #3498 removed the seam rather than leaving it unused |
| [`tests/wire.rs`](tests/wire.rs) | spawns the **compiled binary** against the shared goldens. Imports nothing from the library: this test knows only what a host knows |

## Build and install

```bash
cargo build --release
mkdir -p bin && cp target/release/verify-rs bin/verify-rs

stella plugin install .              # this workspace
stella plugin install . --scope user # every workspace
```

`install` prints the whole declaration — including the disclosure that this
plugin reports its own evidence — and installs nothing until you accept it.
Build before you install: `[runtime].argv` names `bin/verify-rs`, and the host
never invokes a compiler.

`[runtime].argv` is `["${plugin_dir}/bin/verify-rs"]` — a single compiled
binary, which is the cheapest thing the host can spawn (about 1.2 ms against
~21 ms for an interpreted plugin; `doc:plugin-transport-spike` §2).

## Run it by hand

```bash
echo '{"point":"after_turn","body":{"protocol_version":1,"wrapper":"verify-v1",
  "round":0,"goal":"go","candidate":{"handle":"candidate-1","root":"/tmp",
  "test":{"program":"cargo","args":["test","--quiet"],"baseline":"failed"}},
  "turn":{"completed":true}}}' \
| ./bin/verify-rs
```

Nothing else is set: no environment variable, no working directory, no flags.
The grant carries the root and the invocation, which is the whole of #3498. With
no `test` in the grant — or no `candidate` at all — it answers
`{"flip":"unobservable"}`, the honest evidence for "I could not observe
anything", rather than a guess.

A `baseline` of `not-run` or `unobserved` gets the same `unobservable` flip: a
run that never watched an assertion is not red, so a green run after it is not a
flip — and it is not the worker's fault either (#860).

## Test

```bash
cargo test
```
