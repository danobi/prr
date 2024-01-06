# Contributing

There isn't much process yet, so this page is light. But a few things to keep
in mind:

* I highly encourage floating an idea before implementing it. So that we can
  avoid any potential wasted work. My (@danobi) SLA for providing _some_ kind
  of feedback is 1-2 days.

* Tests are mandatory for any parser changes. Tests are currently kinda light
  especially as it's closer to API call layer, but we should be adding tests
  where possible.

* All user facing changes must come with documentation update. Documentation
  lives in `book/` and is rendered by [mdBook][0]. Documentation changes are
  automatically deployed upon merge.

[0]: https://rust-lang.github.io/mdBook/
