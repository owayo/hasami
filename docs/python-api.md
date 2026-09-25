# Python から使う

`hasami-python/` は PyO3 で書いた Python バインディングで、maturin でビルドする。PyPI には公開していない
（パッケージ名の `hasami` は PyPI では別のプロジェクトが使っている）ので、このリポジトリから入れる。

## インストール

maturin と Python は `mise.toml` で固定している（`mise install` で入る）。`maturin develop` は有効にした仮想環境に入れる。

```bash
cd hasami-python
mise install
mise exec -- python -m venv .venv
source .venv/bin/activate
mise exec -- maturin develop --release
```

## 基本的な使い方

```python
import hasami

# 辞書をロード
analyzer = hasami.Analyzer("dict/ipadic-neologd.hsd")

# 形態素解析
tokens = analyzer.tokenize("東京都に住んでいる")
for token in tokens:
    print(f"{token.surface}\t{token.pos}")
```

## 辞書マージ (Python)

```python
builder = hasami.DictBuilder()
builder.load_hsd("dict/ipadic.hsd")    # 既存辞書をロード
builder.add_csv_dir("./extra/")        # CSVを追加
builder.build("merged.hsd")           # 新しい辞書を保存
```

## 分かち書き

```python
print(analyzer.wakachi("東京都に住んでいる"))
# => 東京都 に 住ん で いる
```

## 並行解析（Python マルチスレッド）

`tokenize` 系メソッドは内部で GIL を解放するため、複数スレッドで真の並列処理が可能です。`clone_for_worker()` で辞書を共有しつつ、スレッドごとにワークスペースを独立化します。

```python
import hasami
from concurrent.futures import ThreadPoolExecutor

analyzer = hasami.Analyzer("dict/ipadic-neologd.hsd")
analyzer.prewarm()  # 解析で触れる辞書のページを先に読み込む（初回の待ちを避ける）

def tokenize_one(args):
    worker, text = args
    return [t.surface for t in worker.tokenize(text)]

# ワーカーごとにクローン（辞書はゼロコピー共有）
texts = ["文1", "文2", "文3", "文4"]
workers = [analyzer.clone_for_worker() for _ in texts]

with ThreadPoolExecutor(max_workers=4) as ex:
    results = list(ex.map(tokenize_one, zip(workers, texts)))
```

## Token オブジェクトの属性

```python
token = analyzer.tokenize("猫")[0]
token.surface        # 表層形: "猫"
token.pos            # 品詞: "名詞,一般,*,*"
token.conj_type      # 活用型: ""（活用しない語・未知語は空文字列。動詞なら "五段・カ行イ音便" など）
token.conj_form      # 活用形: ""（動詞なら "連用形" など）
token.base_form      # 原形: "猫"
token.reading        # 読み: "ネコ"
token.pronunciation  # 発音: "ネコ"
token.start          # 開始バイト位置: 0
token.end            # 終了バイト位置: 3
token.word_cost      # 単語コスト: 3987
token.is_known       # 辞書語かどうか: True
token.coarse_pos     # 辞書の品詞体系をそろえた粗い品詞: "Noun"（Rust の CoarsePos の名前）
token.is_negation    # 否定の形態素か: False
token.mora_count     # モーラ数: 2
```

辞書が壊れていて解析中に不正な参照を見つけたときは `ValueError`、辞書ファイルを開けないときは `IOError` を送出する。
