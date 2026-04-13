#!/usr/bin/env bash
#
# ci-run.sh — VCS-agnostic CI wrapper around `make ci`
#
# Exports a canonical set of CI_* environment variables derived from
# the native CI system (GitHub Actions, Gerrit trigger, or manual),
# then invokes `make ci` at the repository root.
#
# Usage:
#   ./ci-run.sh              # auto-detect environment, run make ci
#   ./ci-run.sh --dry-run    # print variables, skip make
#   ./ci-run.sh --env        # print eval-able exports to stdout
#
set -euo pipefail

# ── Resolve repo root (script may live in a subdir) ─────────────
REPO_ROOT="$(git -C "$(dirname "$0")" rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

# ── Helpers ─────────────────────────────────────────────────────
_git_commit()  { git rev-parse HEAD 2>/dev/null || echo "unknown"; }
_git_short()   { git rev-parse --short HEAD 2>/dev/null || echo "unknown"; }
_git_branch()  { git symbolic-ref --short HEAD 2>/dev/null || echo "detached"; }
_git_author()  { git log -1 --format='%an' 2>/dev/null || echo "unknown"; }
_git_email()   { git log -1 --format='%ae' 2>/dev/null || echo "unknown"; }
_git_subject() { git log -1 --format='%s' 2>/dev/null || echo ""; }
_git_tag()     { git describe --tags --exact-match 2>/dev/null || echo ""; }

# Slugify: strip refs/ prefixes, replace / and _ with -
_slug() {
    printf '%s' "$1" \
        | sed -e 's|^refs/heads/||' -e 's|^refs/tags/||' \
        | tr '/_' '--'
}

# ── Source-specific overrides ───────────────────────────────────
if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
    # ---- GitHub Actions ----
    CI_SYSTEM="github"
    CI_EVENT="${GITHUB_EVENT_NAME:-push}"
    CI_REF="${GITHUB_REF:-}"
    CI_BASE_REF="${GITHUB_BASE_REF:-}"
    CI_HEAD_REF="${GITHUB_HEAD_REF:-}"
    CI_COMMIT="${GITHUB_SHA:-$(_git_commit)}"
    CI_COMMIT_SHORT="${CI_COMMIT:0:7}"
    CI_BRANCH="${GITHUB_HEAD_REF:-${GITHUB_REF_NAME:-$(_git_branch)}}"
    CI_TARGET="${GITHUB_BASE_REF:-}"
    CI_SLUG="${GITHUB_REPOSITORY:-}"
    CI_URL="${GITHUB_SERVER_URL:-https://github.com}/${CI_SLUG}"
    CI_CHANGE="${CI_CHANGE:-}"
    if [[ "$CI_EVENT" == "pull_request" && "$CI_REF" =~ refs/pull/([0-9]+)/ ]]; then
        CI_CHANGE="${BASH_REMATCH[1]}"
    fi
    CI_RUN="${GITHUB_RUN_ID:-}"
    CI_RUN_URL="${CI_URL}/actions/runs/${CI_RUN}"
    CI_ACTOR="${GITHUB_ACTOR:-$(_git_author)}"
    CI_EMAIL="${CI_EMAIL:-$(_git_email)}"
    CI_PIPELINE="${GITHUB_WORKFLOW:-ci}"
    CI_JOB="${GITHUB_JOB:-}"
    CI_EVENT_PATH="${GITHUB_EVENT_PATH:-}"

