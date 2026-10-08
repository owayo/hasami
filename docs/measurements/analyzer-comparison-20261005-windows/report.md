# 測定結果

API は表層と UTF-8 span の取得・checksum 消費まで。ロードに runtime 起動は含まない。


## ネイティブ API

| 設定 | ロード ms | ロード＋初回 ms | 解析 ms（中央値） | IQR ms | CV | トークン数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| hasami-ipadic | 0.190 | 0.255 | 18.530 | 1.174 | 0.04 | 57110 |
| hasami-neologd | 0.204 | 0.268 | 20.068 | 0.802 | 0.04 | 56214 |
| hasami-merged | 0.236 | 0.311 | 20.225 | 0.838 | 0.05 | 55790 |
| mecab-ipadic | 0.433 | 0.463 | 14.871 | 0.413 | 0.03 | 57072 |
| sudachi-java-A | 95.613 | 103.301 | 91.183 | 5.115 | 0.13 | 58036 |
| sudachi-rs-A | 34.729 | 34.791 | 68.435 | 1.752 | 0.02 | 58036 |

## Python バインディング

Python からの呼出し・トークンオブジェクトの処理・UTF-8 変換・checksum の Python ループを含む。モジュール import はロード区間の外。

| 設定 | ロード ms | ロード＋初回 ms | 解析 ms（中央値） | IQR ms | CV | トークン数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| hasami-python-ipadic | 0.188 | 0.263 | 213.939 | 4.679 | 0.03 | 57110 |
| hasami-python-neologd | 0.204 | 0.300 | 217.167 | 13.625 | 0.04 | 56214 |
| hasami-python-merged | 0.205 | 0.273 | 215.350 | 8.960 | 0.03 | 55790 |
| mecab-python-ipadic | 2.146 | 2.192 | 206.619 | 10.531 | 0.05 | 57072 |
| sudachi-python-A | 48.170 | 48.308 | 269.983 | 10.137 | 0.04 | 58036 |

## プロセス最大 Working Set

入力・辞書・runtime を含む。各ラウンドの中央値。

| 設定 | MiB |
| --- | ---: |
| hasami-ipadic | 19.875 |
| hasami-neologd | 59.209 |
| hasami-merged | 64.680 |
| hasami-python-ipadic | 34.098 |
| hasami-python-neologd | 73.559 |
| hasami-python-merged | 78.891 |
| mecab-ipadic | 24.889 |
| mecab-python-ipadic | 39.799 |
| sudachi-java-A | 291.406 |
| sudachi-rs-A | 127.148 |
| sudachi-python-A | 143.025 |

## アダプタのプロセス全体（hyperfine）

runtime 起動、入力読込、ロード、初回と全行の解析、表層/span 消費、JSON の測定値出力を含む。上流 CLI の比較ではない。

| 設定 | 中央値 ms | IQR ms |
| --- | ---: | ---: |
| hasami-ipadic | 34.979 | 1.296 |
| hasami-neologd | 54.279 | 1.010 |
| hasami-merged | 56.034 | 1.476 |
| hasami-python-ipadic | 357.565 | 12.746 |
| hasami-python-neologd | 372.207 | 6.549 |
| hasami-python-merged | 371.301 | 7.146 |
| mecab-ipadic | 35.177 | 0.909 |
| mecab-python-ipadic | 403.431 | 3.422 |
| sudachi-java-A | 536.960 | 5.553 |
| sudachi-rs-A | 143.043 | 2.455 |
| sudachi-python-A | 531.237 | 6.673 |

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
