# Shared Action: `autoloop-init`

Bootstrap autoloop in the current project workspace with minimal user interaction.

## Inputs

- Current workspace root
- Existing repository files and tests
- Existing `.autoloop/` directory, if present

## Behavior

1. Check whether `.autoloop/` already exists.
2. If it does not exist, run `autoloop init --verify` from the workspace root.
3. If it already exists, run `autoloop doctor --json` before assuming setup is ready.
4. Treat setup as incomplete until config verification passes.
5. Infer the first usable config from the project itself:
   - choose one primary metric
   - choose the metric direction
   - configure an eval command the project can actually run
   - add one obvious pass/fail guardrail when the repo has a natural correctness command
6. Prefer this inference order:
   - existing test or validation command for the first pass/fail guardrail
   - existing benchmark, perf, or smoke command for the primary eval command
   - `metric_lines` output before regex or custom parsing when the command can be made to emit `METRIC name=value`
7. Keep the first config minimal and executable:
   - one metric
   - zero or one obvious pass/fail guardrail
   - no speculative extra guardrails unless the repo already exposes them
8. Prefer inferring a workable first config from the repo rather than asking the user immediately.
9. If `autoloop init --verify` or `autoloop doctor --json` reports an unhealthy config and a verified repair is available, run `autoloop doctor --fix --json`.
10. If the config is still unhealthy after repair, ask one short blocking question only when the next correction is not obvious.
11. Run `autoloop status --json` after setup to confirm autoloop is ready.

## Rules

- Use the local `autoloop` CLI as the source of truth.
- Do not edit `.autoloop/state.json`, `.autoloop/last_eval.json`, or `.autoloop/experiments.jsonl` by hand.
- Keep the initial config simple and executable; optimize for a reliable first loop, not perfect coverage.
- Treat `autoloop doctor` as the standard way to prove or repair config health.
- Do not invent extra wrapper scripts when an existing repo command is already good enough.
