mod config;
mod discovery;
mod git;
mod sync;

use clap::Parser;

use config::Config;
use discovery::discover_repos;
use sync::{process_repo, SyncOptions};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Try to pull branches after fetching, fast-forward only
    #[arg(long)]
    pull: bool,

    /// Allow pull attempts even when the working tree is dirty.
    ///
    /// This does not stash, reset, force checkout, merge, or rebase.
    /// Git is allowed to reject unsafe switches or pulls.
    #[arg(long)]
    allow_dirty: bool,

    /// Exclude a repository directory. Can be passed multiple times.
    ///
    /// Example:
    ///   repo-sync --exclude repo-a --exclude repo-b
    #[arg(long = "exclude", value_name = "DIR")]
    exclude: Vec<String>,

    /// Only process selected repository directories. Can be passed multiple times.
    ///
    /// .updateignore is still respected and has priority.
    ///
    /// Example:
    ///   repo-sync --only repo-a --only repo-b
    #[arg(long = "only", value_name = "DIR")]
    only: Vec<String>,
}

fn main() {
    let args = Args::parse();

    let root = match std::env::current_dir() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to read current directory: {e}");
            std::process::exit(1);
        }
    };

    let config = Config::load(&root, &args.exclude, &args.only);
    let repos = discover_repos(&root, &config);

    if repos.is_empty() {
        println!("No git repositories found.");
        return;
    }

    let options = SyncOptions {
        pull: args.pull,
        allow_dirty: args.allow_dirty,
    };

    for repo in repos {
        process_repo(&repo, &options);
    }
}
