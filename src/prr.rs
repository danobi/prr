use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use octocrab::Octocrab;
use reqwest::StatusCode;
use serde_derive::Deserialize;
use serde_json::{json, Value};

use crate::parser::{LineLocation, ReviewAction};
use crate::review::Review;

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
struct Config {
    prr: PrrConfig,
}

/// Main struct that coordinates all business logic and talks to GH
pub struct Prr {
    /// User config
    config: Config,
    /// Instantiated github client
    crab: Octocrab,
}

impl Prr {
    pub fn new(config_path: &Path) -> Result<Prr> {
        let config_contents = fs::read_to_string(config_path).context("Failed to read config")?;
        let config: Config = toml::from_str(&config_contents).context("Failed to parse toml")?;
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
}
