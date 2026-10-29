# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# What uc's bash runs commands with, see shell.rs. Nothing here touches a
# file: it comes in on a pipe, fd 4, and where the shell ends up goes back on
# another, fd 3, which bash moves out of the command's way to `$__uc_fd`. `__uc_full` says how much: 0 for the model's commands, the
# directory and the environment; 1 for the user's, their shell's variables,
# options, aliases and functions too, after the prompt's hooks; 2 to read
# their bash at the start, its definitions, completions and history file.

__uc_ret() { return "$1"; }

# The prompt's hooks, as after a command at the prompt: zoxide's, mise's,
# direnv's. They see the command's status, and zoxide's tells a prompt from a
# script by the history option. HISTFILE is /dev/null meanwhile: a hook's
# `history -a` in a bash without the user's history settings would cut their
# history file to 500 lines.
__uc_hooks() {
  local __uc_rc=$1 __uc_hook HISTFILE=/dev/null
  set -o history
  for __uc_hook in "${PROMPT_COMMAND[@]}"; do
    __uc_ret "$__uc_rc"
    builtin eval -- "$__uc_hook"
  done </dev/null >/dev/null 2>&1
  set +o history
}

# The shell's own variables, as `declare -p` lines to run again in the next
# one: not the exported ones, which go in the environment, the ones bash keeps
# itself, the read-only ones, or the history's, which only a bash with the
# user's whole bashrc should have.
__uc_vars() {
  local __uc_n
  while IFS= builtin read -r __uc_n; do
    case $__uc_n in
      _ | BASH | BASH_* | BASHOPTS | BASHPID | COLUMNS | COMP_* | DIRSTACK | EPOCHREALTIME | \
        EPOCHSECONDS | EUID | FUNCNAME | GROUPS | HIST* | HOSTNAME | HOSTTYPE | IFS | LINENO | \
        LINES | MACHTYPE | OPTARG | OPTERR | OPTIND | OSTYPE | PIPESTATUS | PPID | RANDOM | \
        SECONDS | SHELLOPTS | SRANDOM | UID | __uc_*) continue ;;
    esac
    [[ ${!__uc_n@a} == *[xr]* ]] && continue
    builtin declare -p -- "$__uc_n"
  done < <(builtin compgen -v)
}

# The shell's functions but uc's, their names on the first line.
__uc_names() {
  local __uc_n
  while IFS= builtin read -r __uc_n; do
    case $__uc_n in __uc_*) ;; *) builtin printf '%s\n' "$__uc_n" ;; esac
  done < <(builtin compgen -A function)
}

__uc_functions() {
  local __uc_n
  while IFS= builtin read -r __uc_n; do
    builtin declare -f -- "$__uc_n"
  done < <(__uc_names)
}

# Writes where the shell is to `$__uc_fd`, NUL-separated: `uc-state`, then `pwd`
# and the directory, `env` and NAME=value for each exported variable and
# function, `options`, `aliases`, `vars`, `names` and `functions` from a full
# shell, `completions` and `histfile` at the start, and `end`. Once only: it
# runs after the command and again on the way out.
__uc_save() {
  local __uc_rc=$1 __uc_n __uc_v
  [[ -n ${__uc_saved-} || $__uc_full != [012] ]] && return "$__uc_rc"
  __uc_saved=1
  local -
  set +eu +o pipefail
  [[ $__uc_full == 1 ]] && __uc_hooks "$__uc_rc"
  {
    builtin printf 'uc-state\0pwd\0%s\0' "$PWD"
    if [[ $__uc_full != 2 ]]; then
      while IFS= builtin read -r __uc_n; do
        case $__uc_n in _ | PWD | SHLVL | __uc_*) continue ;; esac
        [[ -v $__uc_n ]] || continue
        builtin printf 'env\0%s=%s\0' "$__uc_n" "${!__uc_n}"
      done < <(builtin compgen -e)
      while builtin read -r _ _ __uc_n; do
        case $__uc_n in __uc_*) continue ;; esac
        __uc_v=$(builtin declare -f -- "$__uc_n")
        builtin printf 'env\0BASH_FUNC_%s%%%%=() %s\0' "$__uc_n" "${__uc_v#*$'\n'}"
      done < <(builtin declare -Fx)
    fi
    if [[ $__uc_full != 0 ]]; then
      builtin printf 'options\0%s\0' "$(builtin shopt -p)"
      builtin printf 'aliases\0%s\0' "$(builtin alias -p)"
      builtin printf 'vars\0%s\0' "$(__uc_vars)"
      builtin printf 'names\0%s\0' "$(__uc_names)"
      builtin printf 'functions\0%s\0' "$(__uc_functions)"
    fi
    if [[ $__uc_full == 2 ]]; then
      builtin printf 'completions\0%s\0' "$(builtin complete -p)"
      builtin printf 'histfile\0%s\0' "${HISTFILE-}"
    fi
    builtin printf 'end\0'
  } >&"$__uc_fd" 2>/dev/null
  return "$__uc_rc"
}
