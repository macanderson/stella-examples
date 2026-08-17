# `verify`, in Python

The `after_turn` wrapper-socket point, in ~320 lines of Python with **no SDK** —
`json`, `subprocess`, `sys`, `time`, and nothing else. It used to import `os`
too; the candidate grant carries the root and the test invocation now, so there
is no environment variable left to read (#3498). That is the point:
`doc:pipeline-as-plugins` §9 rule 3 says *"if a plugin cannot be written without
an SDK, the protocol is too complicated."* If you want to learn the protocol,
[`main.py`](main.py) is the shortest complete statement of it in this repo.

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

## Install

```bash
stella plugin install .              # this workspace
stella plugin install . --scope user # every workspace
```

`install` prints the whole declaration — including the disclosure that this
plugin reports its own evidence — and installs nothing until you accept it.
`[runtime].argv` is `["python3", "${plugin_dir}/main.py"]`, so nothing has to be
built and nothing has to be marked executable. Any Python 3.8+ works.

## Run it by hand

```bash
echo '{"point":"after_turn","body":{"protocol_version":1,"wrapper":"verify-v1",
  "round":0,"goal":"go","candidate":{"handle":"candidate-1","root":"/tmp",
  "test":{"program":"pytest","args":["-q"],"baseline":"failed"}},
  "turn":{"completed":true}}}' \
| python3 main.py
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
python3 test_wire.py
```

Two layers: the shared conformance suite that grades all three languages
against one set of goldens, and in-process tests for what a golden cannot reach
cheaply — a timeout, a signal-killed test, a root that does not resolve, a
malformed grant, and the property that no verdict word and no `tamper` key ever
appears in a response.
