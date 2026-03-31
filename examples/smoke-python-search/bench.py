import time

from search import build_demo_catalog, demo_queries, search_catalog


CATALOG = build_demo_catalog(320)
QUERIES = demo_queries() * 6
SAMPLES = 15


def measure_once():
    started = time.perf_counter()
    for query in QUERIES:
        search_catalog(CATALOG, query, limit=5)
    elapsed = time.perf_counter() - started
    return elapsed * 1000.0


def percentile_95(samples):
    ordered = sorted(samples)
    index = round((len(ordered) - 1) * 0.95)
    return ordered[index]


def main():
    samples = [measure_once() for _ in range(SAMPLES)]
    print(f"METRIC latency_p95={percentile_95(samples):.3f}")


if __name__ == "__main__":
    main()
