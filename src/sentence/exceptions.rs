//! 文末記号を含む語の例外表と、その照合
//!
//! 「モーニング娘。」「Yahoo!ニュース」のように表層に文末記号を含む語で文を切らないよう、
//! 語の一覧を持ち、入力の文末記号を覆う出現があるかを調べる。
//!
//! - 組み込みの表（`builtin_exceptions.txt`）は、推奨辞書の表層形を [`extract_candidates`] で
//!   絞ったもの。生成手順はファイルの先頭のコメントにある。索引（[`super::index`]）は build.rs が
//!   ビルド時に作って埋め込むので、初めて使うときにも組み立ての手間がかからない
//! - 照合は文末記号を起点にする（[`Matcher::guards`]）。入力の文末記号から前へ 1 字ずつ進みながら、
//!   鍵がその字の列で始まる錨の範囲を二分探索で狭め、語の先頭に届いた錨について左の境界と
//!   文末記号の後ろを調べる。文末記号から離れた文字は読まないので、入力の大部分を占める文末記号の
//!   ない区間には手間がかからない（計算量は [`Matcher`] を参照）
//! - 比べるときは全角の英数字・記号を半角に畳む（[`fold_width`]）。表の語は畳んで持つ

use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Range;
use std::sync::LazyLock;

use super::chars::{Script, fold_width, is_sentence_ender, script};
use super::index::{self, Index};

/// 組み込みの例外表（1 行 1 語。`#` で始まる行と空行は読み飛ばす）
const BUILTIN_EXCEPTIONS: &str = include_str!("builtin_exceptions.txt");

/// 組み込みの例外表の索引（build.rs が [`Index::to_bytes`] の形式で作る）
static BUILTIN_INDEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/builtin_exceptions.idx"));

/// 組み込みの例外表の版の識別子
///
/// `語の数-語の FNV-1a 64 の 16 進`（例: `21745-0123456789abcdef`）。表の語が変わると変わるので、
/// 下流で分割の結果の変化（統計の前提の変化）を検知するのに使える。分割の規則の変更は含まない。
pub const BUILTIN_EXCEPTIONS_VERSION: &str =
    include!(concat!(env!("OUT_DIR"), "/builtin_exceptions_version.rs"));

/// 組み込みの例外表の照合器（索引は埋め込み済みなので、作るのは索引の先頭を読むだけ）
static BUILTIN_MATCHER: LazyLock<Matcher> = LazyLock::new(Matcher::builtin);

/// 組み込みの例外表の語を、表に書かれた順（バイト順）に返す
pub fn builtin_exceptions() -> impl Iterator<Item = &'static str> {
    BUILTIN_EXCEPTIONS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// 組み込みの例外表の照合器
pub(crate) fn builtin_matcher() -> &'static Matcher {
    &BUILTIN_MATCHER
}

/// 辞書の表層形の一覧から、例外表に載せる語を抽出する
///
/// 全角の英数字・記号（U+FF01〜U+FF5E。畳むと文末記号でなくなる `．` を除く）を半角に畳み（照合も
/// 畳んで比べるので、`Yahoo！` と `Yahoo!` は 1 語になる）、文末記号（`。！？!?‼⁇⁈⁉．｡`）を含む語だけを
/// 残し、次の語を除く。
/// 結果は重複を除いてバイト順（符号位置の順）に並べる。
///
/// - 記号だけの語（英数字・かな・漢字などの文字を 1 字も含まない）
/// - 2 文字未満の語
/// - 文末記号で始まる語
/// - 表の書式（1 行 1 語、`#` で始まる行はコメント）で書けない語: 制御文字（改行を含む）を含む語、
///   `#` で始まる語、前後に空白がある語
/// - 照合に使えない語（文末記号が 16 個より多い語、256 文字より長い語）
pub fn extract_candidates<'a>(surfaces: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut words: Vec<String> = surfaces
        .into_iter()
        .map(fold_word)
        .filter(|w| is_candidate(w))
        .collect();
    words.sort_unstable();
    words.dedup();
    words
}

/// [`extract_candidates`] の規則で残す語か（字幅は畳んであること）
fn is_candidate(word: &str) -> bool {
    let mut chars = word.chars();
    let (Some(first), Some(_)) = (chars.next(), chars.next()) else {
        // 2 文字未満
        return false;
    };
    !is_sentence_ender(first)
        && first != '#'
        && word.trim() == word
        && word.chars().any(char::is_alphanumeric)
        && !word.chars().any(char::is_control)
        && index::is_matchable(word)
}

/// 全角の英数字・記号を半角に畳んだ語
fn fold_word(word: &str) -> String {
    word.chars().map(fold_width).collect()
}

/// 例外語の末尾の文末記号の後ろに来たとき、文末記号を語の一部とみなす語（助詞と、助詞のように続く語）
///
/// 語の頭が同じでも、[`SENTENCE_STARTERS`] などの文頭に立つ語で始まるならみなさない。
const CONTINUATIONS: &[&str] = &[
    "の",
    "は",
    "が",
    "を",
    "に",
    "と",
    "で",
    "も",
    "や",
    "へ",
    "から",
    "まで",
    "より",
    "って",
    "など",
    "だけ",
    "しか",
    "さえ",
    "くらい",
    "ぐらい",
    "ほど",
    "として",
    "について",
];

