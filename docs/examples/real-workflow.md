# Real Workflow Example

Use [smoke-python-search](../../examples/smoke-python-search/README.md) as the first end-to-end integration test. It is intentionally small, deterministic, and easy for an agent to reason about.

## Why This Example

It exercises the actual promised workflow:

- no pre-existing `.autoloop/` directory
- no pre-written AutoLoop config
- one clear latency metric
- one clear correctness guardrail
- several obvious small optimizations
- bounded autonomous loop with minimal user interaction

## Setup

Copy the fixture into a throwaway repo so the agent can edit it freely:

```bash
cp -R examples/smoke-python-search /tmp/autoloop-smoke
cd /tmp/autoloop-smoke
git init
git add .
git commit -m "initial smoke fixture"
autoloop install codex
```

If you want to test a different integration, replace the last command with one of:

```bash
autoloop install claude-code
autoloop install gemini-cli
autoloop install generic
```

## Baseline Sanity Check

Before invoking the agent, verify the fixture itself is healthy:

```bash
python3 -m unittest
python3 bench.py
```

You should see passing tests and a metric line like:

```text
METRIC latency_p95=123.456
```

## Prompt To Use

### Codex

Use `autoloop-run` to reduce the benchmark latency in this repo. Keep behavior unchanged. Use at most 5 experiments, prefer fully automatic setup, and ask me only if you are genuinely blocked.

### Claude Code

Run `/autoloop-run` for this repo. Reduce the benchmark latency, keep behavior unchanged, limit the run to 5 experiments, and only ask me if you are genuinely blocked.

### Generic

Give the agent `program.md` and instruct it:

Use the AutoLoop program in this repo to run a bounded optimization loop. Reduce benchmark latency, preserve behavior, limit the run to 5 experiments, and keep user interaction to a minimum.

## What The Agent Should Do

In a successful run, the agent should:

1. Install or use the generated integration context.
2. Initialize `.autoloop/` if it is missing.
3. Adapt `.autoloop/config.toml` to this repo:
   - `python3 bench.py` as the eval command
   - `latency_p95` as the primary metric
   - `lower` as the direction
   - `python3 -m unittest` as a pass/fail guardrail
4. Record a baseline.
5. Run a bounded experiment loop.
6. Keep winning experiments with commits and discard losing ones.
7. End the session and refresh `.autoloop/learnings.md`.

## What To Inspect After The Run

```bash
autoloop status --all
autoloop learn --all
python3 bench.py
git log --oneline --decorate --graph --all
```

## Success Criteria

- `.autoloop/config.toml` is no longer the template; it reflects the actual repo.
- `autoloop status --all` shows a baseline and recorded experiments.
- `experiments.jsonl` includes at least one `kept` or `discarded` experiment.
- `python3 -m unittest` still passes.
- `python3 bench.py` is lower than the baseline or the agent explains why no safe improvement was found.
- `.autoloop/learnings.md` contains concrete patterns, not placeholder text.

## Good Signals

- The agent makes small changes instead of one large rewrite.
- It uses `autoloop pre --json`, `eval --json`, and automatic `keep` or `discard` decisions internally.
- It stops early if no credible next experiment remains.
- It does not ask the user to drive each experiment.

## Failure Modes Worth Watching

- The agent leaves the default template config untouched.
- It runs the benchmark but forgets a correctness guardrail.
- It keeps large risky changes without strong metric evidence.
- It never ends the session or never updates learnings.
- It asks the user what to do between every experiment.
