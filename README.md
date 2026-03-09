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

## 使い方

### 1. 辞書の構築

MeCab形式の辞書（IPAdic等）からバイナリ辞書をビルドします。

```bash
# IPAdic をダウンロード・展開後
hasami build --input ./ipadic/ --output dict.hsd
```

### 2. 辞書のマージ

既存の辞書にMeCab形式のCSVファイルを追加できます。

```bash
# CSVファイルを既存辞書に追加
hasami merge --dict dict.hsd --input custom_words.csv

# ディレクトリ内の全CSVを追加（別ファイルに出力）
hasami merge --dict dict.hsd --input ./extra_dict/ --output merged.hsd
```

### 3. 形態素解析 (CLI)

```bash
# MeCab形式で出力
hasami tokenize --dict dict.hsd "東京都に住んでいる"

# 分かち書き
hasami tokenize --dict dict.hsd --format wakachi "東京都に住んでいる"

# JSON形式
hasami tokenize --dict dict.hsd --format json "東京都に住んでいる"

# 標準入力から
echo "形態素解析のテスト" | hasami tokenize --dict dict.hsd
```

### 4. Rust API

```rust
use hasami::Analyzer;

let mut analyzer = Analyzer::load("dict.hsd")?;
let tokens = analyzer.tokenize("東京都に住んでいる");

for token in &tokens {
    println!("{}\t{}\t{}", token.surface, token.pos, token.reading);
}

// バッチ処理
let results = analyzer.tokenize_batch(&["文1", "文2", "文3"]);
```

### 5. Python API

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
analyzer = hasami.Analyzer("dict.hsd")

# 形態素解析
tokens = analyzer.tokenize("東京都に住んでいる")
for token in tokens:
    print(f"{token.surface}\t{token.pos}")
```

#### 辞書マージ (Python)

```python
builder = hasami.DictBuilder()
builder.load_hsd("dict.hsd")       # 既存辞書をロード
builder.add_csv_dir("./extra/")     # CSVを追加
builder.build("merged.hsd")        # 新しい辞書を保存
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

### 6. C FFI

```c
#include "hasami.h"

HasamiAnalyzer* analyzer = hasami_new("dict.hsd");
HasamiTokenList tokens = hasami_tokenize(analyzer, "東京都に住んでいる");

for (uint32_t i = 0; i < tokens.len; i++) {
    printf("%s\t%s\n", tokens.tokens[i].surface, tokens.tokens[i].pos);
}

hasami_free_tokens(tokens);
hasami_free(analyzer);
```

## ベンチマーク

```bash
hasami bench --dict dict.hsd --text "東京都に住んでいる人々が増えている。" --iterations 100000
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

## 辞書の入手

以下のMeCab互換辞書が利用可能です：

- **IPAdic**: [mecab-ipadic](https://taku910.github.io/mecab/#download)
- **UniDic**: [unidic.ninjal.ac.jp](https://clrd.ninjal.ac.jp/unidic/)
- **NEologd**: [mecab-ipadic-neologd](https://github.com/neologd/mecab-ipadic-neologd)

## 開発

```bash
# ビルド
make build

# テスト実行
make test

# clippy と フォーマットチェック
make check

# リリースビルド
make release
```

## ライセンス

[MIT](LICENSE)

### 同梱辞書について

本リポジトリに同梱されている `eval/hasami-dict.hsd` は、[MeCab用IPAdic](https://taku910.github.io/mecab/#download) を基に構築されたバイナリ辞書です。

IPAdicの著作権およびライセンスは以下の通りです：

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