/// 文頭に立つ語（接続詞・副詞・感動詞）のうち、続きの語と頭が同じもの
///
/// 例外語の末尾の文末記号の後ろがこれで始まるなら、次の文の始まりとみなす
/// （`好きなのはモーニング娘。もう一度言う。` は 2 文）。前方一致で見るので、`もし` は `もしも`
/// `もしかして` を、`やっぱ` は `やっぱり` を含む。
const SENTENCE_STARTERS: &[&str] = &[
    "もう",
    "もし",
    "もちろん",
    "もっと",
    "もともと",
    "もはや",
    "とにかく",
    "ところで",
    "ところが",
    "ともかく",
    "ともあれ",
    "とりあえず",
    "とても",
    "とっても",
    "とくに",
    "とうとう",
    "ときどき",
    "やはり",
    "やっぱ",
    "やがて",
    "やっと",
    "やれやれ",
    "はじめに",
    "はたして",
    "しかし",
    "しかも",
    "しかたな",
    "しかたが",
    "よりによって",
    "ほどなく",
    "にもかかわらず",
    "へえ",
    "へー",
];

/// 文末記号の直前がひらがなの語（`寒いね。` `好きだ。` のように普通の文末と同じ形で終わる語）の後ろで、
/// 文頭に立つ語とみなすもの
///
/// 助詞としても読める（`Yahoo!では` `モーニング娘。でも`）ので、名詞のように終わる語の後ろでは
/// 続きとみなし、文末と同じ形の語の後ろでだけ次の文の始まりとみなす。
const STARTERS_AFTER_PREDICATE: &[&str] =
    &["でも", "では", "で、", "とはいえ", "だけど", "だけれど"];

/// 例外語の末尾の文末記号の後ろ `rest` が、語の続きとして読めるか
///
/// `after_predicate` は、語の末尾の文末記号の連続の直前がひらがなか（[`ends_like_predicate`]）。
fn continues(rest: &str, after_predicate: bool) -> bool {
    let starts_with_any = |words: &[&str]| words.iter().any(|w| rest.starts_with(w));
    if starts_with_any(SENTENCE_STARTERS)
        || (after_predicate && starts_with_any(STARTERS_AFTER_PREDICATE))
        || is_interjection_hai(rest)
    {
        return false;
    }
    starts_with_any(CONTINUATIONS)
}

/// 感動詞の `はい`（直後が読点・文末記号・空白・終わり）で始まるか。`モーニング娘。はいつも` の `は` は助詞
fn is_interjection_hai(rest: &str) -> bool {
    rest.strip_prefix("はい").is_some_and(|after| {
        after.chars().next().is_none_or(|c| {
            c.is_whitespace() || matches!(c, '、' | '，' | ',' | '…') || is_sentence_ender(c)
        })
    })
}

/// 語の出現の左の境界として認めるか（`prev` は出現の直前の字、`first` は語の先頭の字）
///
/// 語の先頭がひらがなで直前がかな・漢字なら、送り仮名や活用語尾の途中から一致しているとみなして
/// 認めない（`食べる。` の中の `べる。`）。語の先頭がカタカナ・漢字・英数字で直前も同じ字種なら、
/// 語の途中から一致しているとみなして認めない（`主流。` の中の `流。`）。
fn left_boundary(prev: Option<char>, first: char) -> bool {
    let Some(prev) = prev else {
        return true;
    };
    let prev = script(prev);
    match script(first) {
        Script::Hiragana => !matches!(prev, Script::Hiragana | Script::Katakana | Script::Kanji),
        Script::Other => true,
        first => prev != first,
    }
}

/// 語の列の位置 `ender` の文末記号で終わる語が、文末記号の連続の直前がひらがなで終わるか
fn ends_like_predicate(list: &str, ender: usize) -> bool {
    let line_start = list[..ender].rfind('\n').map_or(0, |i| i + 1);
    list[line_start..ender]
        .chars()
        .rev()
        .find(|&c| !is_sentence_ender(c))
        .is_some_and(|c| script(c) == Script::Hiragana)
}

