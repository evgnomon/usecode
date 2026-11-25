# uc try

Dated scratch directories for experiments, clones and worktrees, one fuzzy
pick away. They live in the tries directory, `~/src/github.com/$USER/tries`
unless `UC_TRY_PATH` says otherwise, and are named `YYYY-MM-DD-<name>`.

`uc try` never changes the shell itself: it prints a bash script on stdout
and the `try` function evals it, so the shell ends up `cd`'d into the try.
The user's shell gets the function with:

```bash
eval "$(uc try init)"            # in ~/.bashrc; `uc try init PATH` fixes the tries dir
```

## Making a try

```bash
try <git-url> [name]             # clone into tries/<date>-<user>-<repo> (or <name>)
try https://github.com/o/r/pull/12   # a GitHub PR URL clones and checks the PR out
try . <name>                     # tries/<date>-<name>; in a git repo, a worktree
                                 # of it on a new branch <name>
try ./path [name]                # the same, from another directory
try [query]                      # the picker, pre-filtered by query
```

A name that is taken today gets a number: `foo`, then `foo-2`, or `v3` → `v4`.
`try .` only makes a branch for a name you gave; if a branch of that name
already exists, the worktree is a detached HEAD and it says why.

From the agent the `try` function usually isn't defined, and it writes its
messages to `/dev/tty`, which isn't there. Eval `uc try` directly instead;
the `cd` it ends with carries over to later commands:

```bash
eval "$(uc try . my-experiment)"
eval "$(uc try https://github.com/owner/repo)"
```

Run `uc try ...` without `eval` to see the script it would run, without
running it.

## The picker

`try` alone (or with a query) opens a picker on the terminal. It needs a
real terminal, so the agent can't drive it: `uc try foo` fails with "try
requires an interactive terminal". Give the user the command instead.

- `↑/↓`, `Ctrl-P/N`: move. `Enter`: `cd` into it, or create a new try named
  after the query.
- `Ctrl-R`: rename. For a worktree it runs `git worktree move`, and renames
  its branch too when the branch was named after the try.
- `Ctrl-G`: graduate (below).
- `Ctrl-D`: mark for deletion, `Enter` to confirm. It refuses anything that
  isn't inside the tries directory.
- `Esc`, `Ctrl-C`: cancel.

## Graduating

A try that turned into a real project graduates: it moves out of the tries
directory into the projects directory and leaves a symlink behind under the
old dated name, so old paths still work.

- Where: `UC_TRY_PROJECTS` (or `--try-projects`, or `try_projects:` under
  `uc:` in `config.yaml`; the old `TRY_PROJECTS` still works). Without it,
  the parent of the tries directory, i.e. `~/src/github.com/$USER`.
- The name: the try's name without the date, so `2025-01-12-foo` becomes
  `<projects>/foo`. The dialog lets you change the path; it refuses one that
  exists or whose parent doesn't.
- How: a worktree (`.git` is a file) moves with `git worktree move`, so the
  main repository keeps tracking it; anything else with `mv`.

`Ctrl-G` is in the picker, so it is the user's. When the user asks the agent
to graduate a try, do the same steps by hand:

```bash
src=~/src/github.com/$USER/tries/2025-01-12-foo
dest=~/src/github.com/$USER/foo        # or under $UC_TRY_PROJECTS
git -C "$src" worktree move "$src" "$dest"   # if "$src/.git" is a file
mv -- "$src" "$dest"                         # otherwise
ln -s "$dest" "$src"
cd "$dest"
```

## Settings

Each is `UC_<NAME>` in the environment, or `<name>:` under `uc:` in
`~/.config/usecode/config.yaml`; the old `TRY_PATH`/`TRY_PROJECTS` still work.

- `UC_TRY_PATH`: the tries directory (default `~/src/github.com/$USER/tries`).
- `UC_TRY_PROJECTS`: where tries graduate to (default: the tries' parent).
