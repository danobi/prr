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

See [`token`](./config.md#the-token-field) for further details on how to provide a GitHub token.

Note `workdir` can be any directory. (You don't have to use my unix name)
