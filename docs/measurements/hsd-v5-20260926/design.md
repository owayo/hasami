# 文法情報共有の設計と採用ゲート

候補はv5。セクション18 GRAMMARを追加、要素はrepr(C) Podのu16 pos/conj_type/conj_form（6B）。FEATURESはgrammar_id:u32 LEB128 + flags:u8 + v4と同じ文字列。組は重複排除後の素性の頻度降順、同頻度は3番号辞書順。v4は拒否し上流再構築を案内。他17セクション・64B整列・LE限定・trie配置/ビット幅/TAIL/群の末尾/候補順/行列向き/中間と最終フラグはv4そのまま。支配エントリ除去や発音差分、Viterbi集約は含めない。Token所有APIも維持。

ビルダーは既存の6B+flags+stringsの正準キーを一時的にinternし、戻り値を一意素性の番号とする。重複排除済みレコードに対して組頻度を数え、finishでgrammar表と小さいレコードへ再符号化し、record_id→新offsetの配列を返す。writerは各entryの番号を新offsetに変換する。キーHashMapはfinish前半で捨て追加のピークを抑える。サイズ/offset/countはchecked conversionとchecked arithmetic。最終FEATURES<4GiB、grammar数はu32に収まる。各pos/conjは既存のu16制限維持。65,536組上限は追加しない。

readerはGRAMMARが空でない・6Bで割り切れる・bytemuck cast alignment・各項目が文字列表内かをロード時検証。grammar_idはアクセスごとにvarint最大5B/u32 overflow/範囲外を検査。flagsと文字列は従来同様検査。全体verifyは全参照素性も検査。任意バイトオフセットは従来同様アクセスでdecodeする（レコード境界そのものの全件索引は追加しない）。巨大grammar表は実データで小さいが最悪ロードO(grammar_count)になることを文書化。atomic temp+rename更新維持。

固定長案は試作でgrammar_id=u16を使用し同じ辞書内容で比較。実配布3辞書の組数は669以下。採用する場合にはu32 fallbackの仕様が必要なので、同等性能ならvarintを優先。試作形式は本番v5と混同しない。

検証: まずv4実辞書→試作用v5の物理変換で同一内容のtoken全フィールドを13万ニュース+技術+会話/文学調/記号合成と突合。固定長案も同様。物理変換器はscratch限定で本番互換readerを追加しない。独立ソースからv4とv5を同じbuild-dict.shで作り直して最終回帰、metadata/entry数/素性/活用/コスト/順を確認。3辞書の非圧縮/zstd-19・構築時間RSS・初回load+tokenize・温まった解析を比較。QoSをPコア優先にしてA/B/C交互5回、wallとCPU時間、minと中央値。持続して3%以上遅ければ採用を再考（サイズ7%減も考慮し勝手に採用しない）。cold page cacheは非破壊に保証できなければ未測定と明記。

テスト: shared tuples/頻度順同点/65536超の組/varint境界と破損/grammarの欠落・端数・各ID範囲外・alignment/old v4拒否/既存の往復・再現性・static embedding・乱数壊れた辞書。make ci、配布辞書ignored tests、例外表一致。docs/hsd-format.mdへ実際の採否と数値、README/AGENTS/形式説明更新。リリース公開は今回の採用とは別で実施しない。

前回5項目: 3辞書の共有効果実測済（docs9章）。trie配置等は不変更。破損と上限は上記。中間/最終・ID修復は不変更。発音差分/Viterbi集約は本変更から除外。以上を独立レビューして本番実装開始の可否を判定する。
