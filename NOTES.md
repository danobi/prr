# TODO

- [x] Parse review files
    - [x] Create parser
    - [x] Create `include_str!()` based unit-tests for expected comments
        - [x] Test invalid spans (span that does not have a comment that
              terminates it and another span starts)
- [x] Wire up comment uploading to GH
- [ ] Support [...] snipping
- [ ] Manual test that comments on a changed file work

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
