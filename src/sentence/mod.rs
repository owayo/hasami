//! 文分割（辞書を使わない）
//!
//! 日本語の文の境界を、辞書をロードせずに求める。規則は次のとおり。
//!
//! 1. 文末記号 `。！？!?‼⁇⁈⁉．｡` の連続（`！？`、`。。`）を 1 つの文末とする
//! 2. 括弧類の内側の文末記号では分割しない。対象は
//!    `「」『』（）()〔〕［］[]｛｝{}〈〉《》【】〖〗〘〙〚〛｟｠“”‘’«»‹›｢｣〝〟`（`〝` は `〞` でも閉じる）。
//!    開閉が同じ字の ASCII 引用符（`"` `'`）は、アポストロフィや寸法表記と区別できないので対象外
//! 3. 括弧は先に対応を取り、対応の取れた組だけを「分割しない範囲」にする。対応しない開き括弧・
//!    閉じ括弧はただの文字として扱うので、閉じ忘れた括弧が後続の文を巻き込まない。入れ子が
//!    崩れている場合は、同じ種類の開き括弧まで遡って対応を取り、間の開き括弧は対応なしとする
//! 4. ASCII の `!` `?` は、直後が英数字か ASCII 記号なら文末にしない（URL の `?id=1`、`!important`）。
//!    直後が空白・日本語・閉じ括弧・行末なら文末にする。`?!` のような連続は最後の字の直後で決める
//! 5. 文末記号の直後に対応しない閉じ括弧が続く場合（`終わりだ。」`）は、その閉じ括弧までを
//!    前の文に含める
//! 6. 改行は既定では文の区切りにしない（[`LineBreaks::Join`]）。[`LineBreaks::Split`] なら括弧の
//!    外側の改行で区切る。括弧の内側の改行はどちらでも区切らない
//! 7. 文の前後の空白は範囲から除く。空白だけの区間は文にしない
//! 8. 例外表に載っている語の内側の文末記号では分割しない（`Yahoo!ニュース`）。語の末尾の
//!    文末記号は、直後が続きの語（助詞の `の は が を に と で も や へ` と、`から まで より って
//!    など だけ しか さえ くらい ぐらい ほど として について`）で始まるときだけ分割しない
//!    （`モーニング娘。のライブ。` は 1 文、`好きなのはモーニング娘。次の話題。` は 2 文）。
//!    - 語の出現は左の境界を満たすものだけを数える。語の先頭がひらがなで直前がかな・漢字のとき、
//!      語の先頭がカタカナ・漢字・英数字で直前も同じ字種のときは、語の途中からの一致とみなす
//!      （`食べる。` の中の `べる。`、`主流。` の中の `流。` は数えない）
//!    - 直後が文頭に立つ語（`もう もし もちろん もっと とにかく ところで やはり しかし` など）で
//!      始まるなら分割する（`好きなのはモーニング娘。もう一度言う。` は 2 文）。文末記号の直前が
//!      ひらがなの語（`寒いね。` `好きだ。` のように普通の文末と同じ形の語）の後ろでは、`でも では
//!      で、 とはいえ だけど` も文頭の語とみなす（`Yahoo!では` は助詞として続く）。直後が読点などの
//!      `はい` は感動詞とみなす
//!    - 照合では全角の英数字・記号を半角に畳む（`Yahoo！ニュース` `Ｙａｈｏｏ！ニュース` も守る。
//!      大文字・小文字は区別する）
//! 9. 全角ピリオド `．` と半角の句点 `｡` は、前後がどちらも数字（半角・全角）なら文末にしない
//!    （`３．１４`、`第３．２節`）。`…を述べる．３章では…` のように片側だけが数字なら文末にする
//!
//! 規則で使う字の集合は、[`is_sentence_ender`]・[`ascii_run_is_ender`]・[`closing_bracket`]・
//! [`is_closing_bracket`] で同じ基準のまま判定できる。
//!
//! 例外表は、推奨辞書から抽出した組み込みの表（[`builtin_exceptions`]）と、利用者が加える語
//! （[`SplitOptions::extra_exceptions`]）からなる。1 つの文末記号を複数の語の出現が覆うときは、
//! どれか 1 つの内側にあれば分割しない。そのため `Yahoo!` と `Yahoo!ニュース` が両方あっても、
//! 長い `Yahoo!ニュース` の内側として扱われる。
//!
//! 組み込みの表の索引は build.rs がビルド時に作って埋め込むので、[`Splitter::new`] と最初の分割に
//! 組み立ての手間はかからない。表の版は [`BUILTIN_EXCEPTIONS_VERSION`] で分かる。
//!
//! どの処理も入力の長さにほぼ比例する時間で済む。入力はバイト単位で 1 度だけ走査し、分割に関わらない
//! 文字は復号せずに読み飛ばす。括弧の対応は 1 パスのスタックで取る。例外語の照合は文末記号を起点に、
//! 前へは索引の二分探索で 1 字ずつ狭め、後ろも二分探索で調べる。前へ辿る照合が入力の 1 文字の上を
//! 通る回数は、1 語の中の文末記号の数（組み込みの表では最多 9 個、利用者の語は 16 個まで）で
//! 抑えられる。
//!
//! ```
//! use hasami::sentence::{self, SplitOptions};
//!
//! let text = "「うまく行くかな？」と思った。Yahoo!ニュースを見た。";
//! let sentences: Vec<&str> = sentence::split(text, &SplitOptions::default())
//!     .into_iter()
//!     .map(|s| &text[s.range])
//!     .collect();
//! assert_eq!(sentences, ["「うまく行くかな？」と思った。", "Yahoo!ニュースを見た。"]);
//! ```

use std::fmt;
use std::ops::Range;

mod chars;
mod exceptions;
mod index;

pub use chars::is_sentence_ender;
use exceptions::Matcher;
pub use exceptions::{BUILTIN_EXCEPTIONS_VERSION, builtin_exceptions, extract_candidates};

/// 改行の扱い
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineBreaks {
    /// 改行は文の区切りにしない（既定）
    ///
    /// Markdown の折り返しは見た目上の改行で、一文一行の文書は改行の直前に句点がある。
    #[default]
    Join,
    /// 括弧の外側の改行を文の区切りにする（句点を打たない一文一行の文書向け）
    Split,
}

/// 文分割の設定
#[derive(Debug, Clone)]
pub struct SplitOptions<'a> {
    /// 改行の扱い
    pub line_breaks: LineBreaks,
    /// 組み込みの例外表に加える語
    ///
    /// 文末記号を含まない語は分割に影響しないので無視する。照合の手間を抑えるため、文末記号を
    /// 16 個より多く含む語と 256 文字より長い語も無視する（組み込みの表の語は文末記号が最多 9 個、
    /// 最長 124 文字）。
    pub extra_exceptions: &'a [&'a str],
    /// 組み込みの例外表を使うか（既定 true）
    pub use_builtin_exceptions: bool,
}

impl Default for SplitOptions<'_> {
    fn default() -> Self {
        SplitOptions {
            line_breaks: LineBreaks::default(),
            extra_exceptions: &[],
            use_builtin_exceptions: true,
        }
    }
}

