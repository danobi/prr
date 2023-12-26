use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use git2::{ApplyLocation, Diff, Repository};
use lazy_static::lazy_static;
use octocrab::Octocrab;
use prettytable::{format, row, Table};
use reqwest::StatusCode;
use serde_derive::Deserialize;
use serde_json::{json, Value};

use crate::parser::{FileComment, LineLocation, ReviewAction};
use crate::review::{get_all_existing, Review, ReviewStatus};
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
    /// Local workdir override
    workdir: Option<String>,
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
    /// Path to local config file
    local_config: Option<PathBuf>,
    /// Instantiated github client
    crab: Octocrab,
}

impl Config {
    /// Returns GH URL to use. Sanitizes if necessary.
    fn url(&self) -> String {
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
        let local_config_contents = if let Some(project_config_path) = &local_config_path {
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

        let octocrab = Octocrab::builder()
            .personal_token(config.prr.token.clone())
            .base_url(config.url())
            .context("Failed to parse github base URL")?
            .build()
            .context("Failed to create GH client")?;

        Ok(Prr {
            config,
            local_config: local_config_path,
            crab: octocrab,
        })
    }

    /// Returns path to prr workdir
    fn workdir(&self) -> Result<PathBuf> {
        // Try local config first
        if let Some(lcfg) = &self.config.local {
            // Can't have a parsed local config without a stored path
            debug_assert!(self.local_config.is_some());

            if let Some(wd) = &lcfg.workdir {
                if wd.starts_with('~') {
                    bail!("Invalid workdir={wd}: may not use '~'");
                }

                // We allow resolving relative paths in local config relative to the local config file
                let mut resolved_wd = PathBuf::new();
                // No parent seems impossible but I think it's correct to not push anything
                if let Some(local_dir) = self.local_config.as_ref().unwrap().parent() {
                    resolved_wd.push(local_dir);
                }
                // NB: pushing an absolute path overwrites the PathBuf
                resolved_wd.push(wd);

                return Ok(resolved_wd);
            }
        }

        // Now try global config
        if let Some(wd) = &self.config.prr.workdir {
            if wd.starts_with('~') {
                bail!("Invalid workdir={wd}: may not use '~'");
            }

            let p = Path::new(wd).to_path_buf();
            if !p.is_absolute() {
                bail!("Invalid workdir={wd}: must be absolute path");
            }

            return Ok(p);
        }

        // Default workdir
        let xdg_dirs = xdg::BaseDirectories::with_prefix("prr")?;
        Ok(xdg_dirs.get_data_home())
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

    /// Gets a new review from the internet and writes it to the filesystem
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

        let pr = pr_handler.get(pr_num).await.context("Failed to fetch pr")?;
        let commit_id = pr.head.sha;

        let pr_description = pr.body;

        Review::new(
            &self.workdir()?,
            diff,
            owner,
            repo,
            pr_description,
            pr_num,
            commit_id,
            force,
        )
    }

    /// Gets an existing review from the filesystem
    pub fn get_review(&self, owner: &str, repo: &str, pr_num: u64) -> Result<Review> {
        let workdir = self.workdir()?;
        Ok(Review::new_existing(&workdir, owner, repo, pr_num))
    }

    pub async fn submit_pr(&self, owner: &str, repo: &str, pr_num: u64, debug: bool) -> Result<()> {
        let review = Review::new_existing(&self.workdir()?, owner, repo, pr_num);
        let (review_action, review_comment, inline_comments, file_comments) = review.comments()?;

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

        let commit = review.commit_id()?;
        if let Some(id) = &commit {
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
            self.submit_file_comment(owner, repo, pr_num, commit.as_ref().unwrap(), fc)
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
            }) => {
                eprintln!("Warning: GH response had invalid JSON");
                Ok(())
            }
            Err(e) => bail!("Error during POST: {}", e),
        }
    }

    pub fn apply_pr(&self, owner: &str, repo: &str, pr_num: u64) -> Result<()> {
        let review = Review::new_existing(&self.workdir()?, owner, repo, pr_num);
        let diff = Diff::from_buffer(review.diff()?.as_bytes()).context("Failed to load diff")?;
        let repo = Repository::open_from_env().context("Failed to open git repository")?;

        // Best effort check to prevent clobbering any work in progress
        let statuses = repo.statuses(None).context("Failed to get repo status")?;
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

        let reviews = get_all_existing(&self.workdir()?).context("Failed to get all reviews")?;
        for review in reviews {
            table.add_row(row![
                review.handle(),
                review.status()?,
                review.path().display()
            ]);
        }

        table.printstd();

        Ok(())
    }

    /// Removes reviews from the filesystem
    pub async fn remove(&self, prs: &[String], force: bool, submitted: bool) -> Result<()> {
        for pr in prs {
            let (owner, repo, pr_num) = self.parse_pr_str(pr)?;
            let review = self.get_pr(&owner, &repo, pr_num, force).await?;
            review
                .remove(force)
                .with_context(|| anyhow!("Failed to remove {}", pr))?;
        }

        if !submitted {
            return Ok(());
        }

        let reviews = get_all_existing(&self.workdir()?).context("Failed to all reviews")?;
        for review in reviews {
            if review.status()? == ReviewStatus::Submitted {
                let handle = review.handle();
                review
                    .remove(force)
                    .with_context(|| anyhow!("Failed to remove {}", handle))?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    // Lays down configs in a tempdir
    //
    // NB: Configs get deleted if returned `TempDir` is dropped
    fn config(global: &str, local: Option<&str>) -> (Prr, TempDir) {
        let dir = TempDir::new().unwrap();
        let gpath = dir.path().join("config.toml");
        let mut gfile = File::create(&gpath).unwrap();
        write!(&mut gfile, "{}", global).unwrap();

        let lpath = if let Some(lcontents) = local {
            let lpath = dir.path().join("local_config.toml");
            let mut lfile = File::create(&lpath).unwrap();
            write!(&mut lfile, "{}", lcontents).unwrap();
            Some(lpath)
        } else {
            None
        };

        let prr = Prr::new(&gpath, lpath).unwrap();
        (prr, dir)
    }

    lazy_static! {
        // Basic dummy config just to avoid errors
        static ref PRR: (Prr, TempDir) = {
            let gconfig = r#"
                [prr]
                token = "test"
                workdir = "/tmp"
            "#;

            config(gconfig, None)
        };
    }

    #[test]
    fn test_parse_basic_pr_str() {
        let pr_ref = "example/prr/42";
        assert_eq!(
            PRR.0.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_dotted_pr_str() {
        let pr_ref = "example/prr.test/42";
        assert_eq!(
            PRR.0.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr.test".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_underscored_pr_str() {
        let pr_ref = "example/prr_test/42";
        assert_eq!(
            PRR.0.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr_test".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_dashed_pr_str() {
        let pr_ref = "example/prr-test/42";
        assert_eq!(
            PRR.0.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr-test".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_numbered_pr_str() {
        let pr_ref = "example/prr1/42";
        assert_eq!(
            PRR.0.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr1".to_string(), 42)
        )
    }

    #[test]
    fn test_parse_mixed_pr_str() {
        let pr_ref = "example/prr1.test_test-/42";
        assert_eq!(
            PRR.0.parse_pr_str(pr_ref).unwrap(),
            ("example".to_string(), "prr1.test_test-".to_string(), 42)
        )
    }

    #[test]
    fn test_local_config_repository() {
        let gconfig = r#"
            [prr]
            token = "test"
        "#;
        let lconfig = r#"
            [local]
            repository = "testorg/testrepo"
        "#;

        let (prr, _dir) = config(gconfig, Some(lconfig));
        assert_eq!(
            prr.parse_pr_str("42").unwrap(),
            ("testorg".to_string(), "testrepo".to_string(), 42)
        )
    }

    #[test]
    fn test_global_workdir() {
        let gconfig = r#"
            [prr]
            token = "test"
            workdir = "/globalworkdir"
        "#;

        let (prr, _dir) = config(gconfig, None);
        assert_eq!(prr.workdir().unwrap(), Path::new("/globalworkdir"))
    }

    #[test]
    fn test_local_workdir() {
        let gconfig = r#"
            [prr]
            token = "test"
        "#;
        let lconfig = r#"
            [local]
            workdir = "/localworkdir"
        "#;

        let (prr, _dir) = config(gconfig, Some(lconfig));
        assert_eq!(prr.workdir().unwrap(), Path::new("/localworkdir"))
    }

    #[test]
    fn test_local_workdir_relative() {
        let gconfig = r#"
            [prr]
            token = "test"
        "#;
        let lconfig = r#"
            [local]
            workdir = "localrelativeworkdir"
        "#;

        let (prr, dir) = config(gconfig, Some(lconfig));
        assert_eq!(
            prr.workdir().unwrap(),
            dir.path().join("localrelativeworkdir")
        )
    }

    #[test]
    fn test_local_workdir_override() {
        let gconfig = r#"
            [prr]
            token = "test"
            workdir = "/globalworkdir"
        "#;
        let lconfig = r#"
            [local]
            workdir = "/localworkdir"
        "#;

        let (prr, _dir) = config(gconfig, Some(lconfig));
        assert_eq!(prr.workdir().unwrap(), Path::new("/localworkdir"))
    }

    #[test]
    fn test_invalid_relative_workdir() {
        let gconfig = r#"
            [prr]
            token = "test"
            workdir = "relativeworkdir"
        "#;

        let (prr, _dir) = config(gconfig, None);
        assert!(prr.workdir().is_err());
    }
}
