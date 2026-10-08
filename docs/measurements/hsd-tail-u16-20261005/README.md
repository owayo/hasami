# u16文字符号TAILの評価（2026-10-05）

速度と初回メモリを優先して、第9.3節で容量のみ試算したTAILの文字符号案を試した。
結果と採否の正本は [hsd-format.md 第15節](../../hsd-format.md#15-2026-10-05tailのu16文字符号を評価)。
現行v5と実験版5007の隔離比較であり、正式な辞書移行ではない。

## 保存するもの

- [design.md](design.md)：バイト配置、読み出し・検査と採否条件。
- [reader.patch](reader.patch)：`4de1d02`に適用する隔離試作。小辞書用の構築経路とTAILテストも含む。
- [run.py](run.py)：入力ハッシュ、全件検証、9組の全フィールド比較、交互の測定と単発ロード。
- [audit.py](audit.py)：保存した統計を別の実装で再計算し、標本数・順序・トークン数・ソースZIPを検査。
- `manifest.json` / `measured-sources.zip`：コード、ツール、辞書・入力・実行ファイルと実行時ソースの識別子。
- `conversion.jsonl`：全TAILのUTF-8復元一致、他16セクションの一致、完成した非圧縮ファイルのサイズ。
- `analysis.jsonl` / `load.jsonl` / `load-once.jsonl`：216解析行、2,232再構築行、72独立単発プロセス。
- `summary.json` / `audit.json`：全標本の中央値・範囲・IQR・CV・組の比、順序別集計と単発ロード。
- `resource-logs.jsonl`：解析・再構築プロセスのピークworking set出力。
- `verify-{方式}-{辞書}.txt` / `compare-{入力}-{辞書}.txt` / `trie-tests.txt`：検証の記録。

## 条件

AMD Ryzen 7 5700X、Windows 11 Home 10.0.26300、32 GiB。Rust 1.98.1、Python 3.13.15。
第14節のWindows 10.0.26200とはOSビルドが異なる。今回の両方式は同じOS上で比較した。
実行後の環境確認で、manifestに引き継いでいた前回のOSラベルを修正した。
ソースZIPは実行時のバイト列を保持し、現在のrun.pyはOSを実行時に取得する。
ライブラリは`4de1d02`から同条件のrelease・既定featureでビルドした。
入力は公開リリース`v26.9.107`のv5の3辞書で、第14節と同一SHA-256。
混合143,874行、短文100,000回、文学48,200行も第14節と同じハッシュである。
文学は『坊っちゃん』482行を100回並べた入力で、注釈付きの精度評価ではない。

両方式を論理CPU 2（affinity mask 4）に固定し、通常優先度で12ラウンド測定した。
AB/BA各6回、辞書順を巡回。解析は各プロセスで入力を1周してから次の1周を測る。
時計は`Instant`のwallを主指標とし、15.625ms刻みのWindows CPU値は補助として保存する。
数百µsのロードのCPU値が0でも費用ゼロとは解釈しない。
OSページは温まった条件で、cold-startの測定ではない。

ピークworking setはプロセス全体。解析条件には入力とウォームアップも含む。
初回メモリ用には、コーパスを読まずロード＋固定1文を1回だけ実行する新規プロセスを別に使った。
31回再構築のピークを初回メモリへ転用しない。単発ロード72件には全体CPU使用率を保存していない。
解析プロセスの期間平均CPU使用率は全標本へ保存し、負荷・周波数を固定できたとは扱わない。
中央値の比、各組の比の中央値、AB/BA別の値を分けて読む。

## 再現

ルートのPowerShellで実行する。過去の測定に使った入力を再利用する際も、run.pyがハッシュを検査する。
`target/hsd-repeat-windows`が無ければ、第14節のREADMEの固定配布品・混合入力・文学入力を先に用意する。
辞書・コーパス・ビルド成果物は追跡しない。

```powershell
$trial = 'target/hsd-tail-u16-new'
New-Item -ItemType Directory -Path $trial | Out-Null
git archive --format=tar --output="$trial/base.tar" 4de1d02
New-Item -ItemType Directory -Path "$trial/v5-source" | Out-Null
tar -xf "$trial/base.tar" -C "$trial/v5-source"
Copy-Item -LiteralPath "$trial/v5-source" -Destination "$trial/u16-source" -Recurse
git apply --directory="$trial/u16-source" docs/measurements/hsd-tail-u16-20261005/reader.patch
mise exec -- python scripts/hsd-tail-u16-probe.py "$trial/u16" target/hsd-repeat-windows/v5/ipadic.hsd target/hsd-repeat-windows/v5/ipadic-neologd.hsd target/hsd-repeat-windows/v5/ipadic-neologd-sudachi.hsd
foreach ($variant in @('v5', 'u16')) {
    mise exec -- cargo build --release --locked --lib --manifest-path "$trial/$variant-source/Cargo.toml"
    mise exec -- rustc --edition=2024 -O -C lto=fat -C codegen-units=1 scripts/hsd-format-eval.rs --extern "hasami=$trial/$variant-source/target/release/libhasami.rlib" -L "dependency=$trial/$variant-source/target/release/deps" -o "$trial/eval-$variant.exe"
}
mise exec -- cargo test --locked --lib --no-default-features --features build --manifest-path "$trial/u16-source/Cargo.toml" hsd::trie
mise exec -- python docs/measurements/hsd-tail-u16-20261005/run.py $trial
mise exec -- python docs/measurements/hsd-tail-u16-20261005/audit.py "$trial/results"
```

`run.py --reuse-verified`は、同じ実行ファイル・辞書・入力で全件検証と9組の比較が既に成功し、
タイミング測定がまだ始まっていない場合だけ使える。初回の実行では省略する。
今回、検証後に見つかった測定ドライバーの出力先の衝突を修正し、このオプションで検証を再利用した。
失敗したドライバー実行に解析時間の標本は無く、成功したタイミング実行の標本は全件保存した。

非圧縮サイズの減少を配布圧縮サイズの減少へ換算しない。
圧縮・正式writer・上流からの再構築・構築時間/メモリ・cold cache・UniDic・FFI/Python・32bitは今回の対象外。
