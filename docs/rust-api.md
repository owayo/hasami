# Rust から使う

hasami をライブラリとして使うときの依存の書き方、feature の選び方、主な API をまとめる。
C と Python から使うときは [c-api.md](c-api.md) と [python-api.md](python-api.md) にある。

## ライブラリとして使う

crates.io には公開していない（`hasami` の名前は別のプロジェクトが使っている）。git 依存で使う。

```toml
[dependencies]
# 解析まで（Analyzer・Dictionary・Token と sentence）。依存は memmap2 と bytemuck だけになる
hasami = { git = "https://github.com/owayo/hasami", default-features = false, features = ["analyzer"] }
# 文分割（sentence）だけなら。依存は無い
# hasami = { git = "https://github.com/owayo/hasami", default-features = false }
# 配布辞書をリリースから取るなら（hasami::download）
# hasami = { git = "https://github.com/owayo/hasami", default-features = false, features = ["download"] }
# 辞書も作るなら（DictBuilder、MeCab 形式 CSV の読み書き）
# hasami = { git = "https://github.com/owayo/hasami", default-features = false, features = ["build"] }
```

| feature | 中身 | 追加の依存 |
| --- | --- | --- |
| （なし） | 辞書の要らない文分割（`sentence`） | なし |
| `analyzer` | 解析（`Analyzer`・`Dictionary`・`Token`・品詞の正規化、辞書を埋め込む `include_hsd!`）、C FFI | memmap2, bytemuck |
| `download` | リリースの配布辞書の取得（`hasami::download`。`analyzer` を含む） | ureq（rustls）, sha2, tempfile, serde, serde_json, ruzstd |
| `build` | 辞書の構築・修復・書き出し（`DictBuilder`、`write_lexicon_csv`。`analyzer` を含む） | csv, encoding_rs, glob |
| `cli` | `hasami` コマンド（`build` と `download` を含む） | clap, indicatif, serde_json |

既定は `cli`（`cargo install` やこのリポジトリでのビルドで CLI が使える）。