/// 例外語の照合器
///
/// 語の列（1 行 1 語）と、その索引（錨ごとの文末記号の位置と後半のリンク。[`super::index`]）を持つ。
///
/// - 語の内側の文末記号: 語の出現が左の境界を満たせば、分割しない
/// - 語の末尾の文末記号: 語の出現が左の境界を満たし、直後が続きの語（[`CONTINUATIONS`]）で
///   始まれば分割しない。ただし文頭に立つ語（[`SENTENCE_STARTERS`] など）で始まるなら分割する
///
/// 計算量: 前へ辿る照合が入力の文字の上を通るのは、その文字より後ろにある文末記号のうち
/// 1 語の中の文末記号の数（組み込みの表では最多 9 個）までなので、前へ辿る手間の合計は入力長と
/// 錨の数の対数の積に比例する。後半の照合は 1 回あたり二分探索と後半の長さで抑えられる。利用者が
/// 加える語でこの定数が膨らまないよう、[`index::MAX_WORD_ENDERS`] と [`index::MAX_WORD_CHARS`] を
/// 超える語は捨てる。
#[derive(Clone)]
pub(crate) struct Matcher {
    /// 語の列（組み込みの表はコメント行を含む）
    list: Cow<'static, str>,
    anchors: Anchors,
    /// 文末記号で始まる語があるか（利用者が加える語にだけありうる）
    ender_initial: bool,
    /// 語の数
    words: usize,
}

/// 錨の列（鍵・後半の順）と、鍵の頭の 2 字ごとの錨の範囲（[`Index`] と同じ中身）
#[derive(Clone)]
enum Anchors {
    /// 埋め込んだ索引（LE の数値の列のまま）
    Static {
        enders: &'static [u8],
        links: &'static [u8],
        head_keys: &'static [u8],
        head_starts: &'static [u8],
    },
    /// 実行時に作った索引
    Owned {
        enders: Box<[u32]>,
        links: Box<[u32]>,
        head_keys: Box<[u64]>,
        head_starts: Box<[u32]>,
    },
}

impl Anchors {
    fn len(&self) -> usize {
        match self {
            Anchors::Static { enders, .. } => enders.len() / 4,
            Anchors::Owned { enders, .. } => enders.len(),
        }
    }

    /// 鍵の頭が `first`（[`index::head_key`]）の錨の範囲と、`second` の錨の範囲
    ///
    /// `second` は `first` と文末記号・直前の字が同じ頭（並びの上で `first` の近くにある）で、
    /// `first` を二分探索した位置から倍々に探す。
    #[inline]
    fn head_ranges(
        &self,
        first: u64,
        second: Option<u64>,
    ) -> (Option<Range<usize>>, Option<Range<usize>>) {
        match self {
            Anchors::Static {
                head_keys,
                head_starts,
                ..
            } => find_heads(
                first,
                second,
                head_keys.len() / 8,
                |k| read_u64(head_keys, k),
                |k| read_u32(head_starts, k),
                self.len(),
            ),
            Anchors::Owned {
                head_keys,
                head_starts,
                ..
            } => find_heads(
                first,
                second,
                head_keys.len(),
                |k| head_keys[k],
                |k| head_starts[k],
                self.len(),
            ),
        }
    }

    /// 錨 `i` の文末記号の、語の列の上のバイト位置
    #[inline]
    fn ender(&self, i: usize) -> usize {
        match self {
            Anchors::Static { enders, .. } => read_u32(enders, i) as usize,
            Anchors::Owned { enders, .. } => enders[i] as usize,
        }
    }

    /// 錨 `i` の後半のリンク（[`Index::links`]）
    #[inline]
    fn link(&self, i: usize) -> u32 {
        match self {
            Anchors::Static { links, .. } => read_u32(links, i),
            Anchors::Owned { links, .. } => links[i],
        }
    }
}

/// 頭の番号の列（`heads` 個）から `first` と `second` を探し、それぞれの錨の範囲を返す
///
/// 最後の頭の範囲は `anchors` まで。`second` は `first` 以上の番号であること。
#[inline]
fn find_heads(
    first: u64,
    second: Option<u64>,
    heads: usize,
    head_key: impl Fn(usize) -> u64,
    head_start: impl Fn(usize) -> u32,
    anchors: usize,
) -> (Option<Range<usize>>, Option<Range<usize>>) {
    let range = |k: usize| {
        let end = if k + 1 < heads {
            head_start(k + 1) as usize
        } else {
            anchors
        };
        head_start(k) as usize..end
    };
    let k = partition(0, heads, |k| head_key(k) < first);
    let first = (k < heads && head_key(k) == first).then(|| range(k));
    let second = second.and_then(|second| {
        let j = gallop(k, heads, |j| head_key(j) < second);
        (j < heads && head_key(j) == second).then(|| range(j))
    });
    (first, second)
}

/// u32 LE の列の `i` 番目
#[inline]
fn read_u32(bytes: &[u8], i: usize) -> u32 {
    let at = i * 4;
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 バイト"))
}

/// u64 LE の列の `i` 番目
#[inline]
fn read_u64(bytes: &[u8], i: usize) -> u64 {
    let at = i * 8;
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 バイト"))
}

/// [`partition`] と同じ位置を、`lo` から 1, 2, 4, … 先を調べて範囲を挟んでから求める
///
/// 真の側が短いと見込めるときに、`lo..hi` 全体を二分探索するより調べる回数が少ない。
#[inline]
fn gallop(lo: usize, hi: usize, pred: impl Fn(usize) -> bool) -> usize {
    // `lo..known` はすべて真
    let mut known = lo;
    let mut step = 1;
    loop {
        let probe = known + step - 1;
        if probe >= hi {
            return partition(known, hi, pred);
        }
        if !pred(probe) {
            return partition(known, probe, pred);
        }
        known = probe + 1;
        step *= 2;
    }
}

