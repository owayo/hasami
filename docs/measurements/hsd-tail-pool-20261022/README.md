# UTF-8 TAIL共有の評価（2026-10-22）

結果と判断の正本は [hsd-format.md 第16節](../../hsd-format.md)。
基準は `16a42ddcd0eb0ce3d616c155620583a02133b5f9` のv5、隔離試作は実験版5008。
製品の形式・解析コードは変更していない。

## 記録

- [design.md](design.md)：表現・検査・測定条件・採否条件。
- [reader.patch](reader.patch)：基準コミットへ適用する隔離パッチ。小辞書ビルダーと破損テストを含む。
- [変換器](../../../scripts/hsd-tail-pool-probe.py)：信頼したv5専用。全TAILの値とUTF-8、他16セクションと非TAILノードを直接比較。
- [run.py](run.py)：入力ハッシュ、全件検証、9組の全フィールド比較、交互測定、単発ロード。
- [compress.py](compress.py)：完成ファイルの圧縮と、展開したバイト列のSHA-256照合。
- [audit.py](audit.py)：別実装による集計・件数・順序・トークン数・ソースZIPの監査。
- `manifest.json` / `measured-sources.zip`：入力・ツール・コード・実行ファイルの識別子と測定時ソース。
- `conversion.jsonl` / `compression.jsonl`：3辞書のセクション別サイズ、直接照合と圧縮結果。
- `analysis.jsonl` / `load.jsonl` / `load-once.jsonl`：解析216行、再構築2,232行、単発ロード72行。
- `summary.json` / `audit.json`：全標本の中央値・範囲・IQR・CV・組の比・AB/BA別集計。
- `verify-*.txt` / `compare-*.txt` / `trie-tests.txt` / `library-tests.txt` / `clippy.txt`：検証ログ。
- [review.md](review.md)：独立したCodexの設計・パッチ・結果レビューと対応。

## 条件と限界

AMD Ryzen 7 5700X、物理8コア・論理16コア、32 GiB。OSの実ビルドはmanifestに記録。
Rust 1.98.1、Python 3.13.15、`--release --locked --lib --no-default-features --features build`。
評価器は両方式で同じソース・最適化条件。論理CPU 2（mask 4）、通常優先度。
12ラウンド、AB/BA各6回、辞書順を巡回。解析は1周ウォームアップした後の1周を測る。
主指標はRust Instant wall。Windows CPU時計は短い区間に粗いため補助値とし、
全体CPU使用率も各解析プロセスの期間平均として保存する。負荷・周波数の固定はしていない。
OSページは温まっており、cold cacheの計測ではない。
ピークworking setはプロセス全体。入力・ウォームアップを含む解析条件と、
入力ファイルを読まないロード＋固定1文の新規プロセス条件を分ける。

辞書は公開リリース `v26.9.107`。第14・15節と同じ3辞書・混合143,874行・
短文100,000回・文学48,200行をSHA-256で確認した。文学は『坊っちゃん』482行の100回反復。
混合の生成に使う技術文書の基準は `fefb950`。現在の文書から生成し直すと別入力になる。
圧縮は公式Windows x64版zstd 1.5.7の `-19 -T1`。実行ファイルと配布ZIPのハッシュも保存する。
ビルド・変換・全件検証・出力比較・圧縮は、解析のタイミング測定と重ねない。

小辞書ビルダーは同じ間接表現で非共有の文字列を交互に置く。実測対象は変換器による
8Bレコード列＋共有文字列プールであり、小辞書の構築時間を正式writerの費用へ換算しない。
正式writer・上流からの再構築・構築時間/ピークメモリ・cold cache・UniDic・
試作版のFFI/Python・全feature・32bitは未検証。

## 再現

リポジトリ直下のPowerShellで、新しい出力ディレクトリを使う。
`target/hsd-repeat-windows` の入力は第14節の
[再現手順](../hsd-pronunciation-20261005-windows/README.md)で用意する。
run.pyは過去のmanifestと入力を照合する。辞書・コーパス・圧縮ファイル・ビルド成果物は追跡しない。

```powershell
$trial = 'target/hsd-tail-pool-new'
New-Item -ItemType Directory -Path $trial | Out-Null
git archive --format=zip --output="$trial/source.zip" 16a42ddcd0eb0ce3d616c155620583a02133b5f9
Expand-Archive -LiteralPath "$trial/source.zip" -DestinationPath "$trial/v5-source"
Expand-Archive -LiteralPath "$trial/source.zip" -DestinationPath "$trial/pool-source"
git apply --directory="$trial/pool-source" docs/measurements/hsd-tail-pool-20261022/reader.patch
mise exec -- python scripts/hsd-tail-pool-probe.py "$trial/pool" target/hsd-repeat-windows/v5/ipadic.hsd target/hsd-repeat-windows/v5/ipadic-neologd.hsd target/hsd-repeat-windows/v5/ipadic-neologd-sudachi.hsd
foreach ($variant in @('v5', 'pool')) {
    mise exec -- cargo build --release --locked --lib --no-default-features --features build --manifest-path "$trial/$variant-source/Cargo.toml"
    mise exec -- rustc --edition=2024 -O -C lto=fat -C codegen-units=1 scripts/hsd-format-eval.rs --extern "hasami=$trial/$variant-source/target/release/libhasami.rlib" -L "dependency=$trial/$variant-source/target/release/deps" -o "$trial/eval-$variant.exe"
}
mise exec -- cargo test --locked --lib --no-default-features --features build --manifest-path "$trial/pool-source/Cargo.toml"
mise exec -- cargo clippy --locked --lib --no-default-features --features build --manifest-path "$trial/pool-source/Cargo.toml" -- -D warnings
gh release download v1.5.7 --repo facebook/zstd --pattern zstd-v1.5.7-win64.zip --dir $trial
Expand-Archive -LiteralPath "$trial/zstd-v1.5.7-win64.zip" -DestinationPath "$trial/zstd"
mise exec -- python docs/measurements/hsd-tail-pool-20261022/compress.py $trial
mise exec -- python docs/measurements/hsd-tail-pool-20261022/run.py $trial
mise exec -- python docs/measurements/hsd-tail-pool-20261022/audit.py "$trial/results"
```

`run.py --verify-only` は照合まで実行する。
同じ辞書・実行ファイル・入力で検証済みの場合、`--reuse-verified` で検証を再利用できる。
タイミング出力は既存ディレクトリを拒否するため、測定のやり直しは新しいtrialで行う。
今回の準備では実験用の大きな版番号が旧版テストの連続範囲という前提に合わず、
試作側テストを既知の旧版1〜4と、拒否を確かめる5・5007に分けて修正した。
製品の不具合としては扱っていない。測定は修正後のソースからビルドした評価器で実行した。
