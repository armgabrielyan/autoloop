# Shared Action: `autoloop-baseline`

Record a baseline metric once autoloop is configured.

## Inputs

- Current workspace root
- `.autoloop/config.toml`

## Behavior

1. Confirm autoloop is initialized.
2. Confirm `.autoloop/config.toml` has a real eval command and metric definition.
3. Run `autoloop baseline`.
4. Return the CLI output faithfully.
5. If baseline fails because config is incomplete or the eval command is broken, fix the config first when the correction is obvious; otherwise ask one short blocking question.

## Rules

- Prefer a deterministic baseline over a noisy or flaky one.
- Do not continue into autonomous looping until baseline succeeds.
