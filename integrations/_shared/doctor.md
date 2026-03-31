# Shared Action: `autoloop-doctor`

Verify and, when safe, repair the current autoloop config.

## Inputs

- Current workspace root
- Existing `.autoloop/config.toml`

## Behavior

1. Confirm autoloop is initialized.
2. Run `autoloop doctor --json` from the workspace root.
3. If the report is healthy, return the result faithfully and stop.
4. If the report is unhealthy and a verified inferred repair is available, run `autoloop doctor --fix --json`.
5. If repair succeeds, return the repaired verification result faithfully.
6. If the config is still unhealthy after repair, summarize the specific failing command or parsing issue.
7. Ask the user one short blocking question only when the next correction is not obvious.

## Rules

- Prefer `--json` for decision-making.
- Do not overwrite `.autoloop/config.toml` unless `autoloop doctor --fix` reports a verified repair.
- Do not continue into baseline or autonomous looping while doctor still reports an unhealthy config.
