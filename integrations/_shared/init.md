# Shared Action: `autoloop-init`

Bootstrap autoloop in the current project workspace with minimal user interaction.

## Inputs

- Current workspace root
- Existing repository files and tests
- Existing `.autoloop/` directory, if present

## Behavior

1. Check whether `.autoloop/` already exists.
2. If it does not exist, run `autoloop init` from the workspace root.
3. Inspect `.autoloop/config.toml` and adapt it to the real project:
   - choose the primary metric
   - choose the metric direction
   - configure an eval command the project can actually run
   - add guardrails when there are obvious regressions to protect against
4. Prefer inferring a workable first config from the repo rather than asking the user immediately.
5. If the eval command or metric cannot be determined responsibly, ask one short blocking question.
6. Run `autoloop status` after setup to verify that autoloop is ready.

## Rules

- Use the local `autoloop` CLI as the source of truth.
- Do not edit `.autoloop/state.json`, `.autoloop/last_eval.json`, or `.autoloop/experiments.jsonl` by hand.
- Keep the initial config simple and executable; optimize for a reliable first loop, not perfect coverage.
