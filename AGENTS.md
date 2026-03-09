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
cargo test --workspace    # ワークスペース全体のテスト実行
```
