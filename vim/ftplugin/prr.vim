setlocal foldmethod=expr
setlocal foldexpr=s:DiffFoldLevel()
setlocal foldcolumn=3

" Adapted from https://github.com/sgeb/vim-diff-fold
function! s:DiffFoldLevel()
    let l:line=getline(v:lnum)

    if l:line =~# '^> \(diff\|Index\)' " file
        return '>1'
    elseif l:line =~# '^> \(@@\|\d\)' " hunk
        return '>2'
    else
        return '='
    endif
endfunction

let b:undo_ftplugin = 'setl fdm< | setl fde< | setl fdc<'
