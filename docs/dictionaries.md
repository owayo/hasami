# 辞書

hasami が読む辞書（`.hsd`）の取り方・置き場所・作り方・形式と、配布辞書のライセンスをまとめる。
誤読や誤った品詞の元になるエントリを直す `hasami repair` は [dictionary-repair.md](dictionary-repair.md) にある。

## 配布辞書

ビルド済みの辞書（.hsd）は、リリースの添付ファイルとして配る（リポジトリには置かない）。

| 辞書 | 内容 | 大きさ | 推奨用途 |
|------|------|------:|---------|
| `ipadic` | IPAdic 単体 | 16.5 MB | 軽量・基本用途 |
| `ipadic-neologd` | IPAdic + NEologd | 206.6 MB | 新語・固有名詞対応 |
| `ipadic-neologd-sudachi` | IPAdic + NEologd + SudachiDict | 221.0 MB | **推奨**（最大語彙） |

大きさは v5 形式のローカル再構築値（MB = 1,000,000B）。v5 を含むリリースの公開前にソースから使う場合は、
`make dict` で辞書も作り直す。公開済みの v4 辞書は v5 のリーダーでは読めない。

`hasami dict download` は、実行している hasami と同じ版のリリースから辞書を取り、置き場所に置く。
辞書の形式や repair は版ごとに変わりうるので、既定では版をそろえる。

```bash
hasami dict download                  # 推奨辞書（ipadic-neologd-sudachi）
hasami dict download ipadic           # 名前を挙げて取る（--all で 3 辞書すべて）
hasami dict download --tag v26.9.104  # 別の版のリリースから
hasami dict download --base-url https://mirror.example.com/hasami/v26.9.104   # ミラーから
hasami dict list                      # 置き場所の辞書と状態（通信しない。--remote でリリースの目録と照合）
hasami dict path                      # tokenize が --dict なしで使う辞書のパス（-d "$(hasami dict path)"）
hasami dict install ipadic.hsd.zst    # 手で持ち込んだ辞書を確かめて置く（ネットワークに出られない環境向け）
```

- zstd で圧縮した版（`<名前>.hsd.zst`。3 分の 1 ほど）を取って展開し、リリースの目録（`dictionaries.json`）の
  大きさと SHA-256 で、受け取ったものと展開したものの両方を確かめる。辞書として読めること（形式の版が合うこと）も
  確かめてから、同じディレクトリの一時ファイルを rename して置く。途中で止めても、壊れた辞書を置き場所に残さない。
  圧縮版が HTTP 404 の場合は、その旨を表示して非圧縮版に切り替える。ほかの通信エラーや検証・展開の失敗では
  切り替えない。`--uncompressed` で最初から生の辞書を取る
- 正しいファイルがすでにあれば通信しない。中身の違うファイル（別の版など）は `--force` を付けたときだけ置き換える
  （取得に失敗したら元のファイルを残す）
- `--json` で結果（置いたパス・大きさ・SHA-256）を JSON で出す。`--quiet` で進み具合と結果を出さない
- プロキシは環境変数（`HTTPS_PROXY`・`NO_PROXY`）に従う。証明書は OS の証明書ストアで検証する（社内の CA を
  OS に入れた環境でも通る）
- `hasami dict install` は、同じディレクトリにリリースの `dictionaries.json` があれば（`--catalog` でも渡せる）
  その大きさと SHA-256 で確かめる。なければ辞書全体を検証する

置き場所（`tokenize` が `--dict` なしで辞書を探し、`hasami dict download` が辞書を置くディレクトリ）は次の順に決まる。
複数の辞書があれば、推奨順（ipadic-neologd-sudachi → ipadic-neologd → ipadic → そのほかの名前順）の最初を使う。

1. 環境変数 `HASAMI_DATA_DIR`
2. `$XDG_DATA_HOME/hasami/`
3. `%LOCALAPPDATA%\hasami\`（Windows）
4. `~/.local/share/hasami/`

リリースの添付ファイルは直接取ってもよい（URL は `https://github.com/owayo/hasami/releases/download/<タグ>/<ファイル名>`）。

