# Vim integration

The `vim/` directory in `prr` source contains Vim plugin providing syntax
coloring, filetype detection and folding configuration for `*.prr` files.

To install, modify `&runtimepath` to add this directory or use a plugin manager
of your choice to add this plugin.

## Vundle installation

Add the following to your `.vimrc`:

```
Plugin 'danobi/prr', {'rtp': 'vim/'}
```

## Manual installation

Copy the provided `*.vim` files into their appropriate subdirectories in
`~/.vim`.

## Syntax colors

The `prr` "plugin" exports some preconfigured syntax hooks.

Example [from my dotfiles][0]:

```vim
"Automatically set up highlighting for `.prr` review files
"Use `:hi` to see the various definitions we kinda abuse here
augroup Prr
  autocmd!
  autocmd BufRead,BufNewFile *.prr set syntax=on

  "Make prr added/deleted highlighting more apparent
  autocmd BufRead,BufNewFile *.prr hi! link prrAdded Function
  autocmd BufRead,BufNewFile *.prr hi! link prrRemoved Keyword
  autocmd BufRead,BufNewFile *.prr hi! link prrFile Special

  "Make file delimiters more apparent
  autocmd BufRead,BufNewFile *.prr hi! link prrHeader Directory

  "Reduce all the noise from color
  autocmd BufRead,BufNewFile *.prr hi! link prrIndex Special
  autocmd BufRead,BufNewFile *.prr hi! link prrChunk Special
  autocmd BufRead,BufNewFile *.prr hi! link prrChunkH Special
  autocmd BufRead,BufNewFile *.prr hi! link prrTagName Special
  autocmd BufRead,BufNewFile *.prr hi! link prrResult Special
augroup END
```

## Folding

With default Vim configuration all folds will be closed by default, so if you
want them to be opened then you need to do one of these:

- Add `set foldlevel=9999` in your Vim config to open all folds by default
- Add `set nofoldenable` to disable folding

Consult `:h folding` for more details.


[0]: https://github.com/danobi/dotfiles/blob/ab00f235fffd4c8d5e2496657e8047e1473d9257/vim/.vimrc#L81-L94
