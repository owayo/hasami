# hasami - 高速日本語形態素解析エンジン

## プロジェクト概要
Rust製の日本語形態素解析エンジン。外部エンジン（MeCab等）に一切依存せず、ゼロベースで構築。

## 技術スタック
- **言語**: Rust (2024 edition, MSRV 1.85)
- **辞書**: mmap-native バイナリ形式 (.hsd v4) + bytemuck Pod 構造体。ロード時はヘッダと小さな表だけ検査
- **Trie**: 文字単位 Double-Array Trie（文字を出現頻度順に符号化、単独の末尾は TAIL に圧縮、ゼロコピー mmap 参照）
- **解析アルゴリズム**: ラティス構築 + Viterbi（コスト最小化、文分割最適化、転置した接続行列）。
  文字ごとの前計算（trie の符号・文字種・同じ文字種の長さ）のあと、構築と Viterbi を 1 回の走査で行う
  （ノードを作るときに最良の前ノードを決め、終了位置ごとの列に入れる）。経緯と数値は `docs/performance.md`
- **文字列管理**: 素性レコード（品詞・活用型・活用形の番号と読み・発音・原形）を重複排除して格納。
  読み・発音のカタカナは 1 字 1 バイトに詰める。最良パスのトークンを作るときだけ復号する
- **Python バインディング**: PyO3 + maturin
- **C FFI**: `#[no_mangle] extern "C"`

## プロジェクト構造
```
hasami/
├── src/
│   ├── lib.rs          # ライブラリエントリポイント（sentence 以外のモジュールは `analyzer` feature）
│   ├── main.rs         # CLI (build, merge, repair, export, tokenize, bench, info)
│   ├── dict/
│   │   ├── mod.rs      # DictEntry, UnkEntry, ConnectionMatrix（解析側でも使う型）
│   │   └── builder.rs  # DictBuilder（CSV 読み込み・repair・書き出し）。`build` feature
│   ├── hsd/            # 辞書形式 v4 (.hsd)
│   │   ├── mod.rs      # DictError
│   │   ├── container.rs  # 64B ヘッダとセクション表（id で引く、64B 境界）
│   │   ├── trie.rs     # 文字単位 Double-Array Trie（構築・検索・全件検証）
│   │   ├── records.rs  # ENTRIES（6B）・未知語・文字種の固定長レコード
│   │   ├── features.rs # 素性レコード（varint、カタカナ詰め、重複排除）
│   │   ├── strtab.rs   # 品詞・活用型・活用形の文字列表
│   │   ├── meta.rs     # メタデータ（name, pos_scheme, sources, repairs, ...）
│   │   ├── writer.rs   # DictBuilder の中身 → セクション（一時ファイル + rename で書き出し）
│   │   ├── reader.rs   # Dictionary（mmap / 所有バッファ）、ロード時検査、verify、export 用の列挙
│   │   └── tests.rs    # 往復・再現性・支配エントリの除去・壊れたファイルの拒否
│   ├── char_class.rs   # 文字分類（未知語処理）。辞書は BMP の文字種表を持ち、解析では表を引く
│   ├── sentence/       # 辞書不要の文分割
│   │   ├── mod.rs      # 規則 1〜9、Splitter（split / split_with_breaks / chunk_ends）、判定関数
│   │   ├── exceptions.rs  # 例外表の照合（左の境界、続きの語・文頭の語、字幅の畳み込み）と抽出規則
│   │   ├── index.rs    # 例外表の索引（錨の列 + 鍵の頭 3 字の表）。build.rs と共有
│   │   ├── chars.rs    # 文末記号・字幅の畳み込み・字種。build.rs と共有（std 以外に依存しない）
│   │   ├── builtin_exceptions.txt     # 組み込みの例外表（hasami export-sentence-exceptions が生成）
│   │   └── builtin_exceptions.NOTICE  # 例外表を含むものを配布するときに添える表示
│   ├── pos.rs          # 品詞の正規化（CoarsePos、IPAdic 系・UniDic 系）、否定の判定、モーラ数
│   ├── lattice.rs      # ラティス構築 + Viterbi、Token、トークンの組み立て（既知語キャッシュ、品詞などの Arc は解析器ごと）
│   ├── analyzer.rs     # 高レベルAPI（Analyzer: Arc<Dictionary> + ワークスペース）
│   └── ffi.rs          # C ABI インターフェース
├── dict/               # ビルド済み辞書（Git LFS管理）
│   ├── ipadic.hsd      # IPAdic 単体
│   ├── ipadic-neologd.hsd  # IPAdic + NEologd
│   ├── ipadic-neologd-sudachi.hsd  # IPAdic + NEologd + SudachiDict（推奨・最大語彙）
│   ├── user/           # ユーザー辞書CSV（make dict-neologd でマージ）
│   ├── user-remove/    # repair --remove に渡す削除リスト（make dict-repair で全件適用）
│   └── foreign-names/  # 外国人名の許可・拒否リストと、任意で適用するフルネームの削除リスト
│       ※ unidic-cwj.hsd / unidic-csj.hsd は同梱されず make dict-unidic-cwj/csj でビルド
├── scripts/
│   ├── build-dict.sh          # 配布辞書 3 つを上流の固定版から作る（Makefile の dict 系と CI が呼ぶ）
│   ├── clean-lfs.sh           # 手元の LFS の実体を、いまのコミットが使うものだけにする（make clean-lfs / clean）
│   ├── convert_sudachi_raw.py # SudachiDict の raw CSV → IPAdic 体系の MeCab CSV
│   ├── convert-unidic-csv.py  # UniDic CSV → IPAdic互換フォーマット変換
│   └── find_foreign_names.py  # 外国人名の削除リストを生成（Unihan の字音と照合）
├── hasami-python/      # Python バインディング (PyO3)
│   ├── src/lib.rs
│   ├── build.rs        # PyO3 拡張モジュール向けリンク設定
│   ├── Cargo.toml
│   └── pyproject.toml
├── build.rs            # 例外表の索引と版の識別子を作る（src/sentence/index.rs・chars.rs を #[path] で共有）
├── Cargo.toml          # ワークスペース + メインクレート
└── README.md
```

