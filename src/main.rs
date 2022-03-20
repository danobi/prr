use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use octocrab::Octocrab;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct PrrConfig {
    /// GH personal token
    token: String,
    /// Directory to place review files
    workdir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Config {
    prr: PrrConfig,
}

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

/// Main struct that coordinates all business logic and talks to GH
struct Prr {
    /// User config
    config: Config,
    /// Instantiated github client
    crab: Octocrab,
}

/// Represents the state of a single review
struct Review {
    /// Path to workdir
    workdir: PathBuf,
    /// Name of the owner of the repository
    owner: String,
    /// Name of the repository
    repo: String,
    /// Issue # of the pull request
    pr_num: u64,
}

/// Metadata for a single review. Stored as dotfile next to user-facing review file
#[derive(Serialize, Deserialize, Debug)]
struct ReviewMetadata {
    /// Original .diff file contents. Used to detect corrupted review files
    original: String,
}

/// Represents a single comment on a review
#[derive(Debug, PartialEq)]
struct ReviewComment {
    /// File the comment is in
    ///
    /// Note that this is the new filename if the file was also moved
    file: String,
    /// The "line" a comment applies to. To quote github API:
    ///
    /// The position value equals the number of lines down from the first "@@" hunk header in the
    /// file you want to add a comment. The line just below the "@@" line is position 1, the next
    /// line is position 2, and so on. The position in the diff continues to increase through lines
    /// of whitespace and additional hunks until the beginning of a new file.
    position: u64,
    /// For a spanned comment, the first line of the span. See `position` for docs on semantics
    start_position: Option<u64>,
    /// The user-supplied review comment
    comment: String,
}

fn prefix_lines(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|line| prefix.to_owned() + line + "\n")
        .collect()
}

impl Review {
    /// Creates a new `Review`
    ///
    /// `review_file` is the path where the user-facing review file should
    /// be created. Additional metadata files (dotfiles) may be created in the same
    /// directory.
    fn new(workdir: &Path, diff: String, owner: &str, repo: &str, pr_num: u64) -> Result<Review> {
        let review = Review {
            workdir: workdir.to_owned(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            pr_num,
        };

        // First create directories leading up to review file if necessary
        let review_path = review.path();
        let review_dir = review_path
            .parent()
            .ok_or_else(|| anyhow!("Review path has no parent!"))?;
        fs::create_dir_all(&review_dir).context("Failed to create workdir directories")?;

        // Now create review file
        let mut review_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&review_path)
            .context("Failed to create review file")?;
        let review_contents = prefix_lines(&diff, "> ");
        review_file
            .write_all(review_contents.as_bytes())
            .context("Failed to write review file")?;

        // Create metadata file
        let metadata = ReviewMetadata { original: diff };
        let json = serde_json::to_string(&metadata)?;
        let mut metadata_path = review_path.clone();
        metadata_path.set_file_name(format!(".{}", pr_num));
        let mut metadata_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&metadata_path)
            .context("Failed to create metadata file")?;
        metadata_file
            .write_all(json.as_bytes())
            .context("Failed to write metadata file")?;

        Ok(review)
    }

    /// Creates a `Review` that already exists on disk
    ///
    /// Note we do not check that anything actually exists on disk because that is
    /// inherently racy. We'll handle ENOENT errors when we actually use any files.
    fn new_existing(workdir: &Path, owner: &str, repo: &str, pr_num: u64) -> Review {
        Review {
            workdir: workdir.to_owned(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            pr_num,
        }
    }

    /// Parse the user-supplied comments on a review
    fn comments(&self) -> Result<Vec<ReviewComment>> {
        // XXX: implement
        unimplemented!();
    }

    /// Returns path to user-facing review file
    fn path(&self) -> PathBuf {
        let mut p = self.workdir.clone();
        p.push(&self.owner);
        p.push(&self.repo);
        p.push(self.pr_num.to_string());

        p
    }
}

impl Prr {
    fn new(config_path: &Path) -> Result<Prr> {
        let config_contents = fs::read_to_string(config_path).context("Failed to read config")?;
        let config: Config = toml::from_str(&config_contents).context("Failed to parse toml")?;
        let octocrab = Octocrab::builder()
            .personal_token(config.prr.token.clone())
            .build()
            .context("Failed to create GH client")?;

        Ok(Prr {
            config,
            crab: octocrab,
        })
    }

    fn workdir(&self) -> Result<PathBuf> {
        match &self.config.prr.workdir {
            Some(d) => {
                if d.starts_with('~') {
                    bail!("Workdir may not use '~' to denote home directory");
                }

                Ok(Path::new(d).to_path_buf())
            }
            None => {
                let xdg_dirs = xdg::BaseDirectories::with_prefix("prr")?;
                Ok(xdg_dirs.get_data_home())
            }
        }
    }

    async fn get_pr(&self, owner: &str, repo: &str, pr_num: u64) -> Result<Review> {
        let diff = self
            .crab
            .pulls(owner, repo)
            .get_diff(pr_num)
            .await
            .context("Failed to fetch diff")?;

        Review::new(&self.workdir()?, diff, owner, repo, pr_num)
    }

    async fn submit_pr(&self, owner: &str, repo: &str, pr_num: u64) -> Result<()> {
        let review = Review::new_existing(&self.workdir()?, owner, repo, pr_num);
        let _comments = review.comments()?;

        // XXX: submit comments to GH in a single API call (POST /repos/{owner}/{repo}/pulls/{pull_number}/reviews)

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
