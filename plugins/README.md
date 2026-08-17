# Plugins — the same plugin, in three languages

A **plugin** is a directory with a `plugin.toml` manifest and (usually) a
program. The manifest declares what say the plugin wants in Stella's turn loop
and what it wants to reach outside it; a human reads that declaration at
install and says yes or no. Nothing a plugin does is inferred.

This directory holds **one plugin, implemented three times**:

| Path | Language | Entry point |
| --- | --- | --- |
| [`verify-rs/`](verify-rs/) | Rust | a compiled binary, `bin/verify-rs` |
| [`verify-py/`](verify-py/) | Python | `main.py`, stdlib only |
| [`verify-ts/`](verify-ts/) | TypeScript | `dist/main.js`, built with `tsc` |

They are the same plugin on purpose. `verify` runs a test command after a turn
completes and reports what it saw — whether the test went red-to-green, and how
it exited. It reports **no verdict**: Stella decides done from the rule the
manifest declares as data. Diff the three directories and the only differences
are three languages, never three protocols.

Why this matters, in the words of the design doc that asks for it
(`doc:pipeline-as-plugins` §9): *"a plugin surface exercised only by its
authors' language is a library with extra steps."* These three are the test
that Stella's plugin surface is a platform.

---

## Participation grades — a ladder, declared

`[loop].participation` is how much say a plugin has asked for. Each grade
includes every grade below it, and the powers that separate the rungs are
rejected below the rung that grants them, so a manifest cannot quietly hold
more than its grade.

| Grade | May | Declared by |
| --- | --- | --- |
| `none` | nothing — a content bundle of skills, commands, agents, custom tools | the default when there is no `[loop]` block |
| `observer` | subscribe to the turn event stream; influence nothing | `participation = "observer"` |
| `steering` | act at declared hook points — inject context, rewrite tool input, decide permissions. May not touch completion | `participation = "steering"` + `hooks = [...]` |
| `arbiter` | additionally bind the `Stop` gate: at each would-be completion Stella asks the plugin, and an unmet requirement re-enters the loop | `participation = "arbiter"` + `hooks = ["Stop", ...]` + `[requirements]` |

`verify` is an **arbiter**, because holding a turn open until its tests flip is
exactly what the top rung is for.

Two rules worth knowing before you write a manifest:

- **An undeclared hook is never invoked.** Registering for an event the
  manifest did not name gets you nothing, silently — the manifest is the
  authority, not the process.
- **Unknown keys are a load error.** A typo'd grant fails loudly at install
  rather than granting nothing quietly.

`[[capabilities]]` is orthogonal to the ladder: it declares what the plugin
wants to reach *outside* the turn (a tool, at a named risk level, with the
reason a human reads). A `none`-grade content bundle shipping one custom tool
that runs `git push` is asking for more of the world than an `observer` that
only watches, so tying the two together would let the widest grant hide behind
the weakest grade.

---

## The four points

A wrapper plugin is **two calls it answers, plus two functions the host runs on
its behalf** (`doc:wrapper-socket` §1):

| Point | Who runs it | What it is |
| --- | --- | --- |
| `before_turn` | the plugin, out of process | contribute context, narrow scope, name a role intent, publish signals. May not run the loop, may not reach for ambient authority. |
| `after_turn` | the plugin, out of process | gather evidence: run a test, read a diff, author a witness. Receives the turn's outcome; holds no channel into it. |
| `judge` | **the host**, in process | evidence in, verdict out. Synchronous, total, no model call. |
| `again?` | **the host**, in process | verdict in, continuation out: another turn with a correction, or stop. |

**`judge` and `again?` are not messages a plugin can be sent.** `WrapperPoint`
has two variants and neither is a verdict. That is the design, not an
omission: a plugin declares its verdict *rule* as data in `plugin.toml`, and
Stella evaluates it. "A verification plugin quietly calls a model to decide
done" is impossible by construction, in Rust and in Python alike.

The honest cost, stated plainly: **a plugin author cannot write a verdict as a
loop.** What stays open is what counts as evidence and what done means — and
both of those are where the interesting variation actually lives.

