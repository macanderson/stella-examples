# `verify`, in TypeScript

The `after_turn` wrapper-socket point, with **zero runtime dependencies** —
not even `@types/node`. `package.json` has an empty `dependencies` and one dev
dependency: the TypeScript compiler, which is a build tool and not an SDK. The
Node surface the plugin uses is hand-declared in
[`src/node-min.d.ts`](src/node-min.d.ts), ten lines.

## What it does

Once a turn completes, Stella sends this plugin an `after_turn` request. The
plugin runs a test command and answers with an evidence set: what it saw of the
fail→pass flip, what its tamper check found, and the numbers it measured. It
reports **no verdict** — Stella decides done from the rule
[`plugin.toml`](plugin.toml) declares as data.

## Build

The build step, shown rather than assumed, because `[runtime].argv` names the
*output* and not the source:

```bash
npm install     # fetches tsc, nothing else
npm run build   # tsc -> dist/main.js
```

`tsconfig.json` sets `"types": []` and `"lib": ["ES2022"]` so the compiler pulls
in neither `@types/*` nor the DOM — the plugin compiles against the ten lines of
ambient declarations it actually uses.

## Install

```bash
npm run build
mkdir -p .stella/plugins/verify
cp -r plugin.toml dist .stella/plugins/verify/
```

`[runtime].argv` is `["node", "${plugin_dir}/dist/main.js"]`. Ship `dist/`, not
`src/` — the host runs the built artifact and never invokes a compiler.

## Run it by hand

```bash
echo '{"point":"after_turn","body":{"protocol_version":1,"wrapper":"verify-v1",
  "round":0,"goal":"go","candidate":"candidate-1","turn":{"completed":true}}}' \
| VERIFY_TEST_COMMAND='["npm","test"]' VERIFY_BASELINE_EXIT_CODE=1 \
  node dist/main.js
```

With `VERIFY_TEST_COMMAND` or `VERIFY_BASELINE_EXIT_CODE` unset it answers
`{"flip":"unobservable","tamper":"not-checked"}` — the honest evidence for "I
could not observe anything", rather than a guess. Both names are declared in
`[runtime].env`, which is default-deny; see [`../README.md`](../README.md)
§ "What the wire could not say" for why they exist at all.

## Test

```bash
npm test        # builds first, then `node --test test/wire.test.js`
```

Two layers: the shared conformance suite that grades all three languages
against one set of goldens, and process-level tests for what a golden cannot
reach cheaply — a signal-killed test, a malformed grant.
