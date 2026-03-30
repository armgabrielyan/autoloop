use std::collections::BTreeSet;
use std::path::Path;

pub fn derive_categories<I, P>(paths: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut categories = BTreeSet::new();

    for path in paths {
        let path = path.as_ref();
        for component in path.components() {
            let name = component.as_os_str().to_string_lossy();
            if is_category_candidate(&name) {
                categories.insert(name.to_string());
            }
        }

        if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
            let normalized = stem.trim_start_matches("test_");
            if is_category_candidate(normalized) {
                categories.insert(normalized.to_string());
            }
        }
    }

    categories
}

pub fn derive_paths_from_description(text: &str) -> BTreeSet<String> {
    text.split_whitespace()
        .filter_map(|token| {
            let trimmed = token.trim_matches(|char: char| {
                matches!(
                    char,
                    '"' | '\'' | ',' | '.' | ':' | ';' | '(' | ')' | '[' | ']'
                )
            });
            if trimmed.contains('/') {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect()
}

pub fn derive_terms_from_description(text: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let mut current = String::new();

    for char in text.chars() {
        if char.is_ascii_alphanumeric() || char == '_' {
            current.push(char.to_ascii_lowercase());
        } else if !current.is_empty() {
            push_term_variants(&current, &mut terms);
            current.clear();
        }
    }

    if !current.is_empty() {
        push_term_variants(&current, &mut terms);
    }

    terms
}

fn push_term_variants(term: &str, out: &mut BTreeSet<String>) {
    if !is_term_candidate(term) {
        return;
    }

    out.insert(term.to_string());

    if let Some(stripped) = term.strip_suffix("ing").filter(|value| value.len() >= 4) {
        out.insert(stripped.to_string());
        out.insert(format!("{stripped}e"));
    }
    if let Some(stripped) = term.strip_suffix("ed").filter(|value| value.len() >= 3) {
        out.insert(stripped.to_string());
        out.insert(format!("{stripped}e"));
    }
    if let Some(stripped) = term.strip_suffix("es").filter(|value| value.len() >= 3) {
        out.insert(stripped.to_string());
    }
    if let Some(stripped) = term.strip_suffix('s').filter(|value| value.len() >= 3) {
        out.insert(stripped.to_string());
    }
}

fn is_category_candidate(candidate: &str) -> bool {
    !candidate.is_empty() && candidate != "." && candidate != ".." && !candidate.starts_with('.')
}

fn is_term_candidate(candidate: &str) -> bool {
    is_category_candidate(candidate)
        && candidate.len() >= 3
        && !matches!(
            candidate,
            "the"
                | "and"
                | "for"
                | "with"
                | "that"
                | "this"
                | "from"
                | "into"
                | "onto"
                | "over"
                | "under"
                | "then"
                | "than"
                | "run"
                | "add"
                | "use"
                | "try"
                | "new"
                | "fix"
                | "make"
                | "change"
        )
}

#[cfg(test)]
mod tests {
    use super::{derive_categories, derive_paths_from_description, derive_terms_from_description};

    #[test]
    fn derives_categories_from_paths() {
        let categories = derive_categories([
            "src/api/handler.rs",
            "tests/test_cache.rs",
            "config/dev.toml",
        ]);

        assert!(categories.contains("src"));
        assert!(categories.contains("api"));
        assert!(categories.contains("handler"));
        assert!(categories.contains("tests"));
        assert!(categories.contains("cache"));
        assert!(categories.contains("config"));
        assert!(categories.contains("dev"));
    }

    #[test]
    fn derives_paths_from_description() {
        let paths = derive_paths_from_description("touch src/api/handler.rs and tests/cache.rs");

        assert!(paths.contains("src/api/handler.rs"));
        assert!(paths.contains("tests/cache.rs"));
    }

    #[test]
    fn derives_terms_from_description() {
        let terms = derive_terms_from_description("add response caching to the API handler");

        assert!(terms.contains("response"));
        assert!(terms.contains("caching"));
        assert!(terms.contains("cache"));
        assert!(terms.contains("api"));
        assert!(terms.contains("handler"));
        assert!(!terms.contains("the"));
    }
}