elif [[ -n "${GERRIT_CHANGE_NUMBER:-}" || -n "${GERRIT_EVENT_TYPE:-}" ]]; then
    # ---- Gerrit (via trigger plugin / zuul / jenkins-gerrit-trigger) ----
    CI_SYSTEM="gerrit"
    CI_EVENT="${GERRIT_EVENT_TYPE:-patch-created}"
    CI_REF="${GERRIT_REFSPEC:-}"
    CI_BASE_REF=""
    CI_HEAD_REF="${GERRIT_REFSPEC:-}"
    CI_COMMIT="${GERRIT_PATCHSET_REVISION:-$(_git_commit)}"
    CI_COMMIT_SHORT="${CI_COMMIT:0:7}"
    CI_BRANCH="${GERRIT_BRANCH:-$(_git_branch)}"
    CI_TARGET="${GERRIT_BRANCH:-}"
    CI_SLUG="${GERRIT_PROJECT:-}"
    CI_URL="${GERRIT_CHANGE_URL:-}"
    CI_CHANGE="${GERRIT_CHANGE_NUMBER:-}"
    CI_RUN="${BUILD_NUMBER:-}"
    CI_RUN_URL="${BUILD_URL:-}"
    CI_ACTOR="${GERRIT_CHANGE_OWNER_NAME:-$(_git_author)}"
    CI_EMAIL="${GERRIT_CHANGE_OWNER_EMAIL:-$(_git_email)}"
    CI_PIPELINE="${JOB_NAME:-ci}"
    CI_JOB="${JOB_NAME:-}"
    CI_EVENT_PATH=""
    # Gerrit-specific extras
    export GERRIT_CHANGE_ID="${GERRIT_CHANGE_ID:-}"
    export GERRIT_PATCHSET_NUMBER="${GERRIT_PATCHSET_NUMBER:-}"
    export GERRIT_TOPIC="${GERRIT_TOPIC:-}"

else
    # ---- Local / unknown CI ----
    CI_SYSTEM="local"
    CI_EVENT="manual"
    CI_REF="refs/heads/$(_git_branch)"
    CI_BASE_REF=""
    CI_HEAD_REF=""
    CI_COMMIT="$(_git_commit)"
    CI_COMMIT_SHORT="$(_git_short)"
    CI_BRANCH="$(_git_branch)"
    CI_TARGET=""
    CI_SLUG=""
    CI_URL=""
    CI_CHANGE=""
    CI_RUN="$$"
    CI_RUN_URL=""
    CI_ACTOR="$(_git_author)"
    CI_EMAIL="$(_git_email)"
    CI_PIPELINE="local"
    CI_JOB=""
    CI_EVENT_PATH=""
fi

# ── Computed / shared fields ────────────────────────────────────
CI_MSG="$(_git_subject)"
CI_TAG="$(_git_tag)"
CI_TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
CI_WORKSPACE="$REPO_ROOT"
CI_DIR="$(basename "$REPO_ROOT")"
CI_PARENT="$(basename "$(dirname "$REPO_ROOT")")"

# Derive owner/repo from slug, fall back to directory structure
CI_OWNER="${CI_SLUG%%/*}"
CI_REPO="${CI_SLUG##*/}"
[[ -z "$CI_OWNER" ]] && CI_OWNER="$CI_PARENT"
[[ -z "$CI_REPO" ]]  && CI_REPO="$CI_DIR"
[[ -z "$CI_SLUG" ]]  && CI_SLUG="${CI_OWNER}/${CI_REPO}"

# Slugified names for deployment / environment use
CI_ENV="$(_slug "$CI_BRANCH")"
CI_STATE="$( [[ "$CI_EVENT" == "delete" ]] && echo "absent" || echo "present" )"
CI_TRACK="${CI_TAG:-$CI_ENV}"

# ── Export everything ───────────────────────────────────────────
_CI_VARS=(
    CI_SYSTEM
    CI_EVENT
    CI_EVENT_PATH
    CI_STATE
    CI_REF
    CI_BASE_REF
    CI_HEAD_REF
    CI_COMMIT
    CI_COMMIT_SHORT
    CI_MSG
    CI_BRANCH
    CI_TARGET
    CI_ENV
    CI_TRACK
    CI_TAG
    CI_OWNER
    CI_REPO
    CI_SLUG
    CI_URL
    CI_CHANGE
    CI_RUN
    CI_RUN_URL
    CI_ACTOR
    CI_EMAIL
    CI_PIPELINE
    CI_JOB
    CI_TIMESTAMP
    CI_WORKSPACE
    CI_DIR
    CI_PARENT
)

