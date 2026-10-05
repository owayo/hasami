# 発音差分圧縮の試作（2026-10-04）

採否の要約は [hsd-format.md 第 12 節](../../hsd-format.md#12-2026-10-04発音差分を試作し現行-v5-を維持)。現行 v5 を維持する。
設計は [design.md](design.md)、実際のリーダーの試作は [reader.patch](reader.patch)。
基準コード・辞書と入力の SHA-256・測定条件は [manifest.json](manifest.json)。
試作は独立した worktree と一時ディレクトリで行い、製品のソースには反映していない。

## 保存した生データ

- [probe.jsonl](probe.jsonl)：全固有素性の復元一致、完成ファイルと素性セクションのサイズ。
  単体復号の時間も保存しているが、他の辞書検証と一部重なり、採否の速度根拠には使わない。
  最初の 100 万件だけの測定で、両方式を 3 片として扱い、UTF-8 復号と Arc 確保は含まない。
- [compression.jsonl](compression.jsonl)：完成ファイルを zstd 1.5.7 `-19 -T1`・単一ワーカーで圧縮した 6 ファイルの実バイト数。
- [analysis.jsonl](analysis.jsonl)：実際の Analyzer の交互 6 ラウンド、CPU・wall・RSS。
- [load.jsonl](load.jsonl)：各辞書/方式で Analyzer を 31 回作り直すロード＋初回。
  各辞書は v5 をまとめて測った後に差分案を測っており、交互ではない。
- [summary.json](summary.json)：中央値・CPU の範囲と各組の CPU 時間比。
- [checks.json](checks.json)：単体テスト、全素性・全フィールド比較、型サイズ、通常版の辞書検証。
- [IPAdic](compare-ipadic.txt)、[NEologd](compare-ipadic-neologd.txt)、[推奨辞書](compare-ipadic-neologd-sudachi.txt)：全トークンの全フィールドの直接比較。SHA-256 は比較した長さ付きバイト列のもの。

コーパスは既存の混合評価入力（ニュース、技術文書、境界ケース）。本文は保存せず、構成とハッシュだけを残す。
この大入力の速度測定は変動が大きく、安定した改善・退行・同等性を主張しない。

## 容量と復元の再現

信頼できる v5 配布品だけを入力する。変換器は一般の壊れたファイルの受け入れテスト用でも、移行用でもない。
実験番号は 5006 なので、通常の hasami は実験ファイルを拒否する。

```sh
make dict-download
trial_dir=$(mktemp -d)
mise exec -- rustc --edition=2024 -O scripts/hsd-pronunciation-probe.rs -o "$trial_dir/probe"
"$trial_dir/probe" "$trial_dir/dictionaries" dict/ipadic.hsd dict/ipadic-neologd.hsd dict/ipadic-neologd-sudachi.hsd
mise exec -- rustc --edition=2024 --test scripts/hsd-pronunciation-probe.rs -o "$trial_dir/tests"
"$trial_dir/tests"
zstd -19 -T1 "$trial_dir/dictionaries/ipadic.hsd"
```

この変換器では全固有素性を v5 の正準バイト列へ戻して直接比較する。オフセットも全エントリについて写し、他の 16 セクションは変えない。
変換時間と RSS は、辞書を上流から構築する writer 全体の値へ換算しない。

## 実際のリーダーの評価の再現条件

`fefb950` の独立した worktree に reader.patch を `git apply` する。通常のツリーには適用しない。
Rust 1.98.1 で両方を release ビルドし、各 `libhasami.rlib` に対して
[hsd-format-eval.rs](../../../scripts/hsd-format-eval.rs) を `-O -C lto=fat -C codegen-units=1` でコンパイルする。
同じスクリプト・コンパイラ・最適化条件を使い、ライブラリとバイナリの出力先は方式ごとに分ける。

eval のビルド形は、各 worktree のルートで次のとおり。出力名を方式ごとに変える。

```sh
mise exec -- rustc --edition=2024 -O -C lto=fat -C codegen-units=1 scripts/hsd-format-eval.rs --extern hasami=target/release/libhasami.rlib -L dependency=target/release/deps -o eval
```

全フィールドの直接比較には [hsd-format-compare.py](../../../scripts/hsd-format-compare.py) を使う。
引数は `V5_EVAL V5_DICT PATCH_EVAL PATCH_DICT CORPUS`。出力を書き出してからハッシュだけ比べる方式ではなく、両プロセスの長さ付きバイト列を直接比較する。
速度・ロード・RSS は [hsd-pronunciation-evaluate.py](../../../scripts/hsd-pronunciation-evaluate.py)。
引数は `OUTPUT V5_DICTS PATCH_DICTS V5_EVAL PATCH_EVAL CORPUS 6`。macOS の `time -l` と CPU clock を使う。
パスは実際に作ったものを渡し、前の一時ディレクトリの残存を前提にしない。評価前にビルド・圧縮・回帰比較を終える。

正式採用には、24B の借用表現を維持する候補の計測、通常の writer、上流からの再構築と移行、各 feature と FFI/Python の確認が必要。
今回はそこまで進めず、v5 のソースと配布辞書を維持した。

最終記録は独立した Codex エージェントが生データから数値を再計算して確認した。
サイズ・CPU/wall/RSS・ロード・トークン数と v5 維持の判断に重大な指摘はなかった。