```bash
base=https://github.com/owayo/hasami/releases/download/v26.9.104
mkdir -p ~/.local/share/hasami && cd ~/.local/share/hasami
curl -fL --remote-name-all "$base/ipadic-neologd-sudachi.hsd" "$base/SHA256SUMS"
grep ' ipadic-neologd-sudachi.hsd$' SHA256SUMS | sha256sum --check --strict -   # macOS は shasum -a 256 -c
```

| 添付ファイル | 中身 |
| --- | --- |
| `<名前>.hsd` | 配布辞書 |
| `<名前>.hsd.zst` | 同じ辞書を zstd で圧縮したもの |
| `dictionaries.json` | 目録（hasami の版・辞書の形式の版・推奨の辞書・各辞書の大きさと SHA-256） |
| `SHA256SUMS` | すべての添付ファイルの SHA-256 |
| `THIRD_PARTY_LICENSES.md` | 辞書のライセンス（辞書を再配布するときは一緒に配る） |

リリースの辞書は、Release ワークフローがタグのソースから `scripts/build-dict.sh` で作り、全件の検証・配布辞書の
受け入れテスト・文分割の例外表との照合を通したものだけを添付する。

以下の辞書は配布していないが、ローカルでビルドできる。

| ファイル | 内容 | ビルドコマンド |
|---------|------|--------------|
| `dict/unidic-cwj.hsd` | UniDic CWJ（書き言葉） | `make dict-unidic-cwj` |
| `dict/unidic-csj.hsd` | UniDic CSJ（話し言葉） | `make dict-unidic-csj` |

## 辞書のローカルビルド

配布辞書 3 つは `scripts/build-dict.sh` が上流のソースから作る。`git`・`curl`・`xz`・`unzip` と、`mise.toml` の
Python（`mise install`）が要る。

```bash
# 配布辞書 3 つをすべて作る（dict/ に書き出す）
make dict                 # = scripts/build-dict.sh

# 個別に作る
make dict-ipadic          # IPAdic のみ
make dict-neologd         # IPAdic + NEologd
make dict-sudachi         # IPAdic + NEologd + SudachiDict（推奨）

# 配布しない辞書
make dict-unidic-cwj      # UniDic CWJ（書き言葉）
make dict-unidic-csj      # UniDic CSJ（話し言葉）

# ダウンロードしたソースと中間成果物を削除
make dict-clean
```

| 辞書 | 作り方 |
| --- | --- |
| `ipadic.hsd` | IPAdic を `scripts/prepare_ipadic.py` で整えて build し、外国人名の姓・名だけを除く（発音の修復は掛けない） |
| `ipadic-neologd.hsd` | IPAdic に NEologd の seed を merge し、repair 一式（範囲外 ID・表記ゆれ・漢数字の人名・`dict/user-remove/*.csv`・文や句の名詞・数と単位の組の名詞・一般語の固有名詞の降格）を掛けてから `dict/user/*.csv` を足す |
| `ipadic-neologd-sudachi.hsd` | IPAdic + NEologd に SudachiDict の raw 辞書を `scripts/convert_sudachi_raw.py` で変換して merge し、同じ repair 一式を掛ける |

`scripts/prepare_ipadic.py` は上流の IPAdic を書き換えずに、次の 5 点を変えたソースを作る（何を変えたかは
辞書のメタデータ `ipadic_patch` に残る）。

- **記号の未知語**: IPAdic の char.def は `— 。 、 「 ♪ ⇒` などを SYMBOL（まとめて 1 語）にし、unk.def はその未知語を
  「名詞,サ変接続」にする。このままだと辞書に無い記号の並びが句点ごと 1 つの名詞になる（「楽しみたい——。」の「——。」）。
  SYMBOL を「既知語がある位置では未知語を作らず、作るときも 1 文字ずつ」「記号,一般」に変える
