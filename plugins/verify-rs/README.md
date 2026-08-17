# `verify`, in Rust — the reference implementation

The `after_turn` wrapper-socket point, twice: as the **wire path** every host
uses, and as an **in-process path** an embedding host can call directly.

Track C rule 2 is why both exist and why the wire one is the one CI grades:
*"the Rust example uses the wire path. It may additionally have an in-process
path, but if only Rust can reach a capability, the wire contract is
second-class and will rot."*

## What it does

Once a turn completes, Stella sends this plugin an `after_turn` request. The
plugin runs a test command and answers with an evidence set: what it saw of the
fail→pass flip, what its tamper check found, and the numbers it measured. It
reports **no verdict** — Stella decides done from the rule
[`plugin.toml`](plugin.toml) declares as data.

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
| [`src/main.rs`](src/main.rs) | the wire entrypoint: stdin → response on stdout, or a refusal on stderr with a non-zero exit. The only place the program touches ambient state, and it passes `std::env::var` in as a lookup so everything above stays pure |
| [`tests/wire.rs`](tests/wire.rs) | spawns the **compiled binary** against the shared goldens. Imports nothing from the library: this test knows only what a host knows |

## Build and install

```bash
cargo build --release
mkdir -p bin && cp target/release/verify-rs bin/verify-rs

mkdir -p .stella/plugins/verify
cp -r plugin.toml bin .stella/plugins/verify/
```

`[runtime].argv` is `["${plugin_dir}/bin/verify-rs"]` — a single compiled
binary, which is the cheapest thing the host can spawn (about 1.2 ms against
~21 ms for an interpreted plugin; `doc:plugin-transport-spike` §2).

## Run it by hand

```bash
echo '{"point":"after_turn","body":{"protocol_version":1,"wrapper":"verify-v1",
  "round":0,"goal":"go","candidate":"candidate-1","turn":{"completed":true}}}' \
| VERIFY_TEST_COMMAND='["cargo","test","--quiet"]' VERIFY_BASELINE_EXIT_CODE=1 \
  ./bin/verify-rs
```

With `VERIFY_TEST_COMMAND` or `VERIFY_BASELINE_EXIT_CODE` unset it answers
`{"flip":"unobservable","tamper":"not-checked"}` — the honest evidence for "I
could not observe anything", rather than a guess. Both names are declared in
`[runtime].env`, which is default-deny; see [`../README.md`](../README.md)
§ "What the wire could not say" for why they exist at all.

## Test

```bash
cargo test
```