/// 分割した 1 文
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    /// 入力上のバイト範囲（前後の空白を除く、文末記号を含む）
    pub range: Range<usize>,
    /// 括弧の内側に文末記号があり、括弧ごと 1 文にしたか
    pub embedded_enders: bool,
    /// この文を終わらせたもの
    pub end: SentenceEnd,
}

/// 文を終わらせたもの
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceEnd {
    /// 文末記号（直後の対応しない閉じ括弧を含む）
    Ender,
    /// 改行（[`LineBreaks::Split`] のときだけ）
    LineBreak,
    /// 入力の終わり
    EndOfText,
}

/// テキストを文に分割する
///
/// 辞書をロードせずに呼べる。同じ設定で何度も分割するなら、[`Splitter`] を作って使い回すほうが速い
/// （利用者が加える語の照合器を呼び出しごとに組み立てずに済む）。
pub fn split(text: &str, options: &SplitOptions<'_>) -> Vec<Sentence> {
    Splitter::new(options).split(text)
}

/// 文分割器
///
/// 例外表の照合器を組み立てて持つ。組み込みの例外表の照合器はプロセスで 1 つだけ作って共有するので、
/// 組み立てるのは初めて組み込みの表を使うときの 1 度だけで済む。スレッド間で共有できる（`Send + Sync`）。
#[derive(Clone)]
pub struct Splitter {
    line_breaks: LineBreaks,
    builtin: Option<&'static Matcher>,
    extra: Option<Matcher>,
}

impl Default for Splitter {
    fn default() -> Self {
        Splitter::new(&SplitOptions::default())
    }
}

impl fmt::Debug for Splitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Splitter")
            .field("line_breaks", &self.line_breaks)
            .field("builtin_exceptions", &self.builtin.map_or(0, Matcher::len))
            .field(
                "extra_exceptions",
                &self.extra.as_ref().map_or(0, Matcher::len),
            )
            .finish()
    }
}

impl Splitter {
    /// 設定から文分割器を作る
    pub fn new(options: &SplitOptions<'_>) -> Self {
        let builtin = options
            .use_builtin_exceptions
            .then(exceptions::builtin_matcher);
        let extra = options
            .extra_exceptions
            .iter()
            .any(|w| w.chars().any(is_sentence_ender))
            .then(|| Matcher::new(options.extra_exceptions.iter().copied()));
        Splitter {
            line_breaks: options.line_breaks,
            builtin,
            extra,
        }
    }

    /// テキストを文に分割する（[`split`] と同じ）
    pub fn split(&self, text: &str) -> Vec<Sentence> {
        self.split_at_breaks(text, &[])
    }

    /// 改行とみなす位置を別に渡して、テキストを文に分割する
    ///
    /// `breaks` の各バイト位置に改行があるものとして扱い、[`LineBreaks::Split`] の改行と同じく
    /// 括弧の外側なら区切る（区切った文は [`SentenceEnd::LineBreak`] で終わる）。改行の字を取り除いた
    /// 解析用のテキストを、元の改行の位置で区切りたいときに使う。入力に残っている改行の字は、設定の
    /// [`LineBreaks`] に従う。`breaks` は順不同で、重複してもよい。
    ///
    /// # Panics
    ///
    /// `breaks` の位置が `text` の文字の境界でない（長さを超える場合を含む）とき。
    pub fn split_with_breaks(&self, text: &str, breaks: &[usize]) -> Vec<Sentence> {
        let mut breaks = breaks.to_vec();
        breaks.sort_unstable();
        breaks.dedup();
        for &pos in &breaks {
            assert!(
                text.is_char_boundary(pos),
                "改行とみなす位置 {pos} が入力の文字の境界でない（入力は {} バイト）",
                text.len()
            );
        }
        self.split_at_breaks(text, &breaks)
    }

    /// 昇順の `breaks` の位置に幅 0 の改行を足して分割する
    fn split_at_breaks(&self, text: &str, breaks: &[usize]) -> Vec<Sentence> {
        let line_break = |pos| Mark {
            pos,
            len: 0,
            kind: MarkKind::LineBreak,
        };
        let mut marks = Vec::new();
        let mut pending = breaks.iter().copied().peekable();
        self.scan(text, true, self.line_breaks == LineBreaks::Split, |mark| {
            // 同じ位置なら、渡された改行（その位置の字の直前）を先に置く
            while let Some(pos) = pending.next_if(|&pos| pos <= mark.pos) {
                marks.push(line_break(pos));
            }
            marks.push(mark)
        });
        marks.extend(pending.map(line_break));
        let brackets = match_brackets(&mut marks);

        let mut sentences = Vec::new();
        let mut start = 0;
        let mut embedded_enders = false;
        let mut b = 0;
        let mut i = 0;
        while let Some(&mark) = marks.get(i) {
            while brackets.get(b).is_some_and(|r| r.end <= mark.pos) {
                b += 1;
            }
            if brackets.get(b).is_some_and(|r| mark.is_inside(r)) {
                // 対応の取れた括弧の内側では分割しない
                embedded_enders |= mark.kind == MarkKind::Ender { active: true };
                i += 1;
                continue;
            }
            match mark.kind {
                MarkKind::Ender { active: true } => {
                    let (end, next) = ender_run_end(&marks, i);
                    push_sentence(
                        text,
                        start..end,
                        embedded_enders,
                        SentenceEnd::Ender,
                        &mut sentences,
                    );
                    start = end;
                    embedded_enders = false;
                    i = next;
                }
                MarkKind::LineBreak => {
                    push_sentence(
                        text,
                        start..mark.pos,
                        embedded_enders,
                        SentenceEnd::LineBreak,
                        &mut sentences,
                    );
                    start = mark.end();
                    embedded_enders = false;
                    i += 1;
                }
                _ => i += 1,
            }
        }
        if start < text.len() {
            push_sentence(
                text,
                start..text.len(),
                embedded_enders,
                SentenceEnd::EndOfText,
                &mut sentences,
            );
        }
        sentences
    }

    /// 入力全体を隙間なく区間に分け、各区間の終わりのバイト位置を昇順で返す（最後は `text.len()`）
    ///
    /// 形態素解析の前分割（ラティスを小さく保つための分割）向け。区間は
    /// `0..ends[0]`, `ends[0]..ends[1]`, … で、どれも空でない（空の入力には空の列を返す）。
    /// [`Splitter::split`] とは次の点が違う。
    ///
    /// - 括弧の対応を見ない（括弧の内側の文末記号でも区切る。直後の閉じ括弧は次の区間に入る）
    /// - 改行は [`LineBreaks`] の設定によらず、括弧の内外を問わず区切る（改行の直後で区切る）
    /// - 前後の空白を除かない。空白だけの区間も返す
    ///
    /// 文末記号の扱い（規則 1・4）と例外表（規則 8）は同じなので、`Hey!Say!JUMP` や
    /// `Yahoo!ニュース` のような語の内側では区切らない。
    pub fn chunk_ends(&self, text: &str) -> Vec<usize> {
        let mut ends = Vec::new();
        // 区切る位置が決まっていない文末記号の連続の終わり
        let mut run_end: Option<usize> = None;
        self.scan(text, false, true, |mark| {
            let active_ender = mark.kind == MarkKind::Ender { active: true };
            if let Some(end) = run_end {
                if active_ender && mark.pos == end {
                    run_end = Some(mark.end());
                    return;
                }
                ends.push(end);
                run_end = None;
            }
            if active_ender {
                run_end = Some(mark.end());
            } else if mark.kind == MarkKind::LineBreak {
                ends.push(mark.end());
            }
        });
        ends.extend(run_end);
        if !text.is_empty() && ends.last() != Some(&text.len()) {
            ends.push(text.len());
        }
        ends
    }

