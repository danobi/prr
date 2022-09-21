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

enum ProgramPath {
    OpenEditor,
    OpenWith { path: PathBuf },
}

impl From<Option<PathBuf>> for ProgramPath {
    fn from(op: Option<PathBuf>) -> Self {
        if let Some(path) = op {
            Self::OpenWith { path }
        } else {
            Self::OpenEditor
        }
    }
}

impl ProgramPath {
    /// Resolve the file path using relative/absolute path checks
    /// or fall back to resolving a binary with `which`.
    fn resolve(path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        // don't care if relative or absolute path
        if path.is_file() {
            Ok(if path.is_relative() {
                let program = std::env::current_dir()?.join(path);
                debug_assert!(
                    program.is_file(),
                    "If it was a file before, making the path absolute is also a file. qed"
                );
                program
            } else {
                path.to_owned()
            })
        } else if let Ok(program) = which::which(path) {
            Ok(program)
        } else {
            anyhow::bail!(
                "Could not find program using `which` or using the given path: `{}`",
                path.display()
            )
        }
    }

    /// Return the absolute path to the binary.
    fn path(&self) -> Result<PathBuf> {
        Ok(match self {
            Self::OpenEditor => match std::env::var("EDITOR") {
                Ok(val) if !val.is_empty() => {
                    let program = PathBuf::from(&val);
                    Self::resolve(program)?
                }
                Ok(_val) => anyhow::bail!("EDITOR env var set but empty"),
                Err(e) => {
                    log::debug!("Env var EDITOR is not set: {:?}", e);
                    let program = PathBuf::from(xdg_utils::query_default_app("application/prr")?);
                    Self::resolve(program)?
                }
            },
            Self::OpenWith { ref path } => path.to_owned(),
        })
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Get a pull request and begin a review
    Get {
        /// Ignore unsubmitted review checks
        #[clap(short, long)]
        force: bool,

        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,

        /// Open the PRR file with the default XDG application
        /// or a provided binary residing at path or that can be
        /// looked up with `which`.
        // TODO Currntly can't use the enum directly due to
        // TODOO clap_derive issue <https://github.com/clap-rs/clap/issues/2621>
        #[clap(short, long)]
        editor: Option<Option<PathBuf>>,
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

    let logger = env_logger::builder()
        .filter_level(args.verbosity.log_level_filter())
        .build();
    log::set_logger(Box::leak(Box::new(logger)))?;

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
            if let Some(editor) = editor {
                let program = ProgramPath::from(editor).path()?;
                anyhow::bail!(std::process::Command::new(program)
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
