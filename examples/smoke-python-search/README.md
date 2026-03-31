# Smoke Python Search

This is a deliberately small benchmark fixture for testing the installed AutoLoop integration in a real agent workflow.

## Goal

Reduce the benchmark latency reported by `bench.py` without changing search behavior.

## Files

- `search.py`: naive search implementation with obvious optimization opportunities
- `bench.py`: deterministic latency benchmark that prints `METRIC latency_p95=...`
- `test_search.py`: correctness guardrail

## Manual Commands

```bash
python3 -m unittest
python3 bench.py
```

## Intended AutoLoop Config

The intended first-pass config the agent should infer is:

- metric name: `latency_p95`
- metric direction: `lower`
- eval command: `python3 bench.py`
- pass/fail guardrail: `python3 -m unittest`

## Good Experiment Ideas

- Normalize the query once per search call instead of once per document token check.
- Normalize each document once instead of once per query token.
- Replace repeated linear membership and counting patterns with cached structures.

## Success Criteria

- `python3 -m unittest` stays green.
- `python3 bench.py` reports a lower `latency_p95` than the baseline.
- AutoLoop records at least one kept experiment.
