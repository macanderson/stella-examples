# `verify`, in Python

The `after_turn` wrapper-socket point, in 200 lines of Python with **no SDK** —
`json`, `os`, `subprocess`, `sys`, `time`, and nothing else. That is the point:
`doc:pipeline-as-plugins` §9 rule 3 says *"if a plugin cannot be written without
an SDK, the protocol is too complicated."* If you want to learn the protocol,
[`main.py`](main.py) is the shortest complete statement of it in this repo.

## What it does

Once a turn completes, Stella sends this plugin an `after_turn` request. The
plugin runs a test command and answers with an evidence set: what it saw of the
fail→pass flip, what its tamper check found, and the numbers it measured. It
reports **no verdict** — Stella decides done from the rule
[`plugin.toml`](plugin.toml) declares as data.

## Install

```bash
mkdir -p .stella/plugins/verify
cp plugin.toml main.py .stella/plugins/verify/
```

`[runtime].argv` is `["python3", "${plugin_dir}/main.py"]`, so nothing has to be
built and nothing has to be marked executable. Any Python 3.8+ works.

## Run it by hand

```bash
echo '{"point":"after_turn","body":{"protocol_version":1,"wrapper":"verify-v1",
  "round":0,"goal":"go","candidate":"candidate-1","turn":{"completed":true}}}' \
| VERIFY_TEST_COMMAND='["pytest","-q"]' VERIFY_BASELINE_EXIT_CODE=1 \
  python3 main.py
```

With `VERIFY_TEST_COMMAND` or `VERIFY_BASELINE_EXIT_CODE` unset it answers
`{"flip":"unobservable","tamper":"not-checked"}` — the honest evidence for "I
could not observe anything", rather than a guess. Both names are declared in
`[runtime].env`, which is default-deny; see [`../README.md`](../README.md)
§ "What the wire could not say" for why they exist at all.

## Test

```bash
python3 test_wire.py
```

Two layers: the shared conformance suite that grades all three languages
against one set of goldens, and in-process tests for what a golden cannot reach
cheaply — a timeout, a signal-killed test, a malformed grant.
