# 測定結果

API は表層と UTF-8 span の取得・checksum 消費まで。ロードに runtime 起動は含まない。


## ネイティブ API

| 設定 | ロード ms | ロード＋初回 ms | 解析 ms（中央値） | IQR ms | CV | トークン数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| hasami-ipadic | 2.075 | 3.341 | 67.278 | 14.180 | 0.17 | 57110 |
| hasami-neologd | 1.782 | 2.697 | 86.008 | 28.183 | 0.70 | 56214 |
| hasami-merged | 1.520 | 3.440 | 84.237 | 46.110 | 0.50 | 55790 |
| mecab-ipadic | 4.103 | 4.733 | 67.986 | 36.009 | 0.52 | 57070 |
| sudachi-java-A | 292.771 | 323.665 | 586.534 | 361.114 | 0.56 | 58036 |
| sudachi-rs-A | 124.405 | 124.798 | 231.103 | 51.197 | 0.20 | 58036 |

## Python バインディング

Python からの呼出し・トークンオブジェクトの処理・UTF-8 変換・checksum の Python ループを含む。モジュール import はロード区間の外。

| 設定 | ロード ms | ロード＋初回 ms | 解析 ms（中央値） | IQR ms | CV | トークン数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| hasami-python-ipadic | 2.634 | 4.258 | 673.450 | 94.517 | 0.15 | 57110 |
| hasami-python-neologd | 1.698 | 2.886 | 688.711 | 148.453 | 0.77 | 56214 |
| hasami-python-merged | 2.968 | 4.812 | 712.618 | 242.807 | 0.40 | 55790 |
| mecab-python-ipadic | 5.406 | 5.859 | 860.426 | 483.104 | 0.29 | 57070 |
| sudachi-python-A | 79.878 | 80.855 | 874.538 | 80.438 | 0.08 | 58036 |

## プロセス最大 RSS

入力・辞書・runtime を含む。各ラウンドの中央値。

| 設定 | MiB |
| --- | ---: |
| hasami-ipadic | 19.812 |
| hasami-neologd | 85.133 |
| hasami-merged | 94.234 |
| hasami-python-ipadic | 41.875 |
| hasami-python-neologd | 107.305 |
| hasami-python-merged | 116.219 |
| mecab-ipadic | 24.766 |
| mecab-python-ipadic | 47.844 |
| sudachi-java-A | 337.500 |
| sudachi-rs-A | 147.719 |
| sudachi-python-A | 171.336 |

## アダプタのプロセス全体（hyperfine）

runtime 起動、入力読込、ロード、初回と全行の解析、表層/span 消費、JSON の測定値出力を含む。上流 CLI の比較ではない。

| 設定 | 中央値 ms | IQR ms |
| --- | ---: | ---: |
| hasami-ipadic | 59.890 | 8.659 |
| hasami-neologd | 78.324 | 33.334 |
| hasami-merged | 133.182 | 17.148 |
| hasami-python-ipadic | 1071.643 | 732.042 |
| hasami-python-neologd | 995.378 | 147.305 |
| hasami-python-merged | 970.888 | 42.635 |
| mecab-ipadic | 99.754 | 27.669 |
| mecab-python-ipadic | 837.281 | 94.147 |
| sudachi-java-A | 1429.426 | 214.140 |
| sudachi-rs-A | 301.827 | 70.664 |
| sudachi-python-A | 1169.437 | 148.116 |

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
