use std::fmt::{Display, Formatter, Result as fmt_result, Write as fmt_write};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, bail, Context, Result};
use serde_derive::{Deserialize, Serialize};

use crate::parser::{Comment, FileComment, InlineComment, ReviewAction, ReviewParser};

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
pub struct ReviewMetadata {
    /// Original .diff file contents. Used to detect corrupted review files
    original: String,
    /// Time (seconds since epoch) the review file was last submitted
    submitted: Option<u64>,
    /// The commit hash of the PR at the time the review was started
    commit_id: Option<String>,
}

/// Status of a review
#[derive(PartialEq)]
pub enum ReviewStatus {
    /// Newly downloaded review; no changes yet
    New,
    /// Unsubmitted changes have been made to review file
    Reviewed,
    /// Review has been submitted. Any further changes to the review file are ignored
    Submitted,
}

impl ReviewMetadata {
    pub fn commit_id(&self) -> Option<&str> {
        self.commit_id.as_deref()
    }

    /// Returns the original, raw diff of the review
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Returns last submitted time, if any
    fn submitted(&self) -> Option<u64> {
        self.submitted
    }
}

impl Display for ReviewStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt_result {
        let text = match self {
            Self::New => "NEW",
            Self::Reviewed => "REVIEWED",
            Self::Submitted => "SUBMITTED",
        };

        write!(f, "{text}")
    }
}

fn prefix_lines(s: &str, prefix: &str) -> String {
    let mut ret = String::with_capacity(s.len());

    for line in s.lines() {
        if line.is_empty() {
            ret += prefix;
        } else {
            // Appending to heap allocated string cannot fail
            writeln!(ret, "{} {}", prefix, line).expect("Failed to write to string");
        }
    }

    ret
}