### The wire, in full

The host spawns `[runtime].argv` **directly — never through a shell** — writes
one JSON request on stdin, closes it, and reads one JSON response from stdout.
No handshake, no framing, no state between calls. That is the whole transport,
and it was chosen by measurement rather than by argument
(`doc:plugin-transport-spike`).

```jsonc
// stdin
{"point": "after_turn",
 "body": {"protocol_version": 1,
          "wrapper": "verify-v1",
          "round": 0,
          "goal": "make the failing test pass",
          "candidate": "candidate-1",
          "turn": {"completed": true, "changed_files": ["src/lib.rs"]}}}

// stdout
{"point": "after_turn",
 "body": {"protocol_version": 1,
          "evidence": {"flip": "achieved",
                       "tamper": "not-checked",
                       "measurements": {"test-command-exit-code": 0,
                                        "test-duration-ms": 41}}}}
```

Three properties of that exchange are worth stating, because each one is a
decision someone can get wrong:

1. **`protocol_version` rides on every message and the contract is
   additive-only** — but every table also *denies unknown fields*. Those are
   not in tension: a field the host does not know, at a version it accepts, is
   a typo, and a message that quietly does nothing is worse than one that
   refuses. A genuinely new field arrives with a version bump.
2. **There is no error variant.** `AfterTurnResponse` cannot carry a failure.
   A plugin that cannot answer *fails* — non-zero exit, one line on stderr,
   nothing on stdout — and the host substitutes the "nothing was observed"
   evidence set on its behalf. That set is deliberately not an empty one that
   reads as "nothing was wrong": it makes `judge` abstain rather than blame the
   worker for evidence nobody collected.
3. **Every capability arrives in the request.** A wrapper is handed what it
   needs; it never reaches for a terminal, a git checkout, a credential or a
   working directory. That is what lets the same plugin run under the CLI,
   under `stella-serve`, and inside an application that embedded the loop.

---

## Running them

```bash
# the shared conformance suite — the same vectors and goldens for all three
python3 plugins/ci/conformance.py -- python3 plugins/verify-py/main.py
python3 plugins/ci/conformance.py -- node   plugins/verify-ts/dist/main.js
python3 plugins/ci/conformance.py -- plugins/verify-rs/bin/verify-rs

# rule 1, enforced: the manifests differ only in the argv naming the program
python3 plugins/ci/check-manifests-identical.py
python3 plugins/ci/generate-manifests.py --check
```

`plugins/testdata/` holds the vectors. Each is up to four files:

| file | meaning |
| --- | --- |
| `NN-name.request.json` | written to the plugin's stdin |
| `NN-name.env.json` | the exact environment the plugin is given (default-deny) |
| `NN-name.expected.json` | the plugin must exit 0 and print this |
| `NN-name.refusal.txt` | the plugin must exit non-zero and print this on stderr |

`.github/workflows/plugins.yml` runs all of it on every PR — Track C rule 4.

---

## The four rules, and how they held

`doc:pipeline-as-plugins` §9 states four rules that keep this honest. They are
the deliverable, not decoration.

**1. Identical manifests except `[runtime].argv`.** *Held, with one finding.*
The three `plugin.toml` files are generated from one template and differ in
**four lines** — two argv declarations, each written twice because
`[oracle].command` is a second, mandatory process declaration naming the same
program. Nothing else differs: not a key, not a value, not a comment.
`check-manifests-identical.py` re-establishes that from the committed files
(structurally with `tomllib`, and as a literal line diff), and fails if it ever
stops being true. **No language needed a manifest shape another did not.**

**2. The Rust example uses the wire path.** *Held.*
`verify-rs/tests/wire.rs` spawns the *compiled binary* and drives it through
stdin/stdout against the same goldens the other two run; it does not import the
library. The in-process path exists as well — `lib.rs` exposes `observe()` with
the test runner injected — which rule 2 explicitly allows, but the wire path is
the one CI grades.

