<p align="center">
  <img src="docs/images/app.png" width="128" alt="hasami">
</p>

<h1 align="center">hasami</h1>

<p align="center">
  <strong>高速日本語形態素解析エンジン（Rust製）</strong>
</p>

<p align="center">
  <a href="https://github.com/owayo/hasami/actions/workflows/ci.yml">
    <img alt="CI" src="https://github.com/owayo/hasami/actions/workflows/ci.yml/badge.svg?branch=main">
  </a>
  <a href="https://github.com/owayo/hasami/releases/latest">
    <img alt="Version" src="https://img.shields.io/github/v/release/owayo/hasami">
  </a>
  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/github/license/owayo/hasami">
  </a>
</p>

---

## 概要

外部の形態素解析エンジンに一切依存せず、ゼロベースで構築された高性能・高精度な日本語形態素解析ツールです。

## 特徴

- **高速**: MeCab比 **2.8倍** の解析速度（374,000+ sentences/sec）
- **高精度**: ラティス構築 + Viterbiコスト最小化による最適分割
- **ゼロ依存**: MeCab/Sudachi等の外部エンジンに非依存
- **多言語対応**: Rust / Python / C FFI から利用可能
- **MeCab辞書互換**: IPAdic / UniDic 等のMeCab形式辞書をそのまま利用可能
- **辞書マージ**: 既存辞書にMeCab形式CSVを追加可能
- **高速辞書ロード**: mmap-native バイナリ形式（.hsd）で ~18ms ロード
- **未知語処理**: 文字分類ベースの未知語推定（unk.def対応）

## 動作環境

- **OS**: macOS、Linux
- **Rust**: 1.85以上（ソースからビルドする場合）

## インストール

### ソースからビルド

```bash
make install

# ワークスペース全体をビルド
cargo build --workspace
```

### バイナリダウンロード

