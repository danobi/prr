# TODO

- [ ] Parse review files
    - [ ] Create parser
    - [ ] Create `include_str!()` based unit-tests for expected comments


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
