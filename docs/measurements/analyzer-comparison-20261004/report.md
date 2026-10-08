# 測定結果

API は表層と UTF-8 span の取得・checksum 消費まで。ロードに runtime 起動は含まない。


## ネイティブ API

| 設定 | ロード ms | ロード＋初回 ms | 解析 ms（中央値） | IQR ms | CV | トークン数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| hasami-ipadic | 0.724 | 0.761 | 18.415 | 1.884 | 0.10 | 57110 |
| hasami-neologd | 0.649 | 0.684 | 19.481 | 1.536 | 0.05 | 56214 |
| hasami-merged | 0.606 | 0.640 | 20.022 | 3.515 | 0.14 | 55790 |
| mecab-ipadic | 3.770 | 4.193 | 18.921 | 3.192 | 0.13 | 57070 |
| sudachi-java-A | 91.775 | 100.971 | 131.200 | 28.217 | 0.25 | 58036 |
| sudachi-rs-A | 28.722 | 28.766 | 59.651 | 5.340 | 0.29 | 58036 |

## Python バインディング

Python からの呼出し・トークンオブジェクトの処理・UTF-8 変換・checksum の Python ループを含む。モジュール import はロード区間の外。

| 設定 | ロード ms | ロード＋初回 ms | 解析 ms（中央値） | IQR ms | CV | トークン数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| hasami-python-ipadic | 0.967 | 1.093 | 236.672 | 21.157 | 0.08 | 57110 |
| hasami-python-neologd | 0.674 | 0.733 | 248.431 | 39.270 | 0.14 | 56214 |
| hasami-python-merged | 0.808 | 0.861 | 231.548 | 33.687 | 0.12 | 55790 |
| mecab-python-ipadic | 2.953 | 3.015 | 234.143 | 65.465 | 0.21 | 57070 |
| sudachi-python-A | 33.684 | 33.788 | 272.716 | 39.225 | 0.15 | 58036 |

## プロセス最大 RSS

入力・辞書・runtime を含む。各ラウンドの中央値。

| 設定 | MiB |
| --- | ---: |
| hasami-ipadic | 19.555 |
| hasami-neologd | 84.867 |
| hasami-merged | 93.977 |
| hasami-python-ipadic | 41.422 |
| hasami-python-neologd | 106.844 |
| hasami-python-merged | 115.953 |
| mecab-ipadic | 24.562 |
| mecab-python-ipadic | 47.781 |
| sudachi-java-A | 340.305 |
| sudachi-rs-A | 146.547 |
| sudachi-python-A | 170.406 |

## アダプタのプロセス全体（hyperfine）

runtime 起動、入力読込、ロード、初回と全行の解析、表層/span 消費、JSON の測定値出力を含む。上流 CLI の比較ではない。

| 設定 | 中央値 ms | IQR ms |
| --- | ---: | ---: |
| hasami-ipadic | 28.607 | 2.343 |
| hasami-neologd | 32.433 | 0.825 |
| hasami-merged | 36.173 | 2.647 |
| hasami-python-ipadic | 298.106 | 73.101 |
| hasami-python-neologd | 295.607 | 9.525 |
| hasami-python-merged | 293.986 | 12.124 |
| mecab-ipadic | 32.791 | 0.564 |
| mecab-python-ipadic | 315.440 | 23.490 |
| sudachi-java-A | 779.770 | 91.270 |
| sudachi-rs-A | 140.662 | 15.203 |
| sudachi-python-A | 402.684 | 47.474 |

## 正解 fixture（回帰確認）

| 設定 | 境界 F1 | span F1 | 品詞 | 読み | 原形 |
| --- | ---: | ---: | ---: | ---: | ---: |
| hasami-ipadic | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| hasami-neologd | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| hasami-merged | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| hasami-python-ipadic | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| hasami-python-neologd | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| hasami-python-merged | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| mecab-ipadic | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| mecab-python-ipadic | 100.0% | 100.0% | 100.0% | 100.0% | 100.0% |
| sudachi-java-A | 100.0% | 100.0% | 100.0% | 89.5% | 100.0% |
| sudachi-rs-A | 100.0% | 100.0% | 100.0% | 89.5% | 100.0% |
| sudachi-python-A | 100.0% | 100.0% | 100.0% | 89.5% | 100.0% |

## ネイティブ/Python の出力照合

元入力の表層・UTF-8 span・品詞・読み・原形を全行で比較。正解との精度ではない。

| ネイティブ | Python | 共通フィールド |
| --- | --- | --- |
| hasami-ipadic | hasami-python-ipadic | 一致 |
| hasami-neologd | hasami-python-neologd | 一致 |
| hasami-merged | hasami-python-merged | 一致 |
| mecab-ipadic | mecab-python-ipadic | 一致 |
| sudachi-rs-A | sudachi-python-A | 一致 |
