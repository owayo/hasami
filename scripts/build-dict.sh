#!/usr/bin/env bash
# 上流の辞書ソースから配布辞書 (dict/*.hsd) を作り直す
#
#   ipadic.hsd                   IPAdic。外国人名の姓・名だけを除く
#   ipadic-neologd.hsd           IPAdic + NEologd + dict/user。repair 一式を適用
#   ipadic-neologd-sudachi.hsd   上の IPAdic + NEologd に SudachiDict を足したもの。repair 一式を適用
#
# 上流はすべて版を固定し、git は commit、ダウンロードは SHA-256 で検証する。
# 取得物と中間成果物 (repair 前の辞書) は --src の下に置き、取得物は 2 回目以降は再取得しない。
# repair 前の中間辞書は、`hasami repair` を手で試し直すときの入力に使える。
#
# usage: scripts/build-dict.sh [options] [ipadic|neologd|sudachi ...]
#   対象を省くと 3 辞書すべてを作る。neologd は ipadic の、sudachi は neologd の中間辞書を使う
#   (無ければ先に作る)。
#
# options:
#   --out DIR      配布辞書の出力先 (既定: dict)
#   --src DIR      取得物と中間成果物の置き場所 (既定: .dict-src)
#   --hasami PATH  使う hasami のバイナリ (既定: cargo build --release して target/release/hasami)
#   -h, --help     このヘルプを出す
set -euo pipefail

cd "$(dirname "$0")/.."

# ---------------------------------------------------------------- 上流の版

IPADIC_REPO=https://github.com/taku910/mecab.git
IPADIC_COMMIT=61b90ba6e669dc2d7d533d4a80d206f3b31d52b1 # 2025-02-22

NEOLOGD_REPO=https://github.com/neologd/mecab-ipadic-neologd.git
NEOLOGD_COMMIT=abc61e33d8be3d0ead202e6b1df064c72d5ccf11 # 2023-12-27
# 形容詞の表現・日付・数量の網羅的な生成エントリは誤分割を増やすので入れない
NEOLOGD_EXCLUDE=(
  neologd-adjective-exp-dict-seed.20151126.csv
  neologd-date-time-infreq-dict-seed.20190415.csv
  neologd-quantity-infreq-dict-seed.20190415.csv
)

SUDACHI_VERSION=20260723
SUDACHI_URL=https://sudachi.s3.ap-northeast-1.amazonaws.com/sudachidict-raw/v1/$SUDACHI_VERSION
SUDACHI_FILES=(
  "small_lex.zip e49936daef64043657752eb9f4ada912cf0316e26dd158a6200c770e2f93e706"
  "core_lex.zip d8ed376d8ff368226314a43151ab02591378dd344bff017d8a93ae9839212edf"
)
# SudachiDict から取り込む範囲 (scripts/convert_sudachi_raw.py の --scope)。内容語 (名詞・固有名詞・形状詞・
# 連体詞・副詞・接続詞・感動詞・動詞・形容詞) と記号。助詞・助動詞・数詞・接頭辞・接尾辞・代名詞は、IPAdic の語を
# 押しのけて誤分割・誤読を増やすので入れない (取り込み範囲ごとの比較は README の「辞書のローカルビルド」)
SUDACHI_SCOPE=content-symbol

# 辞書のメタデータ (hasami info で見える) に残す上流の版
SOURCE_IPADIC=ipadic@${IPADIC_COMMIT:0:12}
SOURCE_NEOLOGD=neologd@${NEOLOGD_COMMIT:0:12}
SOURCE_SUDACHI=sudachi-raw@$SUDACHI_VERSION/$SUDACHI_SCOPE

# ---------------------------------------------------------------- 引数

OUT=dict
SRC=.dict-src
HASAMI=
TARGETS=()

usage() { sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT=$2; shift 2 ;;
    --src) SRC=$2; shift 2 ;;
    --hasami) HASAMI=$2; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    ipadic | neologd | sudachi) TARGETS+=("$1"); shift ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done