## feature
- feature なし（`default-features = false`）: 辞書不要の文分割 `sentence` だけ。依存は無い（noslop がこの構成で使う）
- `analyzer`: 形態素解析。`sentence` 以外のモジュール（analyzer・char_class・dict・ffi・hsd・lattice・pos）と
  再エクスポート（`Analyzer`・`DictEntry`・`DictError`・`Dictionary`・`Token`・`CoarsePos`）。依存は memmap2・bytemuck
- `build`: 辞書の構築・修復・書き出し（`DictBuilder`・`write_lexicon_csv`・`hsd::writer`）。`analyzer` を含む。依存は csv・encoding_rs・glob
- `cli`: `hasami` コマンド（`[[bin]]` の required-features）。`build` を含む。既定（`default = ["cli"]`）
- `sentence` はほかのモジュールに依存しない（`pos`・`analyzer` が `sentence` を使う片方向）。`sentence` の doc から
  解析側の項目へ rustdoc のリンク（`` [`crate::pos`] `` など）を張ると、feature なしの `cargo doc` で壊れる
- CI と `make check` は feature なし・`analyzer` の 2 構成で `clippy --lib -D warnings` と `test --lib` を回す

## 主要API
- `Analyzer::load(path)` - .hsd 辞書ロード（mmap、IPAdic で ~1ms）
- `Analyzer::tokenize(text)` - 形態素解析（壊れた辞書の不正な参照で panic）
- `Analyzer::try_tokenize(text)` - 形態素解析（不正な参照は `DictError::Corrupt`。FFI・Python はこちら）
- `Analyzer::load_default()` - `HASAMI_DICT` → `$XDG_DATA_HOME/hasami/*.hsd`（推奨順）の順に辞書を探す。無ければ `DictError::NotFound`
- `Analyzer::tokenize_sentences(text, &SplitOptions)` - 文ごとの範囲とトークン列
- `LatticeWorkspace::tokenize_into(text, &dict, offset, &mut out)` - 前分割なしで 1 チャンクを解析し、位置をずらして `out` に足す
- `analyzer::{format_mecab, format_wakachi}` / `{push_mecab, push_wakachi}` - 出力の書式化（push は既存の String に足す）
- `hasami::sentence::{split, Splitter}` - 辞書不要の文分割。解析の前分割も `Splitter::chunk_ends`（例外語の内側で切らない）
- `Splitter::split_with_breaks(text, &breaks)` - 改行とみなすバイト位置を別に渡す分割（括弧の外側で区切る）
- `sentence::{is_sentence_ender, ascii_run_is_ender, closing_bracket, is_closing_bracket}` - 分割と同じ基準の判定関数
- `sentence::BUILTIN_EXCEPTIONS_VERSION` - 例外表の版（`語の数-語の FNV-1a 64`。build.rs が生成）
- 例外表を変えたら `hasami export-sentence-exceptions --dict dict/ipadic-neologd-sudachi.hsd --output src/sentence/builtin_exceptions.txt`
  で作り直す（字幅を畳み、抽出規則を満たすことをテストが確かめる）。索引は build.rs が作るので手で作らない
