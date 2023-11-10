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

### Installation / Quickstart

1. Install `prr`:

    - **Option 1:** Install rust toolchain (if you haven't already): https://rustup.rs/

        ```sh
        $ cargo install prr
        ```

    - **Option 2:** [Homebrew](https://brew.sh/)

        ```sh
        $ brew install prr
        ```

2. Create a Github Personal Access Token (PAT) for `prr`:

    `prr` will need this token so it can make GH API calls on your behalf.
    Create the token by going to `Settings -> Developer settings -> Personal
    access tokens -> Generate new token` and give the token `repo` permissions.

    Keep the newly generated token handy for the next step.

3. Create a `prr` config file:

    ```sh
    $ mkdir -p ~/.config/prr

    $ cat << EOF > ~/.config/prr/config.toml
    [prr]
    token = "$YOUR_PAT_FROM_LAST_STEP"
    workdir = "/home/dxu/dev/review"
    EOF
    ```

    where `$YOUR_PAT_FROM_LAST_STEP` is the PAT token from step 3 and `workdir`
    is the directory you want `prr` to place all your review files.

4. Review your first PR:

    Feel free to test `prr` out against my test repository, by the way.

    ```sh
    $ prr get danobi/prr-test-repo/6
    /home/dxu/dev/review/danobi/prr-test-repo/6.prr

    $ vim /home/dxu/dev/review/danobi/prr-test-repo/6.prr

    $ prr submit danobi/prr-test-repo/6
    ```

    For details on how to actually mark up the review file, see the next
    section titled "Features"

### Features

#### Review comment

Description: PR-level review comment. You only get one of these per review.

Syntax: Non-whitespace, non-quoted text at the beginning of the review file.

[Example](examples/review_comment.prr)

#### Inline comment

Description: Inline comment attached to a specific line of the diff.

Syntax: None-whitespace, non-quoted text on a newline immediately following
a quoted non-header part of the diff.

[Example](examples/inline_comment.prr)

#### Spanned inline comment

Description: Like an inline comment, except it covers a span of lines.

Syntax: To start a span, insert one or more newlines immediately before
a quoted, non-header part of the diff. To terminate a span, insert a
inline comment.

[Example](examples/spanned_inline_comment.prr)

#### Review directives

Description: Meta-directives to give to `prr` in review comment. Currently
only supports approving, requesting changes to, and commenting on a PR.

Syntax: `@prr approve`, `@prr reject`, or `@prr comment`.

[Example](examples/prr_directive.prr)

### Vim integration

`vim/` directory contains Vim plugin providing syntax coloring, filetype
detection and folding configuration for `*.prr` files.

To install it modify `&runtimepath` to add this directory or use plugin manager
of your choice to add this plugin.

#### Vundle installation

```
Plugin 'danobi/prr', {'rtp': 'vim/'}
```

#### Manual installation

Copy the provided `*.vim` files into their appropriate subdirectories in
`~/.vim`.

#### Folding

With default Vim configuration all folds will be closed by default, so if you
want them to be opened then you need to do one of these:

- Add `set foldlevel=9999` in your Vim config to open all folds by default
- Add `set nofoldenable` to disable folding

Consult `:h folding` for more details.

### Config

`prr` supports various configuration options spread over one or more config
files. The global config file must be located at `$XDG_CONFIG_HOME/prr/config.toml`.
This typically expands to `$HOME/.config/prr/config.toml`.

`prr` also supports local config files. Local config files must be named
`.prr.toml` and will be searched for starting from the current working
directory up every parent directory until either the first match or the root
directory is hit. Local config files override values in the global config.
Table specific semantics are documented below.

#### [prr]

The `[prr]` table controls installation wide settings.

* `prr.token`: Personal authentication token (required)
* `prr.workdir`: Directory to place review files (optional)
* `prr.url`: URL to github API (optional)

If this table is specified in a local config file, it must be fully specified
and will override the global config file.

#### [local]

The `[local]` table contains configuration local to a directory and its
sub-directories.

* `local.repository`: A string in format of `${ORG}/${REPO}` (optional)
    * If specified, you may omit the `${ORG}/${REPO}` from PR string arguments.
      For example, you may run `prr get 6` instead of `prr get danobi/prr/6`.

This table may not be specified in both a local config file and the global
config file.
