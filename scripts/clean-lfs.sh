#!/usr/bin/env bash
# 手元の Git LFS の実体のうち、最新 (いまチェックアウトしているコミット) が使うもの以外を消す
#
#   1. git lfs prune --recent --verify-remote --when-unverified=halt
#      残すのは、いまのコミット・未 push のコミット・stash が参照する実体だけ。
#      コミットから参照される実体は、リモートにあることを確かめてから消す (確かめられない実体が
#      あれば消さずに止まる)。どのコミットからも参照されない実体 (コミットしなかった中間版) は
#      リモートに無いので、確かめずに消す。古い版が要るときは `git lfs fetch <ref>` で取り直せる
#   2. 転送途中で残った一時ファイル (git lfs env の TempDir) は、git-lfs がどのコマンドの実行時にも
#      1 時間より古いものを自分で消す。このスクリプトも最初に git lfs env を呼ぶので、そこで消える
#      (1 時間より新しいものは使用中かもしれないので残り、次に git-lfs を使ったときに消える)
#
# usage: scripts/clean-lfs.sh [-n|--dry-run]
#   -n, --dry-run  消す対象と容量を表示するだけで、何も消さない
set -euo pipefail

cd "$(dirname "$0")/.."

usage() { sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }

DRY_RUN=0
while [ $# -gt 0 ]; do
  case "$1" in
    -n | --dry-run) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

lfs_env() { git lfs env 2>/dev/null | sed -n "s/^$1=//p"; }
MEDIA_DIR=$(lfs_env LocalMediaDir)
TEMP_DIR=$(lfs_env TempDir)
if [ -z "$MEDIA_DIR" ] || [ -z "$TEMP_DIR" ]; then
  echo "git lfs env から LFS の置き場所を取得できない (git-lfs が入っているか確認する)" >&2
  exit 1
fi

size_of() { if [ -d "$1" ]; then du -sh "$1" | cut -f1; else echo 0; fi; }
report() {
  echo "$1: objects $(size_of "$MEDIA_DIR") / tmp $(size_of "$TEMP_DIR")"
}

report "before"

prune_args=(--recent --verify-remote --when-unverified=halt)
[ "$DRY_RUN" = 1 ] && prune_args+=(--dry-run)
git lfs prune "${prune_args[@]}"

if [ -d "$TEMP_DIR" ]; then
  kept=$(find "$TEMP_DIR" -type f | wc -l | tr -d ' ')
  if [ "$kept" != 0 ]; then
    echo "tmp: ${kept} files newer than 1 hour are kept (git-lfs removes them once they are older)"
  fi
fi

[ "$DRY_RUN" = 1 ] || report "after"
