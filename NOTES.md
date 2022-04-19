# TODO

- [x] Parse review files
    - [x] Create parser
    - [x] Create `include_str!()` based unit-tests for expected comments
        - [x] Test invalid spans (span that does not have a comment that
              terminates it and another span starts)
- [x] Wire up comment uploading to GH
- [x] Inspect response error codes and body
- [x] Fix bug where `line` and `start_line` are being set instead of `position`
    - [x] Check if `start_position` is accepted
    - [x] Add test for trying to comment on a hunk start
- [x] Figure out how to calculate line for diffs w/ changes on both sides
- [x] Add test for comment at end of review file
- [x] Prohibit cross hunk spanned comments
- [x] Support review-level comments at top of review file
- [x] Manual test that comments on a changed file work
- [x] Support approve/rejecting PRs
    - [x] Need some kind of meta syntax (like go's //+)
        - [ ] Think about if it could be generalized to comment threads
- [x] Support updating a PR's review file, but ask for confirmation if review file has been modified and not submitted yet
    - [x] Maybe even check mtime between review file and submission time?
- [ ] Support parsing github url from stdin
- [ ] Save commit hash of downloaded review file
- [ ] Support [...] snipping

# Thoughts

* Make a comment spanned by inserting a whitespace line before the
  start of the span

    * To compose with back-to-back spanned comments, the latter comment
      must be assumed to be a single line comment. Otherwise, using
      a single spanned comment always makes the next comment a span.
      This kinda actually makes sense conceptually too cuz if the user
      actually wants back-to-back spans then they should've just used
      a single, larger span.

* Need to be careful to prohibit a spanned comment over multiple files
