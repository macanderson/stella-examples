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

They are the same plugin on purpose. `verify` runs the test the request's
candidate grant names, in the workspace root that grant carries, and reports
what it observed: whether the test went red-to-green, and what it exited with.
It reports **no verdict**: Stella decides done from the rule the manifest
declares as data. Diff the three directories and the only differences are three
languages, never three protocols.

Why this matters, in the words of the design doc that asks for it
(`doc:pipeline-as-plugins` §9): *"a plugin surface exercised only by its
authors' language is a library with extra steps."* These three are the test
that Stella's plugin surface is a platform.

> **Verification is delivered by the plugin, and the install prompt says so.**
> Stella does not run this plugin's oracle and does not re-check what comes
> back: the flip and every measurement are what `verify`'s own process said it
> saw. Stella applies the declared rule to those reported claims and will not
> credit a requirement they leave undecided, and it owns the tamper finding a
> plugin cannot write — but it cannot tell an earned result from a typed one.
> That is the architecture rather than an apology (#3511): base Stella verifies
> nothing, and a verification plugin is the product.

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

Three rules worth knowing before you write a manifest:

- **An undeclared hook is never invoked.** Registering for an event the
  manifest did not name gets you nothing, silently — the manifest is the
  authority, not the process.
- **An undeclared socket point is never dispatched.** `[loop].points` names the
  wrapper points the plugin answers; `verify` declares `["after_turn"]` and
  nothing else. Before #3501 there was nowhere to say this, so a host learned
  that a plugin refuses `before_turn` by *getting the refusal at run time*.
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
| `after_turn` | the plugin, out of process | gather evidence: run the granted test, read a diff. Receives the turn's outcome; holds no channel into it. |
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
          "candidate": {"handle": "candidate-1",
                        "root": "/var/folders/…/candidate-1",
                        "test": {"program": "pytest",
                                 "args": ["-q", "tests/test_flip.py"],
                                 "baseline": "failed"}},
          "turn": {"completed": true, "changed_files": ["src/lib.rs"]}}}

// stdout
{"point": "after_turn",
 "body": {"protocol_version": 1,
          "evidence": {"flip": "achieved",
                       "measurements": {"test-command-exit-code": 0,
                                        "test-duration-ms": 41}}}}
```

Four properties of that exchange are worth stating, because each one is a
decision someone can get wrong:

1. **`protocol_version` rides on every message and the contract is
   additive-only** — but every table also *denies unknown fields*, the envelope
   included. Those are not in tension: a field the host does not know, at a
   version it accepts, is a typo, and a message that quietly does nothing is
   worse than one that refuses. A genuinely new field arrives either as an
   optional one at the same version (`stage` did) or with a version bump.
2. **There is no error variant.** `AfterTurnResponse` cannot carry a failure.
   A plugin that cannot answer *fails* — non-zero exit, one line on stderr,
   nothing on stdout — and the host substitutes the "nothing was observed"
   evidence set on its behalf. That set is deliberately not an empty one that
   reads as "nothing was wrong": it makes `judge` abstain rather than blame the
   worker for evidence nobody collected.
3. **Every capability arrives in the request.** `candidate` is a
   `CandidateGrant`: the handle, the canonical workspace `root`, and — when the
   host has one to give — the `test` to run there, as argv and a baseline. A
   wrapper never reaches for a terminal, a git checkout, a credential, an
   environment variable or a working directory. That is what lets the same
   plugin run under the CLI, under `stella-serve`, and inside an application
   that embedded the loop.
4. **A plugin cannot report a tamper finding.** `ObservedEvidence` has no field
   for one, in any language. Snapshotting witness-artifact identity is host-side
   by design, and the host merges its own finding in before `judge` runs
   (#3499).

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

`plugins/testdata/` holds the vectors. Each is exactly two files:

| file | meaning |
| --- | --- |
| `NN-name.request.json` | written to the plugin's stdin |
| `NN-name.expected.json` | the plugin must exit 0 and print this |
| `NN-name.refusal.txt` | the plugin must exit non-zero and print this on stderr |

A vector carries an `expected.json` **or** a `refusal.txt`, never both. Every
plugin is spawned with `PATH` and nothing else — which is the whole
`[runtime].env` allowlist now, and is the sharpest way to state #3498's result:
a vector that needed an environment would mean the plugin reached for something
the host did not send. The vectors name `/tmp` as the grant root because a
vector has to name a directory that exists on any POSIX runner; a real host
mints the canonical root of the candidate worktree.

`.github/workflows/plugins.yml` runs all of it on every PR — Track C rule 4.

---

## The four rules, and how they held

`doc:pipeline-as-plugins` §9 states four rules that keep this honest. They are
the deliverable, not decoration.

**1. Identical manifests except `[runtime].argv`.** *Held, exactly.*
The three `plugin.toml` files are generated from one template and differ in
**two lines** — one `argv` removed, one added. Nothing else differs: not a key,
not a value, not a comment. `check-manifests-identical.py` re-establishes that
from the committed files (structurally with `tomllib`, and as a literal line
diff), and fails if it ever stops being true. **No language needed a manifest
shape another did not.**

It was four lines in the first cut, because `[oracle].command` was mandatory
and every manifest wrote its `[runtime].argv` out a second time there, byte for
byte. #3501 made it optional beside a `[runtime]` block — absent now means "the
oracle is this plugin's own process" — so the redundant declaration is gone and
the script refuses a manifest that brings it back.

**2. The Rust example uses the wire path.** *Held.*
`verify-rs/tests/wire.rs` spawns the *compiled binary* and drives it through
stdin/stdout against the same goldens the other two run; it does not import the
library. The in-process path exists as well — `lib.rs` exposes `observe()` with
the test runner injected — which rule 2 explicitly allows, but the wire path is
the one CI grades.

**3. No SDK in the first cut.** *Held.*
Python is `json`, `subprocess`, `sys`, `time` — one import fewer than before,
because there is no environment left to read. TypeScript has an empty
`dependencies` and one dev dependency, the compiler, with the Node surface it
uses hand-declared in a short `.d.ts` that also **shrank**: `process.env` is
gone, so a plugin that tried to read one would not compile. Rust depends on
`serde`/`serde_json` and **deliberately not on `stella-plugin`**: a third-party
author could not, so the reference implementation does not either.

**4. CI runs all three on every PR.** *Held.*
See `.github/workflows/plugins.yml`.

---

## What the contract fixed, and how it reached here

Track C exists to find contract defects by building against it. Four of the
findings from the first cut were fixed in `macanderson/stella` and are the
reason this directory changed:

- **#3498 — the request carries the candidate.** `candidate` was a bare handle
  string with no root and no way to spend it, so all three plugins took their
  test command and pre-turn baseline from two `[runtime].env` names
  (`VERIFY_TEST_COMMAND`, `VERIFY_BASELINE_EXIT_CODE`). It is a `CandidateGrant`
  now, and those names are **deleted**: from the code, from the manifests, and
  from this documentation.
- **#3499 — tamper is host-owned.** `EvidenceSet.tamper` was a field the plugin
  filled in, while the snapshot it reports on is host-side, so an honest plugin
  could only ever say `not-checked` and `judge` — correctly refusing to credit
  a flip nobody checked the artifacts of — returned *undecided* forever. What a
  plugin returns is now an `ObservedEvidence`, which has no such field.
- **#3500 — the envelope denies unknown fields.** These plugins refused an extra
  key beside `point` and `body` from the first cut, which made them stricter
  than the host's own decoder. The host now refuses it too.
- **#3501 — a manifest declares its points.** `[loop].points = ["after_turn"]`,
  and `[oracle].command` became optional beside `[runtime]`.

The tracking issue for this half of the work is
[`macanderson/stella#3516`](https://github.com/macanderson/stella/issues/3516).

