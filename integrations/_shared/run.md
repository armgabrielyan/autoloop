# Shared Action: `autoloop-run`

Run an autonomous autoloop session for a bounded number of experiments with minimal user interaction.

## Inputs

- User goal or optimization target
- Current workspace root
- Existing `.autoloop/` state, if present
- Optional experiment or time bounds from the user

## Behavior

1. Treat this action as permission to run the full loop without asking for confirmation between setup steps or experiments.
2. Determine the run bound:
   - use the user-specified experiment limit when present
   - otherwise use the user-specified time limit when present
   - otherwise default to 5 experiments
3. Ensure autoloop is initialized. If not, run `autoloop init --verify`.
4. Before baseline or iteration, run `autoloop doctor --json`.
5. If doctor reports an unhealthy config and a verified repair is available, run `autoloop doctor --fix --json`, then recheck health.
6. If the config is still unhealthy after repair, stop and ask one short blocking question instead of forcing the loop forward.
7. Ensure a baseline exists. If not, perform the `autoloop-baseline` behavior first.
8. Start a session if none is active.
9. If an unresolved pending eval already exists, resolve it before starting a new experiment:
   - keep it with `autoloop keep --description "..." --commit` when the recorded verdict and worktree state support keeping it
   - otherwise discard it with `autoloop discard --description "..." --reason "..." --revert`
10. Read `.autoloop/learnings.md` when it exists.
11. Read `autoloop status --json --all` to understand the current history.
12. Run a bounded optimization loop:
   - propose one small, concrete experiment aligned with the user goal
   - run `autoloop pre --json --description "..."` before making the change
   - if history strongly suggests avoiding the idea, pick a different experiment instead of forcing it
   - make one focused, attributable change
   - run `autoloop eval --json`
   - never leave a pending eval unresolved: immediately keep with `--commit` or discard with `--revert`
   - periodically refresh `autoloop learn --json --session` on longer runs and update `.autoloop/learnings.md`
13. Stop when any stop condition is reached:
   - the experiment limit is reached
   - the time limit is reached
   - repeated blocked or crashed experiments suggest the loop is not progressing
   - no credible next experiments remain
14. Always end the session with `autoloop session end`.
15. Always run `autoloop learn --json --session` before finishing and update `.autoloop/learnings.md` from the CLI output.
16. Return a concise summary of what was tried, what improved, and what branches or follow-up actions are recommended.

## Rules

- Prefer `--json` for decision-making.
- Keep each experiment small and attributable.
- Default to at most 5 experiments when the user does not specify a bound.
- Treat healthy doctor output as a prerequisite for baseline and autonomous looping.
- Use `autoloop keep --commit` for wins and `autoloop discard --revert` for losses whenever the workspace state allows it.
- Do not ask the user between experiments unless blocked by missing information, unsafe ambiguity, or repeated hard failures.
- Do not manually edit `.autoloop/state.json`, `.autoloop/last_eval.json`, or `.autoloop/experiments.jsonl`.
- Bound the run. Never interpret this action as permission to loop forever unless the user explicitly requests that.
