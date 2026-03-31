# Shared Action: `autoloop-finalize`

Create clean review branches from committed kept experiments.

## Inputs

- Current workspace root
- Optional session or all-history scope

## Behavior

1. Confirm the working tree is clean before finalizing.
2. Run `autoloop finalize`, using `--json` when structured output is useful.
3. Present the created review branches and any skipped experiments.
4. If experiments were skipped because they were kept without `--commit`, say so plainly and recommend rerunning future keeps with commits enabled.

## Rules

- Do not manually build review branches outside the CLI when autoloop can do it.
- Treat skipped experiments as a workflow gap, not as silent success.
