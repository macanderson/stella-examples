# `verify`, in TypeScript

The `after_turn` wrapper-socket point, with **zero runtime dependencies** —
not even `@types/node`. `package.json` has an empty `dependencies` and one dev
dependency: the TypeScript compiler, which is a build tool and not an SDK. The
Node surface the plugin uses is hand-declared in
[`src/node-min.d.ts`](src/node-min.d.ts), fourteen lines, eight of them comment.

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

## Build

The build step, shown rather than assumed, because `[runtime].argv` names the
*output* and not the source:

```bash
npm install     # fetches tsc, nothing else
npm run build   # tsc -> dist/main.js
```

`tsconfig.json` sets `"types": []` and `"lib": ["ES2022"]` so the compiler pulls
in neither `@types/*` nor the DOM — the plugin compiles against the handful of
ambient declarations it actually uses. That list got *shorter* in #3516:
`process.env` is gone from [`src/node-min.d.ts`](src/node-min.d.ts), so a plugin
that tried to read an environment variable would not compile.

## Install

```bash
npm run build
stella plugin install .              # this workspace
stella plugin install . --scope user # every workspace
```

`install` prints the whole declaration — including the disclosure that this
plugin reports its own evidence — and installs nothing until you accept it.
`[runtime].argv` is `["node", "${plugin_dir}/dist/main.js"]`, so build before
you install: the host runs `dist/`, and never invokes a compiler.

## Run it by hand

```bash
echo '{"point":"after_turn","body":{"protocol_version":1,"wrapper":"verify-v1",
  "round":0,"goal":"go","candidate":{"handle":"candidate-1","root":"/tmp",
  "test":{"program":"npm","args":["test"],"baseline":"failed"}},
  "turn":{"completed":true}}}' \
| node dist/main.js
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
npm test        # builds first, then `node --test test/wire.test.js`
```

Two layers: the shared conformance suite that grades all three languages
against one set of goldens, and process-level tests for what a golden cannot
reach cheaply — a signal-killed test, a root that does not resolve, a malformed
grant, and the property that no verdict word and no `tamper` key ever appears in
a response.
