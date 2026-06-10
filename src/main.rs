use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use clap::Parser;
use walkdir::WalkDir;

const UPDATEIGNORE_FILE: &str = ".updateignore";

const DEFAULT_EXCLUDED_REPOS: &[&str] = &[
    // "repo-a",
    // "repo-b",
];

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Try to pull branches after fetching, fast-forward only
    #[arg(long)]
    pull: bool,

    /// Exclude a repository directory. Can be passed multiple times.
    ///
    /// Example:
    ///   repo-sync --exclude repo-a --exclude repo-b
    #[arg(long = "exclude", value_name = "DIR")]
    exclude: Vec<String>,
}

#[derive(Debug, Clone)]
struct BranchStatus {
    branch: String,
    upstream: Option<String>,
    incoming: Option<u32>,
    outgoing: Option<u32>,
}

fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn fetch_prune(repo: &Path) -> Result<(), String> {
    run_git(repo, &["fetch", "--prune"])?;
    Ok(())
}

fn current_branch(repo: &Path) -> Result<String, String> {
    run_git(repo, &["branch", "--show-current"])
}

fn local_branches(repo: &Path) -> Result<Vec<String>, String> {
    let output = run_git(repo, &["branch", "--format=%(refname:short)"])?;

    Ok(output
        .lines()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(String::from)
        .collect())
}

fn upstream_for_branch(repo: &Path, branch: &str) -> Option<String> {
    run_git(
        repo,
        &[
            "rev-parse",
            "--abbrev-ref",
            &format!("{branch}@{{upstream}}"),
        ],
    )
    .ok()
}

fn count_commits(repo: &Path, from: &str, to: &str) -> Result<u32, String> {
    let output = run_git(repo, &["rev-list", "--count", &format!("{from}..{to}")])?;

    output
        .trim()
        .parse::<u32>()
        .map_err(|e| e.to_string())
}

fn working_tree_clean(repo: &Path) -> bool {
    match run_git(repo, &["status", "--porcelain"]) {
        Ok(output) => output.trim().is_empty(),
        Err(_) => false,
    }
}

fn checkout_branch(repo: &Path, branch: &str) -> Result<(), String> {
    run_git(repo, &["checkout", branch])?;
    Ok(())
}

fn pull_ff_only(repo: &Path) -> Result<(), String> {
    run_git(repo, &["pull", "--ff-only"])?;
    Ok(())
}

fn branch_status(repo: &Path, branch: &str) -> BranchStatus {
    let Some(upstream) = upstream_for_branch(repo, branch) else {
        return BranchStatus {
            branch: branch.to_string(),
            upstream: None,
            incoming: None,
            outgoing: None,
        };
    };

    let incoming = count_commits(repo, branch, &upstream).ok();
    let outgoing = count_commits(repo, &upstream, branch).ok();

    BranchStatus {
        branch: branch.to_string(),
        upstream: Some(upstream),
        incoming,
        outgoing,
    }
}

fn print_branch_status(status: &BranchStatus, current_branch: Option<&str>) {
    let marker = if current_branch == Some(status.branch.as_str()) {
        "*"
    } else {
        " "
    };

    let Some(upstream) = &status.upstream else {
        println!("  {marker} {}: no upstream", status.branch);
        return;
    };

    let Some(incoming) = status.incoming else {
        println!("  {marker} {}: failed to check incoming commits", status.branch);
        return;
    };

    let Some(outgoing) = status.outgoing else {
        println!("  {marker} {}: failed to check outgoing commits", status.branch);
        return;
    };

    match (incoming, outgoing) {
        (0, 0) => {
            println!("  {marker} {}: up to date with {upstream}", status.branch);
        }
        (n, 0) => {
            println!(
                "  {marker} {}: {n} incoming commit(s) from {upstream}",
                status.branch
            );
        }
        (0, n) => {
            println!(
                "  {marker} {}: {n} local commit(s) ahead of {upstream}",
                status.branch
            );
        }
        (i, o) => {
            println!(
                "  {marker} {}: diverged from {upstream} ({i} incoming, {o} outgoing)",
                status.branch
            );
        }
    }
}

