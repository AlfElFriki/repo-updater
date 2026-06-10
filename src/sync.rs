use std::path::Path;

use crate::git;

#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub pull: bool,
    pub allow_dirty: bool,
}

#[derive(Debug, Clone)]
struct BranchStatus {
    branch: String,
    upstream: Option<String>,
    relation: BranchRelation,
}

#[derive(Debug, Clone)]
enum BranchRelation {
    NoUpstream,
    Unknown { message: String },
    UpToDate,
    Incoming { count: u32 },
    Ahead { count: u32 },
    Diverged { incoming: u32, outgoing: u32 },
}

impl BranchStatus {
    fn can_fast_forward(&self) -> bool {
        matches!(self.relation, BranchRelation::Incoming { .. })
    }
}

pub fn process_repo(repo: &Path, options: &SyncOptions) {
    let name = repo
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>");

    println!("\n=== {name} ===");

    if let Err(e) = git::fetch_prune(repo) {
        println!("Fetch failed: {e}");
        return;
    }

    println!("Fetched");

    let current_branch = match git::current_branch(repo) {
        Ok(branch) => branch,
        Err(e) => {
            println!("Could not determine current branch: {e}");
            None
        }
    };

    let branches = match git::local_branches(repo) {
        Ok(branches) => branches,
        Err(e) => {
            println!("Could not list local branches: {e}");
            return;
        }
    };

    let statuses: Vec<BranchStatus> = branches
        .iter()
        .map(|branch| inspect_branch(repo, branch))
        .collect();

    for status in &statuses {
        print_branch_status(status, current_branch.as_deref());
    }

    if options.pull {
        pull_fast_forwardable_branches(
            repo,
            &statuses,
            current_branch.as_deref(),
            options.allow_dirty,
        );
    }
}

fn inspect_branch(repo: &Path, branch: &str) -> BranchStatus {
    let Some(upstream) = git::upstream_for_branch(repo, branch) else {
        return BranchStatus {
            branch: branch.to_string(),
            upstream: None,
            relation: BranchRelation::NoUpstream,
        };
    };

    let incoming = match git::commit_count(repo, branch, &upstream) {
        Ok(count) => count,
        Err(e) => {
            return BranchStatus {
                branch: branch.to_string(),
                upstream: Some(upstream),
                relation: BranchRelation::Unknown {
                    message: format!("failed to check incoming commits: {e}"),
                },
            };
        }
    };

    let outgoing = match git::commit_count(repo, &upstream, branch) {
        Ok(count) => count,
        Err(e) => {
            return BranchStatus {
                branch: branch.to_string(),
                upstream: Some(upstream),
                relation: BranchRelation::Unknown {
                    message: format!("failed to check outgoing commits: {e}"),
                },
            };
        }
    };

    let relation = match (incoming, outgoing) {
        (0, 0) => BranchRelation::UpToDate,
        (count, 0) => BranchRelation::Incoming { count },
        (0, count) => BranchRelation::Ahead { count },
        (incoming, outgoing) => BranchRelation::Diverged { incoming, outgoing },
    };

    BranchStatus {
        branch: branch.to_string(),
        upstream: Some(upstream),
        relation,
    }
}

fn print_branch_status(status: &BranchStatus, current_branch: Option<&str>) {
    let marker = if current_branch == Some(status.branch.as_str()) {
        "*"
    } else {
        " "
    };

    match &status.relation {
        BranchRelation::NoUpstream => {
            println!("  {marker} {}: no upstream", status.branch);
        }
        BranchRelation::Unknown { message } => {
            println!("  {marker} {}: {message}", status.branch);
        }
        BranchRelation::UpToDate => {
            let upstream = status.upstream.as_deref().unwrap_or("<unknown>");
            println!("  {marker} {}: up to date with {upstream}", status.branch);
        }
        BranchRelation::Incoming { count } => {
            let upstream = status.upstream.as_deref().unwrap_or("<unknown>");
            println!(
                "  {marker} {}: {count} incoming commit(s) from {upstream}",
                status.branch
            );
        }
        BranchRelation::Ahead { count } => {
            let upstream = status.upstream.as_deref().unwrap_or("<unknown>");
            println!(
                "  {marker} {}: {count} local commit(s) ahead of {upstream}",
                status.branch
            );
        }
        BranchRelation::Diverged { incoming, outgoing } => {
            let upstream = status.upstream.as_deref().unwrap_or("<unknown>");
            println!(
                "  {marker} {}: diverged from {upstream} ({incoming} incoming, {outgoing} outgoing)",
                status.branch
            );
        }
    }
}

