# 発音差分案のWindows再測定（2026-10-05）

結果と判断は [hsd-format.md 第14節](../../hsd-format.md#14-2026-10-05windows-pcで発音差分案を再測定)。
v5と3片の発音差分案（5006）を再測定し、v5維持の判断を保った。
製品のリーダー・辞書・公開APIは変更していない。

AMD Ryzen 7 5700X、8コア・16論理CPU、32 GiB、Windows 11 Home 10.0.26200。
Rust 1.98.1、Python 3.13.15。ライブラリは `fefb950`、試作側だけ
[reader.patch](../hsd-pronunciation-20261004/reader.patch) を適用した。
測定時mainは `df6432b` で、解析処理・依存・ビルド条件は `fefb950` と同じ。
測定後の履歴整理で測定時mainのコミットが置き換わっても、再現には整理対象より前の `fefb950` を使う。

## 保存したもの

- [manifest.json](manifest.json)：コード、ツール、6辞書・実行ファイル・測定器・入力のサイズとSHA-256、計測条件。
- [measured-sources.zip](measured-sources.zip)：測定器・比較器・入力生成器など5ソースの実行時バイト列。ZIP内の各ソースはmanifestの `scripts` と同じサイズ・SHA-256。
- [analysis.jsonl](analysis.jsonl)：混合・固定短文・文学、3辞書×2方式×12ラウンドの216件。
- [load.jsonl](load.jsonl)：3辞書×2方式×12プロセス×31回の2,232件。
- [summary.json](summary.json)：全標本の中央値、範囲、IQR、CV、ラウンドごとの比。
- [resource-logs.jsonl](resource-logs.jsonl)：288プロセスのWindowsピークworking set出力。
- [feature-checks.json](feature-checks.json)：全固有素性の復元一致と実ファイル・素性セクションのサイズ。
- `compare-{mixed,short,literature}-{辞書名}.txt`：9組の全トークン全フィールド直接比較。`equal bytes=... sha256=...` が比較成功の行。
- [audit.py](audit.py)、[audit.json](audit.json)：標本数・実行順・トークン数を検査し、保存した統計を別の計算法で再計算。
- [prepare-literature.py](prepare-literature.py)：固定した青空文庫ZIPから文学入力を再生成。

比較ログのトークン数表示は、2子プロセスのstderrが同じファイルへ書くため一部混在している。
比較したバイナリ列は一致している。トークン順、表層・位置・品詞・活用型・活用形・原形・読み・発音・単語コスト・既知語判定を含む。
未知語の壊れたflags・varint・範囲・カタカナを拒否する変換器の単体テストも1件成功した。
変換器の単体復号時間はビルドと重なり、Windows CPU時計にも対応していないため、今回の採否の速度根拠には保存・採用しない。

## 測定条件

両方式のライブラリは `cargo build --release --locked --lib`（default features）。
同じ [hsd-format-eval.rs](../../../scripts/hsd-format-eval.rs) を
`-O -C lto=fat -C codegen-units=1` で各ライブラリへリンクした。
ソースとビルド先は別のスナップショットに分けた。
測定後、本体の測定器の `zip(before, after)` に `strict=True` を追加した。
常に3要素のCPU会計スナップショットの長さを検査する変更で、解析区間・実行順・集計は変更していない。
測定時のソースは上のZIPに保ち、ハッシュと結果を後から書き換えない。
通常のテキストsnapshotはGitのLF/CRLF変換でハッシュが変わるため、元のバイト列をZIPに保存する。

プロセスを論理CPU 2、affinity mask 4、通常優先度へ固定。1周ウォームアップ後の1周を測る。
AB/BAを交互にし、辞書順も巡回する。全12ラウンドを保存し、外れ値除外は0件。
ビルド・辞書生成・比較・圧縮は速度測定と重ねていない。

主指標はRust `Instant` の経過時間。CPUは `GetProcessTimes` のkernel＋userだが、
観測単位が15.625msなので、約0.2秒の短文の数%や数百µsのロードには粗い。
ロードCPU=0を費用ゼロとは解釈しない。メモリは `K32GetProcessMemoryInfo` のプロセス全体の
`PeakWorkingSetSize` で、入力とウォームアップも含む。rawの `rss_bytes` は従来のキー名を保っている。
macOSの最大RSSとはOSをまたいだ直接比較をしない。

`GetSystemTimes` によるPC全体のCPU使用率は各解析プロセス中の平均6.9〜78.1%、中央値15.6%。
解析自身も含む。CPU固定は負荷や周波数を一定にするものではなく、今回も一定の性能差や同等性は証明できない。
`summary.json` のCVはCPU・wall・メモリの分布を見る。正負をまたぐ相対変化のCVは比較指標に使わない。

| 辞書 | 混合wall中央値の比 | 混合wall組の比 | 短文wall中央値の比 | 文学wall中央値の比 | 混合ピークWS中央値の削減 |
| --- | ---: | ---: | ---: | ---: | ---: |
| ipadic | +3.27% | +0.24% | −0.14% | +2.71% | 0.281 MiB |
| ipadic-neologd | +3.55% | −0.50% | +1.13% | −3.48% | 2.061 MiB |
| ipadic-neologd-sudachi | +0.80% | +1.83% | +0.95% | +6.54% | 2.297 MiB |

「中央値の比」は差分案中央値/v5中央値−1、「組の比」は各ラウンドの差分案/v5−1の中央値。
プラスは差分案の時間が長いことを示す。両者を同じ指標と扱わず、範囲・CV・順序別の値も第14節とauditに残す。
3片の借用型は元の試作のままで、24B→56Bの拡大は解消していない。

## 入力

混合入力と固定短文は前回と同一ハッシュ。混合の技術文書は `fefb950` のREADMEと直下の `docs/*.md` を使う。
現在の文書で生成すると入力が変わる。ニュースは固定した `ldcc-20140209.tar.gz` から生成し、本文は追跡しない。

文学は夏目漱石『坊っちゃん』の [青空文庫カード](https://www.aozora.gr.jp/cards/000148/card752.html) の固定ZIP。
Shift_JISをUTF-8・LFにし、本文からルビ・ルビ開始記号・入力者注記を除き、空でない段落を1行にした。
章見出しと著者の日付は残す。原文と本文のハッシュはmanifestと再生成器で検査する。
本文482行・265,281Bを100回並べ、48,200行・26,528,100Bにした。
同じ本文の反復でキャッシュが効く条件であり、独立した48,200文とは扱わない。
形態素の正解注釈もないため、速度と出力一致を測り、精度の推定はしていない。

## Windowsでの再現

リポジトリ直下で実行する。`trial` と結果は新しい名前を使い、既存の測定を上書きしない。
大きな辞書・入力・ZIP・ビルド成果物は `target/` に置き、追跡しない。
以下はPowerShell。`git apply` はリポジトリ直下から適用先を明示する。
リポジトリ内のスナップショットへ移動して通常の `git apply` を呼ぶと、親リポジトリを拾ってパッチをスキップすることがある。

```powershell
$env:MISE_DISABLE_TOOLS = 'aqua:astral-sh/uv,github:PyO3/maturin'
$trial = Join-Path (Get-Location) 'target/hsd-repeat-windows-new'
New-Item -ItemType Directory -Path "$trial/v5" | Out-Null
git archive --format=zip -o "$trial/source.zip" fefb950
Expand-Archive -LiteralPath "$trial/source.zip" -DestinationPath "$trial/v5-source"
Expand-Archive -LiteralPath "$trial/source.zip" -DestinationPath "$trial/patch-source"
git apply --directory=target/hsd-repeat-windows-new/patch-source docs/measurements/hsd-pronunciation-20261004/reader.patch
gh release download v26.9.107 --repo owayo/hasami --pattern ipadic.hsd --pattern ipadic-neologd.hsd --pattern ipadic-neologd-sudachi.hsd --dir "$trial/v5"
curl.exe -fL https://www.rondhuit.com/download/ldcc-20140209.tar.gz -o "$trial/ldcc-20140209.tar.gz"
mise exec -- python scripts/hsd-format-corpus.py "$trial/ldcc-20140209.tar.gz" "$trial/v5-source" "$trial/corpus"
curl.exe -fL https://www.aozora.gr.jp/cards/000148/files/752_ruby_2438.zip -o "$trial/botchan.zip"
mise exec -- python docs/measurements/hsd-pronunciation-20261005-windows/prepare-literature.py "$trial/botchan.zip" "$trial/corpus/literature.txt"
mise exec -- rustc --edition=2024 -O scripts/hsd-pronunciation-probe.rs -o "$trial/probe.exe"
& "$trial/probe.exe" "$trial/patch" "$trial/v5/ipadic.hsd" "$trial/v5/ipadic-neologd.hsd" "$trial/v5/ipadic-neologd-sudachi.hsd"
foreach ($variant in @('v5', 'patch')) {
    mise exec -- cargo build --release --locked --lib --manifest-path "$trial/$variant-source/Cargo.toml"
    mise exec -- rustc --edition=2024 -O -C lto=fat -C codegen-units=1 scripts/hsd-format-eval.rs --extern "hasami=$trial/$variant-source/target/release/libhasami.rlib" -L "dependency=$trial/$variant-source/target/release/deps" -o "$trial/eval-$variant.exe"
}
foreach ($name in @('ipadic', 'ipadic-neologd', 'ipadic-neologd-sudachi')) {
    foreach ($corpus in @('mixed', 'literature')) {
        mise exec -- python scripts/hsd-format-compare.py "$trial/eval-v5.exe" "$trial/v5/$name.hsd" "$trial/eval-patch.exe" "$trial/patch/$name.hsd" "$trial/corpus/$corpus.txt"
    }
}
# 上の検証・ビルドが成功し、manifestと6辞書・入力のハッシュが一致したことを確認してから測る。
$env:HASAMI_BENCH_AFFINITY = '4'
mise exec -- python scripts/hsd-pronunciation-remeasure.py "$trial/results" "$trial/v5" "$trial/patch" "$trial/eval-v5.exe" "$trial/eval-patch.exe" "$trial/corpus/mixed.txt" 12 "$trial/corpus/literature.txt"
```

保存した統計の再確認は `mise exec -- python docs/measurements/hsd-pronunciation-20261005-windows/audit.py`。
新しい測定値を監査する場合は、auditと一緒にその結果のanalysis/load/summaryを別ディレクトリへ置く。
今回の記録に新しい標本を混ぜない。

6辞書は前回と同一で、圧縮は再実行していない。圧縮後サイズは第12節を参照する。
正式writer・上流からの移行・型を広げない別案・UniDic・cold cache・FFI/Pythonなどの未検証範囲は第14.4節に残す。
