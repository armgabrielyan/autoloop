# Shared Action: `autoloop-learn`

Refresh cross-session learnings from experiment history.

## Inputs

- Current workspace root
- Existing `.autoloop/learnings.md`

## Behavior

1. Run `autoloop learn --json --session` for end-of-run updates, or `autoloop learn --json --all` when cross-session history is requested.
2. Interpret the report into a concise update for `.autoloop/learnings.md`.
3. Preserve useful existing learnings that still match the current evidence and delete stale unsupported claims.
4. Use this structure when writing or refreshing the file:
   - `## What Helped`
   - `## What Failed`
   - `## Watchouts`
   - `## Next Ideas`
5. Focus on:
   - categories that reliably help
   - dead ends and repeated failures
   - file or subsystem patterns
   - the best recent improvements
6. Keep each section short, concrete, and evidence-backed.
7. Write the updated `.autoloop/learnings.md`.
8. Return a concise summary of what changed in the learnings file.

## Rules

- Treat the CLI output as the source of truth for statistics.
- Keep the learnings file compact, concrete, and operational.
- Do not invent counts, confidence, or success rates that are not supported by the CLI report.
