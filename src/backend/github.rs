use crate::backend::{Backend, ReviewInfo};
use crate::parser::FileComment;
use crate::prr::Config;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use octocrab::Octocrab;
use reqwest::StatusCode;
use serde_json::{json, Value};

pub struct Github {
    crab: Octocrab,
}

impl Github {
    pub fn new(config: &Config) -> Result<Self> {
        let crab = Octocrab::builder()
            .personal_token(config.prr.token.clone())
            .base_url(config.url())
            .context("Failed to parse github base URL")?
            .build()
            .context("Failed to create GH client")?;

        Ok(Github { crab })
    }
}

#[async_trait]
impl Backend for Github {
    async fn get_pr_info(&self, owner: &str, repo: &str, pr_num: u64) -> Result<ReviewInfo> {
        let pr_handler = self.crab.pulls(owner, repo);

        let diff = pr_handler
            .get_diff(pr_num)
            .await
            .context("Failed to fetch diff")?;

        let commit = pr_handler
            .get(pr_num)
            .await
            .context("Failed to fetch commit ID")?
            .head
            .sha;

        Ok(ReviewInfo { diff, commit })
    }

    async fn submit_review(
        &self,
        owner: &str,
        repo: &str,
        pr_num: u64,
        body: &Value,
    ) -> Result<()> {
        let path = format!("repos/{}/{}/pulls/{}/reviews", owner, repo, pr_num);
        match self
            .crab
            ._post(self.crab.absolute_url(path)?, Some(body))
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

                Ok(())
            }
            // GH is known to send unescaped control characters in JSON responses which
            // serde will fail to parse (not that it should succeed)
            Err(octocrab::Error::Json {
                source: _,
                backtrace: _,
            }) => bail!("Warning: GH response had invalid JSON"),
            Err(e) => bail!("Error during POST: {}", e),
        }
    }

    async fn submit_file_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_num: u64,
        commit_id: &str,
        fc: &FileComment,
    ) -> Result<()> {
        let body = json!({
            "body": fc.comment,
            "commit_id": commit_id,
            "path": fc.file,
            "subject_type": "file",
        });
        let path = format!("repos/{}/{}/pulls/{}/comments", owner, repo, pr_num);
        match self
            .crab
            ._post(self.crab.absolute_url(path)?, Some(&body))
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status != StatusCode::CREATED {
                    let text = resp
                        .text()
                        .await
                        .context("Failed to decode failed response")?;
                    bail!("Error during POST: Status code: {}, Body: {}", status, text);
                }
                Ok(())
            }
            // GH is known to send unescaped control characters in JSON responses which
            // serde will fail to parse (not that it should succeed)
            Err(octocrab::Error::Json {
                source: _,
                backtrace: _,
            }) => bail!("Warning: GH response had invalid JSON"),
            Err(e) => bail!("Error during POST: {}", e),
        }
    }
}
