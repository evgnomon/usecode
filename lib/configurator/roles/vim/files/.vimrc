set nocompatible              " Disable compatibility mode with vi, enabling more advanced Vim features.
set exrc                      " Allow the use of local .vimrc files in each directory (can be a security risk if editing untrusted files).
set secure                    " Disable potentially dangerous commands in local .vimrc files (enhances security when using 'exrc').
set encoding=utf-8            " Set the character encoding to UTF-8, ensuring proper handling of international characters.
set autowrite                 " Automatically save the file before running certain commands like `:make`.
set ic                        " Make search case insensitive, unless an uppercase letter is used in the search.
set mouse=a                   " Enable mouse support in all modes (normal, insert, visual, etc.).
filetype on                   " Enable file type detection, allowing Vim to detect and set settings based on file type.
filetype plugin indent on     " Enable file type-specific plugins and indentation rules.
filetype plugin on            " Enable file type-specific plugins.
set signcolumn=yes            " Always display the sign column (used for displaying Git changes, linting errors, etc.).
set scrolloff=0               " Set the minimum number of lines to keep above and below the cursor when scrolling (0 means no extra lines).
set noswapfile                " Disable swap file creation, preventing Vim from creating temporary files during editing.
set nowritebackup             " Disable backup file creation before overwriting a file.
set nobackup                  " Disable backup file creation when editing a file.
set number                    " Display line numbers in the editor.
set cursorline                " Highlight the line where the cursor is located.
set ttyfast                   " Assume a fast terminal connection, optimizing screen redraws.
syntax on                     " Enable syntax highlighting.
set tabstop=2                 " Set the number of spaces a tab character represents to 2.
set shiftwidth=2              " Set the number of spaces used for indentation to 2.
set smarttab                  " Insert appropriate number of spaces when tab is pressed based on `shiftwidth`.
set autoindent                " Maintain the indentation level of the previous line.
set expandtab                 " Convert tabs to spaces.
set exrc                      " (Duplicate) Allow the use of local .vimrc files in each directory.
nmap <F10> :on<CR>            " Map the F10 key to turn on the current window.
set incsearch                 " Show partial matches for a search as you type.
set hlsearch                  " Highlight all search pattern matches.
let mapleader=","             " Set the leader key to a comma, allowing custom key bindings with this leader.

:set showmatch                  " Briefly jump to the matching parenthesis, bracket, or brace when one is inserted.
:set number relativenumber      " Display both absolute line numbers and relative line numbers.
:let python_highlight_all = 1   " Enable full syntax highlighting for Python, including highlighting of built-in functions and classes.
:set backspace=indent,eol,start " Allow backspacing over indentation, end of line, and insertion start position in insert mode.
set background=dark             " Optimize colors for a dark background.
set belloff=all               " Disable all audible and visual bells, preventing distractions from error notifications.

:augroup numbertoggle
  :  autocmd!
  :  autocmd BufEnter,FocusGained,InsertLeave * set relativenumber
  :  autocmd BufLeave,FocusLost,InsertEnter   * set norelativenumber
:augroup END

if has('win32')
  set clipboard=unnamed
elseif has('mac')
  set clipboard=unnamed
  set term=xterm-256color                    " Set the terminal type to xterm with 256-color support.
elseif has('unix')
  set clipboard=unnamedplus
  set term=xterm-256color                    " Set the terminal type to xterm with 256-color support.
endif

set guifont=Inconsolata\ Nerd\ Font:h15    " Set the GUI font to Inconsolata Nerd Font with a height of 15.
set t_Co=256                               " Set the terminal to use 256 colors.
set fillchars+=stl:\ ,stlnc:\              " Customize statusline fill characters: empty space for both active and inactive status lines.
set termencoding=utf-8                     " Set terminal encoding to UTF-8, ensuring correct character display.
set completeopt=preview,menuone,popup      " Configure completion options: show a preview window, show the menu even with one match, and use a popup menu.

" Plugins
call plug#begin('~/.vim/plugged')
Plug 'voldikss/vim-floaterm'
Plug 'neoclide/jsonc.vim', {'commit': '6fb92460f9e50505c9b93181a00f27d10c9b383f' }
Plug 'morhetz/gruvbox', {'commit': '697c00291db857ca0af00ec154e5bd514a79191f' }