- `Token::coarse_pos()` / `is_negation()` / `mora_count()` - 品詞の正規化・否定・モーラ数（`src/pos.rs`）
- `Token` - `surface`, `start`, `end`, `pos`, `conj_type`, `conj_form`, `base_form`, `reading`, `pronunciation`,
  `word_cost`, `is_known`（活用型・活用形が無い語は空文字列）。半角空白・タブ・改行（char.def の SPACE）は
  MeCab と同じく読み飛ばしてトークンにしない（前後の語を直接つなぐ。`Node::start` はつなぐ位置で、表層は
  `ChunkChars::skip_spaces` の位置から）
- `Dictionary::load(path)` / `Dictionary::verify()` / `Dictionary::for_each_entry(cb)` / `Dictionary::lookup(text)`
- `DictBuilder` - MeCab形式CSVから辞書構築。`write_hsd(path, &opts, progress)` でファイル、`build()` でメモリ上の辞書
- `DictBuilder::load_hsd(path)` - 既存辞書からインポート（マージ・repair 用。支配エントリを除いた辞書は拒否）
- `hasami::dict::write_lexicon_csv(&dict, w)` - MeCab 形式 CSV（13 列、活用型・活用形付き）に書き出す
- `hasami_last_error(handle)` - C FFI の直前エラー取得（`handle == NULL` でも直近のロード失敗を参照可能）

## 辞書形式
- **ビルド**: MeCab互換CSV + matrix.def + char.def + unk.def → .hsd
- **フォーマット**: v4。64B ヘッダ + セクション表（id で引く）+ 64B 境界のセクション 17 種。リトルエンディアン機専用
  - 規範は `~/.claude/skills/hsd-format-redesign/references/v4-spec.md`（第 3 版の追記 A〜K が正）と `src/hsd/*.rs` の冒頭コメント
  - v3 → v4 で測ったこと・試したこと・見送ったことと最終の計測は `docs/hsd-format.md`。形式を見直すときはここから始める
  - v1〜v3 の .hsd は読めない（`scripts/build-dict.sh` で作り直すよう案内するエラー）
  - 接続行列は転置して持つ: `costs[left_id * num_right + right_id]`（matrix.def の 1 行目は「right_id の数 left_id の数」）
  - matrix.def なしで作った辞書は、使われている文脈 ID を覆うゼロ行列を置き、メタデータに `zero_matrix=true` を書く
  - `--prune-dominated` を付けた最終辞書は flags とメタデータに記録し、`merge`・`repair` の入力にできない
  - 書き出しは同じディレクトリの一時ファイル → rename（mmap 中の他プロセスを壊さない）
- **拡張子**: `.hsd` (hasami dictionary)

## CLI コマンド
- `hasami build` - 辞書構築
- `hasami merge` - 既存辞書にCSVを追加マージ
- `hasami tokenize` - 形態素解析。標準入力の行は `-j`（既定は CPU の数）で並列に解析し、入力の順に出す
- `hasami bench` - ベンチマーク（`--text` の繰り返し、または `--file` でファイルの全行を 1 回として測る）
- `hasami info` - 辞書情報表示（メタデータ・セクションのサイズ。`--verify` で全件検証）
- `hasami repair` - 誤読エントリの修復・除去（範囲外の文脈 ID、壊れた発音、表記ゆれ、漢数字の人名、削除リスト、一般語の固有名詞の降格 `--demote-common-proper-nouns <IPAdic.hsd>`、追加マージ）
- `hasami export-sentence-exceptions` - 文分割の例外表（文末記号を含む語）を辞書から抽出する（`src/sentence/builtin_exceptions.txt` の生成）
- `hasami export` - 辞書のエントリを MeCab 形式 CSV に書き出す（活用型・活用形も出る）
- build / merge / repair 共通: `--meta key=value`（メタデータ）、`--prune-dominated`（支配エントリを除いた最終辞書）