## What the grammar still cannot say

Reported rather than worked around.

- **The manifest filename is not specified.** These use `plugin.toml`; nothing
  in the crate or the spec fixes that yet, so it is a guess that a loader may
  contradict.
- **A plugin cannot declare the measurement it reports as absent-on-purpose.**
  `[oracle].measurements` names the numbers a plugin *may* report, and a name
  missing from a response is "missing, never a satisfied budget" — which is the
  right default. But `verify` reports no numbers at all when the test could not
  be run, and there is no way to distinguish that from a plugin that simply
  forgot: both arrive as an absent name.

## What the wire still cannot say

- **A plugin cannot say *why* a flip was unobservable.** `unobservable` is one
  value covering a root that does not resolve, a program that is not there, a
  run that outlived its budget, and a baseline that never watched an assertion.
  Those are four different operational problems and the host is told the same
  thing about each.
- **A plugin cannot report `unsatisfiable`.** `FlipObservation` has the variant
  — the witness failing *the same way* before and after (#2540) — but the
  baseline a plugin receives is one of four enum values, not a failure it can
  compare against the one it just saw. Nothing an out-of-process plugin is given
  can distinguish "red for the same reason" from "red for a different one", so
  this plugin never emits the value in any language.
- **A `TestPlan` names no environment and no timeout.** The host says which
  program to run and where, but not what environment it would have run it in or
  how long it would have waited. So "the same invocation" is the same argv, not
  provably the same run — and each plugin has to pick its own budget (240s here,
  bounded inside `[runtime].timeout_secs`).
- **The version check is the reader's, not the type's.** A body carrying
  `protocol_version: 2` still decodes as the real `WrapperRequest`; the host
  compares the number separately. These plugins refuse it (vector
  `06-bad-version`), which is the behaviour the contract asks for, but it is a
  behaviour every plugin author has to remember rather than one the shape
  enforces.

---

## Installing one

The installer exists now:

```bash
stella plugin install plugins/verify-py            # this workspace
stella plugin install plugins/verify-py --scope user   # every workspace
stella plugin list
stella plugin remove verify                        # by manifest name
```

`install` prints the whole declaration — the grade, the hooks, the requirements,
the process it runs as, the environment slice it inherits, the capabilities it
asks for, and the "it reports its own evidence" disclosure — and installs
nothing until you accept it. Project scope is `<workspace>/.stella/plugins/`,
user scope is `~/.stella/plugins/`, and `${plugin_dir}` in the manifest's argv
resolves to whichever it landed in.

The other half exists now too. `stella run --pipeline verify` — naming this
manifest's own `name` — hands the turn to the wrapper socket, and a real
turn's `after_turn` request reaches the installed plugin's process:
`grep -rn after_turn crates/stella-cli/src` now finds the live dispatcher
(`wrapper_plugin.rs`), not just the install command's own module. `stella
goal` drives the same socket per round, but only at `steering`/`observer`
grade — arbiter participation, the grade `verify` asks for, is refused
pre-flight there (#3832), because a goal run is already one loop with its
own arbiter and does not nest a second one inside it. So these three plugins
are graded two ways today: the shared conformance harness above, against the
wire contract in isolation, and `stella run --pipeline verify`, against a
real turn.
