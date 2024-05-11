mod cli {
    include!("src/cli.rs");
}

const LONG_ABOUT: &str =
    "prr is a tool that brings mailing list style code reviews to Github PRs. This \
means offline reviews and inline comments, more or less.

To that end, prr introduces a new workflow for reviewing PRs:
  1. Download the PR into a \"review file\" on your filesystem
  2. Mark up the review file using your favorite text editor
  3. Submit the review at your convenience

For full documentation, please visit https://doc.dxuuu.xyz/prr/.";

fn main() -> std::io::Result<()> {
    if let Some(out_path) = std::env::var_os("GEN_DIR").or(std::env::var_os("OUT_DIR")) {
        use clap::CommandFactory;
        #[allow(unused_variables)]
        let out_dir = std::path::PathBuf::from(out_path);
        #[allow(unused_mut, unused_variables)]
        let mut cmd = cli::Cli::command()
            .author("Daniel Xu <dxu@apache.org>")
            .about("Mailing list style code reviews for GitHub")
            .long_about(LONG_ABOUT);

        #[cfg(feature = "clap_mangen")]
        {
            let man_dir = std::path::Path::join(&out_dir, "man");
            std::fs::create_dir_all(&man_dir)?;
            clap_mangen::generate_to(cmd.clone(), &man_dir)?;
        }

        #[cfg(feature = "clap_complete")]
        {
            use clap::ValueEnum;
            let completions_dir = std::path::Path::join(&out_dir, "completions");
            std::fs::create_dir_all(&completions_dir)?;
            for shell in clap_complete::Shell::value_variants() {
                clap_complete::generate_to(*shell, &mut cmd, "prr", &completions_dir)?;
            }
        }
    }

    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    println!("cargo:rerun-if-env-changed=GEN_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_CLAP_MANGEN");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_CLAP_COMPLETE");

    Ok(())
}
