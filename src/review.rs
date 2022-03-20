use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_derive::{Deserialize, Serialize};

use crate::parser::{ReviewComment, ReviewParser};

/// Represents the state of a single review
pub struct Review {
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
    pub fn new(
        workdir: &Path,
        diff: String,
        owner: &str,
        repo: &str,
        pr_num: u64,
    ) -> Result<Review> {
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
    pub fn new_existing(workdir: &Path, owner: &str, repo: &str, pr_num: u64) -> Review {
        Review {
            workdir: workdir.to_owned(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            pr_num,
        }
    }

    /// Parse the user-supplied comments on a review
    pub fn comments(&self) -> Result<Vec<ReviewComment>> {
        let contents = fs::read_to_string(self.path()).context("Failed to read review file")?;
        let mut parser = ReviewParser::new();
        let mut comments = Vec::new();

        // XXX: validate review file by comparing quoted lines to original stored in metadata

        for line in contents.lines() {
            let res = parser.parse_line(line).context("Failed to parse review")?;
            if let Some(c) = res {
                comments.push(c);
            }
        }

        Ok(comments)
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