fn pull_possible_branches(repo: &Path, statuses: &[BranchStatus]) {
    let original_branch = match current_branch(repo) {
        Ok(branch) if !branch.is_empty() => branch,
        Ok(_) => {
            println!("Skipping pull: detached HEAD");
            return;
        }
        Err(e) => {
            println!("Skipping pull: could not determine current branch: {e}");
            return;
        }
    };

    if !working_tree_clean(repo) {
        println!("Skipping pull: working tree is dirty");
        return;
    }

    for status in statuses {
        let branch = &status.branch;

        let Some(_upstream) = &status.upstream else {
            println!("Skipping pull on '{branch}': no upstream");
            continue;
        };

        let Some(incoming) = status.incoming else {
            println!("Skipping pull on '{branch}': could not determine incoming commits");
            continue;
        };

        let Some(outgoing) = status.outgoing else {
            println!("Skipping pull on '{branch}': could not determine outgoing commits");
            continue;
        };

        if incoming == 0 {
            continue;
        }

        if outgoing > 0 {
            println!("Skipping pull on '{branch}': branch has local commits ahead");
            continue;
        }

        if !working_tree_clean(repo) {
            println!("Skipping pull on '{branch}': working tree is dirty");
            continue;
        }

        let switched_branch = branch != &original_branch;

        if switched_branch {
            match checkout_branch(repo, branch) {
                Ok(_) => {}
                Err(e) => {
                    println!("Skipping pull on '{branch}': could not switch branch: {e}");
                    continue;
                }
            }
        }

        match pull_ff_only(repo) {
            Ok(_) => {
                println!("Pulled '{branch}' with fast-forward only");
            }
            Err(e) => {
                println!("Pull failed on '{branch}': {e}");
            }
        }

        if switched_branch {
            match checkout_branch(repo, &original_branch) {
                Ok(_) => {}
                Err(e) => {
                    println!(
                        "Failed to restore original branch '{original_branch}': {e}"
                    );
                    return;
                }
            }
        }
    }
}

fn normalize_ignore_entry(line: &str) -> Option<String> {
    let trimmed = line.trim();

    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    Some(
        trimmed
            .trim_start_matches("./")
            .trim_end_matches('/')
            .replace('\\', "/"),
    )
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

fn build_excluded_set(args: &Args, root: &Path) -> HashSet<String> {
    let mut excluded = HashSet::new();

    for repo in DEFAULT_EXCLUDED_REPOS {
        excluded.insert(repo.to_string());
    }

    for repo in &args.exclude {
        if let Some(normalized) = normalize_ignore_entry(repo) {
            excluded.insert(normalized);
        }
    }

    for repo in load_updateignore(root) {
        excluded.insert(repo);
    }

    excluded
}

fn relative_path_string(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map(|p| p.trim_start_matches("./").trim_end_matches('/').to_string())
}

fn is_excluded(root: &Path, path: &Path, excluded: &HashSet<String>) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };

    if excluded.contains(name) {
        return true;
    }

    let Some(relative) = relative_path_string(root, path) else {
        return false;
    };

    excluded.contains(&relative)
}

fn discover_repos(root: &Path, excluded: &HashSet<String>) -> Vec<PathBuf> {
    WalkDir::new(root)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| !is_excluded(root, path, excluded))
        .filter(|path| is_git_repo(path))
        .collect()
}

fn check_repo(repo: &Path, args: &Args) {
    let name = repo
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>");

    println!("\n=== {name} ===");

    if let Err(e) = fetch_prune(repo) {
        println!("Fetch failed: {e}");
        return;
    }

    println!("Fetched");

    let current = current_branch(repo).ok().filter(|branch| !branch.is_empty());

    let branches = match local_branches(repo) {
        Ok(branches) => branches,
        Err(e) => {
            println!("Could not list local branches: {e}");
            return;
        }
    };

    let statuses: Vec<BranchStatus> = branches
        .iter()
        .map(|branch| branch_status(repo, branch))
        .collect();

    for status in &statuses {
        print_branch_status(status, current.as_deref());
    }

    if args.pull {
        pull_possible_branches(repo, &statuses);
    }
}

fn main() {
    let args = Args::parse();

    let root = Path::new(".");
    let excluded = build_excluded_set(&args, root);
    let repos = discover_repos(root, &excluded);

    if repos.is_empty() {
        println!("No git repositories found.");
        return;
    }

    for repo in repos {
        check_repo(&repo, &args);
    }
}