/// `lo..hi` のうち、`pred` が偽になる最初の位置（`pred` は前の側で真、後ろの側で偽）
#[inline]
fn partition(mut lo: usize, mut hi: usize, pred: impl Fn(usize) -> bool) -> usize {
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pred(mid) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

impl Matcher {
    /// 組み込みの例外表の照合器（埋め込んだ索引の先頭を読むだけ）
    fn builtin() -> Matcher {
        let parts =
            index::split_bytes(BUILTIN_INDEX).expect("組み込みの例外表の索引の形式が合わない");
        let ender_initial = (0..parts.head_keys.len() / 8)
            .any(|k| index::head_has_no_prev(read_u64(parts.head_keys, k)));
        Matcher {
            list: Cow::Borrowed(BUILTIN_EXCEPTIONS),
            anchors: Anchors::Static {
                enders: parts.enders,
                links: parts.links,
                head_keys: parts.head_keys,
                head_starts: parts.head_starts,
            },
            ender_initial,
            words: parts.words as usize,
        }
    }

    /// 語の一覧から照合器を組み立てる
    ///
    /// 字幅を畳んでから、文末記号を含まない語（分割に影響しない）、文末記号が
    /// [`index::MAX_WORD_ENDERS`] 個を超える語、[`index::MAX_WORD_CHARS`] 文字を超える語、改行を
    /// 含む語を捨てる。重複は 1 つにまとめる。
    pub(crate) fn new<'a>(words: impl IntoIterator<Item = &'a str>) -> Self {
        let mut words: Vec<String> = words
            .into_iter()
            .map(fold_word)
            .filter(|w| index::is_matchable(w))
            .collect();
        words.sort_unstable();
        words.dedup();
        let list = words.join("\n");
        let index = Index::build(&list, false);
        let ender_initial = index
            .head_keys
            .iter()
            .any(|&key| index::head_has_no_prev(key));
        Matcher {
            ender_initial,
            list: Cow::Owned(list),
            anchors: Anchors::Owned {
                enders: index.enders.into_boxed_slice(),
                links: index.links.into_boxed_slice(),
                head_keys: index.head_keys.into_boxed_slice(),
                head_starts: index.head_starts.into_boxed_slice(),
            },
            words: index.words as usize,
        }
    }

    /// 語の数
    pub(crate) fn len(&self) -> usize {
        self.words
    }

    /// `text` の位置 `pos` にある文末記号 `c` で分割してはいけないか
    pub(crate) fn guards(&self, text: &str, pos: usize, c: char) -> bool {
        let list = &*self.list;
        let anchors = &self.anchors;
        let after = pos + c.len_utf8();
        let ender = fold_width(c);
        // 文末記号で始まる語（利用者が加える語にだけありうる）
        if self.ender_initial {
            let (group, _) = anchors.head_ranges(index::head_key(ender, None, None), None);
            if group.is_some_and(|group| self.group_guards(group, text, pos, after)) {
                return true;
            }
        }
        let Some(prev1) = text[..pos].chars().next_back() else {
            return false;
        };
        let c1 = fold_width(prev1);
        let start1 = pos - prev1.len_utf8();
        let prev2 = text[..start1].chars().next_back();
        let c2 = prev2.map(fold_width);
        let (short, long) = anchors.head_ranges(
            index::head_key(ender, Some(c1), None),
            c2.map(|c2| index::head_key(ender, Some(c1), Some(c2))),
        );
        // 直前の 1 字と文末記号からなる語
        if short.is_some_and(|group| self.group_guards(group, text, start1, after)) {
            return true;
        }
        // 鍵の頭の 3 字（文末記号と直前の 2 字）が同じ錨から始めて、1 字ずつ前へ狭める
        let (
            Some(Range {
                start: mut lo,
                end: mut hi,
            }),
            Some(prev2),
            Some(c2),
        ) = (long, prev2, c2)
        else {
            return false;
        };
        // 比べ終わった鍵の字（文末記号を除く）の、語の列の上のバイト数と、入力の上の語の先頭
        let mut back = c1.len_utf8() + c2.len_utf8();
        let mut start = start1 - prev2.len_utf8();
        while lo < hi {
            // 鍵がここで終わる錨（語の出現は `start` から始まる）は、範囲の先頭に並んでいる。
            // 大半の位置では 1 つもないので、先頭を見てから探す
            let ended_here = |i: usize| index::key_char(list, anchors.ender(i), back).is_none();
            let ended = if ended_here(lo) {
                partition(lo + 1, hi, ended_here)
            } else {
                lo
            };
            if ended > lo && self.group_guards(lo..ended, text, start, after) {
                return true;
            }
            let Some(prev) = text[..start].chars().next_back() else {
                break;
            };
            let folded = fold_width(prev);
            let key = |i: usize| index::key_char(list, anchors.ender(i), back);
            lo = partition(ended, hi, |i| key(i) < Some(folded));
            // 同じ字の範囲は小さいのが普通なので、上限は下限から倍々に広げて挟んでから探す
            hi = gallop(lo, hi, |i| key(i) == Some(folded));
            back += folded.len_utf8();
            start -= prev.len_utf8();
        }
        false
    }

    /// 同じ鍵の錨の組 `group`（語の出現は入力の `start` から始まる）が、`after` の直前の文末記号を守るか
    fn group_guards(&self, group: Range<usize>, text: &str, start: usize, after: usize) -> bool {
        let first = text[start..].chars().next().expect("語の出現の先頭の字");
        if !left_boundary(text[..start].chars().next_back(), first) {
            return false;
        }
        let list = &*self.list;
        let rest = &text[after..];
        let mut lo = group.start;
        // 後半が空の錨（文末記号が語の最後の字）は組の先頭にある
        let ender = self.anchors.ender(lo);
        if index::tail(list, ender).is_empty() {
            if continues(rest, ends_like_predicate(list, ender)) {
                return true;
            }
            lo += 1;
        }
        lo < group.end && self.tail_follows(lo..group.end, rest)
    }

    /// 錨の組 `group`（後半が空でない、同じ鍵の錨）のどれかの後半が、`rest` の先頭に一致するか
    ///
    /// 後半は昇順に並んでいる。`rest` の接頭辞になっている後半 P があれば、`rest` 以下で最大の
    /// 後半 t（二分探索で求める）は P と `rest` の間に並ぶので P で始まり、P は t と `rest` の共通
    /// 接頭辞に収まる。t の接頭辞になっている後半は、t からリンク（真の接頭辞になっている最長の
    /// 後半）を辿ると長い順にすべて現れるので、共通接頭辞に収まる最初のものを探せばよい。
    /// 手間は二分探索と、後半の長さ以下のリンクの鎖の長さで抑えられる。
    fn tail_follows(&self, group: Range<usize>, rest: &str) -> bool {
        let list = &*self.list;
        let tail = |i: usize| index::tail(list, self.anchors.ender(i));
        let n = partition(group.start, group.end, |i| {
            compare_folded(tail(i), rest) != Ordering::Greater
        });
        if n == group.start {
            return false;
        }
        let mut j = n - 1;
        let common = common_prefix_folded(tail(j), rest);
        loop {
            if tail(j).len() <= common {
                return true;
            }
            match self.anchors.link(j) {
                0 => return false,
                link => j = link as usize - 1,
            }
        }
    }
}

