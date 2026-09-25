# ベンチマーク

```bash
# 同じ文を繰り返す
hasami bench --dict dict/ipadic-neologd.hsd --text "東京都に住んでいる人々が増えている。" --iterations 100000

# 1 行 1 文のファイルの全行を解析する時間（ファイル全体を 3 回解析して最速の回）
hasami bench --dict dict/ipadic-neologd.hsd --file corpus.txt
```

livedoor ニュースコーパスの本文 132,876 行（24.3MB）で測った値。Apple M2（P コア 4 + E コア 4）。

## 解析速度（ライブラリ、1 スレッド）

`Analyzer::try_tokenize` を行ごとに呼んで全行を解析する時間（`hasami bench --file` と同じ。出力の書式化なし）。

| 辞書 | 時間 | 速度 |
|------|-----:|-----:|
| ipadic | 1.00s | 24 MB/s |
| ipadic-neologd | 1.29s | 19 MB/s |
| ipadic-neologd-sudachi | 1.34s | 18 MB/s |

## CLI（標準入力 → MeCab 形式）

| | ipadic | ipadic-neologd-sudachi |
|---|---:|---:|
| MeCab 0.996（`mecab -b 1000000`） | 3.09s | — |
| hasami（`-j 1`） | 1.24s | 1.62s |
| hasami（既定。CPU の数だけ並列） | 0.43s | 0.49s |

表の値は、未知語の品詞を unk.def のテンプレートすべてから選ぶようにする（PR #14）前に測ったもの。テンプレートの数だけ
未知語のノードを作るので、解析の時間は約 15% 増えた（変更の前後を交互に走らせた CPU 時間。ipadic・推奨辞書とも）。
推奨辞書は、その前にカタカナの複合語の規則（PR #10）で未知語の候補が減って約 8% 速くなっている（ipadic は変わらない）。

辞書のロードは 3 辞書とも 1ms 未満（mmap。ロード時はヘッダと小さな表だけを検査する）。
計測の方法と、速くしたときに試したこと・見送ったことは [performance.md](performance.md)。
