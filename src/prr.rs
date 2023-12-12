use lazy_static::lazy_static;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use git2::{ApplyLocation, Diff, Repository, StatusOptions};
use prettytable::{format, row, Table};
use serde_derive::Deserialize;
use serde_json::{json, Value};

use crate::backend::{new_backend, Backend};
use crate::parser::{FileComment, LineLocation, ReviewAction};
use crate::review::{get_all_existing, Review};
use regex::{Captures, Regex};

// Use lazy static to ensure regex is only compiled once
lazy_static! {
    // Regex for short input. Example:
    //
    //      danobi/prr-test-repo/6
    //
    static ref SHORT: Regex = Regex::new(r"^(?P<org>[\w\-_\.]+)/(?P<repo>[\w\-_\.]+)/(?P<pr_num>\d+)").unwrap();

    // Regex for url input. Url looks something like:
    //
    //      https://github.com/danobi/prr-test-repo/pull/6
    //
    static ref URL: Regex = Regex::new(r".*github\.com/(?P<org>.+)/(?P<repo>.+)/pull/(?P<pr_num>\d+)").unwrap();
}

const GITHUB_BASE_URL: &str = "https://api.github.com";

#[derive(Debug, Deserialize)]
pub struct PrrConfig {
    /// GH personal token
    pub token: String,
    /// Directory to place review files
    pub workdir: Option<String>,
    /// Github URL
    ///
    /// Useful for enterprise instances with custom URLs
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrrLocalConfig {
    /// Default url for this current project
    pub repository: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub prr: PrrConfig,
    pub local: Option<PrrLocalConfig>,
}

/// Main struct that coordinates all business logic and talks to GH
pub struct Prr {
    /// User config
    config: Config,
    /// SCM backend implementation
    backend: Box<dyn Backend>,
}

impl Config {
    /// Returns GH URL to use. Sanitizes if necessary.
    pub fn url(&self) -> String {
        match &self.prr.url {
            Some(url) => {
                // Custom URLs must have a trailing `/`. Otherwise the custom
                // path can be truncated.
                //
                // See: https://docs.rs/reqwest/0.11.22/reqwest/struct.Url.html#method.join
                let mut sanitized = url.clone();
                if !url.ends_with('/') {
                    sanitized.push('/');
                }

                sanitized
            }
            None => GITHUB_BASE_URL.into(),
        }
    }
}

impl Prr {
    /// Create a new Prr object using the main config and/or the local config.
    /// If a local config has the `[prr]` section use this one instead of the main config.
    /// If `[prr]` section is not defined merge the local config with the main local.
    /// If local config file does not exist, use only the main config.
    ///
    /// A `[prr]` redefinition must be complete; if not, panics with a
    /// `redefinition of table `prr` for key `prr` at ...`
    pub fn new(config_path: &Path, local_config_path: Option<PathBuf>) -> Result<Prr> {
        let config_contents = fs::read_to_string(config_path).context("Failed to read config")?;
        let local_config_contents = if let Some(project_config_path) = local_config_path {
            fs::read_to_string(project_config_path).context("Failed to read local config")?
        } else {
            String::new()
        };

        let override_config = toml::from_str::<Config>(&local_config_contents);

        let config: Config = match override_config {
            // If `override_config` does not raise an error, use this one as config.
            Ok(config) => config,
            // Else merge the two config contents.
            Err(_) => {
                let contents = format!("{}\n{}", config_contents, local_config_contents);

                toml::from_str::<Config>(&contents)?
            }
        };

        let backend = new_backend(&config)?;

        Ok(Prr { config, backend })
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

    /// Parses a PR string in the form of `danobi/prr/24` and returns
    /// a tuple ("danobi", "prr", 24) or an error if string is malformed.
    /// If the local repository config is defined, it just needs the PR number.
    pub fn parse_pr_str(&self, s: &str) -> Result<(String, String, u64)> {
        let f = |captures: Captures| -> Result<(String, String, u64)> {
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

        let repo = if let Some(local_config) = &self.config.local {
            if let Some(url) = &local_config.repository {
                if url.ends_with('/') {
                    format!("{}{}", url, s)
                } else {
                    format!("{}/{}", url, s)
                }
            } else {
                s.to_string()
            }
        } else {
            s.to_string()
        };

        if let Some(captures) = SHORT.captures(&repo) {
            f(captures)
        } else if let Some(captures) = URL.captures(&repo) {
            f(captures)
        } else {
            bail!("Invalid PR ref format")
        }
    }

    pub async fn get_pr(
        &self,
        owner: &str,
        repo: &str,
        pr_num: u64,
        force: bool,
    ) -> Result<Review> {
        let pr_info = self.backend.get_pr_info(owner, repo, pr_num).await?;

        Review::new(
            &self.workdir()?,
            pr_info.diff,
            owner,
            repo,
            pr_num,
            pr_info.commit,
            force,
        )
    }

    pub async fn submit_pr(&self, owner: &str, repo: &str, pr_num: u64, debug: bool) -> Result<()> {
        let review = Review::new_existing(&self.workdir()?, owner, repo, pr_num);
        let (review_action, review_comment, inline_comments, file_comments) = review.comments()?;
        let metadata = review.get_metadata()?;

        if review_comment.is_empty()
            && inline_comments.is_empty()
            && review_action != ReviewAction::Approve
        {
            bail!("No review comments");
        }

        let mut body = json!({
            "body": review_comment,
            "event": match review_action {
                ReviewAction::Approve => "APPROVE",
                ReviewAction::RequestChanges => "REQUEST_CHANGES",
                ReviewAction::Comment => "COMMENT"
            },
            "comments": inline_comments
                .iter()
                .map(|c| {
                    let (line, side) = match c.line {
                        LineLocation::Left(line) => (line, "LEFT"),
                        LineLocation::Right(line) => (line, "RIGHT"),
                    };

                    let mut json_comment = json!({
                        "path": c.file,
                        "line": line,
                        "body": c.comment,
                        "side": side,
                    });
                    if let Some(start_line) = &c.start_line {
                        let (line, side) = match start_line {
                            LineLocation::Left(line) => (line, "LEFT"),
                            LineLocation::Right(line) => (line, "RIGHT"),
                        };

                        json_comment["start_line"] = (*line).into();
                        json_comment["start_side"] = side.into();
                    }

                    json_comment
                })
                .collect::<Vec<Value>>(),
        });
        if let Some(id) = metadata.commit_id() {
            if let serde_json::Value::Object(ref mut obj) = body {
                obj.insert("commit_id".to_string(), json!(id));
            }
        } else if !file_comments.is_empty() {
            bail!(
                "Metadata contained no commit_id, but it's required to leave file-level comments"
            );
        }

        if debug {
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        self.submit_review(&review, owner, repo, pr_num, &body)
            .await?;

        for fc in &file_comments {
            self.submit_file_comment(owner, repo, pr_num, metadata.commit_id().unwrap(), fc)
                .await?
        }

        Ok(())
    }

    async fn submit_review(
        &self,
        review: &Review,
        owner: &str,
        repo: &str,
        pr_num: u64,
        body: &Value,
    ) -> Result<()> {
        self.backend
            .submit_review(owner, repo, pr_num, body)
            .await?;
        review
            .mark_submitted()
            .context("Failed to update review metadata")
    }

    async fn submit_file_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_num: u64,
        commit_id: &str,
        fc: &FileComment,
    ) -> Result<()> {
        self.backend
            .submit_file_comment(owner, repo, pr_num, commit_id, fc)
            .await
    }

    pub fn apply_pr(&self, owner: &str, repo: &str, pr_num: u64) -> Result<()> {
        let review = Review::new_existing(&self.workdir()?, owner, repo, pr_num);
        let metadata = review
            .get_metadata()
            .context("Failed to get review metadata")?;
        let raw = metadata.original();
        let diff = Diff::from_buffer(raw.as_bytes()).context("Failed to load original diff")?;
        let repo = Repository::open_from_env().context("Failed to open git repository")?;

        // Best effort check to prevent clobbering any work in progress
        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true);
        let statuses = repo
            .statuses(Some(&mut status_opts))
            .context("Failed to get repo status")?;
        if !statuses.is_empty() {
            bail!("Working directory is dirty");
        }

        repo.apply(&diff, ApplyLocation::WorkDir, None)
            .context("Failed to apply diff")
    }

    pub fn print_status(&self, no_titles: bool) -> Result<()> {
        let mut table = Table::new();
        let mut table_fmt = *format::consts::FORMAT_CLEAN;
        // Get rid of leading padding on each line
        table_fmt.padding(0, 2);
        table.set_format(table_fmt);
        if !no_titles {
            table.set_titles(row!["Handle", "Status", "Review file"])
        }

        let reviews =
            get_all_existing(&self.workdir()?).context("Failed to get existing reviews")?;

        for review in reviews {
            let metadata = review.get_metadata()?;
            let reviewed = {
                let (_, review_comment, comments, file_comments) =
                    review.comments().with_context(|| {
                        format!("Failed to parse comments for {}", review.path().display())
                    })?;

                !review_comment.is_empty() || !comments.is_empty() || !file_comments.is_empty()
            };
            let status = if metadata.submitted().is_some() {
                "SUBMITTED"
            } else if reviewed {
                "REVIEWED"
            } else {
                "NEW"
            };

            table.add_row(row![review.handle(), status, review.path().display()]);
        }

        table.printstd();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Borrow;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn new_prr() -> Prr {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().join("config.toml");
        let mut file = File::create(path.clone()).unwrap();
        write!(&mut file, "[prr]\ntoken = \"test\"\nworkdir = \"/tmp\"").unwrap();
        Prr::new(path.borrow(), None).unwrap()
    }

    #[test]
    fn test_parse_basic_pr_str() {
        let prr = new_prr();
        let pr_ref = "example/prr/42";
        assert_eq!(
            prr.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_dotted_pr_str() {
        let prr = new_prr();
        let pr_ref = "example/prr.test/42";
        assert_eq!(
            prr.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr.test".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_underscored_pr_str() {
        let prr = new_prr();
        let pr_ref = "example/prr_test/42";
        assert_eq!(
            prr.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr_test".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_dashed_pr_str() {
        let prr = new_prr();
        let pr_ref = "example/prr-test/42";
        assert_eq!(
            prr.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr-test".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_numbered_pr_str() {
        let prr = new_prr();
        let pr_ref = "example/prr1/42";
        assert_eq!(
            prr.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr1".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_mixed_pr_str() {
        let prr = new_prr();
        let pr_ref = "example/prr1.test_test-/42";
        assert_eq!(
            prr.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr1.test_test-".to_string(), 42)
        )
    }
}