for _v in "${_CI_VARS[@]}"; do
    export "$_v"
done

# ── Print summary ───────────────────────────────────────────────
# Skip the human-readable summary when emitting machine-readable env output.
case "${1:-}" in --env|--env-make) _skip_summary=1 ;; *) _skip_summary=0 ;; esac
if [[ "$_skip_summary" -eq 0 ]]; then
echo "┌──────────────────────────────────────────────────┐"
echo "│  ci-run.sh                                       │"
echo "├──────────────────────────────────────────────────┤"
printf "│  %-12s %s\n" "system:"    "$CI_SYSTEM"
printf "│  %-12s %s\n" "event:"     "$CI_EVENT"
printf "│  %-12s %s\n" "state:"     "$CI_STATE"
printf "│  %-12s %s\n" "slug:"      "$CI_SLUG"
printf "│  %-12s %s\n" "branch:"    "$CI_BRANCH"
[[ -n "$CI_TARGET" ]] && \
printf "│  %-12s %s\n" "target:"    "$CI_TARGET"
printf "│  %-12s %s\n" "env:"       "$CI_ENV"
printf "│  %-12s %s\n" "track:"     "$CI_TRACK"
printf "│  %-12s %s\n" "commit:"    "$CI_COMMIT_SHORT"
[[ -n "$CI_CHANGE" ]] && \
printf "│  %-12s %s\n" "change:"    "$CI_CHANGE"
[[ -n "$CI_TAG" ]] && \
printf "│  %-12s %s\n" "tag:"       "$CI_TAG"
printf "│  %-12s %s\n" "actor:"     "$CI_ACTOR"
printf "│  %-12s %s\n" "time:"      "$CI_TIMESTAMP"
printf "│  %-12s %s\n" "workspace:" "$CI_WORKSPACE"
echo "└──────────────────────────────────────────────────┘"
fi

# ── Mode select ─────────────────────────────────────────────────
case "${1:-}" in
    --dry-run)
        echo "[dry-run] skipping make ci"
        exit 0
        ;;
    --env)
        # When invoked from Make (`$(eval $(shell …))`), Make's $(shell)
        # collapses newlines to spaces, which mangles multi-line exports.
        # Detect that case via MAKELEVEL and emit a single `include` line
        # pointing at a file we just wrote — $(eval …) then runs it as
        # a Make include, getting real line-by-line parsing.
        if [[ -n "${MAKELEVEL:-}" ]]; then
            _mk_escape() { printf '%s' "$1" | sed -e 's/\$/$$/g' -e 's/#/\\#/g'; }
            _mk_file="$REPO_ROOT/.ci.env.mk"
            : > "$_mk_file"
            for _v in "${_CI_VARS[@]}"; do
                printf '%s := %s\n' "$_v" "$(_mk_escape "${!_v}")" >> "$_mk_file"
            done
            echo "include $_mk_file"
            exit 0
        fi
        _escape() { printf '%s' "$1" | sed "s/'/'\\\\''/g"; }
        for _v in "${_CI_VARS[@]}"; do
            echo "export ${_v}='$(_escape "${!_v}")'"
        done
        exit 0
        ;;
    --env-make)
        # Emit Make-compatible `VAR := value` lines. Escape `$` and `#`
        # so Make doesn't interpret them as variable refs or comments.
        _mk_escape() { printf '%s' "$1" | sed -e 's/\$/$$/g' -e 's/#/\\#/g'; }
        for _v in "${_CI_VARS[@]}"; do
            echo "${_v} := $(_mk_escape "${!_v}")"
        done
        exit 0
        ;;
    "")
        exec make -C "$REPO_ROOT" ci
        ;;
    *)
        echo "Usage: ci-run.sh [--dry-run|--env]" >&2
        exit 1
        ;;
esac
