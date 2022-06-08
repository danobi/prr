use std::fs;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, bail, Context, Result};
use serde_derive::{Deserialize, Serialize};

use crate::parser::{Comment, InlineComment, ReviewAction, ReviewParser};

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
    /// Time (seconds since epoch) the review file was last submitted
    submitted: Option<u64>,
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
        force: bool,
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

        // Check if there are unsubmitted changes
        if !force
            && review
                .unsubmitted()
                .context("Failed to check for unsubmitted review")?
        {
            bail!(
                "You have unsubmitted changes to the requested review. \
                Either submit the existing changes, delete the existing review file, \
                or re-run this command with --force."
            );
        }

        // Now create review file
        let mut review_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&review_path)
            .context("Failed to create review file")?;
        let review_contents = prefix_lines(&diff, "> ");
        review_file
            .write_all(review_contents.as_bytes())
            .context("Failed to write review file")?;

        // Create metadata file
        let metadata = ReviewMetadata {
            original: diff,
            submitted: None,
        };
        let json = serde_json::to_string(&metadata)?;
        let metadata_path = review.metadata_path();
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
    ///
    /// Returns (overall review action, overall review comment, inline comments)
    pub fn comments(&self) -> Result<(ReviewAction, String, Vec<InlineComment>)> {
        let contents = fs::read_to_string(self.path()).context("Failed to read review file")?;
        self.validate_review_file(&contents)?;

        let mut parser = ReviewParser::new();
        let mut review_action = ReviewAction::Comment;
        let mut review_comment = String::new();
        let mut inline_comments = Vec::new();
        for (idx, line) in contents.lines().enumerate() {
            let res = parser
                .parse_line(line)
                .with_context(|| format!("Failed to parse review on line {}", idx + 1))?;

            match res {
                Some(Comment::Review(c)) => {
                    if !review_comment.is_empty() {
                        bail!("Somehow saw more than one review comment");
                    }

                    review_comment = c;
                }
                Some(Comment::Inline(c)) => inline_comments.push(c),
                Some(Comment::ReviewAction(a)) => review_action = a,
                None => {}
            }
        }

        match parser.finish() {
            Some(Comment::Inline(c)) => inline_comments.push(c),
            // Original diff must have been short to begin with
            Some(Comment::Review(_)) => bail!("Unexpected review comment at parser finish"),
            Some(Comment::ReviewAction(_)) => bail!("Unexpected review action at parser finish"),
            None => {}
        };

        Ok((review_action, review_comment, inline_comments))
    }

    /// Update the review file's submission time
    pub fn mark_submitted(&self) -> Result<()> {
        let metadata_path = self.metadata_path();
        let data = fs::read_to_string(&metadata_path).context("Failed to read metadata file")?;
        let mut metadata: ReviewMetadata =
            serde_json::from_str(&data).context("Failed to parse metadata json")?;

        let submission_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time went backwards");
        metadata.submitted = Some(submission_time.as_secs());

        let json = serde_json::to_string(&metadata)?;
        let mut metadata_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&metadata_path)
            .context("Failed to create metadata file")?;
        metadata_file
            .write_all(json.as_bytes())
            .context("Failed to write metadata file")?;

        Ok(())
    }

    /// Validates whether the user corrupted the quoted contents
    fn validate_review_file(&self, contents: &str) -> Result<()> {
        let mut reconstructed = String::with_capacity(contents.len());
        for line in contents.lines() {
            if let Some(stripped) = line.strip_prefix("> ") {
                reconstructed += stripped;
                reconstructed += "\n";
            }
        }

        let metadata_path = self.metadata_path();
        let data = fs::read_to_string(metadata_path).context("Failed to read metadata file")?;
        let metadata: ReviewMetadata =
            serde_json::from_str(&data).context("Failed to parse metadata json")?;

        if reconstructed != metadata.original {
            // Be helpful and provide exact line number of mismatch.
            //
            // This loop on zip() will work as long as there isn't any truncation or trailing junk
            // in the original text. To handle this case, there's the final bail!()
            for (idx, (l, r)) in reconstructed
                .lines()
                .zip(metadata.original.lines())
                .enumerate()
            {
                if l != r {
                    // Get number of user generated lines up until the mismatch
                    let user_lines = contents
                        .lines()
                        .take(idx)
                        .filter(|l| !l.starts_with("> "))
                        .count();
                    let err = format!("Line {}, found '{l}' expected '{r}'", idx + 1 + user_lines);
                    bail!("Detected corruption in quoted part of review file: {err}");
                }
            }

            bail!("Detected corruption in quoted part of review file: found trailing or truncated lines");
        }

        Ok(())
    }

    /// Returns whether or not there exist unsubmitted changes on disk
    fn unsubmitted(&self) -> Result<bool> {
        let data = match fs::read_to_string(self.metadata_path()) {
            Ok(d) => d,
            Err(e) => match e.kind() {
                // If there's not yet a metadata file, means review not started yet
                ErrorKind::NotFound => return Ok(false),
                _ => bail!("Failed to read review metadata: {}", e),
            },
        };
        let metadata: ReviewMetadata =
            serde_json::from_str(&data).context("Failed to parse metadata json")?;

        let file_metadata = match fs::metadata(self.path()) {
            Ok(m) => m,
            Err(e) => match e.kind() {
                // If there's not yet a review file, it cannot be unsubmitted
                ErrorKind::NotFound => return Ok(false),
                _ => bail!("Failed to open review file: {}", e),
            },
        };
        let mtime: u64 = file_metadata
            .mtime()
            .try_into()
            .context("mtime is negative")?;

        match metadata.submitted {
            // If modified time is more recent than last submission, then unsubmitted
            Some(t) => Ok(mtime > t),
            // If no last submission time, then default to unsubmitted
            None => Ok(true),
        }
    }

    /// Returns path to user-facing review file
    pub fn path(&self) -> PathBuf {
        let mut p = self.workdir.clone();
        p.push(&self.owner);
        p.push(&self.repo);
        p.push(format!("{}.prr", self.pr_num));

        p
    }

    fn metadata_path(&self) -> PathBuf {
        let mut metadata_path = self.path();
        metadata_path.set_file_name(format!(".{}", self.pr_num));

        metadata_path
    }
}
