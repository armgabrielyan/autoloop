# Shared Action: `autoloop-run`

Run an autonomous autoloop session for a bounded number of experiments with minimal user interaction.

## Inputs

- User goal or optimization target
- Current workspace root
- Existing `.autoloop/` state, if present
- Optional experiment or time bounds from the user

## Behavior

1. Ensure autoloop is initialized. If not, perform the `autoloop-init` behavior first.
2. Ensure a baseline exists. If not, perform the `autoloop-baseline` behavior first.
3. Start a session if none is active.
4. Read `.autoloop/learnings.md` when it exists.
5. Read `autoloop status --json --all` to understand the current history.
6. Run a bounded optimization loop:
   - propose one concrete experiment aligned with the user goal
   - run `autoloop pre --json --description "..."` before making the change
   - if history strongly suggests avoiding the idea, pick a different experiment
   - make one focused change
   - run `autoloop eval --json`
   - if the result should be kept, run `autoloop keep --description "..." --commit`
   - otherwise run `autoloop discard --description "..." --reason "..." --revert`
7. Continue until the configured stop condition is reached:
   - user-specified experiment limit
   - user-specified time limit
   - repeated blocked or failed experiments
   - no credible next experiments remain
8. End the session with `autoloop session end`.
9. Run `autoloop learn --session` and update `.autoloop/learnings.md` from the CLI output.
10. Return a concise summary of what was tried, what improved, and what branches or follow-up actions are recommended.

## Rules

- Prefer `--json` for decision-making.
- Keep each experiment small and attributable.
- Do not ask the user between experiments unless blocked by missing information, unsafe ambiguity, or repeated hard failures.
- Do not manually edit `.autoloop/state.json`, `.autoloop/last_eval.json`, or `.autoloop/experiments.jsonl`.
- Bound the run. If the user did not specify a limit, choose a reasonable finite cap instead of looping forever.
