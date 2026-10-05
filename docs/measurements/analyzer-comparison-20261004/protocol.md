# アダプタと評価の契約

アダプタは `--dictionary PATH --input FILE --action ACTION --mode A --warmup N --iterations N` を受け取る。
Sudachi は任意の `--config PATH` も指定できる。入力は UTF-8、改行は LF/CRLF、空行を保持する。
行の終端を解析に渡さず、各行を独立に解析する。位置は各行先頭を 0 とした UTF-8 バイト位置で、終端は排他的。
空のトークンと Unicode White_Space だけのトークンを出力・件数・checksum から除く。

| action | 出力 |
| --- | --- |
| load | ロードとロード＋初回の ns、入力条件の JSON 1 行 |
| bench | warmup で全行を N 周、その後各 iteration の全行解析 ns・件数・checksum を JSONL で出力 |
| tokens | 入力ごとに line/text/tokens/count/checksum。tokens は surface/start/end/pos/reading/lemma |
| cli | warmup なしでロード・初回・全行 1 周・測定 JSON の出力。hyperfine の対象 |

表層は正規化前の原文と一致しなければならない。Java の位置は元の UTF-16 の位置から UTF-8 へ換算する。
重複・範囲外・文字の途中の位置、非空白文字の欠落、入力行の欠落、出力と計測の件数/checksum の不一致はエラーにする。
辞書の読み込み失敗や解析失敗は終了コード非ゼロとし、結果表に採用しない。

checksum は FNV-1a 64。初期値 `cbf29ce484222325`、素数 `100000001b3`、各操作は 64 bit で折り返す。
各トークンの UTF-8 表層、区切り 1B `ff`、start/end の各 little-endian u64 を順に消費する。
`tokens` は行ごとに初期化し、`bench` は全行を通して継続する。表示は小文字 16 桁の hex。
checksum は同じアダプタの検証と測定を照合するためのもの。異なるエンジン同士の正解判定には使わない。

辞書ロードは mmap 全体を読み切った時間ではない。設定読み込みと tokenizer 作成を含む。
Java は runtime 起動後の初回の辞書初期化を測るため、関連クラスの初期化費用が残る。
API 時間には native API が行う解析と表層/span の取得、UTF-8 の消費を含む。手動で結果を捨てて解析を省略しない。
ns の記録は単調時計の区間差で、JSON の書式化・I/O は区間外。
最大 RSS は macOS `time -l` / Linux `time -v` でプロセス全体を測り、bytes にそろえる。
Python 版は公式の hasami PyO3 と mecab-python3 を呼び、共通 Python harness で測る。
import はロード区間の外。トークンの生成・属性取得・UTF-8 変換・Python checksum ループは解析区間に入る。
ネイティブ API と Python API は別表にする。hasami は同じ公式タグ、各エンジンは同じ辞書を共有する。

ラウンドごとに新しいプロセスを起動する。順序は先頭を回転し、奇数ラウンドでは反転する。
各プロセスで最初の 1 行を解析した後、全行の warmup と反復を行う。
Java のウォームアップ完了を反復回数だけで保証しない。CV/IQR、各反復と GC/JIT の影響を見て、必要なら周回を増やす。
プロセス全体は hyperfine `--shell=none --warmup 2 --runs N` で、成功したコマンドだけを測る。

## 正解 JSONL

各行は `text` と `tokens` を持つ。各トークンには UTF-8 バイト位置、surface、任意の pos/reading/lemma を記す。
未注釈フィールドは `null`。読みはカタカナ、原形は辞書形、品詞は共通の大分類とする。
品詞の写像は固定し、補助記号→記号、形状詞→形容動詞、代名詞→名詞、接尾辞→接尾、接頭辞→接頭詞。
IPAdic の下位区分はこの大分類評価には使わない。異なる辞書形や読みの慣習は評価セットの作成時に規約を決める。

境界 F1 は行の終端以外の各トークンの end 位置に対する precision/recall/F1。
span F1 は start/end の組が正解と一致するトークンの precision/recall/F1。
空白のある評価データでは、空白の扱いを同じ規約にそろえたうえで両方を確認する。
品詞・読み・原形の主指標は、分割も該当フィールドも一致した件数を、そのフィールドが注釈された全 gold トークンで割る。
分割の不一致やエンジンの値の欠落を分母から落とさない。
一致 span に限った条件付き正解率と、注釈済み gold に対する値の提供率も JSON に残す。

標準の正解 fixture は自作の 6 文・25 トークン。品詞は全トークン、読み・原形は句点を除いた 19 トークンを注釈した。
人による注釈レビューは未実施のため、回帰確認用の期待値として扱う。
文の内容と正解はエンジンの出力から作っていない。複数の正しい読みがある語についてはこの fixture の選択を明記する。
この fixture の点数で汎用精度や文学全体への性能を主張しない。