**版の方針**: 版は `yy.m.counter` の日付版（例: `26.9.100`。リリースワークフローが年・月・月内の連番で付ける）で、
semver の互換性は表さない。API と辞書形式はどの版でも変わりうるので、git 依存では
`tag = "v<版>"`（[Releases](https://github.com/owayo/hasami/releases) の版）か `rev` で固定する。
辞書形式を変えたときは、古い `.hsd` を読み込むと作り直しを案内するエラーになる
（`scripts/build-dict.sh` で上流から作り直す）。

## 基本

```rust
use hasami::Analyzer;

let mut analyzer = Analyzer::load("dict/ipadic-neologd.hsd")?;
let tokens = analyzer.tokenize("東京都に住んでいる");

for token in &tokens {
    println!("{}\t{}\t{}", token.surface, token.pos, token.reading);
}

// 活用型・活用形（活用しない語・未知語は空文字列）
let tokens = analyzer.tokenize("読み込み、");
println!("{} {}", tokens[0].conj_type, tokens[0].conj_form); // 五段・マ行 連用形

// バッチ処理
let results = analyzer.tokenize_batch(&["文1", "文2", "文3"]);
```

`tokenize` は辞書に不正な参照を見つけると panic する（`hasami info --verify` で検証済みの辞書では起きない）。
検証していない辞書を扱うときは、エラーを返す `try_tokenize` を使う。

```rust
match analyzer.try_tokenize("東京都に住んでいる") {
    Ok(tokens) => { /* ... */ }
    Err(e) => eprintln!("壊れた辞書: {e}"),
}
```

`Token` のフィールドは `surface`・`start`・`end`（入力のバイト位置）・`pos`（品詞 4 階層）・`conj_type`・`conj_form`・
`base_form`・`reading`・`pronunciation`・`word_cost`・`is_known`。

辞書は mmap で読み込むので、読み込み中の辞書ファイルを書き換えたり切り詰めたりしてはいけない。
辞書を差し替えるときは別名で書いてから rename する（`hasami build` / `merge` / `repair` の出力はそうしている）。

## 辞書の既定の場所

`Analyzer::load_default()` は次の順に辞書を探す。見つからなければ探した場所を持つ `DictError::NotFound` を返すので、
辞書なしでも動く利用者はこのエラーのときだけ辞書なしに切り替えればよい。

1. 環境変数 `HASAMI_DICT`（辞書ファイルのパス）
2. 置き場所（`hasami::analyzer::data_dir()`。`HASAMI_DATA_DIR` → `$XDG_DATA_HOME/hasami/` → `%LOCALAPPDATA%\hasami\`
   （Windows）→ `~/.local/share/hasami/`）の `*.hsd`。複数あれば `ipadic-neologd-sudachi.hsd` → `ipadic-neologd.hsd` →
   `ipadic.hsd` → そのほかの名前順（`hasami::analyzer::preferred_dict_in(dir)`）

```rust
let mut analyzer = match hasami::Analyzer::load_default() {
    Ok(a) => Some(a),
    Err(hasami::DictError::NotFound(_)) => None, // 辞書なしで動く
    Err(e) => return Err(e.into()),
};
```

置き場所の規則を写さずに済むよう、`data_dir()` を公開している（辞書を取得するツールは、ここに置けば
`hasami tokenize` や `load_default` がそのまま見つける）。

## 配布辞書を取得する（`download` feature）

`hasami::download` は `hasami dict download` と同じ手順で、リリースの配布辞書を取得して置き場所に置く
（目録の大きさと SHA-256、辞書として読めることを確かめてから、一時ファイルを rename して置く）。

```rust
use hasami::download::{self, DownloadOptions};

// この版（hasami の Cargo.toml の version）のリリースの目録から推奨辞書を取る
let catalog = download::catalog(download::CURRENT_TAG)?;
catalog.check_format()?; // この hasami が読める形式か
let dict = catalog.find(download::RECOMMENDED).expect("推奨辞書は目録にある");
let dir = hasami::analyzer::data_dir().expect("置き場所が決まる");
let mut progress = |received: u64, total: u64| eprint!("\r{received} / {total}");
let options = DownloadOptions {
    progress: Some(&mut progress),
    ..DownloadOptions::default()
};
let outcome = download::download(dict, &dir, options)?; // 正しいファイルがあれば通信しない
let analyzer = hasami::Analyzer::load(outcome.path())?;
```

圧縮版が HTTP 404 の場合は、同じ取得元の非圧縮版 `.hsd` に切り替える。ほかの HTTP エラー、接続失敗、
大きさ・SHA-256 の不一致、展開失敗では切り替えない。切り替え時の `progress` は、受信量 0 と非圧縮版の
全体量で始め直す。

プロキシや User-Agent を指定する場合は `Client` を作る。設定は目録と辞書の両方に適用される。
`ProxySetting::Env`（既定）は環境変数に従い、`None` は環境変数によらずプロキシを無効にする。
`Url("http://proxy.example.com:8080")` は指定した HTTP / HTTPS プロキシを使い、`NO_PROXY` も参照しない。
User-Agent は省略すると `hasami/<版>`、空文字列なら送らない。TLS の検証には OS の証明書ストアを使う。

```rust
use hasami::download::{Client, HttpOptions, ProxySetting, DownloadEvent};

let client = Client::new(HttpOptions {
    proxy: ProxySetting::None,
    user_agent: Some("my-app/1.0"),
})?;
let base = "https://mirror.example.com/hasami";
let catalog = client.catalog_from(base)?; // タグ指定なら client.catalog(tag)
catalog.check_format()?;
let dict = catalog.find("ipadic").expect("目録にある辞書");
let outcome = client.download_with_events(dict, &dir, DownloadOptions {
    base_url: Some(base),
    ..DownloadOptions::default()
}, &mut |event| {
    if let DownloadEvent::UncompressedFallback { uncompressed_url, .. } = event {
        eprintln!("圧縮版がないため {uncompressed_url} を取得します");
    }
})?;
```

通知は非圧縮版を要求する前に呼ばれるため、その取得に失敗した場合も切り替えを把握できる。
通知が不要なら `client.download(dict, &dir, options)` を使う。既存の関数と `DownloadOptions`・`Outcome` は
そのまま使える。

大きさと SHA-256 を自分のソースに固定するなら、目録を取らずに `DistributedDict` を組み立てて渡す
（取得元を信用しきらずに使える。圧縮版も固定するなら `compressed` を埋める）。

```rust
let dict = download::DistributedDict::new(
    "ipadic",
    18_125_804,
    "e917bcdcdb45893fb4dd9b2de88ccb11dba2ecad2471dd0f62bd674a7f89ed73",
);
let base = download::release_url("v26.9.103");
let options = DownloadOptions {
    base_url: Some(&base), // 省くと、この hasami と同じ版のリリース
    ..DownloadOptions::default()
};
download::download(&dict, &dir, options)?;
```

- `download::verify(path, &dict)` は置き場所のファイルを大きさと SHA-256 で確かめる（通信しない）
- `download::install(file, Some(&dict), &dir, force)` は手元のファイル（`.hsd` / `.hsd.zst`）を確かめて置く
- `download::catalog_from(url)` はミラー（`<url>/dictionaries.json`）の目録を取る
- 依存は ureq（TLS は rustls で、証明書は OS の証明書ストアで検証する）、sha2、tempfile、serde、ruzstd（zstd の展開。
  C のライブラリを使わない）。解析だけを使うなら `download` は入れない

## 実行ファイルに辞書を埋め込む

辞書を実行ファイルに埋め込むと、インストールだけで解析できる。`hasami::include_hsd!` で埋め込み、
`Dictionary::from_static` で読む。埋め込んだバイト列を複製せずに参照するので、`Dictionary::load`（mmap）と同じく
解析で触れたページだけが読み込まれ、ヒープに辞書の複製を持たない。

```rust
use hasami::{Analyzer, Dictionary};

// パスはこのファイルからの相対パス（include_bytes! と同じ）
static IPADIC: &[u8] = hasami::include_hsd!("../dict/ipadic.hsd");

let dict = Dictionary::from_static(IPADIC)?;
let mut analyzer = Analyzer::from_dict(dict);
```

- `from_static` は、バイト列の先頭が 8 バイト境界にあることを求める。`include_bytes!` だけでは境界がそろわない
  （そろうかどうかはビルドごとに変わる）。境界になければ、複製に切り替えずに `DictError::Invalid` を返す
- `include_hsd!` は 64 バイト境界（キャッシュライン）にそろえる。セクションはファイルの先頭から 64 の倍数の位置に
  あるので、mmap した辞書と同じくセクションもキャッシュラインの境界に乗る
- `include_hsd!` は呼び出すたびに別の静的領域になる。同じ辞書は 1 か所の `static` に置いて使い回す
- `from_bytes` は、どんなバイト列でも 8 バイト境界の所有バッファに複製して読む（`'static` でないバイト列向け）
- 最初の解析で辞書のページを読み込む待ちを先に払うなら、`analyzer.prewarm()` を呼ぶ

IPAdic（18MB）を埋め込んだ CLI で小さな文書を解析すると、`from_bytes` に比べて起動が約 6ms 速く、
最大 RSS が約 31MB 少ない（mmap の `load` と同じ。高負荷のマシンでの 60 回の中央値）。

マクロを使わずに書くなら、境界をそろえたラッパーに入れる（最低 8。`include_hsd!` と同じ 64 にしておく）。

```rust
#[repr(C, align(64))]
struct Aligned<T: ?Sized>(T);

static IPADIC: &Aligned<[u8]> = &Aligned(*include_bytes!("../dict/ipadic.hsd"));

let dict = hasami::Dictionary::from_static(&IPADIC.0)?;
```

## 文分割（辞書不要）

`hasami::sentence` は辞書をロードせずに日本語の文境界を求める（feature なしで使え、依存も無い）。
括弧の対応を取ってから括弧の内側の文末記号を無視し、`Yahoo!ニュース`・`モーニング娘。`・`Hey!Say!JUMP` のように
文末記号を含む語（推奨辞書から抽出した約 1.9 万語の例外表）の内側では切らない。URL の `?` や `!important`、
直前が英数字で直後が数字の全角ピリオド（`３．１４`・`第３．２節`・`Ｎｏ．１`・`Ｖｏｌ．６`）でも切らない
（`．` を句点に使う文書で、英字で終わる文の次の文が数字で始まる `…ＡＰＩ．１つ目は…` はつながる）。

例外表の語は、次のように普通の文と取り違えないよう照合する（規則の全体は `src/sentence/mod.rs` の冒頭）。

- 語の末尾の文末記号は、直後が続きの語（助詞と `から まで より って など だけ しか さえ くらい ぐらい ほど`）で
  始まるときだけ守る。`もう もし もちろん とにかく やはり しかし` など文頭に立つ語で始まるなら切る。
  `寒いね。` `好きだ。` のように普通の文末と同じ形で終わる語の後ろでは、`でも では とはいえ だけど` も文頭の語とみなす
  （`高すぎ。でも買った。` `好きなのはモーニング娘。もう一度言う。` は 2 文、`Yahoo!では…` は 1 文）
- 語の途中から一致したものは数えない（`食べる。` の中の `べる。`、`主流。` の中の `流。`）
- 全角の英数字・記号は半角に畳んで比べる（`Yahoo！ニュース`・`Ｙａｈｏｏ！ニュース` も守る）

例外表の索引はビルド時に作って埋め込むので、`Splitter::new` の初回と最初の分割は 1ms 未満で済む。表の版は
`sentence::BUILTIN_EXCEPTIONS_VERSION`（`語の数-語のハッシュ`）で分かる。文末記号・括弧の判定は
`is_sentence_ender`・`closing_bracket`・`is_closing_bracket`・`ascii_run_is_ender` で分割と同じ基準のまま使える。

```rust
use hasami::sentence::{self, LineBreaks, SplitOptions};

let text = "「うまく行くかな？」と思った。Yahoo!ニュースを見た。";
let sentences: Vec<&str> = sentence::split(text, &SplitOptions::default())
    .into_iter()
    .map(|s| &text[s.range])
    .collect();
assert_eq!(sentences, ["「うまく行くかな？」と思った。", "Yahoo!ニュースを見た。"]);

// 改行で区切る・例外語を足す。繰り返し使うなら Splitter を作って使い回す
let options = SplitOptions {
    line_breaks: LineBreaks::Split,
    extra_exceptions: &["ヤッター!マン"],
    ..SplitOptions::default()
};
let splitter = sentence::Splitter::new(&options);

// 改行の字を取り除いた解析用のテキストを、元の改行の位置（バイト位置）で区切る
let text = "一行目の途中で折り返して続く文。二文目";
let breaks = ["一行目の途中で折り返して".len()];
let sentences = sentence::Splitter::default().split_with_breaks(text, &breaks);
```

一文の長さを測るときのように、括弧の中の文も分けたいときは `Splitter::split_fragments` を使う。
`split` の文を、括弧の内側で文末として働く文末記号の後ろでさらに区切った断片を返す。

- 断片は文の境界をまたがない。括弧の内側に文末記号のない文（`embedded_enders` が偽の文）は、そのまま 1 つの断片になる
- どの記号が文末として働くか（例外表の語・URL の `?`・小数点）、改行（括弧の内側では区切らない）、前後の空白は `split` と同じ。
  語の末尾の文末記号の直後が閉じ括弧なら、`split` の `embedded_enders` と同じく文末として働く
  （`「モーニング娘。」が好きだ。` は `「モーニング娘。」` / `が好きだ。`）
- 文末記号に隙間なく続く閉じ括弧と文末記号は前の断片に含める（`明日は行く。」` / `と言った。`、`「はい。」。` は 1 つの断片）。
  文末記号と閉じ括弧だけの区間も、同じ文の前の断片に含める
- 改行の位置を別に渡すなら `split_fragments_with_breaks`。断片の `embedded_enders` は常に偽

```rust
let text = "彼は「今日は休む。明日は行く。」と言った。";
let fragments: Vec<&str> = sentence::Splitter::default()
    .split_fragments(text)
    .into_iter()
    .map(|s| &text[s.range])
    .collect();
assert_eq!(fragments, ["彼は「今日は休む。", "明日は行く。」", "と言った。"]);
```

形態素解析の前分割（ラティスを小さく保つための区切り。`Splitter::chunk_ends`）にも同じ規則を使っているので、
例外表の語は解析でも割れない。前分割も括弧の対応を見ずに区切るが、閉じ括弧を次の区間に入れ、改行でも区切り、
空白も除かないので、断片の代わりにはならない。
文ごとにトークン列が欲しいときは `Analyzer::tokenize_sentences` を使う（トークンの位置は入力全体のバイト位置）。

```rust
for (sentence, tokens) in analyzer.tokenize_sentences(text, &SplitOptions::default()) {
    println!("{}: {} tokens", &text[sentence.range.clone()], tokens.len());
}
```

## 品詞の正規化・否定・モーラ数

`Token::coarse_pos` は、辞書の品詞体系（IPAdic 系・UniDic 系）の違いを吸収した粗い品詞 `CoarsePos` を返す。
辞書を替えても同じ判定ができるように、次の違いをそろえている。

- 「の」は IPAdic の `助詞,連体化` と `助詞,格助詞`、UniDic の `助詞,格助詞` のどれでも `CaseParticle`。
  「行くのが」の「の」は `FormalNoun`
- 形式名詞（こと・もの・わけ）は `FormalNoun`。UniDic は普通名詞と区別しないので、仮名書きの形式名詞を表層形で拾う
- 受け身・使役の「れる」「せる」（IPAdic では `動詞,接尾`）と、助動詞の語幹「そう」「よう」「みたい」は `AuxVerb`
- 記号は句点（。！？!? など）・読点（、，,）・開き括弧・閉じ括弧・そのほかを区別する。辞書によって品詞が違う
  半角の `(` `!` `,` や全角の `！` も、表層形で見分けて同じ値にする
- 数に付く単位の記号（`%` `％` `‰` `℃` `℉` `°` と CJK 互換文字の単位 `㎏` `㎞` `㌢` `㍍` など）は、記号の語・未知語でも
  `NounSuffix`（全角の「％」と同じ）

`Token::is_negation` は否定の形態素か（助動詞「ない」「ぬ」「ん」「ず」、形容詞「ない」）を原形で判定する。
`Token::mora_count` は発音（仮名が無ければ読み）からモーラ数を数える。拗音の小書き文字は直前の仮名と合わせて
1 モーラ、促音・撥音・長音は 1 モーラ。

```rust
use hasami::CoarsePos;

let tokens = analyzer.tokenize("運用コストの削減の実現");
let chained = tokens
    .iter()
    .filter(|t| &*t.surface == "の" && t.coarse_pos() == CoarsePos::CaseParticle)
    .count();
assert_eq!(chained, 2);

let tokens = analyzer.tokenize("行かないわけではない");
assert_eq!(tokens.iter().filter(|t| t.is_negation()).count(), 2);

let morae: usize = analyzer.tokenize("東京に行った").iter().map(|t| t.mora_count()).sum();
assert_eq!(morae, 8); // トーキョー ニ イッ タ
```

辞書の品詞に従うので、そろわない違いもある（「しか」は IPAdic では係助詞、UniDic では副助詞など）。

## 並行解析（Rust マルチスレッド）

`Analyzer` は `Clone` を実装しており、辞書（mmap）を `Arc` で共有しつつ各クローンが独自のラティスワークスペースを持ちます。複数スレッドで並列解析する際、辞書はゼロコピー共有・ワークスペースのみ独立になります。

```rust
use hasami::Analyzer;

let analyzer = Analyzer::load("dict/ipadic-neologd.hsd")?;
analyzer.prewarm(); // 解析で触れる辞書のページを先に読み込み、初回の待ちを避ける

let inputs: Vec<&str> = vec!["文1", "文2", "文3", "文4"];
let results: Vec<Vec<_>> = std::thread::scope(|s| {
    inputs
        .iter()
        .map(|input| {
            let mut worker = analyzer.clone(); // 辞書共有・ワークスペース新規
            s.spawn(move || worker.tokenize(input))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect()
});
```
