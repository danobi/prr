" Vim syntax file
" Language:     prr
" Maintainer:   Daniel Xu <dxu@dxuuu.xyz>
" Last Change:  2022 Mar 25
" Credits:      Bram Moolenaar <Bram@vim.org>
"
"               This version is copied and edited from diff.vim

" Check whether an earlier file has defined a syntax already
if exists("b:current_syntax")
  finish
endif

syn match diffAdded     "^> +.*"
syn match diffRemoved   "^> -.*"

" Define the default highlighting.
" Only used when an item doesn't have highlighting yet
hi def link diffAdded           Type
hi def link diffRemoved         Statement

let b:current_syntax = "prr"