" Search
Plug 'junegunn/fzf', { 'do': { -> fzf#install() } }
Plug 'junegunn/fzf.vim'

" Snippet
Plug 'SirVer/ultisnips'

" Formatter
Plug 'tpope/vim-commentary'
Plug 'https://github.com/rhysd/vim-clang-format'
Plug 'prettier/vim-prettier', { 'do': 'yarn install' }

" Terraform
Plug 'hashivim/vim-terraform'

" JS/TS
Plug 'heavenshell/vim-jsdoc', {
      \ 'for': ['javascript', 'javascript.jsx','typescript'],
      \ 'do': 'make install'
      \}

" Copilot
Plug 'github/copilot.vim'

" Auto Completion / LSP
Plug 'neoclide/coc.nvim', {'branch': 'release'}
call plug#end()

" Set up CoC global extensions
let g:coc_global_extensions = [
      \ 'coc-rust-analyzer',
      \ 'coc-prettier',
      \ 'coc-tsserver',
      \ 'coc-go',
      \ 'coc-snippets',
      \ 'coc-pyright',
      \ '@yaegassy/coc-ruff',
      \ 'coc-toml',
      \ 'coc-clangd'
      \]

nmap <silent> <leader>kk ?function<cr>:noh<cr><Plug>(jsdoc)

" Must be after laoding plugins
colorscheme gruvbox

" Snippets
let g:UltiSnipsSnippetStorageDirectoryForUltiSnipsEdit = $HOME."/src/github.com/".$USER."/usecode/lib/snippets/ultisnips"
let g:UltiSnipsSnippetDirectories=[$HOME."/src/github.com/".$USER."/usecode/lib/snippets/ultisnips", "UltiSnips"]
" Avoid <Tab> collision with Copilot; use <C-J>/<C-K> for UltiSnips.
let g:UltiSnipsExpandTrigger="<C-J>"
let g:UltiSnipsJumpForwardTrigger="<C-J>"
let g:UltiSnipsJumpBackwardTrigger="<C-K>"

nnoremap <C-P> :History<CR>
nnoremap <leader>o :Files<CR>

" Reformat
let g:black_linelength=79

" File types
autocmd BufRead,BufNewFile *.j2 setfiletype jinja2

autocmd FileType python,go,javascript,typescript,
      \javascriptreact,typescriptreact,rust,sh,cs              nnoremap <C-l> <Plug>(coc-format)
autocmd FileType proto                                      noremap <C-l> :ClangFormat<CR>
autocmd FileType json,html,yaml                             noremap <C-l> :Prettier<CR>
autocmd FileType toml                                       noremap <C-l> <Plug>(coc-format)
autocmd FileType terraform                                  noremap <C-l> :TerraformFmt<CR>

" Run Command
autocmd FileType rust                                       noremap <C-k><C-r> :CocCommand rust-analyzer.testCurrent<CR>

" Commands
map <C-A> :CocCommand<CR>
map <leader>kp :echo @% <CR>

" Tags
map <leader>t :Tags<CR>
map <leader>f :Rg!<CR>
nmap <Leader>7 :BTags<CR>
nmap <S-F3> :CocList -I symbols<CR>

" References
" Copy Location to clipboard
:nmap <leader>8 :let @+ = join([expand('%'),  line(".")], ':')<cr>

" Copy/Paste
map <Leader>v "+p
map <Leader>c "+y
map ]l :cn<CR>

nmap <leader>i :CocCommand editor.action.organizeImport<CR>
nmap <leader>. <Plug>(coc-codeaction)

let g:loclist_follow = 1
let g:tex_flavor = 'pdflatex'

noremap <Up> <Nop>
noremap <Down> <Nop>
noremap <Left> <Nop>
noremap <Right> <Nop>

let g:go_fmt_command="gopls"
let g:go_gopls_gofumpt=1

:command! CopyPath let @+ = expand('%:p')

" GoTo code navigation
nmap <silent> <leader>g <Plug>(coc-definition)
nmap <silent> gt <Plug>(coc-type-definition)
nmap <silent> gi <Plug>(coc-implementation)
nmap <silent> <leader>3 <Plug>(coc-references)

nmap <silent> <F8> <Plug>(coc-diagnostic-next)
noremap <silent> <S-F8> <Plug>(coc-diagnostic-prev)

" Use K to show documentation in preview window
nnoremap <leader>q :call ShowDocumentation()<CR>

function! ShowDocumentation()
  if CocAction('hasProvider', 'hover')
    call CocActionAsync('doHover')
  else
    call feedkeys('K', 'in')
  endif
endfunction

inoremap <silent><expr> <c-@> coc#refresh()
inoremap <silent><expr> <CR> pumvisible() ? coc#_select_confirm() : "\<CR>"
nmap <S-F6> <Plug>(coc-rename)


" Remap <C-f> and <C-b> to scroll float windows/popups
if has('patch-8.2.0750')
  nnoremap <silent><nowait><expr> <C-f> coc#float#has_scroll() ? coc#float#scroll(1) : "\<C-f>"
  nnoremap <silent><nowait><expr> <C-b> coc#float#has_scroll() ? coc#float#scroll(0) : "\<C-b>"
  inoremap <silent><nowait><expr> <C-f> coc#float#has_scroll() ? "\<c-r>=coc#float#scroll(1)\<cr>" : "\<Right>"
  inoremap <silent><nowait><expr> <C-b> coc#float#has_scroll() ? "\<c-r>=coc#float#scroll(0)\<cr>" : "\<Left>"
  vnoremap <silent><nowait><expr> <C-f> coc#float#has_scroll() ? coc#float#scroll(1) : "\<C-f>"
  vnoremap <silent><nowait><expr> <C-b> coc#float#has_scroll() ? coc#float#scroll(0) : "\<C-b>"
endif

let g:fzf_layout = { 'window': { 'width': 0.9, 'height': 0.9, 'yoffset': 0.5, 'xoffset': 0.5, 'border': 'sharp' } }
command! -bang -nargs=* Rg
    \ call fzf#vim#grep(
    \   'rg --line-number --no-heading --color=always --hidden -i --glob "!{.git,node_modules}/*" -- '.shellescape(<q-args>), 1,
    \   fzf#vim#with_preview(), <bang>0)

