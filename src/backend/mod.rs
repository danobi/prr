use crate::parser::FileComment;
use crate::prr::Config;
use anyhow::Result;
use async_trait::async_trait;
use github::Github;
use serde_json::Value;

mod github;

pub struct ReviewInfo {
    pub diff: String,
    pub commit: String,
}

#[async_trait]
pub trait Backend {
    async fn get_pr_info(&self, owner: &str, repo: &str, pr_num: u64) -> Result<ReviewInfo>;
    async fn submit_review(&self, owner: &str, repo: &str, pr_num: u64, body: &Value)
        -> Result<()>;

    async fn submit_file_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_num: u64,
        commit_id: &str,
        fc: &FileComment,
    ) -> Result<()>;
}

pub fn new_backend(config: &Config) -> Result<Box<dyn Backend>> {
    Ok(Box::new(Github::new(config)?))
}
