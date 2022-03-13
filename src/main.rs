use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use clap::Parser;
use octocrab::Octocrab;
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
struct PrrConfig {
    token: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    prr: PrrConfig,
}

#[derive(Parser, Debug)]
struct Args {
    /// Path to config file
    #[clap(long, parse(from_os_str))]
    config: Option<PathBuf>,
    /// Pull request to review (eg. `danobi/prr/24`)
    pr: String,
}

/// Main struct
struct Prr {
    /// Instantiated github client
    crab: Octocrab,
    /// Name of the owner of the repository
    owner: String,
    /// Name of the repository
    repo: String,
    /// Issue # of the pull request
    pr_num: u64,
}

impl Prr {
    fn new(config_path: &Path, owner: String, repo: String, pr_num: u64) -> Result<Prr> {
        let config_contents = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&config_contents)?;
        let octocrab = Octocrab::builder()
            .personal_token(config.prr.token)
            .build()?;

        Ok(Prr {
            crab: octocrab,
            owner,
            repo,
            pr_num,
        })
    }

    // XXX: save it to somewhere on disk instead of printing to stdout
    async fn fetch_patch(&self) -> Result<()> {
        let patch = self
            .crab
            .pulls(&self.owner, &self.repo)
            .get_patch(self.pr_num)
            .await?;

        print!("{patch}");

        Ok(())
    }
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

    let (owner, repo, pr_num) = parse_pr_str(&args.pr)?;
    let prr = Prr::new(&config_path, owner, repo, pr_num)?;

    // XXX: delete
    prr.fetch_patch().await?;

    Ok(())
}