/// Returns a list of all reviews in a workdir
pub fn get_all_existing(workdir: &Path) -> Result<Vec<Review>> {
    // This pipeline does the following:
    //   * Iterate through all org directories in workdir
    //   * For each org directory, iterate through all contained repo directories
    //   * For each repo directory, enumerate all non-metadata review files
    let reviews: Vec<PathBuf> = fs::read_dir(workdir)
        .context("Failed to read workdir")?
        .filter_map(|entry| entry.ok())
        .map(|org| org.path())
        .filter(|org| org.is_dir())
        .filter_map(|org| fs::read_dir(org).ok())
        .flatten()
        .filter_map(|repo| repo.ok())
        .map(|repo| repo.path())
        .filter(|repo| repo.is_dir())
        .filter_map(|repo| fs::read_dir(repo).ok())
        .flatten()
        .filter_map(|review| review.ok())
        .map(|review| review.path())
        .filter(|review| review.is_file())
        .filter(|review| match review.extension() {
            Some(e) => e == "prr",
            None => false,
        })
        .collect();

    let mut ret = Vec::with_capacity(reviews.len());
    for review in reviews {
        let parts: Vec<_> = review
            .iter()
            .rev()
            .take(3)
            .map(|p| p.to_string_lossy())
            .collect();

        if parts.len() != 3 {
            bail!("malformed review file path: {}", review.display());
        }

        let pr_num: u64 = parts[0]
            .strip_suffix(".prr")
            .unwrap_or(&parts[0])
            .parse()
            .with_context(|| format!("Failed to parse PR num: {}", review.display()))?;

        // Note the vec has components reversed
        let r = Review::new_existing(workdir, &parts[2], &parts[1], pr_num);
        ret.push(r);
    }

    Ok(ret)
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
        commit_id: String,
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
        fs::create_dir_all(review_dir).context("Failed to create workdir directories")?;

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
        let review_contents = prefix_lines(&diff, ">");
        review_file
            .write_all(review_contents.as_bytes())
            .context("Failed to write review file")?;

        // Create metadata file
        let metadata = ReviewMetadata {
            original: diff,
            submitted: None,
            commit_id: Some(commit_id),
        };
        let json = serde_json::to_string(&metadata)?;
        let metadata_path = review.metadata_path();
        let mut metadata_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(metadata_path)
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
    /// Returns (overall review action, overall review comment, inline comments, file comments)
    pub fn comments(&self) -> Result<(ReviewAction, String, Vec<InlineComment>, Vec<FileComment>)> {
        let contents = fs::read_to_string(self.path()).context("Failed to read review file")?;
        self.validate_review_file(&contents)?;

        let mut parser = ReviewParser::new();
        let mut review_action = ReviewAction::Comment;
        let mut review_comment = String::new();
        let mut inline_comments = Vec::new();
        let mut file_comments = Vec::new();
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
                Some(Comment::File(fc)) => file_comments.push(fc),
                None => {}
            }
        }

        match parser.finish() {
            Some(Comment::Inline(c)) => inline_comments.push(c),
            // Original diff must have been short to begin with
            Some(Comment::Review(_)) => bail!("Unexpected review comment at parser finish"),
            Some(Comment::ReviewAction(_)) => bail!("Unexpected review action at parser finish"),
            Some(Comment::File(_)) => bail!("Unexpected file-level comment at parser finish"),
            None => {}
        };

        Ok((
            review_action,
            review_comment,
            inline_comments,
            file_comments,
        ))
    }

    /// Update the review file's submission time
    pub fn mark_submitted(&self) -> Result<()> {
        let metadata_path = self.metadata_path();
        let mut metadata = self.metadata()?;

        let submission_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time went backwards");
        metadata.submitted = Some(submission_time.as_secs());

        let json = serde_json::to_string(&metadata)?;
        let mut metadata_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(metadata_path)
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
                reconstructed += stripped.trim_end();
                reconstructed += "\n";
            }

            if line == ">" {
                reconstructed += "\n";
            }
        }

        let metadata = self.metadata()?;
        let original: String = metadata
            .original
            .lines()
            .map(|line| line.trim_end().to_owned() + "\n")
            .collect();

        if reconstructed != original {
            // Be helpful and provide exact line number of mismatch.
            //
            // This loop on zip() will work as long as there isn't any truncation or trailing junk
            // in the original text. To handle this case, there's the final bail!()
            for (idx, (l, r)) in reconstructed.lines().zip(original.lines()).enumerate() {
                if l != r {
                    // Get number of user generated lines up until the mismatch
                    let user_lines = contents
                        .lines()
                        .take(idx)
                        .filter(|l| !l.starts_with('>'))
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
        // If a review file has been submitted, then any further changes are ignored
        let metadata = self.metadata()?;
        if metadata.submitted().is_some() {
            return Ok(false);
        }

        // Now we know the review is unsubmitted. But did user mark it up?
        self.reviewed()
    }

    /// Returns whether or not there exists review comments
    fn reviewed(&self) -> Result<bool> {
        let (_, review_comment, comments, file_comments) = self
            .comments()
            .with_context(|| anyhow!("Failed to parse comments for {}", self.path().display()))?;

        Ok(!review_comment.is_empty() || !comments.is_empty() || !file_comments.is_empty())
    }

    /// Returns path to user-facing review file
    pub fn path(&self) -> PathBuf {
        let mut p = self.workdir.clone();
        p.push(&self.owner);
        p.push(&self.repo);
        p.push(format!("{}.prr", self.pr_num));

        p
    }

    /// Loads and returns the parsed contents of the metadata file for the review file
    pub fn metadata(&self) -> Result<ReviewMetadata> {
        let meta =
            fs::read_to_string(self.metadata_path()).context("Failed to load metadata file")?;
        serde_json::from_str::<ReviewMetadata>(&meta).context("Failed to parse metadata file")
    }

    fn metadata_path(&self) -> PathBuf {
        let mut metadata_path = self.path();
        metadata_path.set_file_name(format!(".{}", self.pr_num));

        metadata_path
    }

    /// Returns a handle (eg "owner/repo/pr_num") to this review
    pub fn handle(&self) -> String {
        format!("{}/{}/{}", self.owner, self.repo, self.pr_num)
    }

    /// Gets the status of a review
    pub fn status(&self) -> Result<ReviewStatus> {
        let metadata = self.metadata()?;
        let reviewed = self.reviewed()?;
        let status = if metadata.submitted().is_some() {
            ReviewStatus::Submitted
        } else if reviewed {
            ReviewStatus::Reviewed
        } else {
            ReviewStatus::New
        };

        Ok(status)
    }

    /// Remove review from filesystem
    pub fn remove(self, force: bool) -> Result<()> {
        if !force
            && self
                .unsubmitted()
                .context("Failed to check for unsubmitted review")?
        {
            bail!(
                "You have unsubmitted changes to the requested review. \
                Re-run this command with --force to ignore this check."
            );
        }

        fs::remove_file(self.path()).context("Failed to remove review file")?;
        fs::remove_file(self.metadata_path()).context("Failed to remove metadata file")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, File};
    use tempfile::{tempdir, TempDir};

    fn setup(review: &str, metadata: &str) -> (Review, TempDir) {
        let dir = tempdir().expect("Failed to create tempdir");

        // Create directory structure
        let project_dir = dir.path().join("some_owner").join("some_repo");
        create_dir_all(&project_dir).expect("Failed to create workdir structure");

        // Create and write review file
        let mut review_file =
            File::create(project_dir.join("3.prr")).expect("Failed to create review file");
        review_file
            .write_all(review.as_bytes())
            .expect("Failed to write review file");

        // Create and write metadata file
        let mut metadata_file =
            File::create(project_dir.join(".3")).expect("Failed to create metadata file");
        metadata_file
            .write_all(metadata.as_bytes())
            .expect("Failed to write metadata file");

        let r = Review::new_existing(dir.path(), "some_owner", "some_repo", 3);

        (r, dir)
    }

    // Review file has all trailing whitespace stripped
    #[test]
    fn test_validate_stripped() {
        let review = include_str!("../testdata/review/trailing_whitespace/review");
        let metadata = include_str!("../testdata/review/trailing_whitespace/metadata");
        let (r, _dir) = setup(review, metadata);

        r.validate_review_file(review)
            .expect("Failed to validate review file");
    }
}
