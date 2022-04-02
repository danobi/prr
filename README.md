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

1. Install rust toolchain (if you haven't already): https://rustup.rs/

2. Install `prr`:

    ```sh
    $ cargo install prr
    ```

3. Create a Github Personal Access Token (PAT) for `prr`:

    `prr` will need this token so it can make GH API calls on your behalf.
    Create the token by going to `Settings -> Developer settings -> Personal
    access tokens -> Generate new token` and give the token `repo` permissions.

    Keep the newly generated token handy for the next step.

4. Create a `prr` config file:

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

5. Review your first PR:

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

"Vim integration" is a bit overselling it, but I've created some `ftdetect`
and `syntax` configs to enable syntax highlighting for `prr` review files.

It can be pretty hard to look at a diff without having deletes and additions
highlighted in different colors.
