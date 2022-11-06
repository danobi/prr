use lazy_static::lazy_static;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use git2::{ApplyLocation, Diff, Repository, StatusOptions};
use octocrab::Octocrab;
use prettytable::{format, row, Table};
use reqwest::StatusCode;
use serde_derive::Deserialize;
use serde_json::{json, Value};

use crate::parser::{LineLocation, ReviewAction};
use crate::review::{get_all_existing, Review};
use regex::{Captures, Regex};

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

const GITHUB_BASE_URL: &str = "https://api.github.com";

#[derive(Debug, Deserialize)]
struct PrrConfig {
    /// GH personal token
    token: String,
    /// Directory to place review files
    workdir: Option<String>,
    /// Github URL
    ///
    /// Useful for enterprise instances with custom URLs
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrrLocalConfig {
    /// Default url for this current project
    repository: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Config {
    prr: PrrConfig,
    local: Option<PrrLocalConfig>,
}

/// Main struct that coordinates all business logic and talks to GH
pub struct Prr {
    /// User config
    config: Config,
    /// Instantiated github client
    crab: Octocrab,
}

impl Prr {
    /// Create a new Prr object using the main config and/or the local config.
    /// If a local config has the `[prr]` section use this one instead of the main config.
    /// If `[prr]` section is not defined merge the local config with the main local.
    /// If local config file does not exist, use only the main config.
    ///
    /// A `[prr]` redefition must be complete; if not, panics with a
    /// `redefinition of table `prr` for key `prr` at ...`
    pub fn new(config_path: &Path, local_config_path: Option<PathBuf>) -> Result<Prr> {
        let config_contents = fs::read_to_string(config_path).context("Failed to read config")?;
        let local_config_contents = if let Some(project_config_path) = local_config_path {
            let content =
                fs::read_to_string(project_config_path).context("Failed to read local config")?;

            content
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

        let octocrab = Octocrab::builder()
            .personal_token(config.prr.token.clone())
            .base_url(config.prr.url.as_deref().unwrap_or(GITHUB_BASE_URL))
            .context("Failed to parse github base URL")?
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
        let pr_handler = self.crab.pulls(owner, repo);

        let diff = pr_handler
            .get_diff(pr_num)
            .await
            .context("Failed to fetch diff")?;

        let commit_id = pr_handler
            .get(pr_num)
            .await
            .context("Failed to fetch commit ID")?
            .head
            .sha;

        Review::new(
            &self.workdir()?,
            diff,
            owner,
            repo,
            pr_num,
            commit_id,
            force,
        )
    }

    pub async fn submit_pr(&self, owner: &str, repo: &str, pr_num: u64, debug: bool) -> Result<()> {
        let review = Review::new_existing(&self.workdir()?, owner, repo, pr_num);
        let (review_action, review_comment, inline_comments) = review.comments()?;
        let metadata = review.get_metadata()?;

        if review_comment.is_empty() && inline_comments.is_empty() {
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
        }

        if debug {
            println!("{}", serde_json::to_string_pretty(&body)?);
        }

        let path = format!("/repos/{}/{}/pulls/{}/reviews", owner, repo, pr_num);
        match self
            .crab
            ._post(self.crab.absolute_url(path)?, Some(&body))
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status != StatusCode::OK {
                    let text = resp
                        .text()
                        .await
                        .context("Failed to decode failed response")?;
                    bail!("Error during POST: Status code: {}, Body: {}", status, text);
                }

                review
                    .mark_submitted()
                    .context("Failed to update review metadata")?;

                Ok(())
            }
            // GH is known to send unescaped control characters in JSON responses which
            // serde will fail to parse (not that it should succeed)
            Err(octocrab::Error::Json {
                source: _,
                backtrace: _,
            }) => {
                eprintln!("Warning: GH response had invalid JSON");
                Ok(())
            }
            Err(e) => bail!("Error during POST: {}", e),
        }
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
                let (_, review_comment, comments) = review.comments().with_context(|| {
                    format!("Failed to parse comments for {}", review.path().display())
                })?;

                !review_comment.is_empty() || !comments.is_empty()
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
