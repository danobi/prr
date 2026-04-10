# Shell Completion

## Zsh

prr ships a zsh completion function in completions/\_prr.
Install it into a directory in your `$fpath` (for example `~/.zsh/completions`):

`sh
mkdir -p ~/.zsh/completions
cp completions/\_prr ~/.zsh/completions/
`

In your `~/.zshrc`:

```sh
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit
compinit
```
