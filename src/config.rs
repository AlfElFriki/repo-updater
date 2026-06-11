use std::{
    collections::HashSet,
    fs,
    path::Path,
};

use globset::{Glob, GlobSet, GlobSetBuilder};

const UPDATEIGNORE_FILE: &str = ".updateignore";

const DEFAULT_EXCLUDED_REPOS: &[&str] = &[
    "repo-updater"
    // "repo-a",
    // "repo-b",
];

#[derive(Debug)]
pub struct Config {
    excluded: Selector,
    only: Option<Selector>,
}

impl Config {
    pub fn load(root: &Path, cli_excludes: &[String], cli_only: &[String]) -> Self {
        let mut excluded_patterns = Vec::new();

        for repo in DEFAULT_EXCLUDED_REPOS {
            excluded_patterns.push(repo.to_string());
        }

        for repo in cli_excludes {
            if let Some(entry) = normalize_ignore_entry(repo) {
                excluded_patterns.push(entry);
            }
        }

        for repo in load_updateignore(root) {
            excluded_patterns.push(repo);
        }

        let only = if cli_only.is_empty() {
            None
        } else {
            Some(Selector::new(
                cli_only
                    .iter()
                    .filter_map(|entry| normalize_ignore_entry(entry))
                    .collect(),
            ))
        };

        Self {
            excluded: Selector::new(excluded_patterns),
            only,
        }
    }

    pub fn should_process(&self, root: &Path, path: &Path) -> bool {
        // .updateignore and --exclude are hard exclusions.
        if self.excluded.matches(root, path) {
            return false;
        }

        // --only is a positive filter, but it cannot override exclusions.
        if let Some(only) = &self.only {
            return only.matches(root, path);
        }

        true
    }
}

#[derive(Debug)]
struct Selector {
    exact: HashSet<String>,
    globs: GlobSet,
}

impl Selector {
    fn new(patterns: Vec<String>) -> Self {
        let mut exact = HashSet::new();
        let mut glob_builder = GlobSetBuilder::new();

        for pattern in patterns {
            let normalized = normalize_path_string(&pattern);

            if contains_glob_meta(&normalized) {
                match Glob::new(&normalized) {
                    Ok(glob) => {
                        glob_builder.add(glob);
                    }
                    Err(e) => {
                        eprintln!("Ignoring invalid glob pattern '{normalized}': {e}");
                    }
                }
            } else {
                exact.insert(normalized);
            }
        }

        let globs = glob_builder
            .build()
            .unwrap_or_else(|e| {
                eprintln!("Failed to build glob matcher: {e}");
                GlobSetBuilder::new()
                    .build()
                    .expect("empty glob set should always build")
            });

        Self { exact, globs }
    }

    fn matches(&self, root: &Path, path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            return false;
        };

        let name = normalize_path_string(name);

        if self.exact.contains(&name) || self.globs.is_match(&name) {
            return true;
        }

        let Some(relative) = relative_path_string(root, path) else {
            return false;
        };

        self.exact.contains(&relative) || self.globs.is_match(&relative)
    }
}

fn load_updateignore(root: &Path) -> Vec<String> {
    let path = root.join(UPDATEIGNORE_FILE);

    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
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

fn relative_path_string(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| normalize_path_string(&path.to_string_lossy()))
}

fn contains_glob_meta(value: &str) -> bool {
    value.contains('*')
        || value.contains('?')
        || value.contains('[')
        || value.contains('{')
}
