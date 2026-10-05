# Windows の形態素解析 API 比較（2026-10-05）

AMD Ryzen 7 5700X（8 コア・16 論理 CPU）、搭載 RAM 32 GiB、Windows 11 Home 10.0.26300 x64。
Rust 1.99.0、Python 3.14.8、Temurin Java 27+35、MSVC 19.51.36260.0、CMake 4.4.4、hyperfine 1.20.0。
前回と同じ固定正式版を再測定した。[版と commit](versions.json)、[条件・ハッシュ](manifest.json)。
hasami Rust / 公式 PyO3 はともに v26.9.107 / `1c7564aa3ad1031d9cf429d57e42aabec781a755`。
Sudachi Java 0.8.2 / sudachi.rs・SudachiPy 0.7.0 は同じ 20260723.1 core / V1、A モード、組み込み既定設定。
Java は `-Xms256m -Xmx1g`。この再測定では版の更新や最新版の確認はしていない。

入力は青空文庫の夏目漱石『坊っちゃん』482 行・265,281 B。UTF-8・LF。
固定 ZIP から本文を抽出し、ルビ・ルビ開始記号・入力者注記を除き、段落ごとに 1 行とした。章見出しと著者の日付は残す。
[取得元・抽出規約・ハッシュ](corpus-manifest.json)、[底本・入力者・校正者](corpus-NOTICE.txt)。
入力と hasami 3 辞書・SudachiDict は [前回の Mac 測定](../analyzer-comparison-20261004/README.md) と同じハッシュ。
元の CRLF 本文を変更せず、別の LF 入力を使った。

MeCab C API / Python は mecab-python3 1.0.12 の Windows wheel に含まれる同じ MeCab 0.996 DLL と同じ辞書を使う。
IPAdic は [ipadic 1.0.0](https://pypi.org/project/ipadic/1.0.0/) のコンパイル済み UTF-8 辞書。
前回の Mac と char.bin・matrix.bin・dicrc は同じだが sys.dic・unk.dic は異なる。
本文の MeCab トークン数も前回 57,070、今回 57,072。[取得元・全ファイルのハッシュ](dictionary-sources.json)。
OS・ビルド・辞書バイナリが異なるため、前回との時間差を速度改善率には換算しない。
hasami の修復済み辞書とほかの辞書は語彙・分割単位が違い、エンジンだけの優劣には換算しない。

各設定で全行 10 周の warmup 後に 10 周を計測する新しいプロセスを 6 ラウンド、先頭を回転し奇数ラウンドで反転して実行した。
API は各 60 標本、ロードとロード＋初回は各 6 標本。中央値・inclusive quartile の Q3−Q1・母標準偏差÷平均の CV を出した。
[report.md](report.md)、[summary.json](summary.json)、[全 660 標本](api.jsonl)。外れ値は削除していない。
解析を呼ぶスレッドは 1 本で、全 16 論理 CPU を利用できる affinity、通常の優先度。Java の補助スレッドも通常動作のまま。
ビルド・辞書生成・別のベンチマークを重ねていない。
ページと解析器のキャッシュは温まっており、ストレージからの cold-start は未計測。
計測プロセスと他のアプリ・OS を含むプロセス実行期間の全体 CPU 使用率は中央値 8.2%、範囲 6.8〜16.3%。
Java の API CV は 0.13、ほかは 0.02〜0.05で、Java の変動が残る。

ロードは入力読込・runtime 起動・import の後、辞書・設定・tokenizer の作成まで。mmap 全ページを読み切る時間ではない。
初回はロード開始から最初の 1 行を解析して結果を消費するまで。
API は全行の解析・表層・UTF-8 span・FNV-1a 64 checksum の消費まで。ファイル読込と JSON 書式化は区間外。
hasami API が作る全フィールド、Python の checksum ループ、Java の位置換算の費用は含む。
ネイティブ/Python の時間差をバインディングだけの費用とは扱わない。
hyperfine は専用アダプタのプロセス全体。起動・入力・ロード・初回・全行解析・checksum・測定 JSON 出力を含む。
上流 CLI 同士の比較ではない。`--shell=none --warmup 2 --runs 10`。[全 110 回](hyperfine.json)。

Windows の最大メモリは PeakWorkingSetSize。終了後も保持した実プロセスの handle から K32GetProcessMemoryInfo を取得した。
入力・runtime・辞書・warmup・全反復を含む全体のピークを、6 プロセスの中央値で表示する。macOS/Linux の最大 RSS と別の指標。
初回だけの最大メモリは未計測。Python は仮想環境のランチャーを経由せず、実 interpreter が各仮想環境のパッケージを読む。
解放済み 64 MiB のピークが終了後も残ることと Unicode 出力をテストし、日本語の JSON 出力は UTF-8 を明示した。

全 5,302 行の検証出力で表層・UTF-8 span・欠落・各行と全体の checksum を照合した。
計測時の件数と checksum も一致。5 ペアのネイティブ/Python は表層・位置・品詞・読み・原形が全行で一致した。
共通フィールドの SHA-256 は manifest に残している。
別の計算式で全標本の統計・実行順・hyperfine・全行出力・fixture の分母を再確認した。[照合記録](audit.json)。
実行器のソースや大きな全行出力はここには含めていない。

精度は [自作 fixture](regression-fixture.jsonl) 6 文・25 トークンの [回帰確認](accuracy.json) のみ。
品詞は 25、読み・原形は句点を除く 19 トークン。「私」「明日」はワタシ・アシタを期待値とし、Sudachi の別の読みが 89.5% に出る。
人による注釈レビューは未実施。『坊っちゃん』の正解注釈と汎用精度は未計測。
fixture の原本は CRLF で意味は前回と同じ。保存資料は `.gitattributes` で改行変換を抑え、測定時のハッシュを保っている。
