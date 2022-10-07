use std::{
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use fs_err as fs;
use lazy_static::lazy_static;
use regex::{Captures, Regex};

mod parser;
mod prr;
mod review;

use prr::Config;
use prr::Prr;

// Use lazy static to ensure regex is only compiled once
lazy_static! {
    // Regex for short input. Example:
    //
    //      danobi/prr-test-repo/6
    //      danobi/prr-test-repo/pull/6
    //
    static ref SHORT: Regex = Regex::new(r"^(?P<org>[\w\-_]+)/(?P<repo>[\w\-_]+)/(?:pull/)?(?P<pr_num>\d+)").unwrap();

    // Regex for url input. Url looks something like:
    //
    //      https://github.com/danobi/prr-test-repo/pull/6
    //
    static ref URL: Regex = Regex::new(r".*github\.com/(?P<org>.+)/(?P<repo>.+)/pull/(?P<pr_num>\d+)").unwrap();
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]

struct OpenEditor(PathBuf);

impl OpenEditor {
    fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    pub fn new(config: &Config) -> Result<Self> {
        let program = if let Some(ref program) = config.editor {
            log::debug!(
                "Using editor value from configuration: {}",
                program.display()
            );
            program.clone()
        } else {
            match std::env::var("EDITOR") {
                Ok(val) if !val.is_empty() => {
                    let program = PathBuf::from(&val);
                    log::debug!("Using env var EDITOR: {}", program.display());
                    program
                }
                Ok(_val) => anyhow::bail!("EDITOR env var set but empty"),
                Err(err) => {
                    log::debug!("Env var EDITOR is not set: {:?}", err);
                    if let Some(editor) = config.editor.as_ref() {
                        editor.to_owned()
                    } else {
                        anyhow::bail!("Neither env EDITOR is set, nor the `editor=` key in the config file is populated")
                    }
                }
            }
        };
        let abs_path = Self::resolve(program)?;
        Ok(abs_path)
    }

    /// Resolve the file path or program name to its absolute path.
    ///
    /// Resolution is done by priority:
    /// * Check if local file based on dir and executable
    /// * Use `which::which` to resolve parentless paths.
    fn resolve(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        log::trace!("Original program path or name is: {}", path.display());

        // don't care if relative or absolute path
        let is_basename_only = dbg!(path.parent()).is_none() && !path.is_absolute();

        let resolved = if is_basename_only {
            log::trace!(
                "Trying to resolve via PATH env variable using `which {}`",
                path.display()
            );
            which::which(path)?
        } else {
            let abs = if path.is_relative() {
                log::trace!(
                    "Concatentating current dir with {} to get abs path",
                    path.display()
                );
                std::env::current_dir()?.join(path)
            } else {
                path.to_owned()
            };
            abs
        };
        log::trace!("Original program resolved to: {}", resolved.display());
        Ok(OpenEditor(resolved))
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Get a pull request and begin a review
    Get {
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,

        /// Ignore unsubmitted review checks
        #[clap(short, long)]
        force: bool,

        /// Open the editor instantly.
        ///
        /// Uses either the provided editor or falls back to the
        /// environment variable `EDITOR`.
        #[clap(short, long)]
        editor: bool,
    },
    /// Submit a review
    Submit {
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
    },
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to config file
    #[clap(long, parse(from_os_str))]
    config: Option<PathBuf>,

    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    #[clap(subcommand)]
    command: Command,
}

/// Parses a PR string in the form of `danobi/prr/24` and returns
/// a tuple ("danobi", "prr", 24) or an error if string is malformed
fn parse_pr_str<'a>(s: &'a str) -> Result<(String, String, u64)> {
    let f = |captures: Captures<'a>| -> Result<(String, String, u64)> {
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

    if let Some(captures) = SHORT.captures(s) {
        f(captures)
    } else if let Some(captures) = URL.captures(s) {
        f(captures)
    } else {
        bail!("Invalid PR ref format")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    env_logger::Builder::from_env(env_logger::Env::new().filter_or("PRR", "warn"))
        .filter_level(args.verbosity.log_level_filter())
        .init();

    // Figure out where config file is
    let config_path = match args.config {
        Some(c) => c,
        None => {
            let xdg_dirs = xdg::BaseDirectories::with_prefix("prr")?;
            xdg_dirs.get_config_file("config.toml")
        }
    };

    log::debug!("Loading config from {}", config_path.display());

    let prr = Prr::new(&config_path)?;

    match args.command {
        Command::Get { pr, force, editor } => {
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            let review = prr.get_pr(&owner, &repo, pr_num, force).await?;
            log::info!("{}", review.path().display());
            if editor {
                let oe = OpenEditor::new(&prr.config)?;
                anyhow::bail!(std::process::Command::new(oe.as_path())
                    .args(&[review.path()])
                    .exec());
            }
        }
        Command::Submit { pr } => {
            let (owner, repo, pr_num) = parse_pr_str(&pr)?;
            prr.submit_pr(&owner, &repo, pr_num).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_get_force_w_editor() {
        let _args =
            Args::try_parse_from("prr -vvv --config baz.toml get -f -e foo/bar/123".split(' '))
                .unwrap();
        dbg!(_args);
    }
}
