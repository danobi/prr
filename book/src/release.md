# Releases

Releases are currently managed by [`cargo-release`][0].

We are currently pre-1.0, so we are only cutting minor and patch releases. We
try to follow [semantic versioning][1]. Cut a minor release by running:

```sh
cargo release minor
```

Similarly, a patch release is:

```sh
cargo release patch
```

## Github releases and binary artifacts

At some point it'd be good to automatically cut GH releases and upload binary
(statically linked) artifacts to it. It's not too hard to do using GHA. Just
lazy. Contributions welcome.

[0]: https://github.com/crate-ci/cargo-release
[1]: https://semver.org/
