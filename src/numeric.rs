//! 最良パスの数詞と助数詞の読み。ASCII と位を省いた漢数字の数量は数詞列を合成する。
//!
//! 音便は助数詞ごとの対応表で決め、読み以外の情報を保持する。人・日の和語読みは数詞列全体を見る。
//! 一日は日付と期間を区別できないので変更せず、二十日のハツカも分配しない。

use crate::lattice::Token;
use std::ops::Range;
use std::sync::Arc;

mod ascii;
mod kanji;
mod month;
pub(crate) use ascii::{AsciiPlan, apply as apply_ascii_quantities};
pub(crate) use kanji::apply as apply_kanji_quantities;
pub(crate) use month::apply as apply_spaced_months;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Hundred,
    Thousand,
    TenThousand,
}

#[derive(Clone, Copy)]
enum Counter {
    H {
        plain: &'static str,
        voiced: &'static str,
        half: &'static str,
    },
    K,
    ExtraK,
    Soft,
    Step,
    NativeTs,
    Mouth,
    Minute,
    Hundred,
    Thousand,
    Person,
    Day,
    Hour,
    Trillion,
}

fn pos_has(pos: &str, prefix: &str) -> bool {
    pos.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(','))
}

fn digit(c: char) -> Option<u64> {
    match c {
        '0'..='9' => Some(u64::from(c as u32 - '0' as u32)),
        '０'..='９' => Some(u64::from(c as u32 - '０' as u32)),
        '〇' | '零' => Some(0),
        '一' => Some(1),
        '二' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    }
}

fn place(c: char) -> Option<u64> {
    match c {
        '十' => Some(10),
        '百' => Some(100),
        '千' => Some(1_000),
        '万' => Some(10_000),
        '億' => Some(100_000_000),
        '兆' => Some(1_000_000_000_000),
        _ => None,
    }
}

fn is_numeral(token: &Token) -> bool {
    pos_has(&token.pos, "名詞,数")
        && !token.surface.is_empty()
        && (ascii::integer_value(&token.surface).is_some()
            || token
                .surface
                .chars()
                .all(|c| digit(c).is_some() || place(c).is_some()))
}

/// 位を含む数と、位を省いた数字列を読む。桁あふれ・位の逆転は曖昧な数として拒む。
fn numeral_value<'a>(surfaces: impl Iterator<Item = &'a str>) -> Option<u64> {
    let (mut total, mut group, mut digits) = (0u64, 0u64, 0u64);
    let (mut have_digits, mut seen) = (false, false);
    let (mut last_small, mut last_large) = (10_000, u64::MAX);
    for c in surfaces.flat_map(str::chars) {
        seen = true;
        if let Some(n) = digit(c) {
            digits = digits.checked_mul(10)?.checked_add(n)?;
            have_digits = true;
            continue;
        }
        let unit = place(c)?;
        if unit < 10_000 {
            if unit >= last_small {
                return None;
            }
            let coefficient = if have_digits { digits } else { 1 };
            group = group.checked_add(coefficient.checked_mul(unit)?)?;
            last_small = unit;
        } else {
            if unit >= last_large {
                return None;
            }
            group = group.checked_add(digits)?;
            let coefficient = if group == 0 && !have_digits { 1 } else { group };
            total = total.checked_add(coefficient.checked_mul(unit)?)?;
            group = 0;
            last_small = 10_000;
            last_large = unit;
        }
        digits = 0;
        have_digits = false;
    }
    seen.then_some(())?;
    total.checked_add(group)?.checked_add(digits)
}

fn ending(surface: &str) -> Option<Ending> {
    let value = if surface.contains(',') {
        ascii::integer_value(surface)?
    } else {
        numeral_value(std::iter::once(surface))?
    };
    Some(match value % 10 {
        1 => Ending::One,
        2 => Ending::Two,
        3 => Ending::Three,
        4 => Ending::Four,
        5 => Ending::Five,
        6 => Ending::Six,
        7 => Ending::Seven,
        8 => Ending::Eight,
        9 => Ending::Nine,
        _ if value == 0 => return None,
        _ if value % 100 != 0 => Ending::Ten,
        _ if value % 1_000 != 0 => Ending::Hundred,
        _ if value % 10_000 != 0 => Ending::Thousand,
        _ if value % 100_000_000 != 0 => Ending::TenThousand,
        _ => return None,
    })
}

fn has_reading(token: &Token, allowed: &[&str]) -> bool {
    allowed.contains(&token.reading.as_ref()) && allowed.contains(&token.pronunciation.as_ref())
}

fn counter(token: &Token) -> Option<Counter> {
    if pos_has(&token.pos, "名詞,数") {
        return match token.surface.as_ref() {
            "百" if has_reading(token, &["ヒャク", "ビャク", "ピャク"]) => {
                Some(Counter::Hundred)
            }
            "千" if has_reading(token, &["セン", "ゼン"]) => Some(Counter::Thousand),
            "兆" if has_reading(token, &["チョウ", "チョー"]) => Some(Counter::Trillion),
            _ => None,
        };
    }
    let suffix = pos_has(&token.pos, "名詞,接尾,助数詞");
    // IPAdic の単独の「つ」は文語の助動詞にもなる。数詞に直結した標準形だけを
    // 後段で和語の助数詞として読む。活用語の後の「つ」には適用しない。
    if token.surface.as_ref() == "つ"
        && has_reading(token, &["ツ"])
        && (suffix
            || (pos_has(&token.pos, "助動詞")
                && matches!(token.conj_type.as_ref(), "文語・ツ" | "下二・タ行")
                && token.conj_form.as_ref() == "基本形"))
    {
        return Some(Counter::NativeTs);
    }
    // 章見出しでは普通名詞の「章」が選ばれることもある。助数詞と同じ読みを
    // 持つ形に限り、直結した数詞の後だけで促音化する。
    if token.surface.as_ref() == "章"
        && pos_has(&token.pos, "名詞,一般")
        && has_reading(token, &["ショウ", "ショー"])
    {
        return Some(Counter::Soft);
    }
    // IPAdic では「ぷん」が未知の普通名詞になる。仮名補完済みのこの形だけを許す。
    if !suffix
        && !(!token.is_known
            && token.surface.as_ref() == "ぷん"
            && pos_has(&token.pos, "名詞,一般")
            && has_reading(token, &["プン"]))
    {
        return None;
    }
    let h = |plain, voiced, half| {
        has_reading(token, &[plain, voiced, half]).then_some(Counter::H {
            plain,
            voiced,
            half,
        })
    };
    match token.surface.as_ref() {
        "本" => h("ホン", "ボン", "ポン"),
        "杯" => h("ハイ", "バイ", "パイ"),
        "匹" => h("ヒキ", "ビキ", "ピキ"),
        "回" if has_reading(token, &["カイ"]) => Some(Counter::K),
        "個" if has_reading(token, &["コ"]) => Some(Counter::K),
        "句" if has_reading(token, &["ク"]) => Some(Counter::ExtraK),
        "級" if has_reading(token, &["キュウ", "キュー"]) => Some(Counter::ExtraK),
        "件" if has_reading(token, &["ケン"]) => Some(Counter::ExtraK),
        "期" if has_reading(token, &["キ"]) => Some(Counter::ExtraK),
        "曲" if has_reading(token, &["キョク"]) => Some(Counter::ExtraK),
        "階" if has_reading(token, &["カイ"]) => Some(Counter::ExtraK),
        "か月" | "カ月" | "ヶ月" | "ヵ月" if has_reading(token, &["カゲツ"]) => {
            Some(Counter::ExtraK)
        }
        "点" if has_reading(token, &["テン"]) => Some(Counter::Soft),
        "通" if has_reading(token, &["ツウ", "ツー"]) => Some(Counter::Soft),
        "冊" | "刷" if has_reading(token, &["サツ"]) => Some(Counter::Soft),
        "週" if has_reading(token, &["シュウ", "シュー"]) => Some(Counter::Soft),
        "週間" if has_reading(token, &["シュウカン", "シューカン"]) => {
            Some(Counter::Soft)
        }
        "章" if has_reading(token, &["ショウ", "ショー"]) => Some(Counter::Soft),
        "節" if has_reading(token, &["セツ"]) => Some(Counter::Soft),
        "頭" if has_reading(token, &["トウ", "トー"]) => Some(Counter::Soft),
        "着" if has_reading(token, &["チャク"]) => Some(Counter::Soft),
        "足" if has_reading(token, &["ソク"]) => Some(Counter::Soft),
        "歳" | "才" if has_reading(token, &["サイ"]) => Some(Counter::Soft),
        "歩" if has_reading(token, &["ホ", "ポ"]) => Some(Counter::Step),
        "口" if has_reading(token, &["クチ", "グチ"]) => Some(Counter::Mouth),
        "分" | "ぷん" if has_reading(token, &["フン", "プン"]) => Some(Counter::Minute),
        "人" if has_reading(token, &["ニン", "リ"]) => Some(Counter::Person),
        "日" if has_reading(token, &["ニチ", "カ"]) => Some(Counter::Day),
        "時" if has_reading(token, &["ジ"]) => Some(Counter::Hour),
        "時間" if has_reading(token, &["ジカン"]) => Some(Counter::Hour),
        _ => None,
    }
}