fn pull_fast_forwardable_branches(
    repo: &Path,
    statuses: &[BranchStatus],
    original_branch: Option<&str>,
    allow_dirty: bool,
) {
    let Some(original_branch) = original_branch else {
        println!("Skipping pulls: detached HEAD");
        return;
    };

    let original_branch = original_branch.to_string();
    let mut active_branch = original_branch.clone();

    if !allow_dirty {
        match git::working_tree_clean(repo) {
            Ok(true) => {}
            Ok(false) => {
                println!("Skipping pulls: working tree is dirty. Use --allow-dirty to let Git attempt safe pulls.");
                return;
            }
            Err(e) => {
                println!("Skipping pulls: could not inspect working tree: {e}");
                return;
            }
        }
    } else {
        match git::working_tree_clean(repo) {
            Ok(true) => {}
            Ok(false) => {
                println!("Working tree is dirty; attempting safe pulls because --allow-dirty was provided");
            }
            Err(e) => {
                println!("Warning: could not inspect working tree before dirty pull attempt: {e}");
            }
        }
    }

    let eligible_count = statuses
        .iter()
        .filter(|status| status.can_fast_forward())
        .count();

    if eligible_count == 0 {
        println!("No branches need fast-forward pull");
        return;
    }

    let mut pulled_count = 0;

    for status in statuses.iter().filter(|status| status.can_fast_forward()) {
        let branch = &status.branch;

        if !allow_dirty {
            match git::working_tree_clean(repo) {
                Ok(true) => {}
                Ok(false) => {
                    println!("Stopping pulls before '{branch}': working tree became dirty");
                    break;
                }
                Err(e) => {
                    println!("Stopping pulls before '{branch}': could not inspect working tree: {e}");
                    break;
                }
            }
        }

        if active_branch != *branch {
            match git::switch_branch(repo, branch) {
                Ok(()) => {
                    active_branch = branch.clone();
                }
                Err(e) => {
                    if allow_dirty {
                        println!(
                            "Skipping pull on '{branch}': Git refused to switch branch safely: {e}"
                        );
                    } else {
                        println!("Skipping pull on '{branch}': could not switch branch: {e}");
                    }

                    continue;
                }
            }
        }

        match git::pull_ff_only(repo) {
            Ok(()) => {
                println!("Pulled '{branch}' with fast-forward only");
                pulled_count += 1;
            }
            Err(e) => {
                if allow_dirty {
                    println!(
                        "Pull failed on '{branch}'. Git likely refused because local changes would be overwritten: {e}"
                    );
                } else {
                    println!("Pull failed on '{branch}': {e}");
                }
            }
        }
    }

    restore_original_branch(repo, &active_branch, &original_branch, allow_dirty);

    if pulled_count == 0 {
        println!("No branches were pulled");
    }
}

fn restore_original_branch(
    repo: &Path,
    active_branch: &str,
    original_branch: &str,
    allow_dirty: bool,
) {
    if active_branch == original_branch {
        return;
    }

    if !allow_dirty {
        match git::working_tree_clean(repo) {
            Ok(true) => {}
            Ok(false) => {
                println!(
                    "Could not restore original branch '{original_branch}': working tree is dirty"
                );
                return;
            }
            Err(e) => {
                println!(
                    "Could not restore original branch '{original_branch}': could not inspect working tree: {e}"
                );
                return;
            }
        }
    }

    match git::switch_branch(repo, original_branch) {
        Ok(()) => {
            println!("Restored original branch '{original_branch}'");
        }
        Err(e) => {
            if allow_dirty {
                println!(
                    "Failed to restore original branch '{original_branch}'. Git refused to switch safely: {e}"
                );
            } else {
                println!("Failed to restore original branch '{original_branch}': {e}");
            }
        }
    }
}
