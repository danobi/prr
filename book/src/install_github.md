# Acquire Github token

`prr` needs a classic personal access token with `repo` permissions so it can
make Github API calls on your behalf.

1. Create the token, using either the prefilled link or the manual steps below.
2. Copy the token right away. Github only shows it once, and you'll need it in
   the [next step](./install_config.md).

### Prefilled

Open this prefilled [Generate new token page](https://github.com/settings/tokens/new?description=prr%20code%20review%20CLI&scopes=repo&default_expires_at=none),
click "Generate token", and copy the token.

### Manual

Go to:

```
Settings -> Developer settings -> Personal access tokens -> Tokens (classic) -> Generate new token
```

Select the `repo` scope (`public_repo` is enough if you only review public
repositories), click "Generate token", and copy the token.
