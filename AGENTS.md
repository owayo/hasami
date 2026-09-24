# hasami - 高速日本語形態素解析エンジン

## プロジェクト概要
Rust製の日本語形態素解析エンジン。外部エンジン（MeCab等）に一切依存せず、ゼロベースで構築。

## 技術スタック
- **言語**: Rust (2024 edition, MSRV 1.85)
- **辞書**: mmap-native バイナリ形式 (.hsd) + bytemuck Pod 構造体
- **Trie**: Double-Array Trie（ゼロコピー mmap 参照）
- **解析アルゴリズム**: ラティス構築 + Viterbi（コスト最小化、文分割最適化）
- **文字列管理**: StringPool 重複排除 + Arc<str> キャッシュ（ロード時構築）
- **Python バインディング**: PyO3 + maturin
- **C FFI**: `#[no_mangle] extern "C"`

## プロジェクト構造
```
hasami/
├── src/
│   ├── lib.rs          # ライブラリエントリポイント
│   ├── main.rs         # CLI (build, merge, tokenize, bench, info)
│   ├── trie.rs         # Double-Array Trie
│   ├── dict.rs         # Dictionary, DictEntry, DictBuilder（ビルド時中間構造体）
│   ├── mmap_dict.rs    # mmap-native 辞書 (.hsd) - Pod構造体、StringPool、FeaturePool
│   ├── char_class.rs   # 文字分類（未知語処理）
│   ├── lattice.rs      # ラティス構築 + Viterbi
│   ├── analyzer.rs     # 高レベルAPI（DictBackend enum: Mmap/InMemory）
│   └── ffi.rs          # C ABI インターフェース
├── dict/               # ビルド済み辞書（Git LFS管理）
│   ├── ipadic.hsd      # IPAdic 単体
│   ├── ipadic-neologd.hsd  # IPAdic + NEologd
│   ├── ipadic-neologd-sudachi.hsd  # IPAdic + NEologd + SudachiDict（推奨・最大語彙）
│   └── user/           # ユーザー辞書CSV（make dict-neologd でマージ）
│       ※ unidic-cwj.hsd / unidic-csj.hsd は同梱されず make dict-unidic-cwj/csj でビルド
├── scripts/
│   └── convert-unidic-csv.py  # UniDic CSV → IPAdic互換フォーマット変換
├── hasami-python/      # Python バインディング (PyO3)
│   ├── src/lib.rs
│   ├── build.rs        # PyO3 拡張モジュール向けリンク設定
│   ├── Cargo.toml
│   └── pyproject.toml
├── Cargo.toml          # ワークスペース + メインクレート
└── README.md
```

## 主要API
- `Analyzer::load(path)` - .hsd 辞書ロード（mmap、~40ms）
- `Analyzer::tokenize(text)` - 形態素解析
- `DictBuilder` - MeCab形式CSVから辞書構築
- `DictBuilder::load_hsd(path)` - 既存辞書からインポート（マージ用）
- `hasami_last_error(handle)` - C FFI の直前エラー取得（`handle == NULL` でも直近のロード失敗を参照可能）

## 辞書形式
- **ビルド**: MeCab互換CSV + matrix.def + char.def + unk.def → .hsd
- **フォーマット**: mmap-native バイナリ（bytemuck Pod、ゼロコピー）
- **拡張子**: `.hsd` (hasami dictionary)

## CLI コマンド
- `hasami build` - 辞書構築
- `hasami merge` - 既存辞書にCSVを追加マージ
- `hasami tokenize` - 形態素解析
- `hasami bench` - ベンチマーク
- `hasami info` - 辞書情報表示

## ビルド・テスト
```bash
cargo build --release     # リリースビルド
cargo build --workspace   # Python バインディングを含むワークスペース全体をビルド
cargo test --workspace --exclude hasami-python  # テスト実行（hasami-python は extension-module のため
                                                # macOS/Linux でリンク不可。clippy --workspace で検証）
cargo clippy --workspace --all-targets -- -D warnings  # lint（hasami-python のコンパイル検証を含む）
make dict                 # 全辞書ビルド（IPAdic, NEologd, UniDic）
make dict-clean           # ダウンロードした辞書ソースを削除
```

## 辞書ソースの既知の欠陥

複数の辞書ソースをマージしているため、ソース側の欠陥がそのまま解析結果に出る。
`hasami repair` で修復・除去する（詳細は README の「辞書の修復」）。