fn set_readings(token: &mut Token, reading: &str, pronunciation: &str) {
    if token.reading.as_ref() != reading {
        token.reading = Arc::from(reading);
    }
    if token.pronunciation.as_ref() != pronunciation {
        token.pronunciation = if reading == pronunciation {
            Arc::clone(&token.reading)
        } else {
            Arc::from(pronunciation)
        };
    }
}

fn replace_tail(token: &mut Token, old: &[&str], reading: &str, pronunciation: &str) -> bool {
    let Some(r) = old.iter().find_map(|tail| token.reading.strip_suffix(tail)) else {
        return false;
    };
    let Some(p) = old
        .iter()
        .find_map(|tail| token.pronunciation.strip_suffix(tail))
    else {
        return false;
    };
    let r = format!("{r}{reading}");
    let p = format!("{p}{pronunciation}");
    set_readings(token, &r, &p);
    true
}

fn number_tails(e: Ending) -> &'static [&'static str] {
    match e {
        Ending::One => &["イチ", "イッ"],
        Ending::Two => &["ニ"],
        Ending::Three => &["サン"],
        Ending::Four => &["ヨン", "シ"],
        Ending::Five => &["ゴ"],
        Ending::Six => &["ロク", "ロッ"],
        Ending::Seven => &["ナナ", "シチ"],
        Ending::Eight => &["ハチ", "ハッ"],
        Ending::Nine => &["キュウ", "キュー", "ク"],
        Ending::Ten => &["ジュウ", "ジュー", "ジュッ", "ジッ"],
        Ending::Hundred => &["ヒャク", "ビャク", "ピャク", "ヒャッ", "ビャッ", "ピャッ"],
        Ending::Thousand => &["セン", "ゼン"],
        Ending::TenThousand => &["マン"],
    }
}

/// 漢数字の語全体を検査するための字ごとの標準形。位の直前・直後の音便も含める。
fn numeral_glyph_readings(
    previous: Option<char>,
    c: char,
    next: Option<char>,
) -> &'static [&'static str] {
    match c {
        '〇' | '零' => &["ゼロ", "レイ"],
        '一' if matches!(next, Some('千' | '兆')) => &["イッ"],
        '一' => &["イチ"],
        '二' => &["ニ"],
        '三' => &["サン"],
        '四' => &["ヨン", "シ"],
        '五' => &["ゴ"],
        '六' if next == Some('百') => &["ロッ"],
        '六' => &["ロク"],
        '七' => &["ナナ", "シチ"],
        '八' if matches!(next, Some('百' | '千' | '兆')) => &["ハッ"],
        '八' => &["ハチ"],
        '九' => &["キュウ", "キュー", "ク"],
        '十' if matches!(next, Some('千' | '兆')) => &["ジュッ", "ジッ"],
        '十' => &["ジュウ", "ジュー"],
        '百' if previous == Some('三') => &["ビャク"],
        '百' if matches!(previous, Some('六' | '八')) => &["ピャク"],
        '百' => &["ヒャク"],
        '千' if previous == Some('三') => &["ゼン"],
        '千' => &["セン"],
        '万' => &["マン"],
        '億' => &["オク"],
        '兆' => &["チョウ", "チョー"],
        _ => &[],
    }
}

fn standard_number_readings(token: &Token, tails: &[&str]) -> bool {
    let mut chars = token.surface.chars().peekable();
    let Some(_) = chars.next() else {
        return false;
    };
    if chars.peek().is_none() {
        return has_reading(token, tails);
    }
    if token.surface.is_ascii() {
        return ascii::standard_readings(&token.surface, &token.reading, &token.pronunciation);
    }
    if !token.is_known
        && kanji::standard_readings(&token.surface, &token.reading, &token.pronunciation)
    {
        return true;
    }
    standard_kanji_readings(&token.surface, &token.reading, &token.pronunciation)
}

fn standard_kanji_readings(surface: &str, mut reading: &str, mut pronunciation: &str) -> bool {
    let mut chars = surface.chars().peekable();
    let mut previous = None;
    while let Some(c) = chars.next() {
        let allowed = numeral_glyph_readings(previous, c, chars.peek().copied());
        let Some(r) = allowed
            .iter()
            .find_map(|prefix| reading.strip_prefix(prefix))
        else {
            return false;
        };
        let Some(p) = allowed
            .iter()
            .find_map(|prefix| pronunciation.strip_prefix(prefix))
        else {
            return false;
        };
        (reading, pronunciation) = (r, p);
        previous = Some(c);
    }
    reading.is_empty() && pronunciation.is_empty()
}

fn geminate(token: &mut Token, e: Ending) -> bool {
    if token.reading.ends_with('ッ') && token.pronunciation.ends_with('ッ') {
        return true;
    }
    let replacement = match e {
        Ending::One => "イッ",
        Ending::Six => "ロッ",
        Ending::Eight => "ハッ",
        Ending::Ten => "ジュッ",
        Ending::Hundred => {
            // 三百本・六百本では、先に補正したビャク・ピャクの頭音を保持する。
            return replace_tail(token, &["ク"], "ッ", "ッ");
        }
        _ => return false,
    };
    replace_tail(token, number_tails(e), replacement, replacement)
}

fn after_decimal(tokens: &[Token], start: usize) -> bool {
    start >= 2
        && matches!(tokens[start - 1].surface.as_ref(), "." | "．" | "点")
        && tokens[start - 2].end == tokens[start - 1].start
        && tokens[start - 1].end == tokens[start].start
        && is_numeral(&tokens[start - 2])
}

/// 和語の対象になる小さい数の標準形。全体と各トークンを照合し、別読みの接頭部も守る。
fn small_readings(value: u64) -> &'static [&'static str] {
    match value {
        0 => &["ゼロ", "レイ"],
        1 => &["イチ"],
        2 => &["ニ"],
        3 => &["サン"],
        4 => &["ヨン", "シ"],
        5 => &["ゴ"],
        6 => &["ロク"],
        7 => &["ナナ", "シチ"],
        8 => &["ハチ"],
        9 => &["キュウ", "キュー", "ク"],
        10 => &["ジュウ", "ジュー"],
        14 => &["ジュウヨン", "ジュウシ", "ジューヨン", "ジューシ"],
        20 => &["ニジュウ", "ニジュー"],
        24 => &["ニジュウヨン", "ニジュウシ", "ニジューヨン", "ニジューシ"],
        _ => &[],
    }
}

fn standard_native_run(tokens: &[Token], value: u64) -> bool {
    if !tokens.iter().all(|t| {
        numeral_value(std::iter::once(t.surface.as_ref()))
            .is_some_and(|n| has_reading(t, small_readings(n)))
    }) {
        return false;
    }
    let reading: String = tokens.iter().map(|t| t.reading.as_ref()).collect();
    let pronunciation: String = tokens.iter().map(|t| t.pronunciation.as_ref()).collect();
    let allowed = small_readings(value);
    allowed.contains(&reading.as_str()) && allowed.contains(&pronunciation.as_str())
}

