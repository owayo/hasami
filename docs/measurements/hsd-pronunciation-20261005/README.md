# 発音差分案の追試（2026-10-05）

[hsd-format.md 第13節](../../hsd-format.md#13-2026-10-05発音差分案の追試でも-v5-を維持) に結果と採否を残す。
前回の3片の発音差分案を再測定し、現行v5を維持した。
新しいTAIL表現や24Bを維持する表現は試作していない。

準備時の負荷平均は約15.5、入力確認開始のmanifestには約28.6、測定前後の記録は11.36〜20.96。
今回も低負荷の比較ではなく、一定の退行率や同等性を証明する結果ではない。

## 保存したデータ

- [manifest.json](manifest.json)：コミット、辞書・実行ファイル・計測器・入力のサイズとSHA-256、条件。
- [analysis.jsonl](analysis.jsonl)：混合入力と短文、3辞書×2方式×12ラウンドの144件。CPU/wall、プロセス全体のピークRSS、実行前後の負荷、各組の順序、トークン数。
- [load.jsonl](load.jsonl)：3辞書×2方式×12プロセス×31回の2,232件。プロセス内先頭12件と、続く再構築360件を方式・辞書ごとに分ける。
- [summary.json](summary.json)：中央値、全範囲、各組のCPU比、差分案が遅かった組数。
- [time-logs.jsonl](time-logs.jsonl)：macOS `time -l` の216プロセスの出力。ファイル名と本文を1行ずつ保存。
- [IPAdic](compare-ipadic.txt)、[NEologd](compare-ipadic-neologd.txt)、[推奨辞書](compare-ipadic-neologd-sudachi.txt)：今回再実行した全トークン全フィールドの直接比較。子プロセスのstderrが同じファイルへ書くため、トークン数の表示に文字の混在がある。直接比較したバイト列は3辞書とも一致。
- [review.md](review.md)：集計・結果の解釈の検証と指摘への対応。

入力・辞書は [前回のmanifest](../hsd-pronunciation-20261004/manifest.json) と同一。
両方式のビルドをやり直し、全14,895,138トークンの比較を再実行してから速度を測った。
6個の辞書の非圧縮サイズも実ファイルで再確認した。今回は再圧縮していない。
圧縮後サイズは同一辞書に対する [前回の記録](../hsd-pronunciation-20261004/compression.jsonl) を参照する。

## 測定条件と結果

基準実行ファイルは `ec63a40`、試作は `fefb950` に前回の [reader.patch](../hsd-pronunciation-20261004/reader.patch) を適用。
測定時のmainは `12b7659`。以下の差分が空で、解析・依存・ツール条件が同じであることを確認した。

```sh
git diff fefb950 ec63a40 -- src Cargo.toml Cargo.lock build.rs hasami-python mise.toml
git diff ec63a40 12b7659 -- src Cargo.toml Cargo.lock build.rs hasami-python mise.toml
```

Rust 1.98.1、Python 3.13.15、Apple M2・24 GiB、macOS 27.0.1。
各プロセスで1周ウォームアップして次の1周を測る。AB/BAを交互にし、辞書順もラウンドごとに巡回する。
USER_INTERACTIVE QoSを指定するがPコアへの固定ではない。ビルド・回帰比較・圧縮は速度測定と重ねない。
全サンプルを残し、除外は行っていない。混合入力143,874行と、固定短文100,000行は分けて評価する。

| 辞書 | 混合CPU中央値の比 | 混合の各組の比の中央値 | 短文CPU中央値の比 | 混合RSS中央値の削減 |
| --- | ---: | ---: | ---: | ---: |
| ipadic | +2.73% | +2.84% | +5.24% | 0.563 MiB |
| ipadic-neologd | +3.21% | +2.02% | +1.28% | 3.391 MiB |
| ipadic-neologd-sudachi | +2.26% | +2.30% | +5.15% | 3.516 MiB |

プラスは差分案のCPU時間が長いことを表す。差の範囲と各条件の組数は第13節とsummaryに併記した。
推奨辞書の混合入力はAB順で+4.55%、BA順で−2.56%と、順序で向きも変わる。
**速度を保てる採用条件を確認できないため、3片案の採用見送りを維持する。**
今回の測定から2〜5%という一定の退行率を一般化しない。借用型24B→56Bは解消しておらず、遅さの原因も切り分けていない。
ロードはOSページが温まった条件で、プロセス内先頭もストレージからのcold-startではない。
再構築360件は12プロセス内の反復で、360独立試行ではない。

## 再現

大きな辞書・コーパス・ビルド成果物は追跡しない。
辞書とリーダーの生成・ビルドは [前回の手順](../hsd-pronunciation-20261004/README.md) を使い、ライブラリのビルド先を方式ごとに分ける。
入力はリリース `v26.9.107` のv5配布品で、manifestのSHA-256を確認する。実験版5006は製品のリーダーでは拒否される。

混合入力の技術文書は `fefb950` のREADMEと直下の `docs/*.md` を使う。
現在の文書で生成すると入力が変わる。前回のコーパスを取り直す手順は次のとおり。

```sh
trial_dir=$(mktemp -d /tmp/hasami-hsd-repeat.XXXXXX)
mkdir -p "$trial_dir/base-snapshot"
git archive fefb950 README.md docs | tar -x -C "$trial_dir/base-snapshot"
curl -fL --retry 2 --connect-timeout 15 --max-time 300 \
  https://www.rondhuit.com/download/ldcc-20140209.tar.gz -o "$trial_dir/ldcc-20140209.tar.gz"
mise exec -- python scripts/hsd-format-corpus.py \
  "$trial_dir/ldcc-20140209.tar.gz" "$trial_dir/base-snapshot" "$trial_dir/corpus"
```

今回この手順の生成処理も再実行し、news / tech / edge / mixedの4ファイルが前回と同じハッシュになることを確認した。
技術文書のコミット指定は、生成スクリプト冒頭にある古いv4比較用の指定より、この追試のmanifestを優先する。
ニュースの本文は保存せず、アーカイブの識別子と抽出規則だけを残す。

以下は生成した辞書・実行ファイルの置き場所を `trial_dir` にそろえた実行例。
`v5/` と `patch/` は辞書、`eval-v5` と `eval-patch` は同じ `hsd-format-eval.rs` からビルドした実行ファイル。

```sh
for name in ipadic ipadic-neologd ipadic-neologd-sudachi; do
  mise exec -- python scripts/hsd-format-compare.py \
    "$trial_dir/eval-v5" "$trial_dir/v5/$name.hsd" \
    "$trial_dir/eval-patch" "$trial_dir/patch/$name.hsd" "$trial_dir/corpus/mixed.txt"
done
mise exec -- python scripts/hsd-pronunciation-remeasure.py \
  "$trial_dir/results" "$trial_dir/v5" "$trial_dir/patch" \
  "$trial_dir/eval-v5" "$trial_dir/eval-patch" "$trial_dir/corpus/mixed.txt" 12
```

`results` は未作成のディレクトリを指定する。固定短文は測定器が生成し、そのハッシュも今回のmanifestに残した。
再構築した結果のハッシュとmanifestを突き合わせ、一時ファイルの残存だけを再現条件にしない。
正式writer・上流からの再構築と移行・各feature・FFI/Python・32 bit・UniDic・cold cacheの確認は今回行っていない。
