use smoke_rust_cli::{demo_catalog, suggest_commands};

fn main() {
    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "cache memoization performance".to_string());
    let suggestions = suggest_commands(&demo_catalog(60), &query, 5);
    for suggestion in suggestions {
        println!("{suggestion}");
    }
}