fn native_day(tokens: &[Token], start: usize, end: usize) -> Option<(&'static str, &'static str)> {
    let run = &tokens[start..end];
    let value = numeral_value(run.iter().map(|t| t.surface.as_ref()))?;
    if !standard_native_run(run, value) {
        return None;
    }
    // 位のない「1 / 4」はまだ位取りの読みではない。十四・二十四は十を含む列に限定する。
    if run.len() != 1 && !run.iter().any(|t| t.surface.contains('十')) {
        return None;
    }
    if value <= 10 {
        let last = run.last()?;
        if !has_reading(last, number_tails(ending(&last.surface)?)) {
            return None;
        }
    }
    Some(match value {
        2 => ("フツ", "フツ"),
        3 => ("ミッ", "ミッ"),
        4 | 14 | 24 => ("ヨッ", "ヨッ"),
        5 => ("イツ", "イツ"),
        6 => ("ムイ", "ムイ"),
        7 => ("ナノ", "ナノ"),
        8 => ("ヨウ", "ヨー"),
        9 => ("ココノ", "ココノ"),
        10 if run
            .last()
            .is_some_and(|t| ending(&t.surface) == Some(Ending::Ten)) =>
        {
            ("トオ", "トー")
        }
        _ => return None,
    })
}

fn ten_minutes(tokens: &mut [Token], i: usize) {
    if tokens[i].surface.as_ref() == "十分"
        && pos_has(&tokens[i].pos, "名詞,形容動詞語幹")
        && has_reading(&tokens[i], &["ジュウブン", "ジューブン"])
        && tokens.get(i + 1).is_some_and(|next| {
            tokens[i].end == next.start
                && matches!(next.surface.as_ref(), "前" | "後")
                && (pos_has(&next.pos, "名詞,副詞可能") || pos_has(&next.pos, "名詞,接尾,副詞可能"))
        })
    {
        set_readings(&mut tokens[i], "ジュップン", "ジュップン");
    }
}

/// 仮名などの読み補完の後に呼ぶ。数詞列をまとめず、読みと発音だけを変更する。
pub(crate) fn apply_counter_readings(tokens: &mut [Token]) {
    apply_counter_readings_with_protection(tokens, &[]);
}

/// 連続する数詞列の走査状態。列の先頭と、列全体の字種・保護の有無を持つ
struct NumeralRun {
    /// 数詞列の先頭のトークンの位置
    start: usize,
    /// `protected` の走査位置。これより前の範囲は数詞列の手前で終わっている
    protection: usize,
    /// 列の数詞がすべて ASCII か
    ascii: bool,
    /// 列の数詞がすべて漢数字（位の字を含む）か
    kanji: bool,
    /// 列の数詞のどれかが保護された範囲に重なるか
    protected: bool,
}

impl NumeralRun {
    fn new() -> Self {
        Self {
            start: 0,
            protection: 0,
            ascii: true,
            kanji: true,
            protected: false,
        }
    }

    /// 位置 `start` から数詞列をやり直す（`protected` の走査位置は戻さない）
    fn restart(&mut self, start: usize) {
        self.start = start;
        (self.ascii, self.kanji, self.protected) = (true, true, false);
    }

    /// 数詞 `previous` を列に取り込み、その語が保護された範囲に重なるかを返す
    ///
    /// `protected` は位置の昇順に並んでいるので、走査位置は戻さずに進めるだけでよい。
    fn extend(&mut self, previous: &Token, protected: &[Range<usize>]) -> bool {
        self.ascii &= previous.surface.is_ascii();
        self.kanji &= previous.surface.chars().all(|c| {
            matches!(
                c,
                '〇' | '零' | '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九'
            ) || place(c).is_some()
        });
        while self.protection < protected.len() && protected[self.protection].end <= previous.start
        {
            self.protection += 1;
        }
        let previous_protected = protected
            .get(self.protection)
            .is_some_and(|range| range.start < previous.end && previous.start < range.end);
        self.protected |= previous_protected;
        previous_protected
    }
}

/// 音便を決めるために、助数詞の直前の数詞から取り出した情報
struct Numeral {
    /// 数詞の語形
    ending: Ending,
    /// その語形が取り得る読みの語尾
    tails: &'static [&'static str],
    /// 日の和語読み（読み, 発音）。和語読みにしないときは `None`
    day: Option<(&'static str, &'static str)>,
    /// 人の和語読み（一人・二人）にできるか
    person: bool,
    /// 「つ」と「口」の和語読み。数詞が一つの標準形のときだけ使う。
    native: Option<&'static str>,
}

fn native_counter_prefix(counter: Counter, value: u64) -> Option<&'static str> {
    match (counter, value) {
        (Counter::NativeTs | Counter::Mouth, 1) => Some("ヒト"),
        (Counter::NativeTs | Counter::Mouth, 2) => Some("フタ"),
        (Counter::NativeTs, 3) => Some("ミッ"),
        (Counter::NativeTs, 4) => Some("ヨッ"),
        (Counter::NativeTs, 5) => Some("イツ"),
        (Counter::NativeTs, 6) => Some("ムッ"),
        (Counter::NativeTs, 7) => Some("ナナ"),
        (Counter::NativeTs, 8) => Some("ヤッ"),
        (Counter::NativeTs, 9) => Some("ココノ"),
        _ => None,
    }
}

/// 助数詞 `tokens[i]` の読みを直せる数詞列かを判定し、音便に使う情報を返す
///
/// 小数部の数詞、読みが IPAdic の標準形でない数詞、ASCII の複数桁で保護された数詞は直さない。
fn numeral_before_counter(
    tokens: &[Token],
    run: &NumeralRun,
    i: usize,
    counter: Counter,
    previous_protected: bool,
) -> Option<Numeral> {
    // 兆の新しい促音化は、保護されていない純ASCIIまたは純漢数字の列だけに適用する。
    if matches!(counter, Counter::Trillion) && (run.protected || !(run.ascii || run.kanji)) {
        return None;
    }
    if matches!(
        counter,
        Counter::ExtraK | Counter::Soft | Counter::Step | Counter::NativeTs | Counter::Mouth
    ) && run.protected
    {
        return None;
    }
    if after_decimal(tokens, run.start) {
        return None;
    }
    let number = &tokens[i - 1];
    let ending = ending(&number.surface)?;
    let tails = number_tails(ending);
    // 既存の1桁数字の音便は保ち、新たな複数字の補正だけを保護する。
    if number.surface.is_ascii() && number.surface.len() > 1 && previous_protected {
        return None;
    }
    if !standard_number_readings(number, tails) {
        return None;
    }
    if !tails.iter().any(|t| number.reading.ends_with(t))
        || !tails.iter().any(|t| number.pronunciation.ends_with(t))
    {
        return None;
    }
    let day = if matches!(counter, Counter::Day) {
        native_day(tokens, run.start, i)
    } else {
        None
    };
    let person = matches!(counter, Counter::Person)
        && i - run.start == 1
        && has_reading(number, tails)
        && matches!(
            numeral_value(std::iter::once(number.surface.as_ref())),
            Some(1 | 2)
        );
    let native = if matches!(counter, Counter::NativeTs | Counter::Mouth)
        && i - run.start == 1
        && !run.start.checked_sub(1).is_some_and(|j| {
            tokens[j].end == tokens[run.start].start
                && tokens[j].surface.as_ref() == "第"
                && pos_has(&tokens[j].pos, "接頭詞,数接続")
        }) {
        numeral_value(std::iter::once(number.surface.as_ref()))
            .filter(|&value| standard_native_run(&tokens[run.start..i], value))
            .and_then(|value| native_counter_prefix(counter, value))
    } else {
        None
    };
    Some(Numeral {
        ending,
        tails,
        day,
        person,
        native,
    })
}