    /// 入力を 1 度だけ走査して、文末記号・括弧・改行の出現を位置の順に `emit` へ渡す
    ///
    /// 文末記号には、規則 4（ASCII の `!` `?`）と例外表で打ち消されずに文末として働くかを記録する。
    /// `brackets` が偽なら括弧を、`line_breaks` が偽なら改行を渡さない。
    fn scan(&self, text: &str, brackets: bool, line_breaks: bool, mut emit: impl FnMut(Mark)) {
        let bytes = text.as_bytes();
        let mut i = 0;
        // 分割に関わりうる先頭バイトまで一気に進める。1 バイトごとの判定が直前の結果に依存しない
        // ので、文字ごとに長さを引いて進むより速い
        while let Some(n) = bytes[i..].iter().position(|&b| MAY_BE_MARK[b as usize]) {
            i += n;
            let b = bytes[i];
            if b >= 0x80 && !may_be_mark(b, bytes[i + 1]) {
                i += if b < 0xE0 { 2 } else { 3 };
                continue;
            }
            let c = text[i..].chars().next().unwrap_or_default();
            let len = c.len_utf8() as u8;
            match classify(c) {
                CharKind::Other => {}
                CharKind::Ender => {
                    if !is_decimal_point(text, i, c) {
                        emit(self.ender_mark(text, i, c, true));
                    }
                }
                CharKind::AsciiEnder => {
                    // 規則 4: 連続の直後の文字で、連続全体が文末として働くかを決める
                    let run_end = bytes[i..]
                        .iter()
                        .position(|&b| b != b'!' && b != b'?')
                        .map_or(bytes.len(), |n| i + n);
                    let active = ascii_run_is_ender(text[run_end..].chars().next());
                    for (offset, &b) in bytes[i..run_end].iter().enumerate() {
                        emit(self.ender_mark(text, i + offset, b as char, active));
                    }
                    i = run_end;
                    continue;
                }
                CharKind::Open(bracket) => {
                    if brackets {
                        emit(Mark {
                            pos: i,
                            len,
                            kind: MarkKind::Open(bracket),
                        });
                    }
                }
                CharKind::Close(bracket) => {
                    if brackets {
                        emit(Mark {
                            pos: i,
                            len,
                            kind: MarkKind::Close {
                                bracket,
                                matched: false,
                            },
                        });
                    }
                }
                CharKind::LineBreak => {
                    // CRLF は LF の側で 1 つの改行として数える
                    if line_breaks && !(c == '\r' && bytes.get(i + 1) == Some(&b'\n')) {
                        emit(Mark {
                            pos: i,
                            len,
                            kind: MarkKind::LineBreak,
                        });
                    }
                }
            }
            i += len as usize;
        }
    }

    /// 文末記号の印を作る。規則 4 で文末として働くなら、例外表の語に守られていないかも調べる
    #[inline]
    fn ender_mark(&self, text: &str, pos: usize, c: char, active: bool) -> Mark {
        let active = active
            && !self.builtin.is_some_and(|m| m.guards(text, pos, c))
            && !self.extra.as_ref().is_some_and(|m| m.guards(text, pos, c));
        Mark {
            pos,
            len: c.len_utf8() as u8,
            kind: MarkKind::Ender { active },
        }
    }
}

/// 分割に関わる文字の出現
#[derive(Debug, Clone, Copy)]
struct Mark {
    /// 入力上のバイト位置
    pos: usize,
    /// 文字のバイト長
    len: u8,
    kind: MarkKind,
}

impl Mark {
    /// 文字の直後のバイト位置
    fn end(&self) -> usize {
        self.pos + self.len as usize
    }

