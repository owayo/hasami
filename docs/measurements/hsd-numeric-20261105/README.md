# 数値・単位・月の格納と文脈IDの共有（2026-11-05）

**今回の第一候補は、全通常エントリで左右文脈IDが等しい辞書に限り、IDを一方だけ持つ4B形式。**
推奨辞書の容量試作は10,113,664B（4.58%）減った。
数量キーの分離と数量の読み・発音の共有は、この試作ではそれぞれ118,164B・108,763B減にとどまった。
月だけの特別な表現は推奨辞書で2,396B増えた。
実際のreaderの速度・メモリを測っていないため、正式なv6は採用せず、本番はv5を維持する。

## 入力と調べた範囲

- 基準コード：`3b95cde736cc618d2e904b5f79deb138434b69c8`。
- 指定の `78f63181f2f436f8dd819fee5e2ff9d422fa1b52` 以降の61件は [commits.txt](commits.txt)。
  月・日・年・負小数・複合単位の追加処理が形式の圧縮へ与える条件を [design.md](design.md) に整理した。
- 手元のv5辞書3種を固定。メタデータの作成版はすべて `26.10.107`。
  基準コードと辞書の作成版は異なる。最新版の上流を再取得・再構築した比較ではない。
- Ryzen 7 5700X、Windows 10.0.26300、Rust 1.98.1、Python 3.13.15。
  解析用ライブラリは `--release --locked --lib --no-default-features --features build`。
  評価器は `-O -C lto=fat -C codegen-units=1`。
- `.hsd.zst` は公式Windows版zstd 1.5.7、`-19 -T1`、一度に1ファイルずつ測定した。
  失敗した中間実行は集計せず、入力・結果のハッシュと版が同じ標本だけ再利用した。
  圧縮結果は全15件をストリームで展開し、元の`.hsd`のSHA-256との一致も確認した。

| 辞書 | 元のB数 | SHA-256 |
| --- | ---: | --- |
| ipadic | 16,496,162 | `26a0c1b3c5555d7af89a4f81358d96a7b6c556b99b0ac6a7df76d0ec8c7a2369` |
| ipadic-neologd | 206,574,882 | `bc27fae64ed21994c3fd7addcd15e6631a412a7c93715ea684b157aa1667814e` |
| ipadic-neologd-sudachi | 220,985,250 | `4110fcf6bb8d7c98be12894066f592117ce24f920e49ff6102e32308d74637d8` |

公開記録には独自辞書のCSVや入力リポジトリのメタデータを含めず、辞書の識別に必要な集計だけ残した。
大きな辞書・展開結果・実行ファイルは`target/`に置き、追跡しない。

## 数量語・月の実態

計測器の`PREFIX`で、符号付きの数字・漢数字、小数点・桁区切りを含む先頭部分を分ける。
後続部が`SUFFIXES`の明示した集合に完全一致するものを選ぶ。
これは字面の分類で、人名・地名等も含み得る。数値の妥当性や数量の意味を判定するものではない。
先頭が数値でない数量や全単位表記を網羅せず、今回の分類結果を全数量の圧縮余地の上限にしない。

| 集計 | ipadic | ipadic-neologd | 推奨辞書 |
| --- | ---: | ---: | ---: |
| 全キー | 324,821 | 4,344,063 | 4,713,074 |
| 符号または数値から始まるキー | 7,817 | 168,962 | 172,715 |
| 選択した数値＋後続部 | 271 | 59,814 | 59,828 |
| そのエントリ | 371 | 60,117 | 60,131 |
| そのキーだけが参照する素性B数 | 1,933 | 1,006,149 | 1,006,141 |
| 月だけのキー / エントリ | 25 / 30 | 39 / 44 | 39 / 44 |

推奨辞書の22,629種類の数値表記と105種類の後続部の直積は2,376,045組。
実在する59,828組を保持せず直積へ広げると、約40倍の組を認可してしまう。
月の44エントリには43種類の単語コストがあり、単一コストの月規則へ置き換えることも等価ではない。