- **未知語の候補**: hasami は char.def の group・length を MeCab と同じ意味で読む（同じ文字種の並び全体と、1〜length 字の
  接頭辞を未知語の候補にする）。辞書に無いカタカナ語は 1 語になる（「ブログ」「モチベーション」）。IPAdic の値のままだとひらがなの並びまで 1 つの名詞になるので、HIRAGANA を 0 0 2、ALPHA・NUMERIC を
  1 1 1（英数字を 1 文字にも分けられる）にし、中黒 `・` と `×` `÷` を SYMBOL にする（「ジョン・カーター」「microSD×C」を
  つなげない）。ニュース 3.3 万行で MeCab と分かち書きが一致する行は 64.7% から 88.5% になった（IPAdic）。カタカナの複合語を
  既知語に分ける規則（[architecture.md](architecture.md) の「未知語」）を足した後は 87.7%（MeCab が 1 つの未知語にする複合語を分けるため）
- **EUC-JP の変換差**: IPAdic の CSV は EUC-JP で、ダッシュ・波ダッシュ・マイナスなど 7 字は変換表によって
  写し先が分かれる。hasami は JIS の対応表どおり（MeCab と同じ）「—」「〜」「−」に写し、Windows 由来の文章が使う
  「―」「～」「－」の別表記を表層形に足す（33 語。「あ〜」と「あ～」のどちらでも感動詞「アー」になる）
- **空白の文字**: IPAdic の char.def は SPACE に `0x00D0`（Ð）を入れている。ほかの行（タブ・改行）から見て復帰 `0x000D` の
  書き間違いなので `0x000D` に直す（空白は読み飛ばすので、そのままだと「Ð」が解析結果から消える）
- **単位の記号**: 全角の「％」は 名詞,接尾,助数詞 の語だが、半角の `%` と `‰` `℃` `℉` `°`（`°C` `°F`）、CJK 互換文字の単位
  （`㎏` `㎞` `㌢` `㍍` など 170 余り）は辞書に無く、未知の記号（記号,一般）になって句読点と同じ扱いになる。「％」と同じ
  品詞・文脈 ID・コストの語として読み付きで足す（`㎏` はキログラム、`㌢` はセンチ、`℃` はド）

SudachiDict は内容語（名詞・固有名詞・形状詞・連体詞・副詞・接続詞・感動詞・動詞・形容詞）と記号だけを取り込み、
IPAdic・NEologd・`dict/user` に表層形がある語は落とす。品詞は IPAdic 体系に写し、文脈 ID は IPAdic の left-id.def から
引く（対応する ID が無い品詞・活用形は取り込まない）。原形は SudachiDict の辞書形なので、活用語の原形が正しくなる
（「誤っ」→「誤る」、「示し」→「示す」、「読み込み」→「読み込む」）。取り込み範囲はニュース 2 万行で比べて決めた。

| 取り込む範囲 | 追加語数 | 読みが変わる行 | 解析時間（IPAdic + NEologd 比） |
| --- | --- | --- | --- |
| 旧方式（変換済み CSV を範囲外 ID ごと取り込み） | +179 万 | 49.3% | 1.75〜1.9 倍 |
| 全品詞・既存語との重複を残す | +141 万 | 37.8% | 1.22〜1.29 倍 |
| 名詞・表層形が既存語と同じなら落とす | +24 万 | 5.6% | 1.0 倍 |
| **内容語 + 記号・表層形が同じなら落とす（採用）** | +40 万 | 7.0% | 1.0〜1.03 倍 |

採用した範囲で読みが変わった箇所を無作為に 60 件見ると、改善 48・悪化 5・同等 7 だった（改善は英単語の読み
「cafe→カフェ」、複合語「加齢→カレイ」、半角記号が名詞でなく記号になる、など）。