    /// 括弧の範囲 `range`（開き括弧から閉じ括弧の直後まで）の内側にあるか（`range.end` より前にあること
    /// は呼び出し側で確かめてある）
    ///
    /// 幅 0 の改行（[`Splitter::split_with_breaks`] の位置）は、開き括弧と同じ位置なら括弧の直前にある
    /// ので外側とみなす。
    fn is_inside(&self, range: &Range<usize>) -> bool {
        if self.len == 0 {
            range.start < self.pos
        } else {
            range.start <= self.pos
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkKind {
    /// 文末記号。`active` は文末として働くか（規則 4 と例外表で打ち消されていないか）
    Ender { active: bool },
    /// 開き括弧（値は括弧の種類）
    Open(u8),
    /// 閉じ括弧。`matched` は対応する開き括弧があったか
    Close { bracket: u8, matched: bool },
    /// 改行
    LineBreak,
}

/// 対応の取れた括弧の範囲（開き括弧から閉じ括弧の直後まで）のうち外側のものを昇順で返し、
/// 対応の取れた閉じ括弧に印を付ける
///
/// 1 パスのスタックで対応を取る。入れ子が崩れていても、同じ種類の開き括弧まで遡って対応を取り、
/// 間に積まれていた開き括弧は対応なしとして捨てる（各開き括弧は 1 度しか取り出さない）。
/// 積んである開き括弧の数を種類ごとに数えておき、対応する開き括弧のない閉じ括弧は積みを
/// 探さずに読み飛ばすので、開き括弧が大量に残った入力でも線形で済む。
fn match_brackets(marks: &mut [Mark]) -> Vec<Range<usize>> {
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut open_counts = [0usize; BRACKET_KINDS];
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for mark in marks.iter_mut() {
        match &mut mark.kind {
            MarkKind::Open(bracket) => {
                stack.push((*bracket, mark.pos));
                open_counts[*bracket as usize] += 1;
            }
            MarkKind::Close { bracket, matched } => {
                if open_counts[*bracket as usize] == 0 {
                    continue;
                }
                while let Some((open_bracket, open)) = stack.pop() {
                    open_counts[open_bracket as usize] -= 1;
                    if open_bracket == *bracket {
                        *matched = true;
                        // 対応の組は入れ子か離れているかのどちらかなので、内側の範囲は捨ててよい
                        while ranges.last().is_some_and(|r| r.start >= open) {
                            ranges.pop();
                        }
                        ranges.push(open..mark.pos + mark.len as usize);
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    ranges
}

/// `marks[i]` の文末記号から始まる文末（文末記号の連続と、直後の対応しない閉じ括弧）の終わりの
/// バイト位置と、その次の marks 上の位置を返す
fn ender_run_end(marks: &[Mark], mut i: usize) -> (usize, usize) {
    let mut end = marks[i].end();
    i += 1;
    while let Some(mark) = marks.get(i) {
        if mark.pos != end || mark.kind != (MarkKind::Ender { active: true }) {
            break;
        }
        end = mark.end();
        i += 1;
    }
    while let Some(mark) = marks.get(i) {
        if mark.pos != end || !matches!(mark.kind, MarkKind::Close { matched: false, .. }) {
            break;
        }
        end = mark.end();
        i += 1;
    }
    (end, i)
}

/// 前後の空白を除いた文を加える（空白だけなら加えない）
fn push_sentence(
    text: &str,
    range: Range<usize>,
    embedded_enders: bool,
    end: SentenceEnd,
    out: &mut Vec<Sentence>,
) {
    let body = text[range.clone()].trim_start();
    if body.is_empty() {
        return;
    }
    let start = range.end - body.len();
    out.push(Sentence {
        range: start..start + body.trim_end().len(),
        embedded_enders,
        end,
    });
}

/// ASCII の `!` `?` の連続が、直後の文字 `next` のもとで文末として働くか（規則 4）
///
/// 直後が英数字・ASCII 記号なら文末にしない（`?id=1`、`!important`）。直後が空白・日本語・
/// 閉じ括弧・行末（`None`）なら文末にする。`?!` のような連続は、最後の字の直後の文字を渡す。
pub fn ascii_run_is_ender(next: Option<char>) -> bool {
    match next {
        None => true,
        Some(c) => {
            matches!(c, ')' | ']' | '}') || !(c.is_ascii_alphanumeric() || c.is_ascii_punctuation())
        }
    }
}

/// 位置 `pos` の全角ピリオド `．`・半角の句点 `｡` が、数字に挟まれた小数点や節番号の区切りか（規則 9）
///
/// 前後の字がどちらも数字（半角・全角）なら文末にしない（`３．１４`、`第３．２節`）。片側だけを
/// 見ると、`．` を句点に使う文書の「…を述べる．３章では…」をつないでしまうので、両側を見る。
#[inline]
fn is_decimal_point(text: &str, pos: usize, c: char) -> bool {
    let is_digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    (c == '．' || c == '｡')
        && text[..pos].chars().next_back().is_some_and(is_digit)
        && text[pos + c.len_utf8()..]
            .chars()
            .next()
            .is_some_and(is_digit)
}

/// 分割に関わりうる文字の先頭バイトか
///
/// ASCII の `!` `?` `()[]{}`・改行と、2 バイト目で絞り込む多バイト文字（[`may_be_mark`]）の
/// 先頭バイトが該当する。どれも UTF-8 の継続バイト（0x80〜0xBF）ではないので、バイト単位で
/// 探しても文字の途中には止まらない。
static MAY_BE_MARK: [bool; 256] = {
    let mut table = [false; 256];
    let mut b = 0;
    while b < 256 {
        table[b] = matches!(
            b as u8,
            b'\n'
                | b'\r'
                | b'!'
                | b'?'
                | b'('
                | b')'
                | b'['
                | b']'
                | b'{'
                | b'}'
                | 0xC2
                | 0xE2
                | 0xE3
                | 0xEF
        );
        b += 1;
    }
    table
};

/// 先頭バイト `b0`（MAY_BE_MARK が真の多バイト文字）と 2 バイト目 `b1` から、分割に関わりうる文字か
///
/// 対象の文字は `«»`（U+00AB・U+00BB）、一般句読点（U+2000〜U+207F）、CJK の記号と句読点
/// （U+3000〜U+303F）、全角形（U+FF00〜U+FF7F）のどれかにある。
#[inline]
fn may_be_mark(b0: u8, b1: u8) -> bool {
    match b0 {
        0xC2 => b1 == 0xAB || b1 == 0xBB,
        0xE2 => b1 == 0x80 || b1 == 0x81,
        0xE3 => b1 == 0x80,
        0xEF => b1 == 0xBC || b1 == 0xBD,
        _ => true,
    }
}

/// 分割に関わる文字の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharKind {
    Other,
    /// 全角などの文末記号（前後によらず文末）
    Ender,
    /// ASCII の `!` `?`（直後の文字で文末かどうかが決まる）
    AsciiEnder,
    Open(u8),
    Close(u8),
    LineBreak,
}

#[inline]
fn classify(c: char) -> CharKind {
    match c {
        '!' | '?' => CharKind::AsciiEnder,
        '。' | '！' | '？' | '‼' | '⁇' | '⁈' | '⁉' | '．' | '｡' => CharKind::Ender,
        '\n' | '\r' | '\u{2028}' | '\u{2029}' => CharKind::LineBreak,
        _ => match bracket(c) {
            Some((kind, true)) => CharKind::Open(kind),
            Some((kind, false)) => CharKind::Close(kind),
            None => CharKind::Other,
        },
    }
}

/// 開き括弧 `open` に対応する閉じ括弧（規則 2 の開き括弧でなければ `None`）
///
/// `〝` には `〟` を返す（`〝` は `〞` でも閉じる。[`is_closing_bracket`] は両方を閉じ括弧とみなす）。
/// 開閉が同じ字の ASCII 引用符（`"` `'`）は括弧として扱わないので `None`。
pub fn closing_bracket(open: char) -> Option<char> {
    Some(match open {
        '「' => '」',
        '『' => '』',
        '（' => '）',
        '(' => ')',
        '〔' => '〕',
        '［' => '］',
        '[' => ']',
        '｛' => '｝',
        '{' => '}',
        '〈' => '〉',
        '《' => '》',
        '【' => '】',
        '〖' => '〗',
        '〘' => '〙',
        '〚' => '〛',
        '｟' => '｠',
        '“' => '”',
        '‘' => '’',
        '«' => '»',
        '‹' => '›',
        '｢' => '｣',
        '〝' => '〟',
        _ => return None,
    })
}

/// 閉じ括弧か（規則 2 の閉じ括弧。`〟` と `〞` を含む）
#[inline]
pub fn is_closing_bracket(c: char) -> bool {
    matches!(bracket(c), Some((_, false)))
}

/// 括弧の種類の数
const BRACKET_KINDS: usize = 22;

/// 括弧の種類と、開き括弧か（括弧でなければ None）
///
/// `〝` は `〟` と `〞` のどちらでも閉じる。品詞の正規化（`analyzer` feature の `pos` モジュール）も
/// 同じ字の集合を使う。
#[inline]
pub(crate) fn bracket(c: char) -> Option<(u8, bool)> {
    Some(match c {
        '「' => (0, true),
        '」' => (0, false),
        '『' => (1, true),
        '』' => (1, false),
        '（' => (2, true),
        '）' => (2, false),
        '(' => (3, true),
        ')' => (3, false),
        '〔' => (4, true),
        '〕' => (4, false),
        '［' => (5, true),
        '］' => (5, false),
        '[' => (6, true),
        ']' => (6, false),
        '｛' => (7, true),
        '｝' => (7, false),
        '{' => (8, true),
        '}' => (8, false),
        '〈' => (9, true),
        '〉' => (9, false),
        '《' => (10, true),
        '》' => (10, false),
        '【' => (11, true),
        '】' => (11, false),
        '〖' => (12, true),
        '〗' => (12, false),
        '〘' => (13, true),
        '〙' => (13, false),
        '〚' => (14, true),
        '〛' => (14, false),
        '｟' => (15, true),
        '｠' => (15, false),
        '“' => (16, true),
        '”' => (16, false),
        '‘' => (17, true),
        '’' => (17, false),
        '«' => (18, true),
        '»' => (18, false),
        '‹' => (19, true),
        '›' => (19, false),
        '｢' => (20, true),
        '｣' => (20, false),
        '〝' => (21, true),
        '〟' | '〞' => (21, false),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 既定の設定で分割した文の文字列
    fn texts(text: &str) -> Vec<&str> {
        texts_with(text, &SplitOptions::default())
    }

    fn texts_with<'t>(text: &'t str, options: &SplitOptions<'_>) -> Vec<&'t str> {
        split(text, options)
            .into_iter()
            .map(|s| &text[s.range])
            .collect()
    }

    /// chunk_ends の区間の文字列
    fn chunks(text: &str) -> Vec<&str> {
        let mut start = 0;
        Splitter::default()
            .chunk_ends(text)
            .into_iter()
            .map(|end| {
                let chunk = &text[start..end];
                start = end;
                chunk
            })
            .collect()
    }

    // --- 要望書 H1 の入出力例 ---

    #[test]
    fn test_examples_from_requirements() {
        let cases: [(&str, &[&str]); 9] = [
            (
                "これが最初の文。これは二番目の文。",
                &["これが最初の文。", "これは二番目の文。"],
            ),
            ("え、本当…！？嘘だろ…", &["え、本当…！？", "嘘だろ…"]),
            (
                "「うまく行くかな？」と思った。次の文。",
                &["「うまく行くかな？」と思った。", "次の文。"],
            ),
            (
                "1) 手順を読む。「閉じ忘れ。次の文。",
                &["1) 手順を読む。", "「閉じ忘れ。", "次の文。"],
            ),
            ("終わりだ。」次だ。", &["終わりだ。」", "次だ。"]),
            (
                "https://example.com/?q=1 を開く。本当?すごい。",
                &["https://example.com/?q=1 を開く。", "本当?", "すごい。"],
            ),
            ("Really? Yes!", &["Really?", "Yes!"]),
            ("第一文．第二文．", &["第一文．", "第二文．"]),
            ("Yahoo!ニュースを見た。", &["Yahoo!ニュースを見た。"]),
        ];
        for (input, expected) in cases {
            assert_eq!(texts(input), expected, "{input}");
        }
    }

    // --- 要望書 H2 の受け入れ基準 ---

    #[test]
    fn test_acceptance_criteria_of_exceptions() {
        assert_eq!(texts("Hey!Say!JUMPのライブ。"), ["Hey!Say!JUMPのライブ。"]);
        assert_eq!(texts("Yahoo!ニュースを見た。"), ["Yahoo!ニュースを見た。"]);
        assert_eq!(
            texts("モーニング娘。のライブ。"),
            ["モーニング娘。のライブ。"]
        );
        assert_eq!(
            texts("好きなのはモーニング娘。次の話題。"),
            ["好きなのはモーニング娘。", "次の話題。"]
        );
    }

    // --- 規則 1: 文末記号の連続 ---

    #[test]
    fn test_splits_on_enders() {
        assert_eq!(
            texts("これが最初の文。これは二番目の文。これが最後の文。"),
            ["これが最初の文。", "これは二番目の文。", "これが最後の文。"]
        );
    }

    #[test]
    fn test_keeps_runs_of_enders_together() {
        assert_eq!(texts("え、本当…！？嘘だろ…"), ["え、本当…！？", "嘘だろ…"]);
        assert_eq!(texts("まさか。。。そうか‼"), ["まさか。。。", "そうか‼"]);
        assert_eq!(texts("本当?!すごい⁉"), ["本当?!", "すごい⁉"]);
        assert_eq!(texts("えっ！?うそ"), ["えっ！?", "うそ"]);
    }

    #[test]
    fn test_full_width_and_half_width_periods_are_enders() {
        assert_eq!(texts("第一文．第二文．"), ["第一文．", "第二文．"]);
        assert_eq!(texts("ｱｲｳ｡ｴｵ｡"), ["ｱｲｳ｡", "ｴｵ｡"]);
        assert_eq!(texts("何⁇そう⁈"), ["何⁇", "そう⁈"]);
    }

    // --- 規則 2・3: 括弧 ---

    #[test]
    fn test_does_not_split_inside_brackets() {
        assert_eq!(
            texts("「うまく行くかな？」と思った。次の文。"),
            ["「うまく行くかな？」と思った。", "次の文。"]
        );
        assert_eq!(
            texts("注意（詳細は後述。）を読む。"),
            ["注意（詳細は後述。）を読む。"]
        );
        assert_eq!(
            texts("彼は『本当？「嘘だ！」』と言った。次。"),
            ["彼は『本当？「嘘だ！」』と言った。", "次。"]
        );
    }

    #[test]
    fn test_all_bracket_pairs_protect_their_inside() {
        let pairs = [
            ("「", "」"),
            ("『", "』"),
            ("（", "）"),
            ("(", ")"),
            ("〔", "〕"),
            ("［", "］"),
            ("[", "]"),
            ("｛", "｝"),
            ("{", "}"),
            ("〈", "〉"),
            ("《", "》"),
            ("【", "】"),
            ("〖", "〗"),
            ("〘", "〙"),
            ("〚", "〛"),
            ("｟", "｠"),
            ("“", "”"),
            ("‘", "’"),
            ("«", "»"),
            ("‹", "›"),
            ("｢", "｣"),
            ("〝", "〟"),
            ("〝", "〞"),
        ];
        for (open, close) in pairs {
            let text = format!("前{open}中。中！{close}後。次。");
            assert_eq!(
                texts(&text),
                [format!("前{open}中。中！{close}後。"), "次。".to_string()],
                "{open}{close}"
            );
        }
    }

    #[test]
    fn test_ascii_quotes_are_not_brackets() {
        assert_eq!(
            texts("He said \"Wow。\" and left。"),
            ["He said \"Wow。", "\" and left。"]
        );
    }

    #[test]
    fn test_unmatched_open_bracket_does_not_swallow_the_rest() {
        assert_eq!(
            texts("1) 手順を読む。「閉じ忘れ。次の文。"),
            ["1) 手順を読む。", "「閉じ忘れ。", "次の文。"]
        );
    }

    #[test]
    fn test_broken_nesting_is_matched_with_the_same_kind() {
        // 「 と 」 が対応し、間の（ は対応なしになる
        assert_eq!(
            texts("「前（中。」後。次の文。"),
            ["「前（中。」後。", "次の文。"]
        );
        // 」 に対応する 「 がないので （ と ） が対応する
        assert_eq!(texts("（前」中。）後。次。"), ["（前」中。）後。", "次。"]);
    }

    #[test]
    fn test_marks_embedded_enders() {
        let sentences = split(
            "彼は「行こう。」と言った。そうだ。",
            &SplitOptions::default(),
        );
        assert!(sentences[0].embedded_enders);
        assert!(!sentences[1].embedded_enders);

        // 括弧の内側にあっても、規則 4 や例外表で文末にならない記号は数えない
        for text in [
            "(https://example.com/?q=1)を開く。",
            "「Yahoo!ニュース」を見た。",
            "（注意）を読む。",
        ] {
            let sentences = split(text, &SplitOptions::default());
            assert_eq!(sentences.len(), 1, "{text}");
            assert!(!sentences[0].embedded_enders, "{text}");
        }
    }

    // --- 規則 4: ASCII の ! ? ---

    #[test]
    fn test_ascii_question_mark_in_url_is_not_an_ender() {
        assert_eq!(
            texts("https://example.com/?q=1 を開く。本当?すごい。"),
            ["https://example.com/?q=1 を開く。", "本当?", "すごい。"]
        );
    }

    #[test]
    fn test_ascii_enders_followed_by_alphanumerics_or_symbols() {
        assert_eq!(
            texts("color: red !important; を書く。"),
            ["color: red !important; を書く。"]
        );
        // 連続は最後の字の直後で決める
        assert_eq!(texts("what?!x と書く。"), ["what?!x と書く。"]);
        assert_eq!(texts("本当?!次"), ["本当?!", "次"]);
        // 閉じ括弧・空白・行末の前では文末
        assert_eq!(texts("(本当?) 次。"), ["(本当?) 次。"]);
        assert_eq!(texts("本当?」次。"), ["本当?」", "次。"]);
        assert_eq!(texts("Really? Yes!"), ["Really?", "Yes!"]);
        assert_eq!(texts("Yes!"), ["Yes!"]);
    }

    // --- 規則 5: 文末の直後の対応しない閉じ括弧 ---

    #[test]
    fn test_stray_closing_bracket_after_ender_stays_with_sentence() {
        assert_eq!(texts("終わりだ。」次だ。"), ["終わりだ。」", "次だ。"]);
        assert_eq!(texts("終わりだ。』）次だ。"), ["終わりだ。』）", "次だ。"]);
    }

    // --- 規則 6: 改行 ---

    #[test]
    fn test_line_breaks_are_joined_by_default() {
        assert_eq!(
            texts("一行目の途中で折り返して\n続く文。二文目"),
            ["一行目の途中で折り返して\n続く文。", "二文目"]
        );
    }

    #[test]
    fn test_line_breaks_split_outside_brackets() {
        let options = SplitOptions {
            line_breaks: LineBreaks::Split,
            ..SplitOptions::default()
        };
        assert_eq!(
            texts_with("一行目の途中で折り返して\n続く文。二文目", &options),
            ["一行目の途中で折り返して", "続く文。", "二文目"]
        );
        // 括弧の内側の改行では区切らない
        assert_eq!(
            texts_with("「一行目\n二行目」と書いた\n次の行", &options),
            ["「一行目\n二行目」と書いた", "次の行"]
        );
        // CRLF・CR も 1 つの改行、空行は文にしない
        assert_eq!(
            texts_with("一\r\n\r\n二\r三\u{2028}四", &options),
            ["一", "二", "三", "四"]
        );
    }

    #[test]
    fn test_sentence_end_kinds() {
        let options = SplitOptions {
            line_breaks: LineBreaks::Split,
            ..SplitOptions::default()
        };
        let ends: Vec<SentenceEnd> = split("文。改行\n最後", &options)
            .into_iter()
            .map(|s| s.end)
            .collect();
        assert_eq!(
            ends,
            [
                SentenceEnd::Ender,
                SentenceEnd::LineBreak,
                SentenceEnd::EndOfText
            ]
        );
        let ends: Vec<SentenceEnd> = split("文。", &options).into_iter().map(|s| s.end).collect();
        assert_eq!(ends, [SentenceEnd::Ender]);
    }

    // --- 規則 7: 空白 ---

    #[test]
    fn test_ranges_exclude_surrounding_whitespace() {
        let text = "  最初の文。 \u{3000}次の文！\n\t最後  ";
        let sentences = split(text, &SplitOptions::default());
        let ranges: Vec<&str> = sentences.iter().map(|s| &text[s.range.clone()]).collect();
        assert_eq!(ranges, ["最初の文。", "次の文！", "最後"]);
        assert_eq!(sentences[0].range, 2..2 + "最初の文。".len());
    }

    #[test]
    fn test_empty_and_whitespace_only_text_yields_nothing() {
        assert!(texts("").is_empty());
        assert!(texts("   ").is_empty());
        assert!(texts(" \n\u{3000} ").is_empty());
    }

    // --- 規則 8: 例外表 ---

    #[test]
    fn test_builtin_exception_words_keep_sentences_whole() {
        // 語の内側の文末記号
        assert_eq!(
            texts("Yahoo!ニュースとHey!Say!JUMPを見た。"),
            ["Yahoo!ニュースとHey!Say!JUMPを見た。"]
        );
        // 語の末尾の文末記号 + 助詞
        assert_eq!(
            texts("ご注文はうさぎですか?を観た。"),
            ["ご注文はうさぎですか?を観た。"]
        );
        assert_eq!(texts("けいおん!の話。"), ["けいおん!の話。"]);
        assert_eq!(
            texts("やはり俺の青春ラブコメはまちがっている。は面白い。"),
            ["やはり俺の青春ラブコメはまちがっている。は面白い。"]
        );
    }

    #[test]
    fn test_ender_at_end_of_exception_word_splits_unless_particle_follows() {
        let options = SplitOptions {
            extra_exceptions: &["テスト語！"],
            use_builtin_exceptions: false,
            ..SplitOptions::default()
        };
        for particle in ["の", "は", "が", "を", "に", "と", "で", "も", "や", "へ"] {
            let text = format!("テスト語！{particle}続き。");
            assert_eq!(texts_with(&text, &options), [text.as_str()], "{particle}");
        }
        assert_eq!(
            texts_with("テスト語！次の文。", &options),
            ["テスト語！", "次の文。"]
        );
        assert_eq!(texts_with("テスト語！", &options), ["テスト語！"]);
        // 直後に文末記号が続けば、連続ごと文末になる
        assert_eq!(
            texts_with("テスト語！！の続き。", &options),
            ["テスト語！！", "の続き。"]
        );
    }

    #[test]
    fn test_longest_exception_wins() {
        let options = SplitOptions {
            extra_exceptions: &["Yahoo!", "Yahoo!ニュース"],
            use_builtin_exceptions: false,
            ..SplitOptions::default()
        };
        assert_eq!(
            texts_with("Yahoo!ニュースを見た。", &options),
            ["Yahoo!ニュースを見た。"]
        );
        assert_eq!(
            texts_with("Yahoo!の株価。Yahoo!次。", &options),
            ["Yahoo!の株価。", "Yahoo!", "次。"]
        );
    }

    #[test]
    fn test_builtin_exceptions_can_be_disabled() {
        let options = SplitOptions {
            use_builtin_exceptions: false,
            ..SplitOptions::default()
        };
        assert_eq!(
            texts_with("Yahoo!ニュースを見た。", &options),
            ["Yahoo!", "ニュースを見た。"]
        );
        assert_eq!(
            texts_with("モーニング娘。のライブ。", &options),
            ["モーニング娘。", "のライブ。"]
        );
        // 規則 4 は例外表によらず効く
        assert_eq!(
            texts_with("Hey!Say!JUMPのライブ。", &options),
            ["Hey!Say!JUMPのライブ。"]
        );
    }

    #[test]
    fn test_extra_exceptions_are_added_to_builtin_ones() {
        let options = SplitOptions {
            extra_exceptions: &["新語。テスト", "記号なし"],
            ..SplitOptions::default()
        };
        assert_eq!(
            texts_with("新語。テストとYahoo!ニュース。次。", &options),
            ["新語。テストとYahoo!ニュース。", "次。"]
        );
        // 文末記号を含まない語は数えない
        let splitter = Splitter::new(&options);
        assert!(format!("{splitter:?}").contains("extra_exceptions: 1"));
    }

    #[test]
    fn test_overlapping_exceptions_protect_their_union() {
        // 部分的に重なる 2 語の、どちらかの内側にある文末記号では分割しない
        let options = SplitOptions {
            extra_exceptions: &["AB。C", "C。D"],
            use_builtin_exceptions: false,
            ..SplitOptions::default()
        };
        assert_eq!(texts_with("AB。C。D。E。", &options), ["AB。C。D。", "E。"]);
    }

    #[test]
    fn test_exception_inside_brackets_and_across_line_breaks() {
        assert_eq!(
            texts("「モーニング娘。」が好きだ。次。"),
            ["「モーニング娘。」が好きだ。", "次。"]
        );
        let options = SplitOptions {
            line_breaks: LineBreaks::Split,
            ..SplitOptions::default()
        };
        assert_eq!(
            texts_with("Yahoo!ニュース\nモーニング娘。のライブ", &options),
            ["Yahoo!ニュース", "モーニング娘。のライブ"]
        );
    }

    // --- chunk_ends（形態素解析の前分割） ---

    #[test]
    fn test_chunk_ends_cover_the_input_without_gaps() {
        let text = " 最初の文。「括弧？」の中\n次の行！？ Yahoo!ニュース。末尾";
        let ends = Splitter::default().chunk_ends(text);
        assert_eq!(ends.last(), Some(&text.len()));
        assert!(ends.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(chunks(text).concat(), text);
        assert_eq!(
            chunks(text),
            [
                " 最初の文。",
                "「括弧？",
                "」の中\n",
                "次の行！？",
                " Yahoo!ニュース。",
                "末尾"
            ]
        );
    }

    #[test]
    fn test_chunk_ends_keep_exception_words_whole() {
        assert_eq!(chunks("Hey!Say!JUMPのライブ。"), ["Hey!Say!JUMPのライブ。"]);
        assert_eq!(
            chunks("ご注文はうさぎですか?を観た。"),
            ["ご注文はうさぎですか?を観た。"]
        );
        assert_eq!(
            chunks("好きなのはモーニング娘。次の話題。"),
            ["好きなのはモーニング娘。", "次の話題。"]
        );
    }

    #[test]
    fn test_chunk_ends_split_on_every_line_break() {
        assert_eq!(
            chunks("「一行目\n二行目」\r\n\n三行目"),
            ["「一行目\n", "二行目」\r\n", "\n", "三行目"]
        );
        assert_eq!(chunks("文。\n"), ["文。", "\n"]);
        assert!(Splitter::default().chunk_ends("").is_empty());
    }

    // --- 要望書 H12〜H16 の受け入れ条件 ---

    #[test]
    fn test_short_exception_words_do_not_join_ordinary_sentences() {
        // H12: 普通の文末と同じ形の短い語・語の途中からの一致・文頭に立つ語
        let cases: [(&str, &[&str]); 12] = [
            ("高すぎ。でも買った。", &["高すぎ。", "でも買った。"]),
            (
                "来てくださる。もし都合が悪ければ連絡する。",
                &["来てくださる。", "もし都合が悪ければ連絡する。"],
            ),
            ("すごいな！でも疲れた。", &["すごいな！", "でも疲れた。"]),
            (
                "今日は寒いね。では始めよう。",
                &["今日は寒いね。", "では始めよう。"],
            ),
            (
                "君が好きだ。とはいえ言えない。",
                &["君が好きだ。", "とはいえ言えない。"],
            ),
            (
                "意見を述べる。では次に進む。",
                &["意見を述べる。", "では次に進む。"],
            ),
            (
                "もう一度食べる。もちろん残さない。",
                &["もう一度食べる。", "もちろん残さない。"],
            ),
            (
                "結果を調べる。とにかく急ぐ。",
                &["結果を調べる。", "とにかく急ぐ。"],
            ),
            ("行くわ！はい、どうぞ。", &["行くわ！", "はい、どうぞ。"]),
            (
                "ずっと。やはり変わらない。",
                &["ずっと。", "やはり変わらない。"],
            ),
            (
                "好きなのはモーニング娘。もう一度言う。",
                &["好きなのはモーニング娘。", "もう一度言う。"],
            ),
            ("高すぎ。しかし買った。", &["高すぎ。", "しかし買った。"]),
        ];
        for (input, expected) in cases {
            assert_eq!(texts(input), expected, "{input}");
        }
    }

    #[test]
    fn test_exception_words_still_keep_sentences_whole() {
        // H12 の受け入れ条件（1 文のまま）
        for text in [
            "モーニング娘。のライブに行った。",
            "Yahoo!ニュースを見た。",
            "Yahoo!で検索した。",
            "Hey! Say! JUMPのライブ。",
            "「モーニング娘。」が好き。",
        ] {
            assert_eq!(texts(text), [text], "{text}");
        }
    }

    #[test]
    fn test_decimal_points_between_digits_are_not_enders() {
        // H14: 数字に挟まれた `．` `｡` は文末にしない
        assert_eq!(
            texts("円周率は３．１４です。次の文。"),
            ["円周率は３．１４です。", "次の文。"]
        );
        assert_eq!(
            texts("第３．２節を読む。次の文。"),
            ["第３．２節を読む。", "次の文。"]
        );
        assert_eq!(
            texts("約7．8mmと3｡5kg。次。"),
            ["約7．8mmと3｡5kg。", "次。"]
        );
        // 片側だけが数字なら文末
        assert_eq!(texts("…を述べる．３章では…"), ["…を述べる．", "３章では…"]);
        assert_eq!(texts("第一文．第二文．"), ["第一文．", "第二文．"]);
        // 形態素解析の前分割でも同じ
        assert_eq!(chunks("約３．１４です。"), ["約３．１４です。"]);
    }

    #[test]
    fn test_exception_matching_folds_width() {
        // H15: 全角の英数字・記号を半角に畳んで照合する（大文字・小文字は畳まない）
        assert_eq!(
            texts("Yahoo！ニュースを見た。次の文。"),
            ["Yahoo！ニュースを見た。", "次の文。"]
        );
        assert_eq!(
            texts("Ｙａｈｏｏ！ニュースを見た。"),
            ["Ｙａｈｏｏ！ニュースを見た。"]
        );
        assert_eq!(
            texts("Ｈｅｙ！Ｓａｙ！ＪＵＭＰのライブ。"),
            ["Ｈｅｙ！Ｓａｙ！ＪＵＭＰのライブ。"]
        );
        // 利用者が加える語も畳んで照合する
        let options = SplitOptions {
            extra_exceptions: &["ＡＢＣ！ＤＥＦ"],
            use_builtin_exceptions: false,
            ..SplitOptions::default()
        };
        assert_eq!(texts_with("ABC!DEFを見た。", &options), ["ABC!DEFを見た。"]);
    }

    #[test]
    fn test_more_words_continue_after_exception_words() {
        // H16: 語の末尾の文末記号の後ろに続く語
        for text in [
            "モーニング娘。からの発表だ。",
            "モーニング娘。より先に出た。",
            "モーニング娘。って知ってる？",
            "モーニング娘。など多くのグループ。",
            "モーニング娘。だけが残った。",
            "Yahoo!より速い。",
        ] {
            assert_eq!(texts(text), [text], "{text}");
        }
    }

    // --- 要望書 H17: 判定関数 ---

    #[test]
    fn test_public_char_predicates_agree_with_the_splitter() {
        for code in 0..=0x10FFFF {
            let Some(c) = char::from_u32(code) else {
                continue;
            };
            // 規則 1 の字の集合
            assert_eq!(
                is_sentence_ender(c),
                matches!(classify(c), CharKind::Ender | CharKind::AsciiEnder),
                "{c:?}"
            );
            // 規則 2 の括弧
            match bracket(c) {
                Some((kind, true)) => {
                    let close = closing_bracket(c).expect("開き括弧には閉じ括弧がある");
                    assert_eq!(bracket(close), Some((kind, false)), "{c:?}");
                    assert!(!is_closing_bracket(c));
                }
                Some((_, false)) => {
                    assert!(is_closing_bracket(c), "{c:?}");
                    assert_eq!(closing_bracket(c), None);
                }
                None => {
                    assert!(
                        !is_closing_bracket(c) && closing_bracket(c).is_none(),
                        "{c:?}"
                    );
                }
            }
        }
        assert_eq!(closing_bracket('〝'), Some('〟'));
        assert!(is_closing_bracket('〞'));
        assert_eq!(closing_bracket('"'), None);
        assert!(ascii_run_is_ender(None));
        assert!(ascii_run_is_ender(Some('次')));
        assert!(!ascii_run_is_ender(Some('i')));
    }

    // --- 要望書 H19: 改行とみなす位置を別に渡す分割 ---

    #[test]
    fn test_split_with_breaks() {
        let splitter = Splitter::default();
        let with = |text: &str, breaks: &[usize]| -> Vec<String> {
            splitter
                .split_with_breaks(text, breaks)
                .into_iter()
                .map(|s| text[s.range].to_string())
                .collect()
        };
        let text = "一行目の途中で折り返して続く文。二文目";
        let at = "一行目の途中で折り返して".len();
        assert_eq!(
            with(text, &[at]),
            ["一行目の途中で折り返して", "続く文。", "二文目"]
        );
        // 括弧の内側では区切らない。開き括弧の直前は括弧の外側
        let text = "「一行目二行目」と書いた次の行";
        assert_eq!(
            with(text, &["「一行目".len(), "「一行目二行目」と書いた".len()]),
            ["「一行目二行目」と書いた", "次の行"]
        );
        assert_eq!(with("前「中」後", &["前".len()]), ["前", "「中」後"]);
        // 順不同・重複・両端
        let text = "一二三";
        assert_eq!(with(text, &[6, 3, 3, 0, text.len()]), ["一", "二", "三"]);
        let ends: Vec<SentenceEnd> = splitter
            .split_with_breaks("一二", &[3])
            .into_iter()
            .map(|s| s.end)
            .collect();
        assert_eq!(ends, [SentenceEnd::LineBreak, SentenceEnd::EndOfText]);
    }

    #[test]
    fn test_split_with_breaks_matches_line_break_characters() {
        // 改行の字を取り除いて位置を渡した分割は、改行の字を残して LineBreaks::Split で分割したのと
        // 同じ文になる（範囲は取り除いた分だけずれる）
        let options = SplitOptions {
            line_breaks: LineBreaks::Split,
            ..SplitOptions::default()
        };
        let splitter = Splitter::default();
        for text in [
            "一行目の途中で\n続く文。二文目\n「括弧の\n内側」の後\n末尾",
            "\n先頭の改行\n\n空行。Yahoo!\nニュース",
            "（開き\n括弧）と\n（閉じ忘れ\n次",
        ] {
            let expected: Vec<String> = texts_with(text, &options)
                .into_iter()
                .map(|s| s.replace('\n', ""))
                .collect();
            let joined: String = text.split('\n').collect();
            let mut breaks = Vec::new();
            let mut pos = 0;
            for line in text.split('\n') {
                pos += line.len();
                breaks.push(pos);
            }
            breaks.pop();
            let got: Vec<&str> = splitter
                .split_with_breaks(&joined, &breaks)
                .into_iter()
                .map(|s| &joined[s.range])
                .collect();
            assert_eq!(got, expected, "{text:?}");
        }
    }

    #[test]
    #[should_panic(expected = "文字の境界でない")]
    fn test_split_with_breaks_rejects_positions_inside_characters() {
        Splitter::default().split_with_breaks("一二", &[1]);
    }

    // --- 要望書 H20: 例外表の版の識別子 ---

    #[test]
    fn test_builtin_exceptions_version_is_exposed() {
        let (count, hash) = BUILTIN_EXCEPTIONS_VERSION.split_once('-').unwrap();
        assert_eq!(
            count.parse::<usize>().unwrap(),
            builtin_exceptions().count()
        );
        assert_eq!(hash.len(), 16);
    }

    // --- 計算量 ---

    #[test]
    fn test_many_unmatched_brackets_are_handled_in_linear_time() {
        // 開き括弧が大量に残ったまま、対応しない閉じ括弧が大量に続く
        let text = format!("{}{}。次の文。", "(".repeat(50_000), "]".repeat(50_000));
        let sentences = split(&text, &SplitOptions::default());
        assert_eq!(sentences.len(), 2);
        assert!(!sentences[0].embedded_enders);

        // 深い入れ子
        let text = format!("{}。{}次の文。", "(".repeat(50_000), ")".repeat(50_000));
        assert_eq!(split(&text, &SplitOptions::default()).len(), 1);
    }

    #[test]
    fn test_long_runs_are_handled_in_linear_time() {
        let text = format!("{}x{}", "!".repeat(100_000), "?".repeat(100_000));
        assert_eq!(texts(&text), [text.as_str()]);
        let text = "。".repeat(100_000);
        assert_eq!(texts(&text).len(), 1);
        let text = "モーニング娘。の".repeat(20_000);
        assert_eq!(texts(&text).len(), 1);
    }

    // --- API ---

    #[test]
    fn test_split_options_default() {
        let options = SplitOptions::default();
        assert_eq!(options.line_breaks, LineBreaks::Join);
        assert!(options.extra_exceptions.is_empty());
        assert!(options.use_builtin_exceptions);
    }

    #[test]
    fn test_splitter_matches_split_function() {
        let text = "「うまく行くかな？」と思った。Yahoo!ニュースを見た。本当?すごい。";
        let splitter = Splitter::default();
        assert_eq!(splitter.split(text), split(text, &SplitOptions::default()));
        // 繰り返し呼んでも同じ結果
        assert_eq!(splitter.split(text), splitter.split(text));
    }

    #[test]
    fn test_splitter_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Splitter>();
    }
}
