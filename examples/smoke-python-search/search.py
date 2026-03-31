import re


TOPICS = [
    {
        "title": "Python Caching",
        "tags": ["python", "cache", "performance", "memoization"],
        "body": "Caching repeated lookups can turn expensive work into cheap reads.",
    },
    {
        "title": "Rust CLI Design",
        "tags": ["rust", "cli", "parser", "ergonomics"],
        "body": "Command-line tools benefit from predictable flags, fast startup, and clear errors.",
    },
    {
        "title": "API Latency",
        "tags": ["latency", "api", "profiling", "throughput"],
        "body": "Latency tuning often starts with measurement, hot-path profiling, and fewer allocations.",
    },
    {
        "title": "Search Ranking",
        "tags": ["search", "ranking", "query", "index"],
        "body": "Search quality depends on useful tokenization, ranking, and careful result limits.",
    },
]


def normalize(text):
    lowered = text.lower()
    cleaned = re.sub(r"[^a-z0-9\s]+", " ", lowered)
    return [token for token in cleaned.split() if token]


def build_demo_catalog(size=240):
    documents = []
    for index in range(size):
        topic = TOPICS[index % len(TOPICS)]
        title = f"{topic['title']} Note {index:03d}"
        tags = list(topic["tags"])
        body = " ".join(
            [
                topic["body"],
                f"This note discusses {topic['title'].lower()} in a practical project setting.",
                f"Example number {index} revisits {topic['tags'][0]} and {topic['tags'][1]}.",
                "Repeated benchmarks help separate real wins from noisy guesses.",
            ]
        )
        documents.append(
            {
                "id": index,
                "title": title,
                "body": body,
                "tags": tags,
            }
        )
    return documents


def search_catalog(documents, query, limit=5):
    ranked = []
    for document in documents:
        score = 0
        for query_token in normalize(query):
            combined_text = " ".join(
                [document["title"], document["body"], " ".join(document["tags"])]
            )
            document_tokens = normalize(combined_text)
            if query_token in document_tokens:
                score += document_tokens.count(query_token)
        if score > 0:
            ranked.append((score, document["id"], document["title"]))

    ranked.sort(key=lambda item: (-item[0], item[1]))
    return [title for _score, _document_id, title in ranked[:limit]]


def demo_queries():
    return [
        "python cache performance",
        "rust cli parser",
        "api latency profiling",
        "search query ranking",
        "memoization throughput",
        "benchmark error ergonomics",
    ]
