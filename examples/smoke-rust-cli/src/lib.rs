#[derive(Clone, Debug)]
pub struct CommandEntry {
    pub name: String,
    pub summary: String,
    pub tags: Vec<String>,
}

pub fn normalize(text: &str) -> Vec<String> {
    let mut cleaned = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character.is_ascii_whitespace() {
            cleaned.push(character.to_ascii_lowercase());
        } else {
            cleaned.push(' ');
        }
    }
    cleaned
        .split_whitespace()
        .map(|token| token.to_string())
        .collect()
}

pub fn demo_catalog(size: usize) -> Vec<CommandEntry> {
    let topics = [
        (
            "cache",
            "Cache Inspector",
            "Inspects cache keys, eviction behavior, and memoization hot paths.",
            vec!["cache", "memoization", "performance"],
        ),
        (
            "bench",
            "Bench Runner",
            "Runs repeatable CLI benchmarks with stable metric output.",
            vec!["bench", "latency", "metrics"],
        ),
        (
            "logs",
            "Log Triage",
            "Filters application logs and highlights likely failure patterns.",
            vec!["logs", "triage", "errors"],
        ),
        (
            "doctor",
            "Workspace Doctor",
            "Checks workspace setup, missing files, and reproducibility problems.",
            vec!["doctor", "workspace", "diagnostics"],
        ),
    ];

    let mut entries = Vec::with_capacity(size);
    for index in 0..size {
        let topic = &topics[index % topics.len()];
        entries.push(CommandEntry {
            name: format!("{}-tool-{index:03}", topic.0),
            summary: format!("{}: {}", topic.1, topic.2),
            tags: topic.3.iter().map(|value| value.to_string()).collect(),
        });
    }
    entries
}

pub fn suggest_commands(entries: &[CommandEntry], query: &str, limit: usize) -> Vec<String> {
    let mut ranked = Vec::new();
    for entry in entries {
        let mut score = 0usize;
        for query_token in normalize(query) {
            let searchable = format!("{} {} {}", entry.name, entry.summary, entry.tags.join(" "));
            let entry_tokens = normalize(&searchable);
            if entry_tokens.iter().any(|token| token == &query_token) {
                score += entry_tokens
                    .iter()
                    .filter(|token| *token == &query_token)
                    .count();
            }
        }
        if score > 0 {
            ranked.push((score, entry.name.clone()));
        }
    }

    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, name)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{demo_catalog, suggest_commands};

    #[test]
    fn cache_queries_rank_cache_entries_first() {
        let catalog = demo_catalog(40);
        let results = suggest_commands(&catalog, "cache memoization performance", 3);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|name| name.starts_with("cache-tool")));
    }

    #[test]
    fn suggestion_respects_limit() {
        let catalog = demo_catalog(40);
        let results = suggest_commands(&catalog, "bench latency metrics", 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn suggestion_returns_empty_when_nothing_matches() {
        let catalog = demo_catalog(40);
        let results = suggest_commands(&catalog, "quantum banana ledger", 5);
        assert!(results.is_empty());
    }
}
