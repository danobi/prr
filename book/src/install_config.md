# Configure prr

First, create the config directory:

```sh
mkdir -p ~/.config/prr
```

Next, create a basic global config:

```sh
cat << EOF > ~/.config/prr/config.toml
[prr]
token = "$YOUR_PAT_FROM_LAST_STEP"
workdir = "/home/dxu/dev/review"
EOF
```

`token` can be provided in one of a few ways. In order of precedence:
- Referencing an environment variable containing the token e.g. `$PRR_TOKEN`
- Having `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN` or `GITHUB_ENTERPRISE_TOKEN` already defined in your environment
- Passing the token value as-is

If `token` is absent from the config, then the above environment variables will be checked in order and 
the first token found will be used.

Note `workdir` can be any directory. (You don't have to use my unix name)
