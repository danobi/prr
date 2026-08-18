# prr-suggest

This is a fork of [danobi/prr](https://github.com/danobi/prr) that submits
**code suggestions only**. On `prr submit` it posts every ```suggestion block
found in the review file and drops everything else: the review-level summary
comment, file-level comments, and any prose written around a suggestion block.
The review action (approve / request changes / comment) still applies.

## Install

```
brew tap tineoc/prr https://github.com/TineoC/prr
brew trust tineoc/prr
brew install --HEAD prr-suggest
```

The binary installs as `prr-suggest` so it can coexist with upstream `prr`.
From source: `just install` (or `cargo install --path .`).

---

# Pull request review

[![Rust](https://github.com/danobi/prr/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/danobi/prr/actions/workflows/rust.yml)

`prr` is a tool that brings mailing list style code reviews to Github PRs.
This means offline reviews and inline comments, more or less.

To that end, `prr` introduces a new workflow for reviewing PRs:

1. Download the PR into a "review file" on your filesystem
1. Mark up the review file using your favorite text editor
1. Submit the review at your convenience

The tool was born of frustration from using the point-and-click editor text
boxes on PRs. I happen to do a lot of code review and tabbing to and from the
browser to cross reference code from the changes was driving me nuts.

For full documentation, please visit https://doc.dxuuu.xyz/prr/.