## ビルド・テスト
```bash
cargo build --release     # リリースビルド
cargo build --workspace   # Python バインディングを含むワークスペース全体をビルド
cargo test --workspace --exclude hasami-python  # テスト実行（hasami-python は extension-module のため
                                                # macOS/Linux でリンク不可。clippy --workspace で検証）
cargo clippy --workspace --all-targets -- -D warnings  # lint（hasami-python のコンパイル検証を含む）
make check                # 上の lint + ライブラリとして使う 2 構成（feature なし・analyzer）の clippy と lib テスト（CI と同じ）
make dict                 # 配布辞書 3 つを上流から作り直す（= scripts/build-dict.sh）
make dict-sudachi         # 推奨辞書だけ（dict-ipadic / dict-neologd も同様）
make dict-clean           # ダウンロードした辞書ソースを削除（build-dict.sh の中間辞書は実行ごとに消える。
                          # repair 前の辞書が要るときは scripts/build-dict.sh --keep-intermediate）
make clean-lfs            # 手元の LFS の実体を、いまのコミットが使うものだけにする（scripts/clean-lfs.sh。
                          # 消すものはリモートにあることを確かめる。make clean も cargo clean の後に呼ぶ）
target/release/hasami bench --dict dict/ipadic.hsd --file corpus.txt  # 1 行 1 文のファイルの全行を解析する時間
```

解析の処理を変えたら、変更前後で全トークンの全フィールドが一致するか（同点の扱いを含む）を大きなコーパスで確かめ、
速度は変更前後を交互に走らせて比べる（負荷のあるマシンでは E コアに回されて値が倍近く揺れる）。
手順と過去の数値は `docs/performance.md`。

## 辞書ソースの既知の欠陥

複数の辞書ソースをマージしているため、ソース側の欠陥がそのまま解析結果に出る。
`hasami repair` で修復・除去する（詳細は README の「辞書の修復」）。

