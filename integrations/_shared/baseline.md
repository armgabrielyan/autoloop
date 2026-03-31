# Shared Action: `autoloop-baseline`

Record a baseline metric once autoloop is configured.

## Inputs

- Current workspace root
- `.autoloop/config.toml`

## Behavior

1. Confirm autoloop is initialized.
2. Confirm `.autoloop/config.toml` has a real eval command and metric definition, not just the template placeholders.
3. Run `autoloop baseline`.
4. If baseline fails because parsing or formatting is obviously wrong, repair the config and retry once.
5. Return the CLI output faithfully, including the recorded metric.
6. If baseline still fails and the next correction is not obvious, ask one short blocking question.

## Rules

- Prefer a deterministic baseline over a noisy or flaky one.
- Do not continue into autonomous looping until baseline succeeds.
- Do not treat a failed baseline as acceptable setup completion.
