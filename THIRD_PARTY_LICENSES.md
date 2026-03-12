# Third-Party Licenses

hasami の辞書データには以下のサードパーティデータが含まれています。

## SudachiDict

Copyright (c) 2017-2023 Works Applications Co., Ltd.

Licensed under the Apache License, Version 2.0.
See [LICENSE-APACHE-2.0](LICENSE-APACHE-2.0) for the full license text.

- Repository: https://github.com/WorksApplications/SudachiDict

hasami の統合辞書 (`ipadic-neologd-sudachi.hsd`) では、SudachiDict Core の語彙データを
MeCab IPAdic 互換形式に変換し、品詞体系を IPAdic の left_id/right_id にリマッピングしています。
変換スクリプト: `scripts/convert_sudachi_to_mecab.py`, `scripts/remap_sudachi_to_ipadic.py`

### UniDic (SudachiDict に内包)

Copyright (c) 2011-2013, The UniDic Consortium
All rights reserved.

SudachiDict の語彙データおよび接続行列 (matrix.def) の一部は UniDic に由来します。
UniDic は BSD 3-Clause License の下で利用されています。

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

 * Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

 * Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the
   distribution.

 * Neither the name of the UniDic Consortium nor the names of its
   contributors may be used to endorse or promote products derived
   from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

- https://unidic.ninjal.ac.jp/

### NEologd (SudachiDict に内包)

Copyright (C) 2015-2019 Toshinori Sato (@overlast)

Licensed under the Apache License, Version 2.0.

- https://github.com/neologd/mecab-unidic-neologd

NEologd には以下のデータソースが含まれています:

- はてなキーワード一覧ファイル (著作権: 株式会社はてな)
  http://developer.hatena.ne.jp/ja/documents/keyword/misc/catalog
- 郵便番号データ (日本郵便株式会社)
  http://www.post.japanpost.jp/zipcode/dl/readme.html
- 日本全国駅名一覧 (スナフキん氏)
  http://www5a.biglobe.ne.jp/~harako/data/station.htm
- 人名(姓/名)エントリデータ (工藤拓氏)
  http://chasen.org/~taku/software/misc/personal_name.zip

## mecab-ipadic

Copyright 2000, 2001, 2002, 2003 Nara Institute of Science and Technology.

Licensed under the BSD 3-Clause License.
IPAdic 辞書 (`ipadic.hsd`, `ipadic-neologd.hsd`) の基盤データ。

- https://taku910.github.io/mecab/

## mecab-ipadic-NEologd

Copyright (C) 2015-2019 Toshinori Sato (@overlast)

Licensed under the Apache License, Version 2.0.
NEologd 辞書 (`ipadic-neologd.hsd`) の追加語彙データ。

- https://github.com/neologd/mecab-ipadic-neologd
