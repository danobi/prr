use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod parser;
mod prr;
mod review;

use prr::Prr;

#[derive(Subcommand, Debug)]
enum Command {
    /// Get a pull request and begin a review
    Get {
        /// Ignore unsubmitted review checks
        #[clap(short, long)]
        force: bool,
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
    },
    /// Submit a review
    Submit {
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
        #[clap(short, long)]
        debug: bool,
    },
    /// Apply a pull request to the working directory
    ///
    /// This can be useful for building/testing PRs
    Apply { pr: String },
    /// Print a status summary of all known reviews
    Status {
        /// Hide column titles from output
        #[clap(short, long)]
        no_titles: bool,
    },
    /// Remove a review
    Remove {
        /// Ignore unsubmitted review checks
        #[clap(short, long)]
        force: bool,
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
    },
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to config file
    #[clap(long, parse(from_os_str))]
    config: Option<PathBuf>,
    #[clap(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Figure out where config file is
    let config_path = match args.config {
        Some(c) => c,
        None => {
            let xdg_dirs = xdg::BaseDirectories::with_prefix("prr")?;
            xdg_dirs.get_config_file("config.toml")
        }
    };

    let prr = Prr::new(&config_path)?;

    match args.command {
        Command::Get { pr, force } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            let review = prr.get_pr(&owner, &repo, pr_num, force).await?;
            println!("{}", review.path().display());
        }
        Command::Submit { pr, debug } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            prr.submit_pr(&owner, &repo, pr_num, debug).await?;
        }
        Command::Apply { pr } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            prr.apply_pr(&owner, &repo, pr_num)?;
        }
        Command::Status { no_titles } => {
            prr.print_status(no_titles)?;
        }
        Command::Remove { pr, force } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            let review = prr.get_pr(&owner, &repo, pr_num, force).await?;
            review.remove(force)?;
        }
    }

    Ok(())
}