[ ${#TARGETS[@]} -eq 0 ] && TARGETS=(ipadic neologd sudachi)

WORK=$SRC/build
mkdir -p "$OUT" "$WORK"

log() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

if [ -z "$HASAMI" ]; then
  log "cargo build --release"
  cargo build --release --quiet
  HASAMI=./target/release/hasami
fi

# ---------------------------------------------------------------- 取得

# git リポジトリの一部 (sparse checkout) を指定の commit で取り出す
fetch_git() {
  local repo=$1 commit=$2 dir=$3 path=$4
  if [ -d "$dir/.git" ] && [ "$(git -C "$dir" rev-parse HEAD)" = "$commit" ]; then
    return
  fi
  log "fetch $repo @ ${commit:0:12} ($path)"
  rm -rf "$dir"
  git clone --quiet --filter=blob:none --no-checkout "$repo" "$dir"
  git -C "$dir" sparse-checkout set "$path"
  git -C "$dir" checkout --quiet "$commit"
}

# URL のファイルを取得し、SHA-256 を検証する
fetch_url() {
  local url=$1 expected=$2 file=$3
  if [ ! -f "$file" ] || [ "$(sha256 "$file")" != "$expected" ]; then
    log "download $url"
    curl -fsSL --retry 3 -o "$file.part" "$url"
    mv "$file.part" "$file"
  fi
  local actual
  actual=$(sha256 "$file")
  if [ "$actual" != "$expected" ]; then
    echo "SHA-256 mismatch: $file (expected $expected, got $actual)" >&2
    exit 1
  fi
}

# 中間辞書を別名に書いてから差し替える (読み込み中のプロセスの mmap を壊さない)
install_dict() {
  local tmp=$1 dest=$2
  mv "$tmp" "$dest"
  log "wrote $dest ($("$HASAMI" info --dict "$dest" | grep -E 'Entries' | tr -d '\n'))"
}

# ---------------------------------------------------------------- repair の引数

REMOVE_ARGS=()
for f in dict/user-remove/*.csv; do
  REMOVE_ARGS+=(--remove "$f")
done
# NEologd・SudachiDict を含む辞書に掛ける repair 一式。dict/user の追加語は削除の後に足す
FULL_REPAIR=(
  --drop-invalid-context-ids
  --drop-ortho-variants
  --drop-numeral-misreadings
  "${REMOVE_ARGS[@]}"
  --merge dict/user
)

# ---------------------------------------------------------------- 辞書

build_ipadic() {
  fetch_git "$IPADIC_REPO" "$IPADIC_COMMIT" "$SRC/mecab" mecab-ipadic
  # 記号の未知語の扱いと、EUC-JP の変換差を埋める別表記を整えたソースを作る (scripts/prepare_ipadic.py)
  local patch
  patch=$(python3 scripts/prepare_ipadic.py "$SRC/mecab/mecab-ipadic" "$WORK/ipadic-src")
  log "build ipadic ($patch)"
  "$HASAMI" build --input "$WORK/ipadic-src" --output "$WORK/ipadic.base.hsd" \
    --meta name=ipadic --meta "sources=$SOURCE_IPADIC" --meta "ipadic_patch=$patch"
  # IPAdic 単体は発音の修復を掛けない (記号の読みを残す)。外国人名の姓・名だけを除く
  "$HASAMI" repair --dict "$WORK/ipadic.base.hsd" --output "$WORK/ipadic.tmp.hsd" \
    --drop-invalid-context-ids --no-pronunciation-repair \
    --remove dict/user-remove/foreign-names.csv
  install_dict "$WORK/ipadic.tmp.hsd" "$OUT/ipadic.hsd"
}

prepare_neologd_seed() {
  fetch_git "$NEOLOGD_REPO" "$NEOLOGD_COMMIT" "$SRC/mecab-ipadic-neologd" seed
  local seed=$SRC/neologd-seed
  rm -rf "$seed"
  mkdir -p "$seed"
  local xzfile base
  for xzfile in "$SRC"/mecab-ipadic-neologd/seed/*.csv.xz; do
    base=$(basename "$xzfile" .xz)
    if printf '%s\n' "${NEOLOGD_EXCLUDE[@]}" | grep -qxF "$base"; then
      log "skip $base"
      continue
    fi
    xz -dc "$xzfile" >"$seed/$base"
  done
}

build_neologd() {
  [ -f "$WORK/ipadic.base.hsd" ] || build_ipadic
  prepare_neologd_seed
  log "merge neologd"
  "$HASAMI" merge --dict "$WORK/ipadic.base.hsd" --input "$SRC/neologd-seed" \
    --output "$WORK/ipadic-neologd.base.hsd" \
    --meta name=ipadic-neologd --meta "sources=$SOURCE_IPADIC,$SOURCE_NEOLOGD"
  "$HASAMI" repair --dict "$WORK/ipadic-neologd.base.hsd" --output "$WORK/ipadic-neologd.tmp.hsd" \
    "${FULL_REPAIR[@]}"
  install_dict "$WORK/ipadic-neologd.tmp.hsd" "$OUT/ipadic-neologd.hsd"
}

build_sudachi() {
  [ -f "$WORK/ipadic-neologd.base.hsd" ] || build_neologd
  # IPAdic・NEologd に既にある語を除くため、両方のソースを参照する
  [ -d "$SRC/neologd-seed" ] || prepare_neologd_seed
  local raw=$SRC/sudachi-raw/$SUDACHI_VERSION entry name hash
  mkdir -p "$raw"
  for entry in "${SUDACHI_FILES[@]}"; do
    read -r name hash <<<"$entry"
    fetch_url "$SUDACHI_URL/$name" "$hash" "$raw/$name"
    unzip -oq "$raw/$name" -d "$raw"
  done
  log "convert sudachi (scope: $SUDACHI_SCOPE)"
  python3 scripts/convert_sudachi_raw.py \
    --lex "$raw/small_lex.csv" \
    --lex "$raw/core_lex.csv" \
    --ipadic-dir "$SRC/mecab/mecab-ipadic" \
    --exclude-existing "$SRC/mecab/mecab-ipadic" \
    --exclude-existing "$SRC/neologd-seed" \
    --exclude-existing dict/user \
    --scope "$SUDACHI_SCOPE" \
    --output "$WORK/sudachi.csv"
  log "merge sudachi"
  "$HASAMI" merge --dict "$WORK/ipadic-neologd.base.hsd" --input "$WORK/sudachi.csv" \
    --output "$WORK/ipadic-neologd-sudachi.base.hsd" \
    --meta name=ipadic-neologd-sudachi \
    --meta "sources=$SOURCE_IPADIC,$SOURCE_NEOLOGD,$SOURCE_SUDACHI"
  "$HASAMI" repair --dict "$WORK/ipadic-neologd-sudachi.base.hsd" \
    --output "$WORK/ipadic-neologd-sudachi.tmp.hsd" "${FULL_REPAIR[@]}"
  install_dict "$WORK/ipadic-neologd-sudachi.tmp.hsd" "$OUT/ipadic-neologd-sudachi.hsd"
}

for t in "${TARGETS[@]}"; do
  "build_$t"
done
log "done"
