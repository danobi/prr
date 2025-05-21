setlocal foldmethod=expr
setlocal foldexpr=DiffFoldLevel()
setlocal foldcolumn=3

" Adapted from https://github.com/sgeb/vim-diff-fold
function! DiffFoldLevel()
    let l:line=getline(v:lnum)

    if l:line =~# '^> \(diff\|Index\)' " file
        return '>1'
    elseif l:line =~# '^> \(@@\|\d\)' " hunk
        return '>2'
    else
        return '='
    endif
endfunction

let b:undo_ftplugin = 'setl fdm&'
