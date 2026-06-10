use std::{
    path::Path,
    process::Command,
};

pub type GitResult<T> = Result<T, String>;

pub fn is_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

pub fn fetch_prune(repo: &Path) -> GitResult<()> {
    run(repo, &["fetch", "--prune"])?;
    Ok(())
}

pub fn current_branch(repo: &Path) -> GitResult<Option<String>> {
    match run(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Ok(branch) if !branch.is_empty() => Ok(Some(branch)),
        Ok(_) => Ok(None),
        Err(_) => Ok(None),
    }
}

pub fn local_branches(repo: &Path) -> GitResult<Vec<String>> {
    let output = run(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )?;

    Ok(output
        .lines()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(String::from)
        .collect())
}

pub fn upstream_for_branch(repo: &Path, branch: &str) -> Option<String> {
    let upstream_spec = format!("{branch}@{{upstream}}");

    run(
        repo,
        &[
            "rev-parse",
            "--abbrev-ref",
            &upstream_spec,
        ],
    )
    .ok()
    .filter(|upstream| !upstream.is_empty())
}

pub fn commit_count(repo: &Path, from: &str, to: &str) -> GitResult<u32> {
    let range = format!("{from}..{to}");

    let output = run(
        repo,
        &[
            "rev-list",
            "--count",
            &range,
        ],
    )?;

    output
        .trim()
        .parse::<u32>()
        .map_err(|e| format!("Could not parse commit count '{output}': {e}"))
}

pub fn working_tree_clean(repo: &Path) -> GitResult<bool> {
    let output = run(
        repo,
        &[
            "status",
            "--porcelain=v1",
        ],
    )?;

    Ok(output.trim().is_empty())
}

pub fn switch_branch(repo: &Path, branch: &str) -> GitResult<()> {
    run(
        repo,
        &[
            "switch",
            "--",
            branch,
        ],
    )?;

    Ok(())
}

pub fn pull_ff_only(repo: &Path) -> GitResult<()> {
    run(
        repo,
        &[
            "pull",
            "--ff-only",
        ],
    )?;

    Ok(())
}

fn run(repo: &Path, args: &[&str]) -> GitResult<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("Failed to run git {}: {e}", args.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let message = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("git {} exited with {}", args.join(" "), output.status)
        };

        return Err(message);
    }

    Ok(stdout)
}