/// 数詞 `number` と助数詞 `suffix` の読みを、助数詞の種類と数詞の語形で決まる音便にそろえる
fn apply_counter_reading(
    number: &mut Token,
    suffix: &mut Token,
    counter: Counter,
    numeral: &Numeral,
) {
    let &Numeral {
        ending: e,
        tails,
        day,
        person,
        native,
    } = numeral;
    match counter {
        Counter::H {
            plain,
            voiced,
            half,
        } => match e {
            Ending::One | Ending::Six | Ending::Eight | Ending::Ten | Ending::Hundred => {
                if geminate(number, e) {
                    set_readings(suffix, half, half);
                }
            }
            Ending::Three | Ending::Thousand | Ending::TenThousand => {
                set_readings(suffix, voiced, voiced)
            }
            _ => set_readings(suffix, plain, plain),
        },
        Counter::K | Counter::ExtraK => {
            geminate(number, e);
        }
        Counter::Soft => {
            if matches!(e, Ending::One | Ending::Eight | Ending::Ten) {
                geminate(number, e);
            }
        }
        Counter::Step => match e {
            Ending::One | Ending::Six | Ending::Eight | Ending::Ten | Ending::Hundred => {
                if geminate(number, e) {
                    set_readings(suffix, "ポ", "ポ");
                }
            }
            Ending::Three | Ending::Thousand | Ending::TenThousand => {
                set_readings(suffix, "ポ", "ポ");
            }
            _ => set_readings(suffix, "ホ", "ホ"),
        },
        Counter::NativeTs | Counter::Mouth => {
            if let Some(prefix) = native {
                set_readings(number, prefix, prefix);
                let tail = if matches!(counter, Counter::NativeTs) {
                    "ツ"
                } else {
                    "クチ"
                };
                set_readings(suffix, tail, tail);
            }
        }
        Counter::Minute => match e {
            Ending::One | Ending::Six | Ending::Eight | Ending::Ten | Ending::Hundred => {
                if geminate(number, e) {
                    set_readings(suffix, "プン", "プン");
                }
            }
            Ending::Three | Ending::Four | Ending::Thousand | Ending::TenThousand => {
                set_readings(suffix, "プン", "プン")
            }
            _ if suffix.surface.as_ref() == "分" => set_readings(suffix, "フン", "フン"),
            _ => {}
        },
        Counter::Hundred => match e {
            Ending::Three => set_readings(suffix, "ビャク", "ビャク"),
            Ending::Six | Ending::Eight if geminate(number, e) => {
                set_readings(suffix, "ピャク", "ピャク");
            }
            _ => set_readings(suffix, "ヒャク", "ヒャク"),
        },
        Counter::Thousand => match e {
            Ending::Three => set_readings(suffix, "ゼン", "ゼン"),
            Ending::One | Ending::Eight | Ending::Ten => {
                if geminate(number, e) {
                    set_readings(suffix, "セン", "セン");
                }
            }
            _ => set_readings(suffix, "セン", "セン"),
        },
        Counter::Trillion => {
            if matches!(e, Ending::One | Ending::Eight | Ending::Ten) {
                geminate(number, e);
            }
        }
        Counter::Person => {
            if person {
                let r = if e == Ending::One { "ヒト" } else { "フタ" };
                if replace_tail(number, tails, r, r) {
                    set_readings(suffix, "リ", "リ");
                }
            } else if e == Ending::Four {
                replace_tail(number, tails, "ヨ", "ヨ");
                set_readings(suffix, "ニン", "ニン");
            } else {
                set_readings(suffix, "ニン", "ニン");
            }
        }
        Counter::Day => {
            if let Some((r, p)) = day
                && replace_tail(number, tails, r, p)
            {
                set_readings(suffix, "カ", "カ");
            } else {
                let ordinary = match e {
                    Ending::Seven => Some("シチ"),
                    Ending::Nine => Some("ク"),
                    _ => None,
                };
                if let Some(r) = ordinary {
                    replace_tail(number, tails, r, r);
                }
                set_readings(suffix, "ニチ", "ニチ");
            }
        }
        Counter::Hour => {
            let r = match e {
                Ending::Four => Some("ヨ"),
                Ending::Seven => Some("シチ"),
                Ending::Nine => Some("ク"),
                _ => None,
            };
            if let Some(r) = r {
                replace_tail(number, tails, r, r);
            }
        }
    }
}