| ソース | 欠陥 | 影響 | 対処 |
| --- | --- | --- | --- |
| SudachiDict | `.dict-src/sudachi/sudachi.csv` の発音フィールド（13列目）が表層形のまま。92% が非カタカナ | 「方法」の発音が「ホーホー」でなく「ホウホウ」になり長音が失われる。読みがラテン文字の語（Siemens 等）は読みが消える | `repair`（常時） |
| NEologd | 表記ゆれ正規化エントリが活用語の語形を名詞として登録している | 「質の高い」→「シツノコウイ」、「概念を学ぶ」→「ガイネンヲガクブ」 | `repair --drop-ortho-variants` |
| NEologd / SudachiDict | 漢数字だけで綴られた人名・地名 | 「十五」→「トウゴ」、「二十八」→「ツチヤ」 | `repair --drop-numeral-misreadings` |
| SudachiDict | 代名詞と同じ表層の 1 文字の人名 | 「何なのか」→「ガナノカ」 | `repair --drop-ortho-variants` |
| NEologd `mecab-user-dict-seed` | 読みが別語のものに差し替わっているエントリが散在する | 「最終面接」→「イチジメンセツ」、「目標数値」→「スウチモクヒョウ」、「情報収集」→「ジョホウシュウシュウ」 | `dict/user-remove/misreading-entries.csv` に列挙して `repair --remove` |

`convert_sudachi_to_mecab.py` は現在 `pronunciation = reading` で出力するが、
`.dict-src/sudachi/sudachi.csv` は旧版スクリプトの出力が残っているため上記の欠陥を持つ。
CSV から辞書を作り直す場合は変換をやり直すこと。

### 文脈で決まる読み

同じ表層形でも文脈で読みが変わる語は、辞書のどのエントリを選んでも片方が誤読になる。
`lattice.rs` の `apply_contextual_readings()` が Viterbi の結果を見て補正する。

| 語 | 読みの分かれ方 | 判定の根拠 |
| --- | --- | --- |
| 他 | 名詞なら「ホカ」（他ならぬ、他で、他は）、接頭詞なら「タ」（他部門、他業種） | 自分の品詞（`POS_READING_OVERRIDES`） |
| 数 | 数詞・助数詞が続けば接頭辞の「スウ」（数十人、数分）、続かなければ名詞の「カズ」（目玉の数、数が多い） | 後続トークンの品詞（`apply_kazu_reading_override()`） |
| 一節 | 小数点の直後は促音化せず「イチセツ」（4.1節 →「四点一節」）、それ以外は「イッセツ」（詩の一節、第一節） | 直前が「数詞 +『点』」か（`apply_decimal_counter_override()`） |

助詞との組み合わせを列挙する方式では「他ならぬ」や文末の「他」を取りこぼすので、
品詞を根拠にしている。複合語（他人、素数、複数、人数）は 1 トークンなので影響しない。

### 英字の読み

日本語文中の 1〜2 文字の英字は略語（AI, PC, VP 等）が大半で、辞書に登録された
単位読み・略称読み（A→アンペア, G→ギガ, cs→クレディスイス）はほぼ誤読になる。
`lattice.rs` の `should_trust_dict_reading()` は 1〜2 文字の英字と、読みが
カタカナでないエントリについて辞書を信用せず、綴り読み（数字直後なら単位読み）を付与する。
3 文字以上でカタカナの読みを持つ語（NASA→ナサ）は辞書を尊重する。

### 半角記号は IPAdic では未知語になる

IPAdic の `unk.def` は SYMBOL クラスの未知語を「名詞,サ変接続」として扱う。
半角カンマ `,` とアポストロフィ `'` は辞書にエントリが無いためこの規則が適用され、
読点として扱われない。その結果、直後の分割が乱れる。

```
ときなど、日々確実に前進する   → とき / など / 、 / 日々 / 確実 / に / 前進 / する
ときなど,日々確実に前進する   → とき / など / , / 日 / 々 / 確実 / に / 前進 / する
```

Style-Bert-VITS2 は読点を `,` に正規化してから解析に渡すため、この差が直接効く。
`dict/user/ascii-punctuation.csv` で `,` を「記号,読点」、`'` を「記号,一般」として
登録している。`.` `!` `?` `…` `-` は辞書にエントリがあるので対処は要らない。
