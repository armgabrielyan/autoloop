# Shared Action: `autoloop-status`

Inspect the current autoloop state and summarize it for the user.

## Inputs

- Current workspace root
- Optional request for current-session scope or all-history scope

## Behavior

1. Run `autoloop status`, using `--json` when structured output is useful.
2. Explain the most important current state:
   - active session
   - baseline presence
   - pending eval
   - kept/discarded/crashed counts
   - current streak and best improvement
3. If there is a pending eval, tell the user whether the next action is effectively keep or discard.

## Rules

- Do not mutate autoloop state from this action.
