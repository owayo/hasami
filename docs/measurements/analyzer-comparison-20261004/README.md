# 最新の正式版の形態素解析 API 比較（2026-10-04）

Apple M2・24 GiB、macOS 27.0.1 ARM64。『坊っちゃん』482 行・265,281 B。
Rust 1.99.0、Python 3.14.8、Temurin Java 27、hyperfine 1.20.0。
実装の版と最新版の確認元・日時は [versions.json](versions.json)、
Git commit・辞書と入力のハッシュ・条件は [manifest.json](manifest.json)。
hasami の Python は公式 PyO3 を Rust と同じタグからビルドした（Python パッケージのメタデータは 0.1.0）。

## 測定区間

- ロードは入力読込と runtime/import の後、辞書・設定・tokenizer の初期化まで。
  mmap の全ページを読み切った時間ではない。
- 初回はロード開始から最初の行の解析と表層・UTF-8 位置の消費まで。
- API は全行を 1 周し、表層と UTF-8 span を FNV-1a 64 で消費する。
  ファイル読込と JSON 書式化は区間外。hasami が API 内で組み立てる全フィールドの費用は残る。
  Java の UTF-16 と SudachiPy のコードポイントから UTF-8 位置への変換も区間内。
- Python は通常の tokenize 呼出しとトークン処理を測り、checksum は Python のループ。
  SudachiPy の out パラメータは使っていない。ネイティブ/Python の差をバインディングだけの費用とは扱わない。
- hyperfine は専用実行器のプロセス全体。起動・入力・ロード・初回・全行解析・checksum・測定 JSON 出力を含む。
  上流 CLI の比較ではない。`--shell=none --warmup 2 --runs 10`。
- RSS は `time -l` のプロセス全体の最大値。入力・辞書・ワークスペース・runtime を含む。

API は各設定 10 周のウォームアップ後に 10 周を計測するプロセスを 6 ラウンド、先頭を回転し奇数ラウンドで反転して実行した。
解析を呼ぶスレッドは 1 本。Java の GC/JIT の補助スレッドは通常の動作として残る。
OS のページキャッシュと解析器のキャッシュは温まっている。ビルド・辞書生成・圧縮と同時には測っていない。
他のアプリと OS の負荷は残り、特に Java/Rust Sudachi の CV は 0.25 / 0.29 と大きい。
外れ値を削除せず保存した。この測定から安定した速度比や統計的な同等性は主張しない。

集計は中央値、IQR は inclusive quartile の Q3−Q1、CV は母標準偏差÷平均。
ロードと RSS は反復に同じ値が載るため、各プロセス 1 回だけを集計している。
詳細な出力・checksum・位置・品詞の契約は [protocol.md](protocol.md)。
実行器は本リポジトリに含めていない。ここには条件・ハッシュ・全反復と集計を保存している。

## 入力と辞書

入力は [青空文庫の図書カード](https://www.aozora.gr.jp/cards/000148/card752.html)と
[著作権の切れている作品の取り扱い規準](https://www.aozora.gr.jp/guide/kijyunn.html)を確認した『坊っちゃん』。
固定した原文 ZIP を Shift_JIS から UTF-8 にし、本文だけを抽出、ルビ・ルビ開始記号・入力者注記を除いた。
段落の両端を strip して空段落を除き、1 段落 1 行。章見出しと著者の日付は残した。
URL・ZIP と本文の SHA-256・抽出規約は [corpus-manifest.json](corpus-manifest.json)、
底本・入力者・校正者の情報は [corpus-NOTICE.txt](corpus-NOTICE.txt)。本文の形態素注釈はない。

hasami は v26.9.107 の配布 v5 辞書 3 種。MeCab は OS パッケージの UTF-8 IPAdic を複製したもの。
MeCab Python の wheel は同梱の MeCab を使うため C 版とビルドが異なるが、辞書は同じファイル群を使った。
Sudachi Java / Rust / Python は同じ `20260723.1 core / V1` と A モード、各固定版の組み込み既定プラグインを使った。
取得元と SHA-256 は [dictionary-sources.json](dictionary-sources.json)。
hasami の修復・追加語彙と分割単位は他の辞書と異なる。エンジン単独の優劣には換算しない。

## 結果と正解データ

表は [report.md](report.md)、集計 JSON は [summary.json](summary.json)。
全 API 反復は [api.jsonl](api.jsonl)、プロセス全体は [hyperfine.json](hyperfine.json)。
全入力を出力して、欠落・不正な位置がないことと、計測時の件数/checksum が検証出力と一致することを確かめた。
各エンジンのネイティブ/Python は、表層・位置・品詞・読み・原形が全行で一致した。
比較した共通フィールドの SHA-256 は manifest に残している。

精度表は [自作の回帰 fixture](regression-fixture.jsonl) 6 文・25 トークンに対する [評価結果](accuracy.json)。
人による注釈レビューは未実施で、一般文章の精度の推定には使えない。品詞は 25 トークン、読み・原形は句点を除いた 19 トークン。
「私」「明日」はワタシ・アシタを期待値にしたため、Sudachi のワタクシ・アスとの差が読みの 89.5% に出る。
期待値はエンジンの出力から作っていない。『坊っちゃん』の精度と、独立した大規模正解データでの汎用精度は未計測。

独立した Codex エージェントが 660 標本から中央値・IQR・CV を再計算し、ロード/RSS・hyperfine・照合ハッシュ・fixture の分母を確認した。
公開 README と測定資料の数値に重大な指摘はなく、内部情報の混入も見つからなかった。
実行器の計装そのものは、この記録レビューの範囲には含めていない。
