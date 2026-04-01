# Acceptance Matrix

This is the regression bar for AutoLoop's example repos.

Use it after meaningful changes to setup inference, validation, integrations, or the autonomous loop. The goal is not broad coverage. The goal is a small, stable proof that AutoLoop still works end to end on known fixtures.

## Scope

Each fixture must pass two gates:

1. Automated setup gate
2. Manual bounded-run gate

The automated gate proves that AutoLoop can initialize, verify, repair, and baseline the repo without ad hoc hand-editing. The manual gate proves that the installed `autoloop-run` wrapper still drives a real agent session.

## Fixtures

| Fixture | Runtime | Goal | Automated gate | Manual gate |
| --- | --- | --- | --- | --- |
| `examples/smoke-python-search` | Python | Lower `latency_p95` without changing search behavior | `scripts/acceptance-fixture.sh smoke-python-search` | Codex `autoloop-run` with at most 5 experiments |
| `examples/smoke-rust-cli` | Rust | Lower `latency_p95` without changing suggestion behavior | `scripts/acceptance-fixture.sh smoke-rust-cli` | Codex `autoloop-run` with at most 5 experiments |

## Automated Gate

Run the helper from the repo root:

```bash
scripts/acceptance-fixture.sh smoke-python-search
scripts/acceptance-fixture.sh smoke-rust-cli
```

For each fixture, the script proves:

- `autoloop install codex` succeeds and writes the installed wrapper files
- `autoloop init --verify` produces a healthy config
- replacing `.autoloop/config.toml` with the stock template makes `autoloop doctor` report an unhealthy config
- `autoloop doctor --fix` repairs the config back to a verified inferred config
- `autoloop baseline` succeeds against the repaired config
- setup-only integration files are committed and generated cache noise is cleaned so the prepared repo starts the manual run with a clean `git status`

The script leaves a ready temp repo behind for the manual gate.

## Manual Bounded-Run Gate

For each prepared temp repo, open it in Codex and invoke the installed wrapper with:

```text
Use `autoloop-run` to reduce the benchmark latency in this repo. Keep behavior unchanged. Use at most 5 experiments, prefer fully automatic setup, and ask me only if you are genuinely blocked.
```

This gate passes when all of the following are true:

- the agent uses the installed wrapper rather than inventing a separate workflow
- the run starts from the clean prepared repo state instead of treating wrapper-install files as the first experiment
- the run stays bounded and does not ask for routine per-experiment input
- the repo ends with no unresolved pending evals
- `.autoloop/experiments.jsonl` records at least one experiment
- `.autoloop/learnings.md` contains concrete notes
- the fixture correctness command still passes
- the benchmark improves, or the agent gives a credible reason no safe improvement was available

## Verification Commands

After the manual run, inspect the repo with:

```bash
autoloop status --all
autoloop learn --all
git log --oneline --decorate --graph --all
```

Then run the fixture-native commands:

### Python

```bash
python3 -m unittest
python3 bench.py
```

### Rust

```bash
cargo test
cargo run --quiet --bin bench
```

## Pass Criteria

Treat the matrix as passing only if both fixtures pass both gates.

- Python automated gate
- Python manual bounded run
- Rust automated gate
- Rust manual bounded run

If one of these regresses, AutoLoop is not ready for release or wider promotion.
