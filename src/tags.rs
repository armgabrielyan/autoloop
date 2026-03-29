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

fn is_category_candidate(candidate: &str) -> bool {
    !candidate.is_empty() && candidate != "." && candidate != ".." && !candidate.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::derive_categories;

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
}