上流はすべて版を固定している（IPAdic・NEologd は git の commit、SudachiDict はダウンロードの SHA-256）。
取得物は `.dict-src/` に置き、2 回目以降は再取得しない。中間成果物（repair を掛ける前の辞書、SudachiDict の
変換結果など）は実行ごとの作業ディレクトリに作って終了時に消すので、`dict/` の配布辞書のほかには残らない。
repair を手で試し直すために repair 前の辞書が要るときは、`scripts/build-dict.sh --keep-intermediate` で
`.dict-src/build/*.base.hsd` に残す。
3 辞書の作り直しは取得済みなら 5 分ほどで終わる（うち SudachiDict の変換が 3 分、最大 RSS は約 3GB）。

辞書を変える PR では、GitHub Actions の Build Dictionaries（`.github/workflows/dict-build.yml`）をブランチで動かすと、
リリースと同じ手順（作る → 全件の検証 → 受け入れテスト → 例外表との照合 → 圧縮 → 目録）で作った辞書を artifact で
受け取れる。辞書はコミットしない。

```bash
gh workflow run dict-build.yml --ref <ブランチ>
gh run download <run-id> -n dictionaries -D /tmp/dicts   # 辞書は /tmp/dicts/dict/ に入る
```

`dict/user/*.csv` には `#` で始まるコメント行を書ける。`#` で始まってもエントリの列数（13 列）が
そろった行は語として読む（NEologd には `#` で始まるハッシュタグの語がある）。

## 辞書の手動構築

MeCab形式の辞書から直接ビルドすることもできます。

```bash
# MeCab形式CSV ディレクトリから辞書をビルド
hasami build --input ./ipadic/ --output dict.hsd

# 既存辞書にCSVを追加マージ
hasami merge --dict dict.hsd --input custom_words.csv
hasami merge --dict dict.hsd --input ./extra_dict/ --output merged.hsd

# メタデータ（辞書名・品詞体系・上流の版）を付ける
hasami build --input ./unidic/ --output unidic.hsd --meta pos_scheme=unidic --meta sources=unidic-cwj@202512

# 辞書の情報と全件検証、MeCab 形式 CSV（活用型・活用形付き）への書き出し
hasami info --dict dict.hsd --verify
hasami export --dict dict.hsd --output lex.csv
```

## 辞書形式 (.hsd)

`.hsd` は v5 形式（64 バイトのヘッダ + セクション表 + 64 バイト境界のセクション）。mmap してそのまま参照するので、
ロードはヘッダと小さな表の検査だけで 1ms 前後、解析で触れたページだけが読み込まれる。実行ファイルに埋め込んだ辞書も、
`Dictionary::from_static` で複製せずに同じく参照する（[rust-api.md](rust-api.md) の「実行ファイルに辞書を埋め込む」）。

- 表層形は文字単位の double-array trie（単独の末尾は圧縮）に持ち、エントリは 1 件 6 バイト
- 品詞・活用型・活用形の組は文法表に共有する。素性レコードにはその番号と読み・発音・原形を重複排除して持ち、最良パスの語だけ復号する
- 辞書の中身はメタデータ（`hasami info` で表示）に名前・品詞体系・上流の版・掛けた repair が残る
- 壊れたファイルはロード時・解析時に `DictError` になる（panic しない）。全件の検査は `hasami info --verify`
- 形式の版が違う `.hsd` は読めない（作り直しを案内するエラーになる）。`scripts/build-dict.sh`（または `hasami build`）で作り直す
- 書き出しは一時ファイルに書いてから rename で差し替える。読み込み中の辞書ファイルを直接書き換えてはいけない

`--prune-dominated`（build / merge / repair）は、同じ表層形・同じ文脈 ID の中でコストが最小でないエントリを除いた
最終辞書を作る。解析結果（1-best）は変わらないが、除いた辞書は merge・repair の入力にできない。配布辞書には掛けていない。

形式の設計・試したこと・計測は [hsd-format.md](hsd-format.md)、v5 の変更点は [hsd-v5.md](hsd-v5.md) にまとめてある。

## 配布辞書のライセンス

リリースに添付している配布辞書は、以下のソースから構築されています。各辞書の著作権・ライセンスにしたがってご利用ください。
ライセンスの全文はリリースにも `THIRD_PARTY_LICENSES.md` として添付しています。辞書を再配布するときは、このファイルを一緒に配ってください。