[Releases](https://github.com/owayo/hasami/releases) から最新版をダウンロード。

## アーキテクチャ

```
入力テキスト
    ↓
[Double-Array Trie] 辞書引き（共通接頭辞検索）
    ↓
[文字分類] 未知語ノード生成
    ↓
[ラティス構築] 全候補をラティスに展開
    ↓
[Viterbi] 接続コスト + 単語コストで最適パス探索
    ↓
トークン列（形態素解析結果）
```

## 辞書

### ビルド済み辞書

`dict/` ディレクトリにビルド済み辞書（.hsd）が含まれています（Git LFS管理）。

| ファイル | 内容 | 推奨用途 |
|---------|------|---------|
| `dict/ipadic.hsd` | IPAdic 単体 | 軽量・基本用途 |
| `dict/ipadic-neologd.hsd` | IPAdic + NEologd | **推奨**（新語・固有名詞対応） |
| `dict/unidic-cwj.hsd` | UniDic CWJ（書き言葉） | 言語研究・高精度品詞体系 |
| `dict/unidic-csj.hsd` | UniDic CSJ（話し言葉） | 音声認識・会話分析 |

### 辞書のローカルビルド

3つの辞書ソースをダウンロード・統合してビルドします。`curl`, `tar`, `xz`, `unzip`, `python3` が必要です。

```bash
# 全辞書をビルド（IPAdic, IPAdic+NEologd, UniDic CWJ/CSJ）
make dict

# 個別にビルド
make dict-ipadic      # IPAdic のみ
make dict-neologd     # IPAdic + NEologd
make dict-unidic-cwj  # UniDic CWJ（書き言葉）
make dict-unidic-csj  # UniDic CSJ（話し言葉）

# ダウンロードしたソースを削除
make dict-clean
```

ソースファイルは `.dict-src/` にキャッシュされ、2回目以降は再ダウンロードされません。

### 辞書の手動構築

MeCab形式の辞書から直接ビルドすることもできます。

```bash
# MeCab形式CSV ディレクトリから辞書をビルド
hasami build --input ./ipadic/ --output dict.hsd

# 既存辞書にCSVを追加マージ
hasami merge --dict dict.hsd --input custom_words.csv
hasami merge --dict dict.hsd --input ./extra_dict/ --output merged.hsd
```

## 使い方

### 形態素解析 (CLI)

```bash
# MeCab形式で出力
hasami tokenize --dict dict/ipadic-neologd.hsd "東京都に住んでいる"

# 分かち書き
hasami tokenize --dict dict/ipadic-neologd.hsd --format wakachi "東京都に住んでいる"

# JSON形式
hasami tokenize --dict dict/ipadic-neologd.hsd --format json "東京都に住んでいる"

# 標準入力から
echo "形態素解析のテスト" | hasami tokenize --dict dict/ipadic-neologd.hsd
```

### Rust API

```rust
use hasami::Analyzer;

let mut analyzer = Analyzer::load("dict/ipadic-neologd.hsd")?;
let tokens = analyzer.tokenize("東京都に住んでいる");

for token in &tokens {
    println!("{}\t{}\t{}", token.surface, token.pos, token.reading);
}

// バッチ処理
let results = analyzer.tokenize_batch(&["文1", "文2", "文3"]);
```

### Python API

#### インストール

```bash
cd hasami-python
pip install maturin
maturin develop --release
```

#### 基本的な使い方

```python
import hasami

# 辞書をロード
analyzer = hasami.Analyzer("dict/ipadic-neologd.hsd")

# 形態素解析
tokens = analyzer.tokenize("東京都に住んでいる")
for token in tokens:
    print(f"{token.surface}\t{token.pos}")
```

#### 辞書マージ (Python)

```python
builder = hasami.DictBuilder()
builder.load_hsd("dict/ipadic.hsd")    # 既存辞書をロード
builder.add_csv_dir("./extra/")        # CSVを追加
builder.build("merged.hsd")           # 新しい辞書を保存
```

#### 分かち書き

```python
print(analyzer.wakachi("東京都に住んでいる"))
# => 東京都 に 住ん で いる
```

#### Token オブジェクトの属性

```python
token = analyzer.tokenize("猫")[0]
token.surface        # 表層形: "猫"
token.pos            # 品詞: "名詞,一般,*,*"
token.base_form      # 原形: "猫"
token.reading        # 読み: "ネコ"
token.pronunciation  # 発音: "ネコ"
token.start          # 開始バイト位置: 0
token.end            # 終了バイト位置: 3
token.word_cost      # 単語コスト: 3987
token.is_known       # 辞書語かどうか: True
```

### C FFI

```c
#include "hasami.h"

HasamiAnalyzer* analyzer = hasami_new("dict/ipadic-neologd.hsd");
if (!analyzer) {
    fprintf(stderr, "load error: %s\n", hasami_last_error(NULL));
    return 1;
}

HasamiTokenList tokens = hasami_tokenize(analyzer, "東京都に住んでいる");
const char* error = hasami_last_error(analyzer);
if (error) {
    fprintf(stderr, "tokenize error: %s\n", error);
    hasami_free(analyzer);
    return 1;
}

for (uint32_t i = 0; i < tokens.len; i++) {
    printf("%s\t%s\n", tokens.tokens[i].surface, tokens.tokens[i].pos);
}

hasami_free_tokens(tokens);
hasami_free(analyzer);
```

## ベンチマーク

```bash
hasami bench --dict dict/ipadic-neologd.hsd --text "東京都に住んでいる人々が増えている。" --iterations 100000
```

### 解析速度

| エンジン | sentences/sec | MeCab比 |
|----------|--------------|---------|
| MeCab (fugashi) | ~135,000 | 1.00x |
| Sudachi | ~80,600 | 0.60x |
| **hasami** | **~374,000** | **2.77x** |

### 辞書ロード速度

| エンジン | 平均 | 最速 |
|----------|------|------|
| MeCab (fugashi) | 3.1 ms | 1.9 ms |
| **hasami** (mmap) | 18.1 ms | 11.4 ms |
| Sudachi | 25.8 ms | 11.5 ms |

*Apple Silicon (M4)、IPAdic辞書使用、10文×3000イテレーションでの計測*

## 開発

```bash
# ワークスペース全体のビルド
cargo build --workspace

# ビルド
make build

# テスト実行
cargo test --workspace

# clippy と フォーマットチェック
make check

# リリースビルド
make release

# 辞書ビルド（全辞書）
make dict
```

## ライセンス

[MIT](LICENSE)

### 同梱辞書のライセンス

本リポジトリの `dict/` ディレクトリに同梱されている辞書は、以下のソースから構築されています。各辞書の著作権・ライセンスに従ってご利用ください。

#### IPAdic (`dict/ipadic.hsd`, `dict/ipadic-neologd.hsd`)

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

#### mecab-ipadic-NEologd (`dict/ipadic-neologd.hsd`)

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

#### UniDic (`dict/unidic-cwj.hsd`, `dict/unidic-csj.hsd`)

[UniDic](https://clrd.ninjal.ac.jp/unidic/) を基に構築。CWJ（現代書き言葉 202512）および CSJ（現代話し言葉 202512）。

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
