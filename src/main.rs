use std::env;
use std::path::{Path, PathBuf};
use std::process;

use anyhow::{bail, Context, Result};
use clap::Parser;

mod cli;
mod parser;
mod prr;
mod review;

use cli::*;
use prr::Prr;

/// The name of the local configuration file
pub const LOCAL_CONFIG_FILE_NAME: &str = ".prr.toml";

/// Returns if exists the config file for the current project
fn find_project_config_file() -> Option<PathBuf> {
    env::current_dir().ok().and_then(|mut path| loop {
        path.push(LOCAL_CONFIG_FILE_NAME);
        if path.exists() {
            return Some(path);
        }

        path.pop();
        if !path.pop() {
            return None;
        }
    })
}

/// Opens a file in $EDITOR
fn open_review(file: &Path) -> Result<()> {
    // This check should only ever trip for prr-edit
    if !file.try_exists()? {
        bail!("Review file does not exist yet");
    }

    let editor = env::var("EDITOR").context("Failed to read $EDITOR")?;
    let status = process::Command::new(editor)
        .arg(file)
        .status()
        .context("Failed to execute editor process")?;

    match status.code() {
        Some(0) => Ok(()),
        Some(rc) => bail!("EDITOR exited unclean: {}", rc),
        None => bail!("Failed to get EDITOR exit status"),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    // Figure out where config file is
    let config_path = match args.config {
        Some(c) => c,
        None => {
            let xdg_dirs = xdg::BaseDirectories::with_prefix("prr")?;
            xdg_dirs.get_config_file("config.toml")
        }
    };

    let prr = Prr::new(&config_path, find_project_config_file())?;

    match args.command {
        Command::Get { pr, force, open } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            let review = prr.get_pr(&owner, &repo, pr_num, force).await?;
            let path = review.path();
            println!("{}", path.display());
            if open {
                open_review(&path).context("Failed to open review file")?;
            }
        }
        Command::Edit { pr } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            let review = prr.get_review(&owner, &repo, pr_num)?;
            open_review(&review.path()).context("Failed to open review file")?;
        }
        Command::Submit { pr, debug } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            prr.submit_pr(&owner, &repo, pr_num, debug).await?;
        }
        Command::Apply { pr } => {
            let (owner, repo, pr_num) = prr.parse_pr_str(&pr)?;
            prr.apply_pr(&owner, &repo, pr_num, Path::new("./"))?;
        }
        Command::Status { no_titles } => {
            prr.print_status(no_titles)?;
        }
        Command::Remove {
            prs,
            force,
            submitted,
        } => {
            prr.remove(&prs, force, submitted).await?;
        }
    }

    Ok(())
}
