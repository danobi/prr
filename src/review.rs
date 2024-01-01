use std::fmt::{Display, Formatter, Result as fmt_result, Write as fmt_write};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, bail, Context, Result};
use serde_derive::{Deserialize, Serialize};

use crate::parser::{Comment, FileComment, InlineComment, ReviewAction, ReviewParser};

/// We support a few common variants of snips.
/// These are semantically identical.
const SNIP_VARIANTS: &[&str] = &["[..]", "[...]"];

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
    /// The commit hash of the PR at the time the review was started
    commit_id: Option<String>,
}

/// Status of a review
#[derive(PartialEq, Debug)]
pub enum ReviewStatus {
    /// Newly downloaded review; no changes yet
    New,
    /// Unsubmitted changes have been made to review file
    Reviewed,
    /// Review has been submitted. Any further changes to the review file are ignored
    Submitted,
}

/// Represents a single line in a review file.
enum LineType<'a> {
    /// Original text (but stored without the leading `> `)
    Quoted(&'a str),
    /// A snip (`[..]`)
    Snip,
    /// User supplied comment
    Comment(&'a str),
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

impl<'a> From<&'a str> for LineType<'a> {
    fn from(line: &'a str) -> Self {
        if let Some(text) = line.strip_prefix("> ") {
            Self::Quoted(text)
        } else if SNIP_VARIANTS.contains(&line.trim()) {
            Self::Snip
        } else {
            Self::Comment(line)
        }
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

/// Recursive helper for `resolve_snips()`.
///
/// This function will return Some(lines), where lines is a Vec of resolved
/// lines. There should not be any trailing newlines in `lines`.
///
/// The problem of resolving snips transposes pretty cleanly to the classic
/// glob matching algorithm. We implement the glob matching fairly naively
/// using recursion b/c it's cleaner to recurse when we want to eventually
/// return a value.
///
/// This would be in contrast to rsc's glob algorithm [0] where it's more
/// efficient and has less pathological corner cases. We choose to trade off
/// performance for simplicity here.
///
/// [0]: https://research.swtch.com/glob
fn resolve_snips_recurse<'a>(pattern: &[LineType<'a>], text: &[&'a str]) -> Option<Vec<String>> {
    let mut resolved = Vec::new();
    let mut pattern_idx = 0;
    let mut text_idx = 0;
    while pattern_idx < pattern.len() || text_idx < text.len() {
        if pattern_idx < pattern.len() {
            match pattern[pattern_idx] {
                LineType::Quoted(line) => {
                    if text_idx < text.len() && text[text_idx] == line {
                        resolved.push(format!("> {line}"));
                        pattern_idx += 1;
                        text_idx += 1;
                        continue;
                    }
                }
                // Comments are semantically irrelevant to snip resolution. But we still
                // need to account for them in returned output.
                LineType::Comment(line) => {
                    resolved.push(line.to_string());
                    pattern_idx += 1;
                    continue;
                }
                // Begin glob logic
                LineType::Snip => {
                    // Here we try making the snip consume 0 lines, 1 line, and so forth.
                    //
                    // Skipping comments is technically a noop and in theory we could rework
                    // this code to only skip matchable text. But that is just an optimization.
                    for cand_text_idx in text_idx..=text.len() {
                        let cand_pattern = &pattern[pattern_idx + 1..];
                        let cand_text = &text[cand_text_idx..];
                        if let Some(mut r) = resolve_snips_recurse(cand_pattern, cand_text) {
                            let skipped: Vec<String> = text[text_idx..cand_text_idx]
                                .iter()
                                .map(|&line| format!("> {line}"))
                                .collect();
                            resolved.extend_from_slice(&skipped);
                            resolved.append(&mut r);
                            return Some(resolved);
                        }
                    }
                }
            }
        }

        // If we reach here, we either have some `pattern` or `text` still left to
        // process. Meaning one ran out before the other. Which implies a resolution
        // failure.
        return None;
    }

    // We've finished processing all of `text` and `pattern`. So resolution success.
    Some(resolved)
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
        if !force && review.has_metadata() && review.status()? == ReviewStatus::Reviewed {
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
        let raw = fs::read_to_string(self.path()).context("Failed to read review file")?;
        let contents = self.resolve_snips(&raw)?;
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

    /// Replaces all snips (`[...]`s) from `contents` with original, quoted text.
    /// Returns resolved contents as new string.
    fn resolve_snips(&self, contents: &str) -> Result<String> {
        // First, classify contents into line types. This is henceforce
        // known as the "pattern" we want to resolve against original text.
        let pattern: Vec<LineType> = contents.lines().map(LineType::from).collect();

        // Next, store original text as lines. It's easier to index into this way.
        // The original text here is unquoted.
        let original = self.metadata()?.original;
        let text: Vec<&str> = original.lines().collect();

        Ok(resolve_snips_recurse(&pattern, &text)
            .ok_or_else(|| anyhow!("Failed to resolve snips. Did you corrupt quoted text?"))?
            .iter()
            .map(|line| format!("{line}\n"))
            .collect())
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
    fn metadata(&self) -> Result<ReviewMetadata> {
        let meta =
            fs::read_to_string(self.metadata_path()).context("Failed to load metadata file")?;
        serde_json::from_str::<ReviewMetadata>(&meta).context("Failed to parse metadata file")
    }

    fn has_metadata(&self) -> bool {
        fs::metadata(self.metadata_path()).is_ok()
    }

    fn metadata_path(&self) -> PathBuf {
        let mut metadata_path = self.path();
        metadata_path.set_file_name(format!(".{}", self.pr_num));

        metadata_path
    }

    /// Returns the commit_id associated with the review
    pub fn commit_id(&self) -> Result<Option<String>> {
        Ok(self.metadata()?.commit_id.clone())
    }

    /// Returns the original review diff
    pub fn diff(&self) -> Result<String> {
        Ok(self.metadata()?.original.clone())
    }

    /// Returns a handle (eg "owner/repo/pr_num") to this review
    pub fn handle(&self) -> String {
        format!("{}/{}/{}", self.owner, self.repo, self.pr_num)
    }

    /// Gets the status of a review
    pub fn status(&self) -> Result<ReviewStatus> {
        let metadata = self.metadata()?;
        let reviewed = self.reviewed()?;
        let status = if metadata.submitted.is_some() {
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
        if !force && self.status()? == ReviewStatus::Reviewed {
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
    use std::collections::VecDeque;
    use std::fs::{create_dir_all, File};

    use pretty_assertions::assert_eq as assert_eq_pretty;
    use tempfile::{tempdir, TempDir};

    use super::*;

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

    // Step through review status state machine and validate each state
    #[test]
    fn test_review_status() {
        let review = include_str!("../testdata/review/status/review");
        let metadata = include_str!("../testdata/review/status/metadata");
        let (r, _dir) = setup(review, metadata);

        // Using more verbose match to ensure build failure if new states added.
        // We only need this verbosity once.
        match r.status().expect("Failed to get review status") {
            ReviewStatus::New => (),
            ReviewStatus::Reviewed => panic!("Unexpected Reviewed state"),
            ReviewStatus::Submitted => panic!("Unpexected Submitted state"),
        };

        // Do a "review"
        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .open(r.path())
            .expect("Failed to open review file");
        file.write_all(b"asdf\n")
            .expect("Failed to write review comment");
        assert_eq!(r.status().unwrap(), ReviewStatus::Reviewed);

        // "Submit" the review
        r.mark_submitted().expect("Failed to submit review");
        assert_eq!(r.status().unwrap(), ReviewStatus::Submitted);
    }

    // Tests creation of a new review
    #[test]
    fn test_new_review() {
        // Create directory structure
        let workdir = tempdir().expect("Failed to create tempdir");

        // Create a review
        let review = Review::new(
            workdir.path(),
            "some_review_contents".to_string(),
            "some_owner",
            "some_repo",
            3,
            "111".to_string(),
            false,
        )
        .expect("Failed to create new non-existent review");

        // Check on disk "database"
        fs::metadata(review.path()).expect("Failed to read review file");
        fs::metadata(review.metadata_path()).expect("Failed to read review file");
    }

    #[test]
    fn test_snip_single() {
        let review = include_str!("../testdata/review/snip_single/review");
        let gold = include_str!("../testdata/review/snip_single/gold");
        let metadata = include_str!("../testdata/review/snip_single/metadata");

        let (r, _dir) = setup(review, metadata);
        assert_eq_pretty!(r.resolve_snips(review).unwrap(), gold);
    }

    #[test]
    fn test_snip_multiple() {
        let review = include_str!("../testdata/review/snip_multiple/review");
        let gold = include_str!("../testdata/review/snip_multiple/gold");
        let metadata = include_str!("../testdata/review/snip_multiple/metadata");

        let (r, _dir) = setup(review, metadata);
        assert_eq_pretty!(r.resolve_snips(review).unwrap(), gold);
    }

    #[test]
    fn test_snip_comments() {
        let review = include_str!("../testdata/review/snip_comments/review");
        let gold = include_str!("../testdata/review/snip_comments/gold");
        let metadata = include_str!("../testdata/review/snip_comments/metadata");

        let (r, _dir) = setup(review, metadata);
        assert_eq_pretty!(r.resolve_snips(review).unwrap(), gold);
    }

    // Here we exhaustively check all possible single snips. It may be worth doing something
    // similar for multiple snips but it'll be a bit more complicated to implement.
    #[test]
    fn test_snip_single_exhaustive() {
        let gold = include_str!("../testdata/review/snip_single/gold");
        let metadata = include_str!("../testdata/review/snip_single/metadata");
        let (r, _dir) = setup("", metadata);

        let nr_lines = gold.lines().count();

        for position in 0..=nr_lines {
            for length in 0..=nr_lines {
                let mut lines: VecDeque<&str> = gold.lines().collect();
                let mut contents = String::new();
                let mut idx = 0;

                while !lines.is_empty() {
                    if idx == position {
                        writeln!(&mut contents, "[...]").unwrap();
                        for _ in 0..length {
                            lines.pop_front();
                            idx += 1;
                        }
                    }

                    // A snip appended to gold file will go past "end" of lines
                    if let Some(line) = lines.pop_front() {
                        writeln!(&mut contents, "{line}").unwrap();
                    }

                    idx += 1;
                }

                // Handle 0 length trailing snip
                if idx == position {
                    writeln!(&mut contents, "[...]").unwrap();
                }

                assert_eq_pretty!(r.resolve_snips(&contents).unwrap(), gold);
            }
        }
    }
}