### IPAdic (`ipadic.hsd`, `ipadic-neologd.hsd`)

[MeCab用IPAdic](https://taku910.github.io/mecab/#download) (2.7.0-20070801) を基に構築。

> Copyright 2000, 2001, 2002, 2003 Nara Institute of Science and Technology. All Rights Reserved.
>
> Use, reproduction, and distribution of this software is permitted. Any copy of this software, whether in its original form or modified, must include both the above copyright notice and the following paragraphs.
>
> Nara Institute of Science and Technology (NAIST), the copyright holders, disclaims all warranties with regard to this software, including all implied warranties of merchantability and fitness, in no event shall NAIST be liable for any special, indirect or consequential damages or any damages whatsoever resulting from loss of use, data or profits, whether in an action of contract, negligence or other tortuous action, arising out of or in connection with the use or performance of this software.
>
> A large portion of the dictionary entries originate from ICOT Free Software. The following conditions for ICOT Free Software apply to the current dictionary as well.
>
> Each User may also freely distribute the Program, whether in its original form or modified, to any third party or parties, PROVIDED that the provisions of Section 3 ("NO WARRANTY") will ALWAYS appear on, or be attached to, the Program, which is distributed substantially in the same form as set out herein and that such intended distribution, if actually made, will neither violate or otherwise contravene any of the laws and regulations of the countries having jurisdiction over the User or the intended distribution itself.

詳細は [NAIST-jdic](https://ja.osdn.net/projects/naist-jdic/) を参照してください。

### mecab-ipadic-NEologd (`ipadic-neologd.hsd`)

[mecab-ipadic-NEologd](https://github.com/neologd/mecab-ipadic-neologd) のシードデータを IPAdic に統合。

> Copyright 2015-2019 Toshinori Sato (@overlast)
>
> Licensed under the Apache License, Version 2.0 (the "License");
> you may not use this file except in compliance with the License.
> You may obtain a copy of the License at
>
>     http://www.apache.org/licenses/LICENSE-2.0
>
> Unless required by applicable law or agreed to in writing, software
> distributed under the License is distributed on an "AS IS" BASIS,
> WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
> See the License for the specific language governing permissions and
> limitations under the License.

NEologd は Apache License 2.0 に加え、IPAdic のライセンス条件も適用されます。

### SudachiDict (`ipadic-neologd-sudachi.hsd`)

[SudachiDict](https://github.com/WorksApplications/SudachiDict) の raw 辞書ソース（small + core）の語彙データを変換して構築。統合辞書では品詞体系と文脈 ID を IPAdic に写しています。

> Copyright (c) 2017-2023 Works Applications Co., Ltd.
>
> Licensed under the Apache License, Version 2.0

SudachiDict には UniDic（BSD 3-Clause）および NEologd（Apache 2.0）由来のデータが含まれます。詳細は [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md) を参照してください。

### UniDic (`dict/unidic-cwj.hsd`, `dict/unidic-csj.hsd` — ローカルビルド時)

[UniDic](https://clrd.ninjal.ac.jp/unidic/) を基に構築。CWJ（現代書き言葉 202512）および CSJ（現代話し言葉 202512）。リポジトリには同梱されず、`make dict-unidic-cwj` / `make dict-unidic-csj` でビルドした場合に適用されます。

> Copyright (c) 2011-2021, The UniDic Consortium
>
> All rights reserved.
>
> UniDic is released under any of the following licenses:
> - GNU General Public License (GPL), version 2.0 or later
> - GNU Lesser General Public License (LGPL), version 2.1 or later
> - BSD License (3-clause)
>
> You may choose any of the above licenses.

UniDic は GPL v2 / LGPL v2.1 / BSD 3-clause のトリプルライセンスです。商用利用の場合は BSD ライセンスを選択できます。

詳細は [UniDic ダウンロードページ](https://clrd.ninjal.ac.jp/unidic/download.html) を参照してください。