pub(crate) fn apply_counter_readings_with_protection(
    tokens: &mut [Token],
    protected: &[Range<usize>],
) {
    let mut run = NumeralRun::new();
    for i in 0..tokens.len() {
        ten_minutes(tokens, i);
        if i == 0 || tokens[i - 1].end != tokens[i].start || !is_numeral(&tokens[i - 1]) {
            run.restart(i);
            continue;
        }
        let previous_protected = run.extend(&tokens[i - 1], protected);
        let Some(counter) = counter(&tokens[i]) else {
            continue;
        };
        let Some(numeral) = numeral_before_counter(tokens, &run, i, counter, previous_protected)
        else {
            continue;
        };
        let (before, after) = tokens.split_at_mut(i);
        apply_counter_reading(&mut before[i - 1], &mut after[0], counter, &numeral);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NUMBER: &str = "名詞,数,*,*";
    const COUNTER: &str = "名詞,接尾,助数詞,*";

    fn tokens(parts: &[(&str, &str, &str)]) -> Vec<Token> {
        let mut start = 37;
        parts
            .iter()
            .map(|&(surface, pos, reading)| {
                let end = start + surface.len();
                let token = Token {
                    surface: surface.into(),
                    start,
                    end,
                    pos: pos.into(),
                    conj_type: "fixture-type".into(),
                    conj_form: "fixture-form".into(),
                    base_form: format!("base:{surface}").into(),
                    reading: reading.into(),
                    pronunciation: reading.into(),
                    word_cost: 321,
                    is_known: true,
                };
                start = end;
                token
            })
            .collect()
    }

    fn non_reading_fields(tokens: &[Token]) -> Vec<String> {
        tokens
            .iter()
            .map(|t| {
                format!(
                    "{:?}",
                    (
                        &t.surface,
                        t.start,
                        t.end,
                        &t.pos,
                        &t.conj_type,
                        &t.conj_form,
                        &t.base_form,
                        t.word_cost,
                        t.is_known,
                    )
                )
            })
            .collect()
    }

    fn readings(tokens: &[Token]) -> (String, String) {
        (
            tokens.iter().map(|t| t.reading.as_ref()).collect(),
            tokens.iter().map(|t| t.pronunciation.as_ref()).collect(),
        )
    }

    #[test]
    fn numeral_parser_handles_places_and_rejects_invalid_or_overflowing_numbers() {
        for (surface, value) in [
            ("一", 1),
            ("十四", 14),
            ("二十四", 24),
            ("二〇二六", 2026),
            ("２０２６", 2026),
            ("二千二十六", 2026),
            ("一億二万三", 100020003),
            ("18446744073709551615", u64::MAX),
        ] {
            assert_eq!(
                numeral_value(std::iter::once(surface)),
                Some(value),
                "{surface}"
            );
        }
        assert_eq!(numeral_value(["二", "十", "四"].into_iter()), Some(24));
        for surface in [
            "",
            "十百",
            "百百",
            "万億",
            "2.5",
            "1,000",
            "18446744073709551616",
        ] {
            assert_eq!(numeral_value(std::iter::once(surface)), None, "{surface}");
        }
    }

    #[test]
    fn counter_table_preserves_every_non_reading_field() {
        for (n, nr, c, cr, expected) in [
            ("三", "サン", "本", "ホン", "サンボン"),
            ("一", "イチ", "本", "ホン", "イッポン"),
            ("六", "ロク", "本", "ホン", "ロッポン"),
            ("八", "ハチ", "本", "ホン", "ハッポン"),
            ("一", "イチ", "回", "カイ", "イッカイ"),
            ("六", "ロク", "分", "フン", "ロップン"),
            ("三", "サン", "分", "フン", "サンプン"),
            ("四", "ヨン", "分", "フン", "ヨンプン"),
            ("八", "ハチ", "個", "コ", "ハッコ"),
            ("三", "サン", "杯", "ハイ", "サンバイ"),
            ("一", "イチ", "杯", "ハイ", "イッパイ"),
            ("六", "ロク", "匹", "ヒキ", "ロッピキ"),
            ("三", "サン", "匹", "ヒキ", "サンビキ"),
            ("千", "セン", "本", "ホン", "センボン"),
            ("万", "マン", "杯", "ハイ", "マンバイ"),
            ("千", "セン", "分", "フン", "センプン"),
            ("万", "マン", "分", "フン", "マンプン"),
            ("1", "イチ", "分", "フン", "イップン"),
            ("１", "イチ", "分", "フン", "イップン"),
            ("四", "ヨン", "時", "ジ", "ヨジ"),
            ("七", "ナナ", "時間", "ジカン", "シチジカン"),
            ("九", "キュウ", "時", "ジ", "クジ"),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), (c, COUNTER, cr)]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (expected.into(), expected.into()), "{n}{c}");
            assert_eq!(non_reading_fields(&out), fields, "{n}{c}");
            apply_counter_readings(&mut out);
            assert_eq!(
                readings(&out),
                (expected.into(), expected.into()),
                "repeat {n}{c}"
            );
        }
    }

    #[test]
    fn reviewed_counter_families_keep_six_plain_and_preserve_every_other_field() {
        for (n, nr, c, cr, cp, expected_r, expected_p) in [
            ("1", "イチ", "つ", "ツ", "ツ", "ヒトツ", "ヒトツ"),
            ("2", "ニ", "つ", "ツ", "ツ", "フタツ", "フタツ"),
            ("8", "ハチ", "つ", "ツ", "ツ", "ヤッツ", "ヤッツ"),
            ("1", "イチ", "口", "クチ", "クチ", "ヒトクチ", "ヒトクチ"),
            ("2", "ニ", "口", "グチ", "グチ", "フタクチ", "フタクチ"),
            ("一", "イチ", "歩", "ホ", "ホ", "イッポ", "イッポ"),
            ("三", "サン", "歩", "ホ", "ホ", "サンポ", "サンポ"),
            ("六", "ロク", "歩", "ホ", "ホ", "ロッポ", "ロッポ"),
            ("百", "ヒャク", "歩", "ホ", "ホ", "ヒャッポ", "ヒャッポ"),
            ("1", "イチ", "点", "テン", "テン", "イッテン", "イッテン"),
            ("6", "ロク", "点", "テン", "テン", "ロクテン", "ロクテン"),
            ("8", "ハチ", "点", "テン", "テン", "ハッテン", "ハッテン"),
            ("1", "イチ", "通", "ツウ", "ツー", "イッツウ", "イッツー"),
            ("六", "ロク", "通", "ツウ", "ツー", "ロクツウ", "ロクツー"),
            (
                "十",
                "ジュウ",
                "冊",
                "サツ",
                "サツ",
                "ジュッサツ",
                "ジュッサツ",
            ),
            (
                "1",
                "イチ",
                "週間",
                "シュウカン",
                "シューカン",
                "イッシュウカン",
                "イッシューカン",
            ),
            (
                "6",
                "ロク",
                "週間",
                "シュウカン",
                "シューカン",
                "ロクシュウカン",
                "ロクシューカン",
            ),
            (
                "一",
                "イチ",
                "章",
                "ショウ",
                "ショー",
                "イッショウ",
                "イッショー",
            ),
            (
                "六",
                "ロク",
                "章",
                "ショウ",
                "ショー",
                "ロクショウ",
                "ロクショー",
            ),
            ("1", "イチ", "刷", "サツ", "サツ", "イッサツ", "イッサツ"),
            (
                "十",
                "ジュウ",
                "足",
                "ソク",
                "ソク",
                "ジュッソク",
                "ジュッソク",
            ),
            ("六", "ロク", "足", "ソク", "ソク", "ロクソク", "ロクソク"),
            (
                "十",
                "ジュウ",
                "歳",
                "サイ",
                "サイ",
                "ジュッサイ",
                "ジュッサイ",
            ),
            (
                "十",
                "ジュウ",
                "才",
                "サイ",
                "サイ",
                "ジュッサイ",
                "ジュッサイ",
            ),
            ("一", "イチ", "句", "ク", "ク", "イック", "イック"),
            ("六", "ロク", "句", "ク", "ク", "ロック", "ロック"),
            (
                "十",
                "ジュウ",
                "階",
                "カイ",
                "カイ",
                "ジュッカイ",
                "ジュッカイ",
            ),
            ("三", "サン", "階", "ガイ", "ガイ", "サンガイ", "サンガイ"),
            (
                "1",
                "イチ",
                "か月",
                "カゲツ",
                "カゲツ",
                "イッカゲツ",
                "イッカゲツ",
            ),
            (
                "6",
                "ロク",
                "ヶ月",
                "カゲツ",
                "カゲツ",
                "ロッカゲツ",
                "ロッカゲツ",
            ),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), (c, COUNTER, cr)]);
            out[1].pronunciation = cp.into();
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(
                readings(&out),
                (expected_r.into(), expected_p.into()),
                "{n}{c}"
            );
            assert_eq!(non_reading_fields(&out), fields, "{n}{c}");
            apply_counter_readings(&mut out);
            assert_eq!(
                readings(&out),
                (expected_r.into(), expected_p.into()),
                "repeat {n}{c}"
            );
        }
    }

    #[test]
    fn new_counter_readings_respect_identifiers_decimals_pos_and_explicit_readings() {
        for (c, cr) in [
            ("つ", "ツ"),
            ("口", "クチ"),
            ("歩", "ホ"),
            ("点", "テン"),
            ("句", "ク"),
        ] {
            let mut out = tokens(&[("1", NUMBER, "イチ"), (c, COUNTER, cr)]);
            let before = out.clone();
            let protected = out[0].start..out[0].end;
            apply_counter_readings_with_protection(&mut out, &[protected]);
            assert_eq!(readings(&out), readings(&before), "protected {c}");
            assert_eq!(non_reading_fields(&out), non_reading_fields(&before));
        }
        for parts in [
            vec![("1", NUMBER, "イチ"), ("つ", "助詞,終助詞,*,*", "ツ")],
            vec![("1", NUMBER, "イチ"), ("歩", "名詞,一般,*,*", "ホ")],
            vec![("1", NUMBER, "イチ"), ("章", COUNTER, "アキラ")],
            vec![("1", NUMBER, "イチャ"), ("つ", COUNTER, "ツ")],
            vec![("1", NUMBER, "イチ"), ("つ", COUNTER, "ヅ")],
            vec![("10", NUMBER, "ジュウ"), ("つ", COUNTER, "ツ")],
            vec![
                ("1", NUMBER, "イチ"),
                ("2", NUMBER, "ニ"),
                ("つ", COUNTER, "ツ"),
            ],
            vec![
                ("1", NUMBER, "イチ"),
                (".", "記号,一般,*,*", "."),
                ("2", NUMBER, "ニ"),
                ("つ", COUNTER, "ツ"),
            ],
            vec![
                ("第", "接頭詞,数接続,*,*", "ダイ"),
                ("1", NUMBER, "イチ"),
                ("口", COUNTER, "クチ"),
            ],
        ] {
            let mut out = tokens(&parts);
            let before = out.clone();
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), readings(&before), "{parts:?}");
            assert_eq!(non_reading_fields(&out), non_reading_fields(&before));
        }
        for conj_type in ["文語・ツ", "下二・タ行"] {
            let mut out = tokens(&[("1", NUMBER, "イチ"), ("つ", "助動詞,*,*,*", "ツ")]);
            out[1].conj_type = conj_type.into();
            out[1].conj_form = "基本形".into();
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), ("ヒトツ".into(), "ヒトツ".into()));
            assert_eq!(non_reading_fields(&out), fields);
        }
        let mut out = tokens(&[("一", NUMBER, "イチ"), ("章", "名詞,一般,*,*", "ショウ")]);
        out[1].pronunciation = "ショー".into();
        let fields = non_reading_fields(&out);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out), ("イッショウ".into(), "イッショー".into()));
        assert_eq!(non_reading_fields(&out), fields);
    }

    #[test]
    fn protected_ascii_numbers_skip_new_euphony_and_keep_existing_single_digit_rules() {
        for (n, nr, c, cr, expected) in [
            ("1", "イチ", "兆", "チョウ", "イチチョウ"),
            ("8", "ハチ", "兆", "チョウ", "ハチチョウ"),
            ("10", "ジュウ", "兆", "チョウ", "ジュウチョウ"),
            ("11", "ジュウイチ", "本", "ホン", "ジュウイチホン"),
            ("1", "イチ", "本", "ホン", "イッポン"),
            ("8", "ハチ", "分", "フン", "ハップン"),
        ] {
            let cpos = if c == "兆" { NUMBER } else { COUNTER };
            let mut out = tokens(&[(n, NUMBER, nr), (c, cpos, cr)]);
            let fields = non_reading_fields(&out);
            let protected = out[0].start..out[0].end;
            apply_counter_readings_with_protection(&mut out, std::slice::from_ref(&protected));
            assert_eq!(readings(&out), (expected.into(), expected.into()), "{n}{c}");
            assert_eq!(non_reading_fields(&out), fields, "{n}{c}");
        }
    }

    #[test]
    fn trillion_euphony_requires_a_uniform_supported_number_run() {
        for parts in [
            vec![("1", NUMBER, "イチ"), ("８", NUMBER, "ハチ")],
            vec![("1", NUMBER, "イチ"), ("八", NUMBER, "ハチ")],
            vec![("1８", NUMBER, "イチハチ")],
            vec![("1八", NUMBER, "イチハチ")],
            vec![("８", NUMBER, "ハチ")],
            vec![("１８", NUMBER, "ジュウハチ")],
            vec![("１", NUMBER, "イチ"), ("８", NUMBER, "ハチ")],
        ] {
            let mut parts = parts;
            parts.push(("兆", NUMBER, "チョウ"));
            let mut out = tokens(&parts);
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before, "{parts:?}");
        }
        for (n, reading, expected) in [
            ("1", "イチ", "イッチョウ"),
            ("8", "ハチ", "ハッチョウ"),
            ("10", "ジュウ", "ジュッチョウ"),
            ("一", "イチ", "イッチョウ"),
            ("八", "ハチ", "ハッチョウ"),
            ("十", "ジュウ", "ジュッチョウ"),
        ] {
            let mut out = tokens(&[(n, NUMBER, reading), ("兆", NUMBER, "チョウ")]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (expected.into(), expected.into()), "{n}兆");
            assert_eq!(non_reading_fields(&out), fields, "{n}兆");
        }
        let mut out = tokens(&[
            ("1", NUMBER, "イチ"),
            ("8", NUMBER, "ハチ"),
            ("兆", NUMBER, "チョウ"),
        ]);
        let before = format!("{out:?}");
        let protected = out[0].start..out[0].end;
        apply_counter_readings_with_protection(&mut out, std::slice::from_ref(&protected));
        assert_eq!(format!("{out:?}"), before);
    }

    #[test]
    fn protection_is_matched_per_range_across_several_protected_runs() {
        // 保護された範囲は位置の昇順に渡る。数詞列を走査する側は走査位置を戻さずに進めるので、
        // 2 つ目以降の範囲でも、その範囲に重なる数詞だけが保護されることを確かめる。
        let parts: &[(&str, &str, &str)] = &[
            ("10", NUMBER, "ジュウ"),
            ("本", COUNTER, "ホン"),
            ("、", "記号,読点,*,*", "、"),
            ("10", NUMBER, "ジュウ"),
            ("本", COUNTER, "ホン"),
            ("、", "記号,読点,*,*", "、"),
            ("10", NUMBER, "ジュウ"),
            ("本", COUNTER, "ホン"),
        ];

        // 保護なしでは 3 つとも促音化する
        let mut out = tokens(parts);
        apply_counter_readings_with_protection(&mut out, &[]);
        assert_eq!(
            readings(&out).0,
            "ジュッポン、ジュッポン、ジュッポン",
            "保護なし"
        );

        // 1 つ目と 3 つ目だけを保護すると、真ん中の数詞だけが促音化する
        let mut out = tokens(parts);
        let protected = [out[0].start..out[0].end, out[6].start..out[6].end];
        let fields = non_reading_fields(&out);
        apply_counter_readings_with_protection(&mut out, &protected);
        assert_eq!(readings(&out).0, "ジュウホン、ジュッポン、ジュウホン");
        assert_eq!(non_reading_fields(&out), fields);
    }

    #[test]
    fn compound_numerals_require_a_standard_reading_for_the_entire_word() {
        for (surface, reading, pronunciation, expected_r, expected_p) in [
            (
                "十一",
                "テンイチ",
                "テンイチ",
                "テンイチホン",
                "テンイチホン",
            ),
            (
                "十一",
                "ジュウイチ",
                "ジューイチ",
                "ジュウイッポン",
                "ジューイッポン",
            ),
            (
                "三百",
                "サンビャク",
                "サンビャク",
                "サンビャッポン",
                "サンビャッポン",
            ),
            (
                "三百",
                "サンヒャク",
                "サンヒャク",
                "サンヒャクホン",
                "サンヒャクホン",
            ),
            (
                "六百",
                "ロッピャク",
                "ロッピャク",
                "ロッピャッポン",
                "ロッピャッポン",
            ),
            (
                "三千",
                "サンゼン",
                "サンゼン",
                "サンゼンボン",
                "サンゼンボン",
            ),
            (
                "二十四",
                "ニジュウシ",
                "ニジューシ",
                "ニジュウシホン",
                "ニジューシホン",
            ),
            ("11", "テンイチ", "テンイチ", "テンイチホン", "テンイチホン"),
        ] {
            let mut out = tokens(&[(surface, NUMBER, reading), ("本", COUNTER, "ホン")]);
            out[0].pronunciation = pronunciation.into();
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(
                readings(&out),
                (expected_r.into(), expected_p.into()),
                "{surface}: {reading}"
            );
            assert_eq!(non_reading_fields(&out), fields);
        }
    }

    #[test]
    fn place_allomorphs_are_selected_from_the_preceding_number() {
        for (n, nr, place, pr, expected) in [
            ("二", "ニ", "百", "ピャク", "ニヒャク"),
            ("二", "ニ", "千", "ゼン", "ニセン"),
            ("一", "イチ", "千", "ゼン", "イッセン"),
            ("三", "サン", "百", "ピャク", "サンビャク"),
            ("八", "ハチ", "千", "ゼン", "ハッセン"),
            ("二", "ニ", "百", "ハンドレッド", "ニハンドレッド"),
            ("二", "ニ", "千", "サウザンド", "ニサウザンド"),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), (place, NUMBER, pr)]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (expected.into(), expected.into()));
            assert_eq!(non_reading_fields(&out), fields);
        }
    }

    #[test]
    fn counter_allomorphs_are_selected_from_the_preceding_number() {
        for (n, nr, c, cr, expected) in [
            ("二", "ニ", "本", "ポン", "ニホン"),
            ("五", "ゴ", "杯", "バイ", "ゴハイ"),
            ("七", "ナナ", "匹", "ピキ", "ナナヒキ"),
            ("二", "ニ", "分", "プン", "ニフン"),
            ("二", "ニ", "ぷん", "プン", "ニプン"),
            ("三", "サン", "人", "リ", "サンニン"),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), (c, COUNTER, cr)]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (expected.into(), expected.into()));
            assert_eq!(non_reading_fields(&out), fields);
        }
        let mut out = tokens(&[
            ("十", NUMBER, "ジュウ"),
            ("三", NUMBER, "サン"),
            ("日", COUNTER, "カ"),
        ]);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out).0, "ジュウサンニチ");
    }

    #[test]
    fn native_days_require_standard_readings_for_the_entire_numeric_run() {
        for parts in [
            vec![
                ("十", NUMBER, "テン"),
                ("四", NUMBER, "ヨン"),
                ("日", COUNTER, "ニチ"),
            ],
            vec![("十四", NUMBER, "テンヨン"), ("日", COUNTER, "ニチ")],
            vec![
                ("二", NUMBER, "トゥ"),
                ("十", NUMBER, "ジュウ"),
                ("四", NUMBER, "ヨン"),
                ("日", COUNTER, "ニチ"),
            ],
        ] {
            let mut out = tokens(&parts);
            let before = readings(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), before);
        }
        let mut out = tokens(&[("十四", NUMBER, "ジュウヨン"), ("日", COUNTER, "ニチ")]);
        out[0].pronunciation = "テンヨン".into();
        let before = readings(&out);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out), before);
    }

    #[test]
    fn ordinary_days_use_shichi_and_ku_after_the_tens() {
        for (n, nr, expected) in [
            ("七", "ナナ", "ジュウシチニチ"),
            ("九", "キュウ", "ジュウクニチ"),
        ] {
            let mut out = tokens(&[
                ("十", NUMBER, "ジュウ"),
                (n, NUMBER, nr),
                ("日", COUNTER, "ニチ"),
            ]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (expected.into(), expected.into()));
            assert_eq!(non_reading_fields(&out), fields);
        }
    }

    #[test]
    fn place_euphony_composes_with_following_counter() {
        for (n, nr, place, pr, c, cr, expected) in [
            ("三", "サン", "百", "ヒャク", "本", "ホン", "サンビャッポン"),
            ("六", "ロク", "百", "ヒャク", "本", "ホン", "ロッピャッポン"),
            ("八", "ハチ", "百", "ヒャク", "回", "カイ", "ハッピャッカイ"),
            ("三", "サン", "千", "セン", "杯", "ハイ", "サンゼンバイ"),
            ("八", "ハチ", "千", "セン", "本", "ホン", "ハッセンボン"),
            ("一", "イチ", "千", "セン", "本", "ホン", "イッセンボン"),
            ("十", "ジュウ", "千", "セン", "本", "ホン", "ジュッセンボン"),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), (place, NUMBER, pr), (c, COUNTER, cr)]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (expected.into(), expected.into()));
            assert_eq!(non_reading_fields(&out), fields);
        }
    }

    #[test]
    fn native_people_are_split_into_two_nonempty_readings() {
        for (n, nr, expected_n) in [("一", "イチ", "ヒト"), ("二", "ニ", "フタ")] {
            let mut out = tokens(&[(n, NUMBER, nr), ("人", COUNTER, "ニン")]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(out[0].reading.as_ref(), expected_n);
            assert_eq!(out[1].reading.as_ref(), "リ");
            assert_eq!(non_reading_fields(&out), fields);
            assert!(out.iter().all(|t| t.mora_count() > 0));
        }
        let mut out = tokens(&[
            ("十", NUMBER, "ジュウ"),
            ("二", NUMBER, "ニ"),
            ("人", COUNTER, "ニン"),
        ]);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out).0, "ジュウニニン");
        let mut out = tokens(&[("02", NUMBER, "ゼロニ"), ("人", COUNTER, "ニン")]);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out).0, "ゼロニニン");
    }

    #[test]
    fn native_days_keep_reading_and_pronunciation_distinct() {
        for (n, nr, r, p) in [
            ("二", "ニ", "フツ", "フツ"),
            ("三", "サン", "ミッ", "ミッ"),
            ("四", "ヨン", "ヨッ", "ヨッ"),
            ("五", "ゴ", "イツ", "イツ"),
            ("六", "ロク", "ムイ", "ムイ"),
            ("七", "ナナ", "ナノ", "ナノ"),
            ("八", "ハチ", "ヨウ", "ヨー"),
            ("九", "キュウ", "ココノ", "ココノ"),
            ("十", "ジュウ", "トオ", "トー"),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), ("日", COUNTER, "ニチ")]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(
                readings(&out),
                (format!("{r}カ"), format!("{p}カ")),
                "{n}日"
            );
            assert_eq!(non_reading_fields(&out), fields);
            assert!(
                out.iter()
                    .all(|t| !t.reading.is_empty() && !t.pronunciation.is_empty())
            );
        }
        for parts in [
            vec![
                ("十", NUMBER, "ジュウ"),
                ("四", NUMBER, "ヨン"),
                ("日", COUNTER, "ニチ"),
            ],
            vec![
                ("二", NUMBER, "ニ"),
                ("十", NUMBER, "ジュウ"),
                ("四", NUMBER, "ヨン"),
                ("日", COUNTER, "ニチ"),
            ],
        ] {
            let mut out = tokens(&parts);
            let prefix: String = out[..out.len() - 2]
                .iter()
                .map(|t| t.reading.as_ref())
                .collect();
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out).0, format!("{prefix}ヨッカ"));
        }
        for parts in [
            vec![
                ("十", NUMBER, "ジュウ"),
                ("三", NUMBER, "サン"),
                ("日", COUNTER, "ニチ"),
            ],
            vec![
                ("二", NUMBER, "ニ"),
                ("十", NUMBER, "ジュウ"),
                ("日", COUNTER, "ニチ"),
            ],
            vec![("一", NUMBER, "イチ"), ("日", COUNTER, "ニチ")],
            vec![
                ("1", NUMBER, "イチ"),
                ("4", NUMBER, "ヨン"),
                ("日", COUNTER, "ニチ"),
            ],
            vec![("03", NUMBER, "ゼロサン"), ("日", COUNTER, "ニチ")],
        ] {
            let mut out = tokens(&parts);
            let before = readings(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), before);
        }
    }

    #[test]
    fn compound_counters_and_long_vowels_keep_the_numeric_prefix() {
        let mut out = tokens(&[
            ("二", NUMBER, "ニ"),
            ("十", NUMBER, "ジュウ"),
            ("一", NUMBER, "イチ"),
            ("本", COUNTER, "ホン"),
        ]);
        out[1].pronunciation = "ジュー".into();
        apply_counter_readings(&mut out);
        assert_eq!(
            readings(&out),
            ("ニジュウイッポン".into(), "ニジューイッポン".into())
        );
        let mut out = tokens(&[("十", NUMBER, "ジュウ"), ("分", COUNTER, "フン")]);
        out[0].pronunciation = "ジュー".into();
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out), ("ジュップン".into(), "ジュップン".into()));
    }

    #[test]
    fn counter_readings_require_adjacency_pos_and_standard_readings() {
        for (npos, cpos, nr, cr) in [
            ("名詞,固有名詞,人名,姓", COUNTER, "イチ", "ホン"),
            (NUMBER, "名詞,固有名詞,一般,*", "イチ", "ホン"),
            ("名詞,数詞別読み,*,*", COUNTER, "イチ", "ホン"),
            (NUMBER, "名詞,接尾,助数詞別読み,*", "イチ", "ホン"),
            (NUMBER, COUNTER, "ワン", "ホン"),
            (NUMBER, COUNTER, "オイチ", "ホン"),
            (NUMBER, COUNTER, "イチ", "モト"),
        ] {
            let mut out = tokens(&[("一", npos, nr), ("本", cpos, cr)]);
            let before = readings(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), before);
        }
        for gap in [1, 2, 3] {
            let mut out = tokens(&[("一", NUMBER, "イチ"), ("本", COUNTER, "ホン")]);
            out[1].start += gap;
            out[1].end += gap;
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out).0, "イチホン");
        }
        let mut out = tokens(&[("六", NUMBER, "ロク"), ("冊", COUNTER, "サツ")]);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out).0, "ロクサツ");
    }

    #[test]
    fn decimal_fractions_are_not_geminated_or_counted_in_native_japanese() {
        for point in ["点", ".", "．"] {
            for (n, nr, c, cr) in [
                ("一", "イチ", "回", "カイ"),
                ("二", "ニ", "人", "ニン"),
                ("三", "サン", "日", "ニチ"),
            ] {
                let mut out = tokens(&[
                    ("四", NUMBER, "ヨン"),
                    (point, "名詞,一般,*,*", "テン"),
                    (n, NUMBER, nr),
                    (c, COUNTER, cr),
                ]);
                let before = readings(&out);
                apply_counter_readings(&mut out);
                assert_eq!(readings(&out), before);
            }
        }
        let mut out = tokens(&[("2.1", NUMBER, "ニテンイチ"), ("回", COUNTER, "カイ")]);
        let before = readings(&out);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out), before);
    }

    #[test]
    fn unknown_pun_is_the_only_ordinary_noun_accepted_as_a_counter() {
        for known in [false, true] {
            let mut out = tokens(&[("十", NUMBER, "ジュウ"), ("ぷん", "名詞,一般,*,*", "プン")]);
            out[1].is_known = known;
            apply_counter_readings(&mut out);
            assert_eq!(
                readings(&out).0,
                if known {
                    "ジュウプン"
                } else {
                    "ジュップン"
                }
            );
        }
        let mut out = tokens(&[("十", NUMBER, "ジュウ"), ("ぷん", COUNTER, "プン")]);
        apply_counter_readings(&mut out);
        assert_eq!(readings(&out).0, "ジュップン");
    }

    #[test]
    fn ten_minutes_only_reinterprets_the_temporal_context() {
        for (word, pos) in [("前", "名詞,副詞可能,*,*"), ("後", "名詞,接尾,副詞可能,*")]
        {
            let mut out = tokens(&[
                ("十分", "名詞,形容動詞語幹,*,*", "ジュウブン"),
                (word, pos, "アト"),
            ]);
            out[0].pronunciation = "ジューブン".into();
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(out[0].reading.as_ref(), "ジュップン");
            assert_eq!(out[0].pronunciation.as_ref(), "ジュップン");
            assert_eq!(non_reading_fields(&out), fields);
        }
        for (word, pos) in [
            ("に", "助詞,副詞化,*,*"),
            ("な", "助動詞,*,*,*"),
            ("前", "名詞,固有名詞,人名,姓"),
        ] {
            let mut out = tokens(&[
                ("十分", "名詞,形容動詞語幹,*,*", "ジュウブン"),
                (word, pos, "ナ"),
            ]);
            let before = readings(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), before);
        }
        let mut out = tokens(&[
            ("十分", "名詞,形容動詞語幹,*,*", "ジュウブン"),
            ("前", "名詞,副詞可能,*,*", "マエ"),
        ]);
        out[1].start += 1;
        out[1].end += 1;
        apply_counter_readings(&mut out);
        assert_eq!(out[0].reading.as_ref(), "ジュウブン");
    }

    #[test]
    #[ignore = "配布辞書 3 個が必要（make dict-download）"]
    fn distributed_numeric_counter_readings() {
        for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("dict")
                .join(format!("{name}.hsd"));
            let mut analyzer = crate::Analyzer::load(path).unwrap();
            for (text, expected) in [
                ("三本", "サンボン"),
                ("一本", "イッポン"),
                ("一回", "イッカイ"),
                ("六分", "ロップン"),
                ("八個", "ハッコ"),
                ("三杯", "サンバイ"),
                ("三日", "ミッカ"),
                ("二人", "フタリ"),
                ("四時", "ヨジ"),
                ("三百", "サンビャク"),
                ("八千", "ハッセン"),
                ("1分", "イップン"),
                ("十分前", "ジュップンマエ"),
                ("十分後", "ジュップンゴ"),
                ("十ぷん", "ジュップン"),
                ("三百本", "サンビャッポン"),
                ("六百本", "ロッピャッポン"),
                ("二十一本", "ニジューイッポン"),
                ("十四日", "ジューヨッカ"),
                ("二十四日", "ニジューヨッカ"),
                ("十七日", "ジューシチニチ"),
                ("十九日", "ジュークニチ"),
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                assert_eq!(readings(&out).1, expected, "{name}: {text}");
                for t in &out {
                    assert_eq!(&text[t.start..t.end], t.surface.as_ref());
                }
            }
        }
    }
}