現在の月や単位の読み表はRustコードにあり、それをHSDへ移すだけではHSDは小さくならない。
「1日」の暦日・期間の読み分けや、名前・識別子・独自素性の保護も含め、
保存のための共有化とラティス候補を規則から生成する機能は分ける。

## 完成ファイルの容量

次は元のファイルから**減ったB数**。負数は増加。各案は単独で、合算していない。
書き出したセクション表・64B整列を含み、単なるレコード長の試算ではない。

| 試作 | 版 | ipadic | ipadic-neologd | 推奨辞書 |
| --- | ---: | ---: | ---: | ---: |
| 数量キーを接頭部＋後続部の疎な表へ | 5011 | −4,680 | 67,403 | 118,164 |
| 月キーだけ同じ表へ | 5011 | 25 | −1,628 | −2,396 |
| 数量素性の前半＋語尾を共有 | 5012 | −307 | 108,834 | 108,763 |
| 左右IDの組番号＋共有表 | 5009 | 776,406 | 9,308,246 | 10,108,310 |
| 左右が同じ場合の一方省略 | 5010 | 781,760 | 9,313,600 | 10,113,664 |

キーの分離では一般trieを再構築するため、文字頻度・符号・配置まで変わり、
キーを減らせば必ずtrieが小さくなるわけではない。月だけの分離で大きい2辞書が増えるのもこのため。
数量の語尾共有は音便を推定せず、元のバイト列を分割して保存した。
共有後の前半文字列に別のプールが必要なので、本体の削減量をそのまま容量削減にはできない。
この方式の結果であって、さらに良い分解や規則生成を試し尽くした結果ではない。

3辞書の全390,861 / 4,656,790 / 5,056,839通常エントリは左右IDが同一だった。
これをビルド時に確認できる辞書なら、`context_and_last u16 / cost i16`で6Bを4Bにできる。
追加の表の参照が要らず、同点・群の末尾・単語コスト・論理エントリ番号を保てる。
非対称な独自辞書は6Bに戻す。正式なreaderでモード分岐をホットループの外へ出せるかは未検証。

## 圧縮配布サイズ

同じ順に**減ったB数**を示す。基準`.zst`は5,818,761 / 66,909,873 / 71,763,030B。

| 試作 | ipadic | ipadic-neologd | 推奨辞書 |
| --- | ---: | ---: | ---: |
| 数量キーの分離 | 112,757 | 117,020 | 96,023 |
| 月キーだけ分離 | 113,192 | −7,344 | 9,254 |
| 数量素性の語尾共有 | 108,969 | −107,471 | −93,691 |
| 左右が同じ場合の一方省略 | 11,971 | 131,403 | 149,076 |

IPAdicで月の非圧縮削減25Bに対して圧縮後113,192B減という値も、完成ファイルの実測である。
ただしセクション追加・配置・zstdのブロック境界も変わるため、差を月データ自体の削減量へ帰属させない。
圧縮前後は別の評価軸であり、正式なwriterの最終配置・ヘッダを含む配布サイズの保証でもない。
組番号案は追加表を使わない省略案より複雑になるため、圧縮比較の対象から外した。
全標本・入力ハッシュ・圧縮時間・展開検証は [compression.jsonl](compression.jsonl)。
記録した秒数は圧縮の時間で、解析・ロード・辞書構築の性能ではない。

## 検証と未検証のもの

入力3辞書は現行ライブラリの全件検証に成功した。
`verify-*.txt`はその結果。試作では次を直接比較した。

- 数量・月の追加表から復元する全選択キーと群ID、一般trieの全残存キーの完全一致検索。
- 固有素性327,989 / 3,039,242 / 3,358,670件の元v5バイト列への復元。
  全エントリの素性オフセットの対応も確認した。
- 文脈ID両案の全通常エントリを元の6B列へ復元。
- 最終`.hsd`を読み直した全セクションのペイロードとヘッダ・範囲・整列。
  別ディレクトリで再生成した全試作のSHA-256も一致した。
- 独立レビューで別実装による範囲・整列・非重複と変更外の一致、および省略案の全エントリ復元。

