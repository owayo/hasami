# 文法共有 v5 の検証資料

採否と集計は [../../hsd-format.md](../../hsd-format.md) の第 10 節。
[design.md](design.md) は実装前の設計、[final-review.md](final-review.md) は独立レビュー。
設計時の交互測定 5 回を、測定順を均等にするため実施時に 6 回へ増やした。

| 記録 | 内容 |
| --- | --- |
| `bench.jsonl` / `summary.json` | ユーザーが負荷を停止した後に最初から測り直した 108 件と集計 |
| `high-load-discarded.jsonl` | 中断した高負荷時の参考値。採用判断には使わない |
| `process-time.txt` / `load.txt` | 各プロセスの time -l、Analyzer を作り直したロード＋初回解析 |
| `production-compression.jsonl` | 上流から再構築した完成品の非圧縮・zstd -19 サイズと SHA-256 |
| `sections-*.json` | 期待値と本番 writer の全セクションをバイト比較して一致した SHA-256 |
| `compare-*.log` / `verify-*.txt` | 全 Token フィールドの一致と全件検証 |
| `build-cost.txt` / `checks.txt` | 上流再構築の時間・RSS、CI・配布辞書テスト・例外表の結果 |

測定に使った既存辞書のメタデータは hasami_version=26.9.102、再構築品は 26.9.105。
この値以外の全セクションは一致することを確認した。辞書の内容が違うものを速度比較していない。

## 再現する条件

- 基準ソース: `23e5e51a87a9ababfa3f326f72fd67ca79abe1cc`（形式 v4）。候補は同じソースに今回の文法共有を適用したもの。
- macOS 27.0 (26A428)、Apple M2、Rust 1.98.1 (48a229cea)、LLVM 22.1.8。
- Cargo の release 設定（fat LTO、codegen-units=1）。計測器も同じ設定。
- この環境では Cargo の子プロセスが Homebrew の rustc を使った。単独の `mise exec -- rustc` は同版でも
  異なる配布元の rustc を選び、rlib と互換でなかった。計測器は `mise exec -- sh -c 'rustc ...'` で
  Cargo と同じコンパイラを使った。別環境でもライブラリと計測器のコンパイラを揃える。
- zstd 1.5.7、`zstd -q -19 -c`。非圧縮ファイルと配布時の圧縮ファイルは別々に比べる。

上流ソース・repair・ユーザー辞書は両者で同じ `scripts/build-dict.sh` を使う。
基準 CLI と候補 CLI をそれぞれ保存し、別々の出力先を渡す。

```sh
mise exec -- /usr/bin/time -l bash scripts/build-dict.sh --hasami "$eval_dir/hasami-v4" --out "$eval_dir/rebuilt-v4"
mise exec -- /usr/bin/time -l bash scripts/build-dict.sh --hasami "$eval_dir/hasami-v5" --out "$eval_dir/rebuilt-v5"
```

## 計測器とコーパス

`scripts/hsd-format-eval.rs` を、各ソースでビルドした release の `libhasami.rlib` にリンクする。
以下のパスを基準・候補にそれぞれ置き換え、`eval-v4` / `eval-v5` として保存する。

```sh
mise exec -- cargo build --release --locked
mise exec -- sh -c 'rustc --edition 2024 -O -C lto=fat -C codegen-units=1 scripts/hsd-format-eval.rs --extern hasami=target/release/libhasami.rlib -L dependency=target/release/deps -o /tmp/eval-v5'
```

固定長 ID の比較器は、候補ソースの別コピーに [fixed-reader.patch](fixed-reader.patch) を適用して同様に作る。
版を 5005 にした **読み取り専用の実験**。このパッチの writer で辞書を作らない。
[convert-v4.py](convert-v4.py) は信頼できる v4 辞書を同じ内容の実験形式に変換する。
変換先の mode は `varint` / `fixed`、試験用辞書の置き場所は `$eval_dir/varint` / `$eval_dir/fixed`。
本番への移行にはこの変換器を使わず、上流からの再構築を使う。

コーパスは [livedoor の配布アーカイブ](https://www.rondhuit.com/download/ldcc-20140209.tar.gz) と
基準コミットの checkout から作る。

```sh
mise exec -- python scripts/hsd-format-corpus.py "$eval_dir/ldcc-20140209.tar.gz" "$base_checkout" "$eval_dir"
mise exec -- python scripts/hsd-format-bench.py "$eval_dir" "$v4_dict_dir" 6
mise exec -- python scripts/hsd-format-compare.py "$eval_dir/eval-v4" "$old_hsd" "$eval_dir/eval-v5" "$new_hsd" "$eval_dir/mixed.txt"
```

`mixed.txt` は 143,663 行、25,613,826B、SHA-256
`629d7e83b5fa0fc832e8a4e6db86fc693b6b7a610af68c78d76472c31025f4e7`。
ニュース 132,880 行、README と docs の空白以外の行 1,494 行、合成入力 9,289 行。
合成入力は会話・文学調の組み合わせ、乱数文字列（seed 20260926）、長い同字反復、制御文字など。
独立した文学作品コーパスを評価したものではない。
短文は「東京都に住んでいる人々が増えている。」を 100,000 行繰り返す。

`dump` は全 Token フィールドと行・トークンの順番を長さ付きで出し、比較器が逐次バイト一致を検査する。
`compare-*.log` の SHA-256 は一致したストリームの識別子で、ハッシュだけによる同値判定ではない。
上流再構築は、v4 再構築品を変換した期待値と v5 再構築品の全 18 セクションを
`scripts/hsd-format-sections.py EXPECTED ACTUAL` でバイト比較する。

## 速度の読み方

`bench.jsonl` は毎回新しいプロセスを起動し、コーパス 1 周でページと解析器を温めてから 1 周を測る。
3 方式の順序を入れ替え、各方式が各順番に同数ずつ現れる 6 回にした。
macOS の USER_INTERACTIVE QoS は P コアへの厳密な固定ではない。
本測定中は今回のビルド・圧縮・回帰比較を並行実行しないが、他のアプリや OS の負荷は残る。

`load` は同一プロセス内で新しい Analyzer を 31 回作り、ロードと最初の 1 文を測る。
OS とアロケータのキャッシュが温まった値であり、プロセスの起動時間や cold-start の値ではない。
初回プロセスの時間・最大 RSS は CLI を `/usr/bin/time -l` で別に起動して測る。
ストレージから読む cold-start とヒープ確保回数は未計測。