#[cfg(all(test, feature = "build"))]
mod integration_tests {
    use crate::dict::{DictBuilder, DictEntry};
    use crate::lattice::{LatticeWorkspace, Token};
    use crate::{Analyzer, Dictionary};

    const NUMBER: &str = "名詞,数,*,*";
    const COUNTER: &str = "名詞,接尾,助数詞,*";

    fn dictionary() -> Dictionary {
        let mut builder = DictBuilder::new();
        for (surface, pos, reading, pronunciation) in [
            ("一", NUMBER, "イチ", "イチ"),
            ("二", NUMBER, "ニ", "ニ"),
            ("三", NUMBER, "サン", "サン"),
            ("四", NUMBER, "ヨン", "ヨン"),
            ("六", NUMBER, "ロク", "ロク"),
            ("八", NUMBER, "ハチ", "ハチ"),
            ("十", NUMBER, "ジュウ", "ジュー"),
            ("百", NUMBER, "ヒャク", "ヒャク"),
            ("本", COUNTER, "ホン", "ホン"),
            ("回", COUNTER, "カイ", "カイ"),
            ("分", COUNTER, "フン", "フン"),
            ("人", COUNTER, "ニン", "ニン"),
            ("日", COUNTER, "ニチ", "ニチ"),
            ("十分", "名詞,形容動詞語幹,*,*", "ジュウブン", "ジューブン"),
            ("前", "名詞,副詞可能,*,*", "マエ", "マエ"),
            ("後", "名詞,接尾,副詞可能,*", "ゴ", "ゴ"),
            ("に", "助詞,格助詞,一般,*", "ニ", "ニ"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                reading: reading.into(),
                pronunciation: pronunciation.into(),
                base_form: format!("base:{surface}").into(),
                conj_type: "fixture-type".into(),
                conj_form: "fixture-form".into(),
                cost: if surface == "十分" { -100 } else { 0 },
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    fn pronunciation(tokens: &[Token]) -> String {
        tokens.iter().map(|t| t.pronunciation.as_ref()).collect()
    }

    #[test]
    fn analyzer_preserves_dictionary_fields_and_does_not_cache_contextual_readings() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for _ in 0..2 {
            for (text, expected) in [
                ("二人", "フタリ"),
                ("三日", "ミッカ"),
                ("三百本", "サンビャッポン"),
                ("六百本", "ロッピャッポン"),
                ("二十一本", "ニジューイッポン"),
                ("十二人", "ジューニニン"),
                ("十三日", "ジューサンニチ"),
                ("十四日", "ジューヨッカ"),
                ("二十四日", "ニジューヨッカ"),
                ("十分前", "ジュップンマエ"),
                ("十分後", "ジュップンゴ"),
                ("十分に", "ジューブンニ"),
                ("一回", "イッカイ"),
                ("一", "イチ"),
                ("三 本", "サンホン"),
                ("三\t本", "サンホン"),
                ("三\n本", "サンホン"),
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                assert_eq!(pronunciation(&out), expected, "{text}");
                for t in &out {
                    assert_eq!(&text[t.start..t.end], t.surface.as_ref());
                    assert_eq!(t.base_form.as_ref(), format!("base:{}", t.surface));
                    assert_eq!(t.conj_type.as_ref(), "fixture-type");
                    assert_eq!(t.conj_form.as_ref(), "fixture-form");
                    assert_eq!(
                        t.word_cost,
                        if t.surface.as_ref() == "十分" {
                            -100
                        } else {
                            0
                        }
                    );
                    assert!(t.is_known);
                    assert_eq!(
                        t.pos.as_ref(),
                        match t.surface.as_ref() {
                            "一" | "二" | "三" | "四" | "六" | "八" | "十" | "百" => NUMBER,
                            "十分" => "名詞,形容動詞語幹,*,*",
                            "前" => "名詞,副詞可能,*,*",
                            "後" => "名詞,接尾,副詞可能,*",
                            "に" => "助詞,格助詞,一般,*",
                            _ => COUNTER,
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn tokenize_into_preserves_offsets_and_does_not_modify_previous_output() {
        let dict = dictionary();
        let mut workspace = LatticeWorkspace::new();
        let mut out = workspace.tokenize("一", &dict).unwrap();
        let previous = format!("{:?}", out[0]);
        workspace
            .tokenize_into("三百本", &dict, 91, &mut out)
            .unwrap();
        assert_eq!(format!("{:?}", out[0]), previous);
        assert_eq!(pronunciation(&out[1..]), "サンビャッポン");
        for t in &out[1..] {
            assert_eq!(&"三百本"[t.start - 91..t.end - 91], t.surface.as_ref());
        }
        let mut out = workspace.tokenize("一", &dict).unwrap();
        workspace
            .tokenize_into("本", &dict, "一".len(), &mut out)
            .unwrap();
        assert_eq!(pronunciation(&out), "イチホン");
    }
}
