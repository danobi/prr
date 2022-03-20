use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

mod prr;
mod review;
use prr::Prr;

#[derive(Subcommand, Debug)]
enum Command {
    /// Get a pull request and begin a review
    Get {
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
    },
    /// Submit a review
    Submit {
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
    },
}

#[derive(Parser, Debug)]
struct Args {
    /// Path to config file
    #[clap(long, parse(from_os_str))]
    config: Option<PathBuf>,
    #[clap(subcommand)]
    command: Command,
}

/// Parses a PR string in the form of `danobi/prr/24` and returns
/// a tuple ("danobi", "prr", 24) or an error if string is malformed
fn parse_pr_str(s: &str) -> Result<(String, String, u64)> {
    let pieces: Vec<&str> = s.split('/').map(|ss| ss.trim()).collect();
    if pieces.len() != 3 {
        bail!("Invalid PR ref format: does not contain two '/'");
    }

    let owner = pieces[0].to_string();
    let repo = pieces[1].to_string();
    let pr_nr: u64 = pieces[2].parse()?;

    Ok((owner, repo, pr_nr))
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
        Command::Get { pr } => {
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            prr.get_pr(&owner, &repo, pr_num).await?;
        }
        Command::Submit { pr } => {
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            prr.submit_pr(&owner, &repo, pr_num).await?;
        }
    }

    Ok(())
}
