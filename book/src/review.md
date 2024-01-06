# Review file

`prr` supports various review file markups. This document captures all of them.

## Review directives

Description: Meta-directives to give to `prr` in review comment. Currently
only supports approving, requesting changes to, and commenting on a PR.

Syntax: `@prr approve`, `@prr reject`, or `@prr comment`.

[Example](./examples/prr_directive.md)

## Review comment

Description: PR-level review comment. You only get one of these per review.

Syntax: Non-whitespace, non-quoted text at the beginning of the review file.

[Example](./examples/review_comment.md)

## Inline comment

Description: Inline comment attached to a specific line of the diff.

Syntax: None-whitespace, non-quoted text on a newline immediately following
a quoted non-header part of the diff.

[Example](./examples/inline_comment.md)

## Spanned inline comment

Description: Like an inline comment, except it covers a span of lines.

Syntax: To start a span, insert one or more newlines immediately before
a quoted, non-header part of the diff. To terminate a span, insert a
inline comment.

[Example](./examples/spanned_inline_comment.md)

## File comment

Description: File-level comment.

Syntax: Non-whitespace, non-quoted text immediately following the `diff --git` header

[Example](./examples/file_comment.md)

## Snips

Description: Use `[...]` to replace (ie. snip) contiguous quoted lines.

Syntax: `[...]` or `[..]` on its own line. Multiple snips may be used in a review file.

[Example](./examples/snip.md)
