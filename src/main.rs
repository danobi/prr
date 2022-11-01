use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use lazy_static::lazy_static;
use regex::{Captures, Regex};

mod parser;
mod prr;
mod review;

use prr::Prr;

// Use lazy static to ensure regex is only compiled once
lazy_static! {
    // Regex for short input. Example:
    //
    //      danobi/prr-test-repo/6
    //
    static ref SHORT: Regex = Regex::new(r"^(?P<org>[\w\-_]+)/(?P<repo>[\w\-_]+)/(?P<pr_num>\d+)").unwrap();

    // Regex for url input. Url looks something like:
    //
    //      https://github.com/danobi/prr-test-repo/pull/6
    //
    static ref URL: Regex = Regex::new(r".*github\.com/(?P<org>.+)/(?P<repo>.+)/pull/(?P<pr_num>\d+)").unwrap();
}

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

/// Parses a PR string in the form of `danobi/prr/24` and returns
/// a tuple ("danobi", "prr", 24) or an error if string is malformed
fn parse_pr_str<'a>(s: &'a str) -> Result<(String, String, u64)> {
    let f = |captures: Captures<'a>| -> Result<(String, String, u64)> {
        let owner = captures.name("org").unwrap().as_str().to_owned();
        let repo = captures.name("repo").unwrap().as_str().to_owned();
        let pr_nr: u64 = captures
            .name("pr_num")
            .unwrap()
            .as_str()
            .parse()
            .context("Failed to parse pr number")?;

        Ok((owner, repo, pr_nr))
    };

    if let Some(captures) = SHORT.captures(s) {
        f(captures)
    } else if let Some(captures) = URL.captures(s) {
        f(captures)
    } else {
        bail!("Invalid PR ref format")
    }
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
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            let review = prr.get_pr(&owner, &repo, pr_num, force).await?;
            println!("{}", review.path().display());
        }
        Command::Submit { pr, debug } => {
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            prr.submit_pr(&owner, &repo, pr_num, debug).await?;
        }
        Command::Apply { pr } => {
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            prr.apply_pr(&owner, &repo, pr_num)?;
        }
        Command::Status { no_titles } => {
            prr.print_status(no_titles)?;
        }
        Command::Remove { pr, force } => {
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            let review = prr.get_pr(&owner, &repo, pr_num, force).await?;
            review.remove(force)?;
        }
    }

    Ok(())
}