**3. No SDK in the first cut.** *Held.*
Python is `json`, `os`, `subprocess`, `sys`, `time` — nothing else.
TypeScript has an empty `dependencies` and one dev dependency, the compiler,
with the Node surface it uses hand-declared in a ten-line `.d.ts` so not even
`@types/node` is fetched. Rust depends on `serde`/`serde_json` and
**deliberately not on `stella-plugin`**: a third-party author could not, so the
reference implementation does not either. Each program is under ~250 lines
including comments that are mostly explaining *why*.

**4. CI runs all three on every PR.** *Held.*
See `.github/workflows/plugins.yml`.

---

## What the grammar could not say

Track C exists to find these, so they are reported rather than worked around.

- **`[oracle].command` is not optional, so one plugin declares two processes.**
  A plugin whose evidence comes from the `after_turn` socket point still has to
  name an oracle argv, because `Oracle` has a required `command` field. Both
  lines here name the same program. This is the entire reason rule 1's diff is
  four lines instead of two.
- **A manifest cannot say which socket points a plugin implements.** `verify`
  answers `after_turn` and refuses `before_turn`, and there is nowhere in
  `plugin.toml` to declare that. `[loop].hooks` names *hook* events, not
  wrapper points. A host has to discover the answer by asking and getting a
  refusal.
- **`tamper` has one value and it is mandatory.** `TamperPolicy` is
  `artifact-identity` and `Oracle.tamper` is required, so a plugin that does no
  tamper checking cannot say so. See the next section for why that matters
  here.
- **The manifest filename is not specified.** These use `plugin.toml`; nothing
  in the crate or the spec fixes that yet, so it is a guess that a loader may
  contradict.

## What the wire could not say

- **A plugin cannot use the `CandidateHandle` it is given.** `AfterTurnRequest`
  carries `candidate: Option<CandidateHandle>`, and
  `stella_protocol::CandidateOp` declares six operations a plugin *may* ask for
  — including `run_test`. But the exchange is one-shot: request in, evidence
  out, no callback channel, and no root path anywhere in the request. The only
  implementation of those operations (`CandidateHandles` in `stella-pipeline`)
  is an in-process async Rust API over `&dyn CandidateWorkspacePort`, which is
  precisely the thing an out-of-process plugin cannot be handed.

  So these three plugins take the test command and the pre-turn baseline from
  two environment variables named in `[runtime].env`
  (`VERIFY_TEST_COMMAND`, `VERIFY_BASELINE_EXIT_CODE`). That allowlist is
  default-deny and appears in the install consent text, so it is at least
  *visible* — but it is out of band, and "every capability arrives in the
  request" is the rule it bends. **With either name unset the plugins report
  `unobservable` rather than guessing**, which is the honest answer and keeps
  the example from quietly claiming a pass it did not observe.

- **A plugin can only ever report `tamper: not-checked`.** The tamper snapshot
  is host-side by design — a plugin vouching for its own witness is exactly
  what the policy exists to prevent — but `EvidenceSet.tamper` is a field the
  *plugin* fills in. Combined with the mandatory `tamper = "artifact-identity"`
  above, a manifest that declares the policy plus a plugin that honestly
  reports `not-checked` yields a verdict of *undecided*, forever. One of the
  two halves has to move.

- **The request envelope cannot deny unknown fields.** `WrapperRequest` is an
  adjacently-tagged enum, so serde ignores an extra key beside `point` and
  `body` even though "every table denies unknown fields" is the stated rule.
  These plugins refuse it (vector `09-unknown-envelope-field`), which makes
  them *stricter than the host's own decoder* — verified by round-tripping the
  vectors through the real `stella_plugin::WrapperRequest`.

---

## Installing one

There is no `stella plugin install` yet — the loader is Track A's A4. When it
lands, a plugin will be a directory under `.stella/plugins/<name>/` (project
scope) or `~/.stella/plugins/<name>/` (user scope), and `${plugin_dir}` in the
manifest's argv will resolve to it. Until then, each directory's README shows
how to build and exercise the plugin directly, which is also how CI does it.
