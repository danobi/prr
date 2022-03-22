use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use octocrab::Octocrab;
use serde_derive::Deserialize;
use serde_json::{json, Value};

use crate::review::Review;

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

    pub async fn get_pr(&self, owner: &str, repo: &str, pr_num: u64) -> Result<Review> {
        let diff = self
            .crab
            .pulls(owner, repo)
            .get_diff(pr_num)
            .await
            .context("Failed to fetch diff")?;

        Review::new(&self.workdir()?, diff, owner, repo, pr_num)
    }

    pub async fn submit_pr(&self, owner: &str, repo: &str, pr_num: u64) -> Result<()> {
        let review = Review::new_existing(&self.workdir()?, owner, repo, pr_num);
        let comments = review.comments()?;

        let body = json!({
            "body": "",
            "event": "COMMENT",
            "comments": comments
                .iter()
                .map(|c| {
                    let mut json_comment = json!({
                        "path": c.file,
                        "line": c.position,
                        "body": c.comment,
                    });
                    if let Some(start_position) = c.start_position {
                        json_comment["start_line"] = start_position.into();
                    }

                    json_comment
                })
                .collect::<Vec<Value>>(),
        });

        let path = format!("/repos/{}/{}/pulls/{}/reviews", owner, repo, pr_num);
        match self
            .crab
            ._post(self.crab.absolute_url(path)?, Some(&body))
            .await
        {
            Ok(_) => Ok(()),
            // GH is known to send unescaped control characters in JSON responses which
            // serde will fail to parse (not that it should succeed)
            Err(octocrab::Error::Json {
                source: _,
                backtrace: _,
            }) => Ok(()),
            Err(e) => bail!("Error during POST: {}", e),
        }
    }
}
