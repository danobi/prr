# Configuration

`prr` supports two types of configuration: global and local.

Global configuration controls global behavior. Local configuration controls
behavior for the directory the configuration is in as well as its
subdirectories.

`prr` respects the [XDG Base Directory Specification][0] where possible.

## Global configuration

`prr` reads global configuration from both `prr --config` as well as
`$XDG_CONFIG_HOME/prr/config.toml`. It places priority on the `--config` flag.

The following global configuration options are supported:

* `[prr]`
    * [`token`](#the-token-field)
    * [`workdir`](#the-workdir-field)
    * [`url`](#the-url-field)
    * [`activate_pr_metadata_experiment`](#the-activate_pr_metadata_experiment-field)

### The `token` field

The `token` field is your Github Personal Authentical Token as a string.

Example:

```toml
[prr]
token = "ghp_Kuzzzzzzzzzzzzdonteventryzzzzzzzzzzz"
```

If the `token` field is absent - or set to an empty string - then the following environment
variables will be checked in order and the first token found will be used:

- `GH_TOKEN`
- `GITHUB_TOKEN`
- `GH_ENTERPRISE_TOKEN`
- `GITHUB_ENTERPRISE_TOKEN`

### The `workdir` field

The optional `workdir` field takes a path in string form.

Review files and metadata will be placed here. Note `~` and `$HOME` do not
expand. Paths must also be absolute.

If omitted, `workdir` defaults to `$XDG_DATA_HOME/prr`.

Example:

```toml
[prr]
workdir = "/home/dxu/dev/review"
```

### The `url` field

The optional `url` field takes a URL to the Github API in string form.

This is useful for Github Enterprise deployments where the API endpoint is non-standard.

Example:

```toml
[prr]
url = "https://github.company.com/api/v3"
```

### The `activate_pr_metadata_experiment` field

The optional `activate_pr_metadata_experiment` field determines whether,
prr is downloading the PR description as well as the diff of the PR. Note
that the effect as well as the name of this option may change in the
future.

If this is not explicitly set to "true", it is considered to be set to
"false".

Example:

```toml
[prr]
activate_pr_metadata_experiment = true
```

## Local configuration

Local config files must be named `.prr.toml` and will be searched for starting
from the current working directory up every parent directory until either the
first match or the root directory is hit. Local config files override values in
the global config in some cases.

If the [`[prr]`](#global-configuration) table is provided in a local config
file, it must be fully specified and will override the global config file.

The following local configuration options are supported:

* `[local]`
    * [`repository`](#the-repository-field)
    * [`workdir`](#the-local-workdir-field)

### The `repository` field

The optional `repository` field takes a string in format of
`${ORG}/${REPO}`.

If specified, you may omit the `${ORG}/${REPO}` from PR string arguments.
For example, you may run `prr get 6` instead of `prr get danobi/prr/6`.


Example:

```toml
[local]
repository = "danobi/prr"
```

### The local `workdir` field

The optional `workdir` field takes a string that represents a path.

The semantics are the same as [prr.workdir](#the-workdir-field) with the
following exception: in contrast to global workdir, relative local workdir
paths are interpreted as relative to the local config file.

Example:

```toml
[local]
workdir = ".prr"
```

[0]: https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html
