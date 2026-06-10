use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::{
    config::Config,
    git,
};

pub fn discover_repos(root: &Path, config: &Config) -> Vec<PathBuf> {
    WalkDir::new(root)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| config.should_process(root, path))
        .filter(|path| git::is_repo(path))
        .collect()
}
