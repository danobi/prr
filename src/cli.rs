use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Get a pull request and begin a review
    Get {
        /// Ignore unsubmitted review checks
        #[clap(short, long)]
        force: bool,
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
        /// Open review file in $EDITOR after download
        #[clap(long)]
        open: bool,
    },
    /// Open an existing review in $EDITOR
    Edit {
        /// Pull request to edit (eg. `danobi/prr/24`)
        pr: String,
    },
    /// Submit a review
    Submit {
        /// Pull request to review (eg. `danobi/prr/24`)
        pr: String,
        #[clap(short, long)]
        debug: bool,
    },
    /// Apply a pull request to the working directory
    ///
    /// This can be useful for building/testing PRs
    Apply { pr: String },
    /// Print a status summary of all known reviews
    Status {
        /// Hide column titles from output
        #[clap(short, long)]
        no_titles: bool,
    },
    /// Remove a review
    Remove {
        /// Pull requests to remove (eg. `danobi/prr/24`)
        prs: Vec<String>,
        /// Ignore unsubmitted review checks
        #[clap(short, long)]
        force: bool,
        /// Remove submitted reviews in addition to provided reviews
        #[clap(short, long)]
        submitted: bool,
    },
}

#[derive(Parser, Debug)]
#[clap(version)]
#[command(name = "prr")]
pub struct Cli {
    /// Path to config file
    #[clap(long)]
    pub(crate) config: Option<PathBuf>,
    #[clap(subcommand)]
    pub(crate) command: Command,
}
