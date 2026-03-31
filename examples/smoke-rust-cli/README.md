# Smoke Rust CLI

This is a small Rust fixture for testing AutoLoop on a repo that uses Cargo-native commands.

## Goal

Reduce the benchmark latency reported by the `bench` binary without changing suggestion behavior.

## Files

- `src/lib.rs`: naive command suggestion logic with obvious optimization opportunities
- `src/main.rs`: tiny CLI entrypoint
- `src/bin/bench.rs`: deterministic benchmark that prints `METRIC latency_p95=...`

## Manual Commands

```bash
cargo test
cargo run --quiet --bin bench
```

## Intended AutoLoop Config

The intended first-pass config the agent should infer is:

- metric name: `latency_p95`
- metric direction: `lower`
- eval command: `cargo run --quiet --bin bench`
- pass/fail guardrail: `cargo test`

## Good Experiment Ideas

- Normalize the query once instead of once per catalog entry.
- Avoid rebuilding the combined searchable string for every query token.
- Avoid repeated scans through the same normalized token list.

## Success Criteria

- `cargo test` stays green.
- `cargo run --quiet --bin bench` reports a lower `latency_p95` than baseline.
- AutoLoop records at least one kept experiment.
