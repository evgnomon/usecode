" License-Identifier: HGL
" Copyright (C) The Usecode Authors (see AUTHORS)

" Clipboard providers (and the tuple type) need a newer Vim than Debian's.
if !exists('v:clipproviders')
    finish
endif

vim9script

def WlAvailable(): bool
    return executable('wl-copy') && executable('wl-paste')
enddef

def WlCopy(reg: string, type: string, str: list<string>)
    var args = "wl-copy"
    if reg == "*"
        args ..= " -p"
    endif
    system(args, str)
enddef

def WlPaste(reg: string): tuple<string, list<string>>
    var args = "wl-paste --no-newline --type text/plain;charset=utf-8"
    if reg == "*"
        args ..= " -p"
    endif
    return ("", systemlist(args))
enddef

v:clipproviders["wl_clipboard"] = {
    available: WlAvailable,
    copy:  { "+": WlCopy,  "*": WlCopy },
    paste: { "+": WlPaste, "*": WlPaste }
}

set clipmethod=wayland,wl_clipboard