| ソース | 欠陥 | 影響 | 対処 |
| --- | --- | --- | --- |
| SudachiDict | raw 辞書に発音の列が無い（旧版の変換 CSV は発音に表層形が入っていた） | 発音が読みのままだと「方法」が「ホーホー」でなく「ホウホウ」になり長音が失われる | `scripts/convert_sudachi_raw.py` は発音に読みを入れ、`repair`（常時）が同じ語の健全なエントリから長音の発音を借りるか組み立てる |
| NEologd | 表記ゆれ正規化エントリが活用語の語形を名詞として登録している | 「質の高い」→「シツノコウイ」、「概念を学ぶ」→「ガイネンヲガクブ」 | `repair --drop-ortho-variants` |
| NEologd / SudachiDict | 漢数字だけで綴られた人名・地名 | 「十五」→「トウゴ」、「二十八」→「ツチヤ」 | `repair --drop-numeral-misreadings` |
| SudachiDict | 代名詞と同じ表層の 1 文字の人名 | 「何なのか」→「ガナノカ」 | `repair --drop-ortho-variants` |
| NEologd `mecab-user-dict-seed` | 読みが別語のものに差し替わっているエントリが散在する | 「最終面接」→「イチジメンセツ」、「目標数値」→「スウチモクヒョウ」、「情報収集」→「ジョホウシュウシュウ」 | `dict/user-remove/misreading-entries.csv` に列挙して `repair --remove` |
| SudachiDict（旧版の変換） | IPAdic の文脈 ID に写し漏れ、SudachiDict の文脈 ID（接続行列の範囲外）のまま入った重複が 137 万件あった | 範囲外の ID は接続コスト 0 として扱われ、UniDic 体系の品詞の語が不当に勝つ | `scripts/convert_sudachi_raw.py` は ID を引けない語を取り込まない。古い辞書は `repair --drop-invalid-context-ids`（build / merge / repair は範囲外 ID をエラーにする） |
| SudachiDict（旧版の変換） | 活用語の原形が表層形のまま（「示し」の原形が「示し」） | 原形で動詞を引く処理（noslop の述語照合など）が空振りする | raw 変換は SudachiDict の辞書形を原形にする（「示す」「誤る」「読み込む」） |
| IPAdic | char.def が `— 。 、 「 ♪` などを SYMBOL（まとめて 1 語）にし、unk.def がその未知語を「名詞,サ変接続」にする | 辞書に無い記号の並びが句点ごと 1 つの名詞になる（「楽しみたい——。」の「——。」） | `scripts/prepare_ipadic.py` が SYMBOL を「既知語がある位置では作らない・1 文字ずつ・記号,一般」に変える |
| IPAdic | char.def の未知語の候補（group・length。hasami は MeCab と同じ意味で読む）が、ひらがなの並びを 1 つの名詞にし（HIRAGANA 0 1 2）、英数字を 1 文字に分けない（ALPHA・NUMERIC 1 1 0）。中黒・× ÷ がカタカナ・英字の範囲にある | 「なき / ゃいけないってこともないし」、「ジョン・カーター」が 1 語になって読みを補えない、「microSD×C」が 1 語 | `scripts/prepare_ipadic.py` が HIRAGANA 0 0 2、ALPHA・NUMERIC 1 1 1 にし、U+30FB・U+00D7・U+00F7 を SYMBOL にする（カタカナは 1 1 2 のまま、並び全体を 1 語にできる） |
| IPAdic | CSV が EUC-JP で、ダッシュ・波ダッシュ・マイナス等 7 字は変換表で写し先が分かれる | encoding_rs（WHATWG）の変換だと「—」「〜」「−」の語が辞書に無くなる | `DictBuilder::decode_to_utf8` が JIS の対応表（MeCab と同じ字）にそろえ、`prepare_ipadic.py` が Windows 側の「―」「～」「－」の別表記を足す |
| IPAdic | 「−」「－」を「ヒク」と読む | 文章ではハイフン代わりが多く「K−POP」が「ケーヒクポップ」になる | `dict/user-remove/misreading-entries.csv`（NEologd を含む 2 辞書） |
| IPAdic / NEologd / SudachiDict | 中国・朝鮮系の姓・名（1 文字姓の音読み、朝鮮語・普通話の字音で読む名、カタカナの外国人名） | 「金がない」→「キムガナイ」（朝鮮の姓の「金(キム)」）。「何なのか」→「ガナノカ」 | `dict/user-remove/foreign-names.csv` を `repair --remove`。生成は `scripts/find_foreign_names.py`（README の「外国人名の除去」） |
| NEologd | 一般語を「名詞,固有名詞,一般」で登録している（成果物・多角的・包括的・可視化・言語化・心理的・安全性・担当者 など） | 固有名詞を具体性の手掛かりに数える処理（noslop）で抽象的な文が具体的に見える。品詞が固有名詞なので接続も固有名詞のもの（「言語化と」が「言語 / 化 / と」に割れる） | `repair --demote-common-proper-nouns <IPAdic の中間辞書>`。IPAdic で「一般名詞 + 一般名詞を作る接尾辞」に分かれる語を `名詞,一般`（〜化はサ変接続、〜的は形容動詞語幹）にし、文脈 ID も付け替える（README の「一般語の固有名詞の降格」） |
| NEologd | 規則で拾えない一般語が固有名詞・人名になっている（ステークホルダー、原形が「ANGAGEMENT」「Youth case」の人名もあるエンゲージメント・ユースケース、爆速） | 同上 | `dict/user-remove/common-words-as-proper-nouns.csv` で固有名詞のエントリを落とし、`dict/user/common-word-fixes.csv` で一般名詞を足す |
| IPAdic / NEologd | 「深掘り」「深堀り」「腹落ち」が 1 語にならない | 「深(形容詞) / 掘り(動詞)」「深堀(人名) / り」「腹 / 落ち(接尾)」 | `dict/user/common-word-fixes.csv`（名詞,サ変接続。「深堀り」の原形は「深掘り」） |
| NEologd `neologd-adjective-std-dict-seed` | 形容詞・イ段の 143 語で、ガル接続のエントリの表層形が基本形のまま（「うそ寂しい」がガル接続） | 原形は正しい。このエントリが選ばれると活用形がガル接続になる（「くどくどしい説明」の「くどくどしい」） | 対処なし（原形の修復は不要。活用語で「活用形が基本形でも `*` でもなく原形 = 表層形」の 173 件は、この 143 件と IPAdic の「乞う(連用タ接続)」「あり(ラ変連用形)」など原形と同形の活用形だけ） |
| IPAdic | char.def が SPACE に `0x00D0`（Ð）を入れている（復帰 `0x000D` の書き間違い） | 空白は読み飛ばすので「Ð」が解析結果から消える | `scripts/prepare_ipadic.py` が `0x000D` に直す |
| NEologd / SudachiDict | 漢字の 1 字をカタカナにした表記ゆれ（「公キ」原形「公器」、「神キ」「王シ」「娘シ」「ト場」） | カタカナの並びの境界をまたぐので、境界をまたぐ既知語を手掛かりにする規則が誤作動する（「主人公キャリー」→「主人 / 公キ / ャリー」） | 対処なし。カタカナの並びの規則（下の「カタカナの未知語」）は 3 字以上の語だけを数え、並び全体を覆えるときだけ効くので、2 字のこれらの語では誤作動しない |