/// 後半 `tail`（畳んである）と、入力の続き `rest` を畳んだものの順序。`tail` が `rest` の接頭辞なら Less
fn compare_folded(tail: &str, rest: &str) -> Ordering {
    let mut rest = rest.chars().map(fold_width);
    for t in tail.chars() {
        match rest.next() {
            None => return Ordering::Greater,
            Some(r) if r != t => return t.cmp(&r),
            Some(_) => {}
        }
    }
    Ordering::Less
}

/// 後半 `tail` と、入力の続き `rest` を畳んだものの共通接頭辞の、`tail` の上のバイト数
fn common_prefix_folded(tail: &str, rest: &str) -> usize {
    let mut rest = rest.chars().map(fold_width);
    for (i, t) in tail.char_indices() {
        if rest.next() != Some(t) {
            return i;
        }
    }
    tail.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 語の出現をすべて調べて、位置 `pos` の文末記号で分割してはいけないかを総当たりで求める
    fn guards_naive(words: &[&str], text: &str, pos: usize) -> bool {
        let c = text[pos..].chars().next().unwrap();
        let after = pos + c.len_utf8();
        let chars: Vec<(usize, char)> = text
            .char_indices()
            .map(|(i, c)| (i, fold_width(c)))
            .collect();
        for word in words.iter().map(|w| fold_word(w)) {
            if !index::is_matchable(&word) {
                continue;
            }
            let wchars: Vec<char> = word.chars().collect();
            // 重なり合う出現もすべて数える
            for s in 0..chars.len() {
                let Some(window) = chars.get(s..s + wchars.len()) else {
                    break;
                };
                if window.iter().map(|&(_, c)| c).ne(wchars.iter().copied()) {
                    continue;
                }
                let start = chars[s].0;
                let end = chars.get(s + wchars.len()).map_or(text.len(), |&(i, _)| i);
                let first = text[start..].chars().next().unwrap();
                if !left_boundary(text[..start].chars().next_back(), first) {
                    continue;
                }
                if start <= pos && after < end {
                    return true;
                }
                if after == end
                    && continues(
                        &text[after..],
                        ends_like_predicate(&word, word.len() - wchars.last().unwrap().len_utf8()),
                    )
                {
                    return true;
                }
            }
        }
        false
    }

    /// 文末として働きうる文末記号の位置（規則 4 の ASCII の `!` `?` は直後の字で決める）
    fn active_enders(text: &str) -> Vec<(usize, char)> {
        text.char_indices()
            .filter(|&(pos, c)| {
                if c == '!' || c == '?' {
                    let run_end = text[pos..]
                        .find(|ch: char| ch != '!' && ch != '?')
                        .map_or(text.len(), |n| pos + n);
                    super::super::ascii_run_is_ender(text[run_end..].chars().next())
                } else {
                    is_sentence_ender(c)
                }
            })
            .collect()
    }

    #[test]
    fn test_extract_candidates_keeps_words_with_enders() {
        let words = extract_candidates([
            "モーニング娘。",
            "Yahoo!",
            "Yahoo!ニュース",
            "Hey!Say!JUMP",
            "けいおん!",
            "ご注文はうさぎですか?",
            "猫",
            "東京都",
        ]);
        assert_eq!(
            words,
            vec![
                "Hey!Say!JUMP",
                "Yahoo!",
                "Yahoo!ニュース",
                "けいおん!",
                "ご注文はうさぎですか?",
                "モーニング娘。",
            ]
        );
    }

    #[test]
    fn test_extract_candidates_excludes_symbols_short_and_leading_enders() {
        let words = extract_candidates([
            "!?",         // 記号だけ
            "(^^)!",      // 記号だけ
            "…！",        // 記号だけ
            "娘",         // 文末記号を含まない
            "!",          // 2 文字未満
            "！ニュース", // 文末記号で始まる（畳むと `!ニュース`）
            "。はい",     // 文末記号で始まる
            "#タグ!",     // 表の書式で書けない
            " Yahoo!",    // 表の書式で書けない
            "改\n行!",    // 表の書式で書けない
            "〇〇!",      // 漢数字は英数字として数える
            "ｵｯｹｰ｡",      // 半角の句点も文末記号（半角カタカナは畳まない）
        ]);
        assert_eq!(words, vec!["〇〇!", "ｵｯｹｰ｡"]);
    }

    #[test]
    fn test_extract_candidates_folds_width_and_dedups() {
        let words = extract_candidates([
            "b!",
            "a!",
            "b!",
            "Ｙａｈｏｏ！",
            "Yahoo!",
            "Yahoo！",
            "第１．２",
        ]);
        // `．` は畳まない（畳むと文末記号でなくなる）
        assert_eq!(words, vec!["Yahoo!", "a!", "b!", "第1．2"]);
    }

    #[test]
    fn test_builtin_exceptions_follow_extraction_rules() {
        let words: Vec<&str> = builtin_exceptions().collect();
        assert!(!words.is_empty());
        // 組み込みの表は extract_candidates の出力そのもの（規則を満たし、畳んであり、重複がなく、並んでいる）
        assert_eq!(extract_candidates(words.iter().copied()), words);
    }

    #[test]
    fn test_builtin_exceptions_contain_known_words() {
        let words: Vec<&str> = builtin_exceptions().collect();
        for word in [
            "モーニング娘。",
            "Yahoo!",
            "Yahoo!ニュース",
            "Hey!Say!JUMP",
            "けいおん!",
            "ご注文はうさぎですか?",
            "やはり俺の青春ラブコメはまちがっている。",
        ] {
            assert!(words.binary_search(&word).is_ok(), "{word} がない");
        }
    }

    #[test]
    fn test_builtin_index_matches_the_table() {
        // build.rs が埋め込んだ索引は、いまの表と組み立ての規則から作ったものと一致する
        let index = Index::build(BUILTIN_EXCEPTIONS, true);
        assert_eq!(index.to_bytes(), BUILTIN_INDEX);
        assert_eq!(index.words as usize, builtin_exceptions().count());
        assert_eq!(builtin_matcher().len(), builtin_exceptions().count());
    }

    #[test]
    fn test_builtin_exceptions_version() {
        let (count, hash) = BUILTIN_EXCEPTIONS_VERSION.split_once('-').unwrap();
        assert_eq!(
            count.parse::<usize>().unwrap(),
            builtin_exceptions().count()
        );
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_guards_inside_and_at_end_of_words() {
        let words = ["Yahoo!", "Yahoo!ニュース", "モーニング娘。", "けいおん!!"];
        let matcher = Matcher::new(words);
        assert_eq!(matcher.len(), 4);
        let cases = [
            ("Yahoo!ニュースを見た", "Yahoo".len(), true), // 語の内側
            ("Yahoo!の株価", "Yahoo".len(), true),         // 語の末尾 + 助詞
            ("Yahoo!次", "Yahoo".len(), false),            // 語の末尾 + 助詞以外
            ("Yahoo!", "Yahoo".len(), false),              // 語の末尾 + 入力の終わり
            ("モーニング娘。のライブ", "モーニング娘".len(), true),
            ("モーニング娘。次", "モーニング娘".len(), false),
            ("モーニング娘。もう一度", "モーニング娘".len(), false), // 文頭に立つ語
            ("モーニング娘。からの発表", "モーニング娘".len(), true), // 助詞のように続く語
            ("娘。のライブ", "娘".len(), false),                     // 語の途中から始まる
            ("けいおん!!次", "けいおん".len(), true),                // 1 つ目は内側
            ("けいおん!!次", "けいおん!".len(), false),              // 2 つ目は末尾
            ("けいおん!!の話", "けいおん!".len(), true),
            ("けいおん!!でも", "けいおん!".len(), false), // 文末と同じ形の語の後ろの「でも」
            ("Yahoo!でも検索", "Yahoo".len(), true),      // 名詞のように終わる語の後ろの「でも」
        ];
        for (text, pos, expected) in cases {
            let c = text[pos..].chars().next().unwrap();
            assert_eq!(matcher.guards(text, pos, c), expected, "{text} @ {pos}");
            assert_eq!(guards_naive(&words, text, pos), expected, "{text} @ {pos}");
        }
    }

    #[test]
    fn test_left_boundary() {
        let words = ["べる。", "流。", "モーニング娘。", "Yahoo!", "けいおん!"];
        let matcher = Matcher::new(words);
        let cases = [
            ("食べる。では次", "食べる".len(), false), // ひらがなの語が漢字に続く
            ("べる。の話", "べる".len(), true),        // 入力の先頭
            ("主流。です", "主流".len(), false),       // 漢字の語が漢字に続く
            ("この流。です", "この流".len(), true),    // 漢字の語がひらがなに続く
            ("元モーニング娘。の", "元モーニング娘".len(), true), // カタカナの語が漢字に続く
            ("プチモーニング娘。の", "プチモーニング娘".len(), false), // カタカナの語がカタカナに続く
            ("MyYahoo!の", "MyYahoo".len(), false),                    // 英字の語が英字に続く
            ("のYahoo!の", "のYahoo".len(), true),
            ("アニメけいおん!の", "アニメけいおん".len(), false), // ひらがなの語がカタカナに続く
            ("「けいおん!の", "「けいおん".len(), true),
        ];
        for (text, pos, expected) in cases {
            let c = text[pos..].chars().next().unwrap();
            assert_eq!(matcher.guards(text, pos, c), expected, "{text} @ {pos}");
            assert_eq!(guards_naive(&words, text, pos), expected, "{text} @ {pos}");
        }
    }

    #[test]
    fn test_width_is_folded_in_matching() {
        let words = ["Yahoo!ニュース", "Hey!Say!JUMP", "Ｑさま！！"];
        let matcher = Matcher::new(words);
        let cases = [
            ("Yahoo！ニュース", "Yahoo".len(), true),
            ("Ｙａｈｏｏ！ニュース", "Ｙａｈｏｏ".len(), true),
            ("YAHOO!ニュース", "YAHOO".len(), false), // 大文字・小文字は畳まない
            ("Ｈｅｙ！Ｓａｙ！ＪＵＭＰ", "Ｈｅｙ".len(), true), // 全角の `！` は常に文末になりうる
            ("Qさま!!の", "Qさま!".len(), true),      // 表の語が全角でも畳んで持つ
        ];
        for (text, pos, expected) in cases {
            let c = text[pos..].chars().next().unwrap();
            assert_eq!(matcher.guards(text, pos, c), expected, "{text} @ {pos}");
            assert_eq!(guards_naive(&words, text, pos), expected, "{text} @ {pos}");
        }
    }

    #[test]
    fn test_continuations_and_sentence_starters() {
        // 名詞のように終わる語
        for (rest, expected) in [
            ("の", true),
            ("から", true),
            ("まで", true),
            ("より", true),
            ("って", true),
            ("など", true),
            ("だけ", true),
            ("しか", true),
            ("さえ", true),
            ("くらい", true),
            ("ぐらい", true),
            ("ほど", true),
            ("として", true),
            ("について", true),
            ("では", true),
            ("でも", true),
            ("はいつも", true),
            ("もう一度", false),
            ("もちろん", false),
            ("とにかく", false),
            ("やはり", false),
            ("しかし", false),
            ("はい、", false),
            ("はい", false),
            ("次", false),
            ("", false),
            ("！", false),
        ] {
            assert_eq!(continues(rest, false), expected, "{rest}");
        }
        // 文末と同じ形で終わる語
        for (rest, expected) in [
            ("の", true),
            ("を", true),
            ("でも", false),
            ("では", false),
            ("で、", false),
            ("で有名", true),
            ("とはいえ", false),
            ("と思った", true),
            ("だけど", false),
            ("だけが", true),
        ] {
            assert_eq!(continues(rest, true), expected, "{rest}");
        }
    }

    #[test]
    fn test_matcher_ignores_words_without_enders() {
        let matcher = Matcher::new(["猫", "", "犬!", "改\n行!"]);
        assert_eq!(matcher.len(), 1);
        assert!(matcher.guards("犬!の", "犬".len(), '!'));
        assert!(!matcher.guards("猫!の", "猫".len(), '!'));
    }

    #[test]
    fn test_empty_matcher_never_guards() {
        let matcher = Matcher::new([]);
        assert_eq!(matcher.len(), 0);
        assert!(!matcher.guards("Yahoo!の", 5, '!'));
    }

    #[test]
    fn test_matcher_agrees_with_naive_search_on_random_input() {
        // 小さな字母で語と入力を作り、語が入れ子・部分的に重なる状況、字種の境界、字幅の畳み込み、
        // 続きの語・文頭に立つ語を総当たりと突き合わせる
        let alphabet = [
            'a', 'b', '!', '?', '。', 'の', 'も', 'う', 'ア', '字', 'ｂ', '！', '．',
        ];
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for _ in 0..3000 {
            let words: Vec<String> = (0..next(8) + 1)
                .map(|_| {
                    (0..next(5) + 1)
                        .map(|_| alphabet[next(alphabet.len())])
                        .collect()
                })
                .collect();
            let text: String = (0..next(40))
                .map(|_| alphabet[next(alphabet.len())])
                .collect();
            let refs: Vec<&str> = words.iter().map(String::as_str).collect();
            let matcher = Matcher::new(refs.iter().copied());
            for (pos, c) in active_enders(&text) {
                assert_eq!(
                    matcher.guards(&text, pos, c),
                    guards_naive(&refs, &text, pos),
                    "words={refs:?} text={text} pos={pos}"
                );
            }
        }
    }

    #[test]
    fn test_many_tails_after_the_same_key() {
        // 同じ「それいけ!」に続く後半が多く、互いに接頭辞を共有する
        let tails = [
            "ア",
            "アン",
            "アンパン",
            "アンパンマン",
            "アンパンマンの歌",
            "アイ",
            "カ",
            "カレー",
            "ン",
            "アンコ",
            "アンパ",
            "アンパンマン2",
        ];
        let words: Vec<String> = tails.iter().map(|t| format!("それいけ!{t}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let matcher = Matcher::new(refs.iter().copied());
        let pos = "それいけ".len();
        for after in [
            "アンパンマンの歌を歌う",
            "アンパンマンを見る",
            "アンパを",
            "アンパ",
            "アンダー",
            "アウト",
            "カレーパン",
            "キャラ",
            "ンン",
            "",
            "Z",
            "Ａｎ",
        ] {
            let text = format!("それいけ!{after}");
            assert_eq!(
                matcher.guards(&text, pos, '!'),
                guards_naive(&refs, &text, pos),
                "{text}"
            );
        }
    }

    #[test]
    fn test_tails_sharing_long_prefixes_are_checked_quickly() {
        // 後半 "a", "za", "zza", … と続き "zzz…" の組み合わせは、共通接頭辞を 1 文字ずつ
        // 縮める探し方だと二乗の手間になる。接頭辞の鎖で辿れば一度の二分探索で済む
        let m = 200;
        let words: Vec<String> = (0..m).map(|k| format!("X。{}a", "z".repeat(k))).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let matcher = Matcher::new(refs.iter().copied());
        for tail in [
            "z".repeat(m),
            format!("{}a", "z".repeat(m / 2)),
            "zz".to_string(),
            "a".to_string(),
        ] {
            let text = format!("X。{tail}");
            let pos = "X".len();
            assert_eq!(
                matcher.guards(&text, pos, '。'),
                guards_naive(&refs, &text, pos),
                "{text}"
            );
        }
    }

    #[test]
    fn test_words_over_the_limits_are_ignored() {
        use index::{MAX_WORD_CHARS, MAX_WORD_ENDERS};
        let enders_ok = format!("語{}", "!".repeat(MAX_WORD_ENDERS));
        let enders_over = format!("語{}", "!".repeat(MAX_WORD_ENDERS + 1));
        let chars_ok = format!("{}!", "あ".repeat(MAX_WORD_CHARS - 1));
        let chars_over = format!("{}!", "あ".repeat(MAX_WORD_CHARS));
        assert!(index::is_matchable(&enders_ok));
        assert!(!index::is_matchable(&enders_over));
        assert!(index::is_matchable(&chars_ok));
        assert!(!index::is_matchable(&chars_over));
        let matcher = Matcher::new([
            enders_ok.as_str(),
            enders_over.as_str(),
            chars_ok.as_str(),
            chars_over.as_str(),
        ]);
        assert_eq!(matcher.len(), 2);
        // 組み込みの表の語はどれも上限に収まる
        assert!(builtin_exceptions().all(index::is_matchable));
    }

    #[test]
    fn test_dense_enders_with_ender_heavy_word_stay_linear() {
        // 文末記号だけの語と、文末記号だけの長い入力（前へ辿る手間は語の文末記号の数で抑えられる）
        let word = "。".repeat(index::MAX_WORD_ENDERS);
        let matcher = Matcher::new([word.as_str()]);
        let text = "。".repeat(200_000);
        let guarded = text
            .char_indices()
            .filter(|&(pos, c)| matcher.guards(&text, pos, c))
            .count();
        // 最後の 1 つ以外は語の内側にある
        assert_eq!(guarded, 200_000 - 1);
    }
}