独立レビューの指摘と反映は [review.md](review.md)。指定CLIによるadvancedレビューとは区別した。
Python計測器のRuff検査・整形と、Rust計測器の警告ゼロのコンパイル・整形も確認した。

**Analyzer用の試作readerは未実装**。そのため、検索・解析速度、ロード＋1文、cold cache、
メモリ・ヒープ確保、全トークン全フィールドの回帰、破損入力のreaderテストは未実施。
一度のtrie再構築の内部時間は集計に残るが、並行負荷の統制はしておらず、
writer全体の構築時間・ピークメモリの比較に使わない。
全feature・FFI/Python・32bit・UniDicも試作版では未検証。
本番コードを変えていないため、形式変更後の`make ci`や上流からの正式再構築は実施していない。
`.hsd`の削減量から、速度やmmapのメモリ削減量を推定しない。

## 再現手順

リポジトリの基準コミットで、上表と同じSHA-256の辞書を`dict/`に用意する。
異なる内容の辞書で同じバイト数になるとは限らない。次はPowerShell。

```powershell
$trial = 'target/hsd-numeric-reproduce'
New-Item -ItemType Directory -Force -Path $trial | Out-Null
mise exec -- cargo build --release --locked --lib --no-default-features --features build
mise exec -- rustc --edition=2024 -O -C lto=fat -C codegen-units=1 -D warnings scripts/hsd-numeric-probe.rs --extern hasami=target/release/libhasami.rlib -L dependency=target/release/deps -o "$trial/probe.exe"
foreach ($destination in @($trial, "$trial-verify")) {
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    if ($destination -ne $trial) {
        Copy-Item -LiteralPath "$trial/probe.exe" -Destination "$destination/probe.exe"
    }
    mise exec -- python scripts/hsd-numeric-analyze.py $destination dict/ipadic.hsd dict/ipadic-neologd.hsd dict/ipadic-neologd-sudachi.hsd --rebuild-tries > "$destination/probe.jsonl"
    mise exec -- python scripts/hsd-numeric-features.py $destination dict/ipadic.hsd dict/ipadic-neologd.hsd dict/ipadic-neologd-sudachi.hsd > "$destination/features.jsonl"
}
mise exec -- rustc --edition=2024 -O -C lto=fat -C codegen-units=1 -D warnings scripts/hsd-format-eval.rs --extern hasami=target/release/libhasami.rlib -L dependency=target/release/deps -o "$trial/eval.exe"
foreach ($name in @('ipadic', 'ipadic-neologd', 'ipadic-neologd-sudachi')) {
    & "$trial/eval.exe" verify "dict/$name.hsd" > "$trial/verify-$name.txt"
}
gh release download v1.5.7 --repo facebook/zstd --pattern zstd-v1.5.7-win64.zip --dir $trial
Expand-Archive -LiteralPath "$trial/zstd-v1.5.7-win64.zip" -DestinationPath "$trial/zstd"
mise exec -- python scripts/hsd-numeric-compress.py $trial "$trial/zstd/zstd-v1.5.7-win64/zstd.exe" dict/ipadic.hsd dict/ipadic-neologd.hsd dict/ipadic-neologd-sudachi.hsd
mise exec -- python docs/measurements/hsd-numeric-20261105/collect.py $trial "$trial-verify"
```

集計器は`summary.json`の全入力・出力ハッシュを検査し、公開するメタデータを絞る。
再現コードの生バイト列は [measured-sources.zip](measured-sources.zip) に保存した。
改行変換でソースSHA-256が変わった場合も、アーカイブ内のバイト列と照合できる。
集計器を再実行すると今回の結果ファイルを更新するので、比較を残す場合は先に別の保存先へ退避する。

数量のFSTや生成規則は今後の別案。まず左右同一IDの省略だけを単独でreader/writerへ試作し、
3辞書・混合/短文/文学の全フィールド回帰とAB/BAの速度・メモリを確かめる。
採用時だけ版を上げ、固定上流・同じrepair工程から正式に再構築する。