### カタカナの未知語

未知語の候補は MeCab と同じ（`UnkGrouping::for_each_len`）だが、`lattice.rs` はカタカナの並び全体の候補に規則を足す
（詳細は README の「未知語」、経緯は `docs/hsd-format.md` の 7 章）。

- 3 字以上の並び全体の候補は、3 字以上（`COMPOUND_MIN_CHARS`）の既知語を隙間なく並べて覆えるなら作らない（`RunCover`）。
  最初の語は並びの前から、最後の語は並びの後まで伸びてよい。候補と同じ表層の 1 語だけでは覆えたことにしない
- 最良パスに残ったカタカナの未知語（3 字以上）と同じ表層の語が辞書にあれば、その語の素性で出す
  （`dictionary_word_for_unknown`。前後の語との接続コスト + 単語コストが最小のエントリ。`word_cost` は未知語の値）
- 並び全体の候補は長さによらず作る（MeCab の `max-grouping-size` の 25 字の上限は持たない）

規則の良し悪しは、ニュースコーパスを以前の実装・MeCab と比べて、境界が食い違う最小の区間を「既知語の並び → 1 語」
「断片 → 1 語」「既知語 → 未知語を含む」などに分けて数えて決めた。しきい値を 2 字にすると人名が断片に割れ
（ドミ / ニク）、一律のコストの上乗せでは良い併合と悪い併合を分けられなかった。


### 文脈で決まる読み

同じ表層形でも文脈で読みが変わる語は、辞書のどのエントリを選んでも片方が誤読になる。
`lattice.rs` の `apply_contextual_readings()` が Viterbi の結果を見て補正する。

| 語 | 読みの分かれ方 | 判定の根拠 |
| --- | --- | --- |
| 他 | 名詞なら「ホカ」（他ならぬ、他で、他は）、接頭詞なら「タ」（他部門、他業種） | 自分の品詞（`POS_READING_OVERRIDES`） |
| 数 | 数詞・助数詞が続けば接頭辞の「スウ」（数十人、数分）、続かなければ名詞の「カズ」（目玉の数、数が多い） | 後続トークンの品詞（`apply_kazu_reading_override()`） |
| 一節 | 小数点の直後は促音化せず「イチセツ」（4.1節 →「四点一節」）、それ以外は「イッセツ」（詩の一節、第一節） | 直前が「数詞 +『点』」か（`apply_decimal_counter_override()`） |

助詞との組み合わせを列挙する方式では「他ならぬ」や文末の「他」を取りこぼすので、
品詞を根拠にしている。複合語（他人、素数、複数、人数）は 1 トークンなので影響しない。

### 英字の読み

日本語文中の 1〜2 文字の英字は略語（AI, PC, VP 等）が大半で、辞書に登録された
単位読み・略称読み（A→アンペア, G→ギガ, cs→クレディスイス）はほぼ誤読になる。
`lattice.rs` の `should_trust_dict_reading()` は 1〜2 文字の英字と、読みが
カタカナでないエントリについて辞書を信用せず、綴り読み（数字直後なら単位読み）を付与する。
3 文字以上でカタカナの読みを持つ語（NASA→ナサ）は辞書を尊重する。

### 半角記号は IPAdic では未知語になる

IPAdic の `unk.def` は SYMBOL クラスの未知語を「名詞,サ変接続」として扱う。
半角カンマ `,` とアポストロフィ `'` は辞書にエントリが無いためこの規則が適用され、
読点として扱われない。その結果、直後の分割が乱れる。

```
ときなど、日々確実に前進する   → とき / など / 、 / 日々 / 確実 / に / 前進 / する
ときなど,日々確実に前進する   → とき / など / , / 日 / 々 / 確実 / に / 前進 / する
```

Style-Bert-VITS2 は読点を `,` に正規化してから解析に渡すため、この差が直接効く。
`dict/user/ascii-punctuation.csv` で `,` を「記号,読点」、`'` を「記号,一般」として
登録している。`.` `!` `?` `…` `-` は辞書にエントリがあるので対処は要らない。
