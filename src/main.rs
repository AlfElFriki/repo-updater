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

    /// Exclude a repository directory. Can be passed multiple times.
    ///
    /// Example:
    ///   repo-sync --exclude repo-a --exclude repo-b
    #[arg(long = "exclude", value_name = "DIR")]
    exclude: Vec<String>,
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

    let config = Config::load(&root, &args.exclude);
    let repos = discover_repos(&root, &config);

    if repos.is_empty() {
        println!("No git repositories found.");
        return;
    }

    let options = SyncOptions { pull: args.pull };

    for repo in repos {
        process_repo(&repo, &options);
    }
}
