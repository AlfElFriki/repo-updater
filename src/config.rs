use std::{
    collections::HashSet,
    fs,
    path::Path,
};

const UPDATEIGNORE_FILE: &str = ".updateignore";

const DEFAULT_EXCLUDED_REPOS: &[&str] = &[
    // "repo-a",
    // "repo-b",
];

#[derive(Debug)]
pub struct Config {
    excluded: HashSet<String>,
    only: Option<HashSet<String>>,
}

impl Config {
    pub fn load(root: &Path, cli_excludes: &[String], cli_only: &[String]) -> Self {
        let mut excluded = HashSet::new();

        for repo in DEFAULT_EXCLUDED_REPOS {
            excluded.insert(normalize_path_string(repo));
        }

        for repo in cli_excludes {
            if let Some(entry) = normalize_ignore_entry(repo) {
                excluded.insert(entry);
            }
        }

        for repo in load_updateignore(root) {
            excluded.insert(repo);
        }

        let only = if cli_only.is_empty() {
            None
        } else {
            Some(
                cli_only
                    .iter()
                    .filter_map(|entry| normalize_ignore_entry(entry))
                    .collect(),
            )
        };

        Self { excluded, only }
    }

    pub fn should_process(&self, root: &Path, path: &Path) -> bool {
        if self.is_excluded(root, path) {
            return false;
        }

        if let Some(only) = &self.only {
            return matches_selector(root, path, only);
        }

        true
    }

    fn is_excluded(&self, root: &Path, path: &Path) -> bool {
        matches_selector(root, path, &self.excluded)
    }
}

fn load_updateignore(root: &Path) -> HashSet<String> {
    let path = root.join(UPDATEIGNORE_FILE);

    let Ok(contents) = fs::read_to_string(path) else {
        return HashSet::new();
    };

    contents
        .lines()
        .filter_map(normalize_ignore_entry)
        .collect()
}

fn normalize_ignore_entry(line: &str) -> Option<String> {
    let trimmed = line.trim();

    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    Some(normalize_path_string(trimmed))
}

fn normalize_path_string(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("./")
        .trim_end_matches('/')
        .replace('\\', "/")
}

fn matches_selector(root: &Path, path: &Path, selectors: &HashSet<String>) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };

    if selectors.contains(&normalize_path_string(name)) {
        return true;
    }

    let Some(relative) = relative_path_string(root, path) else {
        return false;
    };

    selectors.contains(&relative)
}

fn relative_path_string(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| {
            path.to_string_lossy()
                .trim_start_matches("./")
                .trim_end_matches('/')
                .replace('\\', "/")
        })
}
