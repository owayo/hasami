#!/usr/bin/env bash
# build-dict.sh からsourceする上流Gitキャッシュの取得処理。
# 既存pathは削除しない。独立checkoutと入力を検査してから、必要なら固定版へ移る。

source_cache_git() {
  git --no-optional-locks -c core.fsmonitor=false -c core.untrackedCache=false "$@"
}

source_cache_error() {
  printf 'dictionary source cache: %s (%s)\n' "$2" "$1" >&2
  return 1
}

source_cache_root() {
  local dir=$1 actual top gitdir common expected
  if [ -L "$dir" ] || [ ! -d "$dir" ] || [ -L "$dir/.git" ] || [ ! -d "$dir/.git" ]; then
    source_cache_error "$dir" 'expected an independent checkout with a .git directory'
    return 1
  fi
  actual=$(cd -- "$dir" && pwd -P) || return 1
  expected=$(cd -- "$dir/.git" && pwd -P) || return 1
  top=$(source_cache_git -C "$dir" rev-parse --show-toplevel 2>/dev/null) || {
    source_cache_error "$dir" 'cannot resolve checkout root'; return 1;
  }
  gitdir=$(source_cache_git -C "$dir" rev-parse --absolute-git-dir) || return 1
  common=$(source_cache_git -C "$dir" rev-parse --path-format=absolute --git-common-dir) || return 1
  if [ "$(cd -- "$top" && pwd -P)" != "$actual" ] ||
     [ "$(cd -- "$gitdir" && pwd -P)" != "$expected" ] ||
     [ "$(cd -- "$common" && pwd -P)" != "$expected" ]; then
    source_cache_error "$dir" 'checkout root or shared Git directory is unexpected'
    return 1
  fi
}

source_cache_clean() {
  local dir=$1 status
  status=$(source_cache_git -C "$dir" status --porcelain=v1 --untracked-files=all --ignored --ignore-submodules=none) || return 1
  if [ -n "$status" ]; then
    source_cache_error "$dir" 'dirty checkout: tracked, staged, untracked or ignored files'
    return 1
  fi
  # assume-unchangedはstatusに出ない変更を隠す。sparseのskip-worktreeは入力外だけ許す。
  (
    set -o pipefail
    source_cache_git -C "$dir" ls-files -v -z |
      while IFS= read -r -d '' item; do
        case "${item:0:1}" in
          [a-z]) source_cache_error "$dir" 'assume-unchanged index flags are unsupported'; return 1 ;;
        esac
      done
  )
}

source_cache_inputs() {
  local dir=$1 commit=$2 path=$3 kind
  kind=$(source_cache_git -C "$dir" cat-file -t "$commit:$path" 2>/dev/null) || {
    source_cache_error "$dir" "required input subtree is missing: $path"; return 1;
  }
  if [ "$kind" != tree ] || [ ! -d "$dir/$path" ] || [ -L "$dir/$path" ]; then
    source_cache_error "$dir" "required input is not a materialized directory: $path"
    return 1
  fi
  (
    set -o pipefail
    source_cache_git -C "$dir" ls-files -v -z -- "$path" |
      while IFS= read -r -d '' item; do
        if [ "${item:0:1}" != H ]; then
          source_cache_error "$dir" "required input has hidden index flags: $path"
          return 1
        fi
      done || return 1
    source_cache_git -C "$dir" ls-tree -r -z --name-only "$commit" -- "$path" | {
      local count=0 file
      while IFS= read -r -d '' file; do
        if [ ! -f "$dir/$file" ] || [ -L "$dir/$file" ]; then
          source_cache_error "$dir" "required tracked input is missing or not a regular file: $path"
          return 1
        fi
        count=$((count + 1))
      done
      if [ "$count" -eq 0 ]; then
        source_cache_error "$dir" "required input subtree is empty: $path"
        return 1
      fi
    }
  )
}

source_cache_keep_head() {
  local dir=$1 head=$2 ref=refs/hasami/source-cache/$2 saved
  if saved=$(source_cache_git -C "$dir" rev-parse --verify "$ref" 2>/dev/null); then
    if [ "$saved" != "$head" ]; then
      source_cache_error "$dir" 'backup ref already points to an unexpected commit'
      return 1
    fi
  else
    source_cache_git -C "$dir" update-ref "$ref" "$head" '' || return 1
  fi
}

# pathは現在の2ソースで使う、一階層の入力ディレクトリ (mecab-ipadic / seed)。
fetch_git() {
  local repo=$1 commit=$2 dir=$3 path=$4 head variable
  for variable in GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES; do
    if [ -n "${!variable-}" ]; then
      source_cache_error "$dir" "Git repository override is set: $variable"
      return 1
    fi
  done
  if [[ ! "$commit" =~ ^[0-9a-f]{40}$ || ! "$path" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
    source_cache_error "$dir" 'expected a full commit SHA and a simple input directory'
    return 1
  fi
  if [ -e "$dir" ] || [ -L "$dir" ]; then
    source_cache_root "$dir" || return 1
    source_cache_clean "$dir" || return 1
    head=$(source_cache_git -C "$dir" rev-parse --verify 'HEAD^{commit}') || return 1
    source_cache_inputs "$dir" "$head" "$path" || return 1
    [ "$head" = "$commit" ] && return 0
    source_cache_keep_head "$dir" "$head" || return 1
  else
    log "clone $repo ($path)"
    git clone --quiet --filter=blob:none --no-checkout -- "$repo" "$dir" || return 1
    source_cache_git -C "$dir" sparse-checkout set "$path" || return 1
  fi
  if ! source_cache_git -C "$dir" cat-file -e "$commit^{commit}" 2>/dev/null; then
    log "fetch $repo @ ${commit:0:12} ($path)"
    source_cache_git -C "$dir" fetch --quiet --no-tags --no-write-fetch-head -- "$repo" "$commit" || return 1
  fi
  source_cache_git -C "$dir" -c core.hooksPath=/dev/null checkout --quiet --detach "$commit" || return 1
  source_cache_root "$dir" || return 1
  source_cache_clean "$dir" || return 1
  head=$(source_cache_git -C "$dir" rev-parse --verify 'HEAD^{commit}') || return 1
  [ "$head" = "$commit" ] || { source_cache_error "$dir" 'checked out an unexpected commit'; return 1; }
  source_cache_inputs "$dir" "$commit" "$path"
}
