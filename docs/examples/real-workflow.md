# Real Workflow Example

Use [smoke-python-search](../../examples/smoke-python-search/README.md) as the first end-to-end integration test. It is intentionally small, deterministic, and easy for an agent to reason about.

Once that works, use [smoke-rust-cli](../../examples/smoke-rust-cli/README.md) as the second test. It exercises the same loop with Cargo-native setup and commands.

## Why This Example

It exercises the actual promised workflow:

- no pre-existing `.autoloop/` directory
- no pre-written AutoLoop config
- one clear latency metric
- one clear correctness guardrail
- several obvious small optimizations
- bounded autonomous loop with minimal user interaction

## Fixture Order

Use the fixtures in this order:

1. `examples/smoke-python-search`
2. `examples/smoke-rust-cli`

The first validates the basic autonomous loop with a tiny Python benchmark. The second validates that the integration can infer a real Cargo-based eval command and guardrail without switching to a different workflow model.

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
2. Initialize `.autoloop/` with verification if it is missing.
3. Verify the config before baselining:
   - `python3 bench.py` as the eval command
   - `latency_p95` as the primary metric
   - `lower` as the direction
   - `python3 -m unittest` as a pass/fail guardrail
4. If the config is still the template or otherwise broken, run `autoloop doctor --fix` before continuing.
5. Record a baseline only after doctor reports a healthy config.
6. Run a bounded experiment loop.
7. Keep winning experiments with commits and discard losing ones.
8. End the session and refresh `.autoloop/learnings.md`.

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
- It uses `autoloop init --verify` or `autoloop doctor` during setup instead of assuming the inferred config is already valid.
- It uses `autoloop pre --json`, `eval --json`, and automatic `keep` or `discard` decisions internally.
- It stops early if no credible next experiment remains.
- It does not ask the user to drive each experiment.

## Failure Modes Worth Watching

- The agent leaves the default template config untouched.
- It baselines against the placeholder metric instead of verifying or repairing config first.
- It runs the benchmark but forgets a correctness guardrail.
- It keeps large risky changes without strong metric evidence.
- It never ends the session or never updates learnings.
- It asks the user what to do between every experiment.

## Acceptance Checklist

Use this as the pass/fail checklist for an installed integration run:

- [ ] The agent used the installed wrapper or command surface, not an ad hoc manual workflow.
- [ ] `.autoloop/` was initialized automatically when missing.
- [ ] `.autoloop/config.toml` was adapted to the real repo and is not still the template.
- [ ] `autoloop init --verify` or `autoloop doctor --json` proved the config healthy before baseline.
- [ ] If the initial config was broken, `autoloop doctor --fix` repaired it before baseline.
- [ ] The inferred eval command actually runs in the repo.
- [ ] The inferred guardrail command actually runs in the repo.
- [ ] `autoloop baseline` succeeded before the experiment loop started.
- [ ] The run stayed bounded and did not require user input between normal experiments.
- [ ] Each experiment was small and attributable.
- [ ] Every `autoloop eval` result was resolved with keep or discard instead of being left pending.
- [ ] At least one experiment was recorded in `.autoloop/experiments.jsonl`.
- [ ] `autoloop session end` happened before the run finished.
- [ ] `.autoloop/learnings.md` was updated with concrete evidence-backed notes.
- [ ] The repo's correctness command still passes after the run.
- [ ] The final benchmark metric improved, or the agent gave a credible explanation for why no safe improvement was found.
