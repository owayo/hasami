//! 最良パスの数詞と助数詞の読み。ASCII と位を省いた漢数字の数量は数詞列を合成する。
//!
//! 音便は助数詞ごとの対応表で決め、読み以外の情報を保持する。人・日の和語読みは数詞列全体を見る。
//! 一日は日付と期間を区別できないので変更せず、二十日は単一数詞にまとまったときだけ読む。

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
    /// 「三拍」は濁音にせず半濁音にする。本・匹・杯・遍とは別の系列。
    UnvoicedH {
        plain: char,
        half: char,
        three: bool,
    },
    VoicedH {
        plain: char,
        voiced: char,
        half: char,
    },
    Ken,
    MetricK,
    Plain,
    K,
    ExtraK,
    Soft,
    Step,
    NativeTs,
    Mouth,
    Box,
    NativePair(&'static str),
    Minute,
    Hundred,
    Thousand,
    Person,
    Day,
    DaySpan,
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
    let suffix = pos_has(&token.pos, "名詞,接尾,助数詞")
        || (token.is_known
            && matches!(token.surface.as_ref(), "都" | "府" | "県")
            && pos_has(&token.pos, "名詞,接尾,地域"));
    // 読みが単位として解決済みのSI略号だけを認可し、固有名詞は再解釈しない。
    if suffix || pos_has(&token.pos, "名詞,一般") {
        let unit = match token.surface.as_ref() {
            "kg" | "㎏" | "㌕" => Some(("キログラム", Counter::MetricK)),
            "km" | "㎞" | "㌖" => Some(("キロメートル", Counter::MetricK)),
            "㌔" => Some(("キロ", Counter::MetricK)),
            "cm" | "㎝" => Some(("センチメートル", Counter::Soft)),
            "㌢" => Some(("センチ", Counter::Soft)),
            _ => None,
        };
        if let Some((reading, kind)) = unit
            && has_reading(token, &[reading])
        {
            return Some(kind);
        }
    }
    // 号はIPAdicで「接尾,一般」。一般の接尾辞を数量へ広げず、この標準形だけ認可する。
    if token.surface.as_ref() == "号"
        && pos_has(&token.pos, "名詞,接尾,一般")
        && has_reading(token, &["ゴウ", "ゴー"])
    {
        return Some(Counter::Plain);
    }
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
    let unvoiced_h = |plain: &'static str,
                      pronunciation: &'static str,
                      half: &'static str,
                      half_pronunciation: &'static str| {
        // 読み・発音の長音差は保ち、ヒョウ/ヒョーを同じ系列として検査する。
        (has_reading(token, &[plain, pronunciation])
            || has_reading(token, &[half, half_pronunciation]))
        .then_some(Counter::UnvoicedH {
            plain: plain.chars().next().unwrap(),
            half: half.chars().next().unwrap(),
            three: true,
        })
    };
    match token.surface.as_ref() {
        "本" => h("ホン", "ボン", "ポン"),
        "杯" => h("ハイ", "バイ", "パイ"),
        "匹" => h("ヒキ", "ビキ", "ピキ"),
        "遍" if has_reading(token, &["ヘン", "ベン", "ペン"]) => Some(Counter::VoicedH {
            plain: 'ヘ',
            voiced: 'ベ',
            half: 'ペ',
        }),
        "拍" | "泊" => unvoiced_h("ハク", "ハク", "パク", "パク"),
        "票" => unvoiced_h("ヒョウ", "ヒョー", "ピョウ", "ピョー"),
        // 三俵にはビョウ/ピョウが併存する。既に選ばれたB/Pは上書きしない。
        "俵" if has_reading(token, &["ヒョウ", "ヒョー"]) => Some(Counter::VoicedH {
            plain: 'ヒ',
            voiced: 'ビ',
            half: 'ピ',
        }),
        "発" => unvoiced_h("ハツ", "ハツ", "パツ", "パツ"),
        "筆" => unvoiced_h("ヒツ", "ヒツ", "ピツ", "ピツ"),
        "品" => unvoiced_h("ヒン", "ヒン", "ピン", "ピン"),
        "波" => unvoiced_h("ハ", "ハ", "パ", "パ"),
        "敗" => unvoiced_h("ハイ", "ハイ", "パイ", "パイ"),
        "服" if has_reading(token, &["フク", "プク"]) => Some(Counter::UnvoicedH {
            plain: 'フ',
            half: 'プ',
            three: false,
        }),
        "篇" | "編" if has_reading(token, &["ヘン", "ペン"]) => Some(Counter::UnvoicedH {
            plain: 'ヘ',
            half: 'ペ',
            three: false,
        }),
        "府" => unvoiced_h("フ", "フ", "プ", "プ"),
        "都" if has_reading(token, &["ト"]) => Some(Counter::Soft),
        "県" if has_reading(token, &["ケン"]) => Some(Counter::ExtraK),
        "軒" if has_reading(token, &["ケン"]) => Some(Counter::Ken),
        "校" if has_reading(token, &["コウ", "コー"]) => Some(Counter::ExtraK),
        "巻" if has_reading(token, &["カン"]) => Some(Counter::ExtraK),
        "課" if has_reading(token, &["カ"]) => Some(Counter::ExtraK),
        "機" if has_reading(token, &["キ"]) => Some(Counter::ExtraK),
        "石" if has_reading(token, &["コク"]) => Some(Counter::ExtraK),
        "か所" | "箇所" | "ヶ所" | "カ所" | "ヵ所" if has_reading(token, &["カショ"]) => {
            Some(Counter::ExtraK)
        }
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
        "棟" if has_reading(token, &["トウ", "トー"]) => Some(Counter::Soft),
        "体" if has_reading(token, &["タイ"]) => Some(Counter::Soft),
        "束" if has_reading(token, &["ソク"]) => Some(Counter::Soft),
        "隻" | "席" | "石" if has_reading(token, &["セキ"]) => Some(Counter::Soft),
        "勝" if has_reading(token, &["ショウ", "ショー"]) => Some(Counter::Soft),
        "戦" if has_reading(token, &["セン"]) => Some(Counter::Soft),
        "尺" if has_reading(token, &["シャク"]) => Some(Counter::Soft),
        "寸" if has_reading(token, &["スン"]) => Some(Counter::Soft),
        "センチ" | "センチメートル" if has_reading(token, &[token.surface.as_ref()]) => {
            Some(Counter::Soft)
        }
        "キロ" | "キログラム" | "キロメートル" if has_reading(token, &[token.surface.as_ref()]) => {
            Some(Counter::MetricK)
        }
        "箱" if has_reading(token, &["ハコ", "バコ", "パコ"]) => Some(Counter::Box),
        "皿" if has_reading(token, &["サラ"]) => Some(Counter::NativePair("サラ")),
        "束" if has_reading(token, &["タバ"]) => Some(Counter::NativePair("タバ")),
        "歩" if has_reading(token, &["ホ", "ポ"]) => Some(Counter::Step),
        "口" if has_reading(token, &["クチ", "グチ"]) => Some(Counter::Mouth),
        "分" | "ぷん" if has_reading(token, &["フン", "プン"]) => Some(Counter::Minute),
        "人" if has_reading(token, &["ニン", "リ"]) => Some(Counter::Person),
        "日" if has_reading(token, &["ニチ", "カ"]) => Some(Counter::Day),
        "日間" if token.is_known && has_reading(token, &["ニチカン"]) => {
            Some(Counter::DaySpan)
        }
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

/// 確認済みのH/P系列の頭だけを替え、票のヒョウ/ヒョー等の長音を保持する。
fn set_initial(token: &mut Token, initial: char) {
    let replace = |s: &str| {
        let first = s.chars().next().unwrap();
        if first == initial {
            s.to_owned()
        } else {
            format!("{initial}{}", &s[first.len_utf8()..])
        }
    };
    set_readings(
        token,
        &replace(&token.reading),
        &replace(&token.pronunciation),
    );
}

fn apply_initial_h(
    number: &mut Token,
    suffix: &mut Token,
    ending: Ending,
    plain: char,
    three: char,
    half: char,
) {
    match ending {
        Ending::One | Ending::Six | Ending::Eight | Ending::Ten | Ending::Hundred => {
            if geminate(number, ending) {
                set_initial(suffix, half);
            }
        }
        Ending::Three => set_initial(suffix, three),
        // 四拍・四票のH/Pや、未確認の千/万の選択は保持する。
        Ending::Four | Ending::Thousand | Ending::TenThousand => {}
        _ => set_initial(suffix, plain),
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
    if value == 20 {
        // 二/十へ分割したままハツを語尾へ配分すると「ニハツカ」になる。
        if run.len() != 1
            || !match run[0].surface.as_ref() {
                "20" => true,
                // global planで識別子を除外した合成済み数詞だけを認可する。
                "二十" => !run[0].is_known,
                _ => false,
            }
        {
            return None;
        }
        let identifier = |t: &Token| {
            t.surface
                .chars()
                .any(|c| c.is_ascii_alphabetic() || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '_'))
        };
        let left = start.checked_sub(1).and_then(|i| tokens.get(i));
        let right = tokens.get(end + 1);
        if left
            .is_some_and(|t| t.end == run[0].start && (identifier(t) || t.surface.as_ref() == "第"))
            || right.is_some_and(|t| tokens[end].end == t.start && identifier(t))
        {
            return None;
        }
        return Some(("ハツ", "ハツ"));
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

fn native_day_span(
    tokens: &[Token],
    start: usize,
    end: usize,
) -> Option<(&'static str, &'static str)> {
    // 日間は新しい系列なので、既存の日の一桁読みより保守的に識別子と序数を保護する。
    let identifier = |t: &Token| {
        t.surface
            .chars()
            .any(|c| c.is_ascii_alphabetic() || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '_'))
    };
    if start.checked_sub(1).is_some_and(|i| {
        tokens[i].end == tokens[start].start
            && (identifier(&tokens[i]) || tokens[i].surface.as_ref() == "第")
    }) || tokens
        .get(end + 1)
        .is_some_and(|t| tokens[end].end == t.start && identifier(t))
    {
        return None;
    }
    native_day(tokens, start, end)
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
    /// 二十日だけは数詞の語尾でなく、単一数詞の全体をハツへ替える。
    whole_day: bool,
    /// 人の和語読み（一人・二人）にできるか
    person: bool,
    /// 「つ」と「口」の和語読み。数詞が一つの標準形のときだけ使う。
    native: Option<&'static str>,
}

fn native_counter_prefix(counter: Counter, value: u64) -> Option<&'static str> {
    match (counter, value) {
        (Counter::NativeTs | Counter::Mouth | Counter::Box | Counter::NativePair(_), 1) => {
            Some("ヒト")
        }
        (Counter::NativeTs | Counter::Mouth | Counter::Box | Counter::NativePair(_), 2) => {
            Some("フタ")
        }
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
        Counter::ExtraK
            | Counter::Soft
            | Counter::Step
            | Counter::NativeTs
            | Counter::Mouth
            | Counter::UnvoicedH { .. }
            | Counter::VoicedH { .. }
            | Counter::Ken
            | Counter::MetricK
            | Counter::Plain
            | Counter::Box
            | Counter::NativePair(_)
            | Counter::DaySpan
    ) && run.protected
    {
        return None;
    }
    // 新しい二十日の和語読みは、既存のID保護を全字幅で尊重する。
    if matches!(counter, Counter::Day)
        && run.protected
        && i - run.start == 1
        && numeral_value(std::iter::once(tokens[i - 1].surface.as_ref())) == Some(20)
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
    let day = match counter {
        Counter::Day => native_day(tokens, run.start, i),
        Counter::DaySpan => native_day_span(tokens, run.start, i),
        _ => None,
    };
    let whole_day = day.is_some()
        && i - run.start == 1
        && numeral_value(std::iter::once(number.surface.as_ref())) == Some(20);
    let person = matches!(counter, Counter::Person)
        && i - run.start == 1
        && has_reading(number, tails)
        && matches!(
            numeral_value(std::iter::once(number.surface.as_ref())),
            Some(1 | 2)
        );
    let native = if matches!(
        counter,
        Counter::NativeTs | Counter::Mouth | Counter::Box | Counter::NativePair(_)
    ) && i - run.start == 1
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
        whole_day,
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
        whole_day,
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
        Counter::UnvoicedH { plain, half, three } => {
            // 万票の放送用表と「千・万は三と同様」の規則から、この末尾だけBを選ぶ。
            if suffix.surface.as_ref() == "票"
                && matches!(e, Ending::Thousand | Ending::TenThousand)
                && has_reading(suffix, &["ヒョウ", "ヒョー"])
            {
                set_initial(suffix, 'ビ');
            } else if matches!(e, Ending::Thousand | Ending::TenThousand) {
                // 三の系列が確認できた語だけ延長し、元のB/P異読は保持する。
                if three
                    && suffix.reading.starts_with(plain)
                    && suffix.pronunciation.starts_with(plain)
                {
                    set_initial(suffix, half);
                }
            } else if e != Ending::Three || three {
                apply_initial_h(number, suffix, e, plain, half, half);
            }
        }
        Counter::VoicedH {
            plain,
            voiced,
            half,
        } => {
            if matches!(e, Ending::Three | Ending::Thousand | Ending::TenThousand) {
                if suffix.reading.starts_with(plain) && suffix.pronunciation.starts_with(plain) {
                    set_initial(suffix, voiced);
                }
            } else {
                apply_initial_h(number, suffix, e, plain, voiced, half);
            }
        }
        Counter::Ken => {
            if e == Ending::Three {
                set_readings(suffix, "ゲン", "ゲン");
            } else {
                geminate(number, e);
            }
        }
        Counter::MetricK => {
            // 一キロはイチ、八キロはハチ/ハッが併存。確実な六・十・百だけ補う。
            if matches!(e, Ending::Six | Ending::Ten | Ending::Hundred) {
                geminate(number, e);
            }
        }
        Counter::Plain => {}
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
        Counter::Box => {
            if let Some(prefix) = native {
                set_readings(number, prefix, prefix);
                set_readings(suffix, "ハコ", "ハコ");
            } else if e == Ending::Ten && geminate(number, e) {
                // 六・八にはハコ/パコの併存がある。ここでは十箱だけを補う。
                set_readings(suffix, "パコ", "パコ");
            }
        }
        Counter::NativePair(tail) => {
            if let Some(prefix) = native {
                set_readings(number, prefix, prefix);
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
            if let Some((r, p)) = day {
                if whole_day {
                    set_readings(number, r, p);
                    set_readings(suffix, "カ", "カ");
                } else if replace_tail(number, tails, r, p) {
                    set_readings(suffix, "カ", "カ");
                }
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
        Counter::DaySpan => {
            // 日間を一語にした辞書でも、日の確定済みの和語読みだけを再利用する。
            // 一日間や十七日間など、native_dayの対象外には普通読みの補正もしない。
            if let Some((r, p)) = day {
                if whole_day {
                    set_readings(number, r, p);
                    set_readings(suffix, "カカン", "カカン");
                } else if replace_tail(number, tails, r, p) {
                    set_readings(suffix, "カカン", "カカン");
                }
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
        // 都府県・都道府県・府県等の分類名は「都」「府」の個数ではない。
        // 数字が続く一都二府のような列挙は抑止せず、数詞の合成自体も変えない。
        if matches!(tokens[i].surface.as_ref(), "都" | "府" | "県")
            && tokens.get(i + 1).is_some_and(|next| {
                tokens[i].end == next.start && next.surface.starts_with(['都', '道', '府', '県'])
            })
        {
            continue;
        }
        // 「一点五」「十点零」の点は数量の助数詞ではない。
        // 後続数詞にもafter_decimalの保護が働くよう、元の点と数詞を保持する。
        if tokens[i].surface.as_ref() == "点"
            && tokens
                .get(i + 1)
                .is_some_and(|next| tokens[i].end == next.start && is_numeral(next))
        {
            continue;
        }
        let Some(counter) = counter(&tokens[i]) else {
            continue;
        };
        if matches!(counter, Counter::DaySpan)
            && protected
                .iter()
                .skip(run.protection)
                .take_while(|range| range.start < tokens[i].end)
                .any(|range| tokens[i].start < range.end)
        {
            continue;
        }
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
    fn expanded_counter_families_preserve_fields_and_pronunciation_tails() {
        // 国語研 qa-225、国語研日本語インドネシア語辞典、成田(1998)の系列と異読。
        for (n, nr, np, c, cr, cp, pos, er, ep) in [
            (
                "1",
                "イチ",
                "イチ",
                "軒",
                "ケン",
                "ケン",
                COUNTER,
                "イッケン",
                "イッケン",
            ),
            (
                "3",
                "サン",
                "サン",
                "軒",
                "ケン",
                "ケン",
                COUNTER,
                "サンゲン",
                "サンゲン",
            ),
            (
                "千",
                "セン",
                "セン",
                "軒",
                "ケン",
                "ケン",
                COUNTER,
                "センケン",
                "センケン",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "校",
                "コウ",
                "コー",
                COUNTER,
                "イッコウ",
                "イッコー",
            ),
            (
                "6",
                "ロク",
                "ロク",
                "校",
                "コウ",
                "コー",
                COUNTER,
                "ロッコウ",
                "ロッコー",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "巻",
                "カン",
                "カン",
                COUNTER,
                "ジュッカン",
                "ジュッカン",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "機",
                "キ",
                "キ",
                COUNTER,
                "ジュッキ",
                "ジュッキ",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "箇所",
                "カショ",
                "カショ",
                COUNTER,
                "イッカショ",
                "イッカショ",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "拍",
                "ハク",
                "ハク",
                COUNTER,
                "イッパク",
                "イッパク",
            ),
            (
                "3",
                "サン",
                "サン",
                "泊",
                "ハク",
                "ハク",
                COUNTER,
                "サンパク",
                "サンパク",
            ),
            (
                "4",
                "ヨン",
                "ヨン",
                "泊",
                "パク",
                "パク",
                COUNTER,
                "ヨンパク",
                "ヨンパク",
            ),
            (
                "6",
                "ロク",
                "ロク",
                "発",
                "ハツ",
                "ハツ",
                COUNTER,
                "ロッパツ",
                "ロッパツ",
            ),
            (
                "3",
                "サン",
                "サン",
                "票",
                "ヒョウ",
                "ヒョー",
                COUNTER,
                "サンピョウ",
                "サンピョー",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "票",
                "ヒョウ",
                "ヒョー",
                COUNTER,
                "ジュッピョウ",
                "ジュッピョー",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "俵",
                "ヒョウ",
                "ヒョー",
                COUNTER,
                "イッピョウ",
                "イッピョー",
            ),
            (
                "3",
                "サン",
                "サン",
                "俵",
                "ヒョウ",
                "ヒョー",
                COUNTER,
                "サンビョウ",
                "サンビョー",
            ),
            (
                "2",
                "ニ",
                "ニ",
                "筆",
                "ピツ",
                "ピツ",
                COUNTER,
                "ニヒツ",
                "ニヒツ",
            ),
            (
                "3",
                "サン",
                "サン",
                "筆",
                "ヒツ",
                "ヒツ",
                COUNTER,
                "サンピツ",
                "サンピツ",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "品",
                "ヒン",
                "ヒン",
                COUNTER,
                "イッピン",
                "イッピン",
            ),
            (
                "3",
                "サン",
                "サン",
                "波",
                "ハ",
                "ハ",
                COUNTER,
                "サンパ",
                "サンパ",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "敗",
                "ハイ",
                "ハイ",
                COUNTER,
                "ジュッパイ",
                "ジュッパイ",
            ),
            (
                "3",
                "サン",
                "サン",
                "遍",
                "ヘン",
                "ヘン",
                COUNTER,
                "サンベン",
                "サンベン",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "服",
                "フク",
                "フク",
                COUNTER,
                "イップク",
                "イップク",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "都",
                "ト",
                "ト",
                "名詞,接尾,地域,*",
                "イット",
                "イット",
            ),
            (
                "3",
                "サン",
                "サン",
                "府",
                "フ",
                "フ",
                "名詞,接尾,地域,*",
                "サンプ",
                "サンプ",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "県",
                "ケン",
                "ケン",
                "名詞,接尾,地域,*",
                "ジュッケン",
                "ジュッケン",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "号",
                "ゴウ",
                "ゴー",
                "名詞,接尾,一般,*",
                "ジュウゴウ",
                "ジューゴー",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "棟",
                "トウ",
                "トー",
                COUNTER,
                "イットウ",
                "イットー",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "体",
                "タイ",
                "タイ",
                COUNTER,
                "イッタイ",
                "イッタイ",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "束",
                "ソク",
                "ソク",
                COUNTER,
                "イッソク",
                "イッソク",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "隻",
                "セキ",
                "セキ",
                COUNTER,
                "イッセキ",
                "イッセキ",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "席",
                "セキ",
                "セキ",
                COUNTER,
                "ジュッセキ",
                "ジュッセキ",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "勝",
                "ショウ",
                "ショー",
                COUNTER,
                "ジュッショウ",
                "ジュッショー",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "戦",
                "セン",
                "セン",
                COUNTER,
                "ジュッセン",
                "ジュッセン",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "センチ",
                "センチ",
                "センチ",
                COUNTER,
                "イッセンチ",
                "イッセンチ",
            ),
            (
                "6",
                "ロク",
                "ロク",
                "センチ",
                "センチ",
                "センチ",
                COUNTER,
                "ロクセンチ",
                "ロクセンチ",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "キロ",
                "キロ",
                "キロ",
                COUNTER,
                "イチキロ",
                "イチキロ",
            ),
            (
                "6",
                "ロク",
                "ロク",
                "キロ",
                "キロ",
                "キロ",
                COUNTER,
                "ロッキロ",
                "ロッキロ",
            ),
            (
                "8",
                "ハチ",
                "ハチ",
                "キロ",
                "キロ",
                "キロ",
                COUNTER,
                "ハチキロ",
                "ハチキロ",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "キログラム",
                "キログラム",
                "キログラム",
                COUNTER,
                "ジュッキログラム",
                "ジュッキログラム",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "箱",
                "ハコ",
                "ハコ",
                COUNTER,
                "ヒトハコ",
                "ヒトハコ",
            ),
            (
                "2",
                "ニ",
                "ニ",
                "箱",
                "ハコ",
                "ハコ",
                COUNTER,
                "フタハコ",
                "フタハコ",
            ),
            (
                "6",
                "ロク",
                "ロク",
                "箱",
                "ハコ",
                "ハコ",
                COUNTER,
                "ロクハコ",
                "ロクハコ",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "箱",
                "ハコ",
                "ハコ",
                COUNTER,
                "ジュッパコ",
                "ジュッパコ",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "皿",
                "サラ",
                "サラ",
                COUNTER,
                "ヒトサラ",
                "ヒトサラ",
            ),
            (
                "2",
                "ニ",
                "ニ",
                "束",
                "タバ",
                "タバ",
                COUNTER,
                "フタタバ",
                "フタタバ",
            ),
            (
                "21",
                "ニジュウイチ",
                "ニジューイチ",
                "皿",
                "サラ",
                "サラ",
                COUNTER,
                "ニジュウイチサラ",
                "ニジューイチサラ",
            ),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), (c, pos, cr)]);
            out[0].pronunciation = np.into();
            out[1].pronunciation = cp.into();
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (er.into(), ep.into()), "{n}{c}");
            assert_eq!(non_reading_fields(&out), fields);
            let once = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), once, "repeat {n}{c}");
        }
    }

    #[test]
    fn new_counter_families_keep_names_explicit_readings_ids_zero_and_decimals() {
        for (c, pos, cr) in [
            ("拍", COUNTER, "ハク"),
            ("票", COUNTER, "ヒョウ"),
            ("俵", COUNTER, "ヒョウ"),
            ("遍", COUNTER, "ヘン"),
            ("軒", COUNTER, "ケン"),
            ("校", COUNTER, "コウ"),
            ("棟", COUNTER, "トウ"),
            ("体", COUNTER, "タイ"),
            ("都", "名詞,接尾,地域,*", "ト"),
            ("府", "名詞,接尾,地域,*", "フ"),
            ("県", "名詞,接尾,地域,*", "ケン"),
            ("号", "名詞,接尾,一般,*", "ゴウ"),
            ("服", "名詞,一般,*,*", "フク"),
            ("箱", COUNTER, "ハコ"),
            ("皿", COUNTER, "サラ"),
            ("束", COUNTER, "タバ"),
            ("センチ", COUNTER, "センチ"),
            ("キロ", COUNTER, "キロ"),
        ] {
            for defect in 0..8 {
                let mut out = tokens(&[("1", NUMBER, "イチ"), (c, pos, cr)]);
                match defect {
                    0 => out[1].pos = "名詞,固有名詞,人名,名".into(),
                    1 => {
                        out[1].reading = "ベツヨミ".into();
                        out[1].pronunciation = "ベツヨミ".into();
                    }
                    2 => {
                        out[0].reading = "ベツヨミ".into();
                        out[0].pronunciation = "ベツヨミ".into();
                    }
                    3 => {
                        out[1].start += 1;
                        out[1].end += 1;
                    }
                    4 => {
                        out[0].surface = "0".into();
                        out[0].reading = "ゼロ".into();
                        out[0].pronunciation = "ゼロ".into();
                    }
                    5 => {}
                    6 => {
                        out = tokens(&[
                            ("1", NUMBER, "イチ"),
                            ("点", COUNTER, "テン"),
                            ("1", NUMBER, "イチ"),
                            (c, pos, cr),
                        ]);
                    }
                    _ => {
                        out[1].pronunciation = "ベツヨミ".into();
                    }
                }
                let before = format!("{out:?}");
                if defect == 5 {
                    let protected = out[0].start..out[0].end;
                    apply_counter_readings_with_protection(&mut out, &[protected]);
                } else {
                    apply_counter_readings(&mut out);
                }
                assert_eq!(format!("{out:?}"), before, "{c} defect {defect}");
            }
        }
        // 既知のB/P異読と、ムネ・テイ・シナなど別の数え方を推測で上書きしない。
        for (c, reading) in [
            ("俵", "ビョウ"),
            ("俵", "ピョウ"),
            ("票", "ビョウ"),
            ("棟", "ムネ"),
            ("体", "テイ"),
            ("品", "シナ"),
        ] {
            let mut out = tokens(&[("3", NUMBER, "サン"), (c, COUNTER, reading)]);
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before, "{c}:{reading}");
        }
    }

    #[test]
    fn decimal_point_is_not_geminated_as_a_point_counter() {
        for (surface, reading, pronunciation) in
            [("一", "イチ", "イチ"), ("十", "ジュウ", "ジュー")]
        {
            let mut out = tokens(&[
                (surface, NUMBER, reading),
                ("点", COUNTER, "テン"),
                ("五", NUMBER, "ゴ"),
                ("本", COUNTER, "ホン"),
            ]);
            out[0].pronunciation = pronunciation.into();
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before);
        }
        for after in [None, Some(("集中", "名詞,サ変接続,*,*", "シュウチュウ"))] {
            let mut out = tokens(&[("一", NUMBER, "イチ"), ("点", COUNTER, "テン")]);
            if let Some(after) = after {
                out.extend(tokens(&[after]));
            }
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(out[0].reading.as_ref(), "イッ");
            assert_eq!(out[0].pronunciation.as_ref(), "イッ");
            assert_eq!(non_reading_fields(&out), fields);
        }
    }

    #[test]
    fn documented_nasal_endings_keep_long_vowels_and_explicit_variants() {
        // 道浦(2009)の放送用表「万票」、自治体花火教材の「千発」「万発」。
        for (n, nr, np, c, cr, cp, er, ep) in [
            (
                "10000",
                "イチマン",
                "イチマン",
                "票",
                "ヒョウ",
                "ヒョー",
                "イチマンビョウ",
                "イチマンビョー",
            ),
            (
                "1000000",
                "ヒャクマン",
                "ヒャクマン",
                "票",
                "ヒョウ",
                "ヒョー",
                "ヒャクマンビョウ",
                "ヒャクマンビョー",
            ),
            (
                "千",
                "セン",
                "セン",
                "発",
                "ハツ",
                "ハツ",
                "センパツ",
                "センパツ",
            ),
            (
                "万",
                "マン",
                "マン",
                "発",
                "ハツ",
                "ハツ",
                "マンパツ",
                "マンパツ",
            ),
            (
                "10000",
                "イチマン",
                "イチマン",
                "票",
                "ピョウ",
                "ピョー",
                "イチマンピョウ",
                "イチマンピョー",
            ),
            (
                "10000",
                "イチマン",
                "イチマン",
                "票",
                "ビョウ",
                "ビョー",
                "イチマンビョウ",
                "イチマンビョー",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "篇",
                "ヘン",
                "ヘン",
                "イッペン",
                "イッペン",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "編",
                "ヘン",
                "ヘン",
                "イッペン",
                "イッペン",
            ),
            (
                "3",
                "サン",
                "サン",
                "編",
                "ヘン",
                "ヘン",
                "サンヘン",
                "サンヘン",
            ),
            (
                "1",
                "イチ",
                "イチ",
                "石",
                "コク",
                "コク",
                "イッコク",
                "イッコク",
            ),
            (
                "3",
                "サン",
                "サン",
                "石",
                "コク",
                "コク",
                "サンコク",
                "サンコク",
            ),
            (
                "3",
                "サン",
                "サン",
                "石",
                "ゴク",
                "ゴク",
                "サンゴク",
                "サンゴク",
            ),
        ] {
            let mut out = tokens(&[(n, NUMBER, nr), (c, COUNTER, cr)]);
            out[0].pronunciation = np.into();
            out[1].pronunciation = cp.into();
            let before = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (er.into(), ep.into()), "{n}{c}");
            assert_eq!(non_reading_fields(&out), before);
            let once = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), once);
        }
    }

    #[test]
    fn thousand_and_myriad_follow_the_reviewed_three_series_only() {
        for (c, r, p, target, target_p) in [
            ("拍", "ハク", "ハク", "パク", "パク"),
            ("泊", "ハク", "ハク", "パク", "パク"),
            ("筆", "ヒツ", "ヒツ", "ピツ", "ピツ"),
            ("品", "ヒン", "ヒン", "ピン", "ピン"),
            ("波", "ハ", "ハ", "パ", "パ"),
            ("敗", "ハイ", "ハイ", "パイ", "パイ"),
            ("府", "フ", "フ", "プ", "プ"),
            ("発", "ハツ", "ハツ", "パツ", "パツ"),
            ("票", "ヒョウ", "ヒョー", "ビョウ", "ビョー"),
            ("俵", "ヒョウ", "ヒョー", "ビョウ", "ビョー"),
            ("遍", "ヘン", "ヘン", "ベン", "ベン"),
        ] {
            for (n, nr) in [("千", "セン"), ("万", "マン")] {
                let mut out = tokens(&[(n, NUMBER, nr), (c, COUNTER, r)]);
                out[1].pronunciation = p.into();
                let before = non_reading_fields(&out);
                apply_counter_readings(&mut out);
                assert_eq!(
                    readings(&out),
                    (format!("{nr}{target}"), format!("{nr}{target_p}")),
                    "{n}{c}"
                );
                assert_eq!(non_reading_fields(&out), before);
            }
        }
        for (c, r, p) in [
            ("票", "ピョウ", "ピョー"),
            ("票", "ビョウ", "ビョー"),
            ("俵", "ピョウ", "ピョー"),
            ("遍", "ペン", "ペン"),
            ("服", "フク", "フク"),
            ("篇", "ヘン", "ヘン"),
            ("編", "ヘン", "ヘン"),
        ] {
            let mut out = tokens(&[("万", NUMBER, "マン"), (c, COUNTER, r)]);
            out[1].pronunciation = p.into();
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before, "{c}:{r}");
        }
    }

    #[test]
    fn administrative_classification_compounds_keep_plain_numbers() {
        for (n, r, p, first, rest) in [
            ("41", "ヨンジュウイチ", "ヨンジューイチ", "都", "府県警"),
            ("11", "ジュウイチ", "ジューイチ", "都", "道府県"),
            ("1", "イチ", "イチ", "府", "県"),
            ("10", "ジュウ", "ジュー", "都", "府県"),
        ] {
            let mut out = tokens(&[
                (n, NUMBER, r),
                (
                    first,
                    "名詞,接尾,地域,*",
                    if first == "都" { "ト" } else { "フ" },
                ),
                (rest, "名詞,一般,*,*", "フケン"),
            ]);
            out[0].pronunciation = p.into();
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before, "{n}{first}{rest}");
        }
        let mut out = tokens(&[
            ("1", NUMBER, "イチ"),
            ("都", "名詞,接尾,地域,*", "ト"),
            ("2", NUMBER, "ニ"),
            ("府", "名詞,接尾,地域,*", "フ"),
            ("43", NUMBER, "ヨンジュウサン"),
            ("県", "名詞,接尾,地域,*", "ケン"),
        ]);
        let fields = non_reading_fields(&out);
        apply_counter_readings(&mut out);
        assert_eq!(out[0].reading.as_ref(), "イッ");
        assert_eq!(out[3].reading.as_ref(), "フ");
        assert_eq!(non_reading_fields(&out), fields);
        let mut out = tokens(&[
            ("1", NUMBER, "イチ"),
            ("府", "名詞,接尾,地域,*", "フ"),
            ("22", NUMBER, "ニジュウニ"),
            ("省庁", "名詞,一般,*,*", "ショウチョウ"),
        ]);
        apply_counter_readings(&mut out);
        assert_eq!(out[0].reading.as_ref(), "イッ");
        assert_eq!(out[1].reading.as_ref(), "プ");

        // 数量認可は残し、41のASCII合成と原コストの合計・位置は保持する。
        let text = "41都府県警";
        let mut out = tokens(&[
            ("4", NUMBER, "ヨン"),
            ("1", NUMBER, "イチ"),
            ("都", "名詞,接尾,地域,*", "ト"),
            ("府県警", "名詞,一般,*,*", "フケンケイ"),
        ]);
        let protected = apply_ascii_quantities(
            text,
            37,
            &mut out,
            0,
            &AsciiPlan::new(text, 37),
            &mut 0,
            |_| false,
        );
        let merged = format!("{out:?}");
        apply_counter_readings_with_protection(&mut out, &protected);
        assert_eq!(format!("{out:?}"), merged);
        assert_eq!(out[0].surface.as_ref(), "41");
        assert_eq!(out[0].reading.as_ref(), "ヨンジュウイチ");
        assert_eq!(out[0].pronunciation.as_ref(), "ヨンジューイチ");
        assert_eq!(out[0].word_cost, 642);
        assert_eq!((out[0].start, out[0].end), (37, 39));
    }

    #[test]
    fn twenty_days_replace_one_safe_number_as_a_whole() {
        for (surface, known) in [("20", true), ("20", false), ("二十", false)] {
            let mut out = tokens(&[(surface, NUMBER, "ニジュウ"), ("日", COUNTER, "ニチ")]);
            out[0].pronunciation = "ニジュー".into();
            out[0].is_known = known;
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), ("ハツカ".into(), "ハツカ".into()));
            assert_eq!(non_reading_fields(&out), fields);
            let once = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), once);
        }
        for (surface, known) in [
            ("二十", true),
            ("２０", true),
            ("２０", false),
            ("二〇", false),
        ] {
            let mut out = tokens(&[(surface, NUMBER, "ニジュウ"), ("日", COUNTER, "ニチ")]);
            out[0].pronunciation = "ニジュー".into();
            out[0].is_known = known;
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before);
        }
        let mut split = tokens(&[
            ("二", NUMBER, "ニ"),
            ("十", NUMBER, "ジュウ"),
            ("日", COUNTER, "ニチ"),
        ]);
        let before = format!("{split:?}");
        apply_counter_readings(&mut split);
        assert_eq!(format!("{split:?}"), before);
        for prefix in ["A", "Ａ", "ID", "第"] {
            let mut out = tokens(&[
                (prefix, "名詞,一般,*,*", "エー"),
                ("20", NUMBER, "ニジュウ"),
                ("日", COUNTER, "ニチ"),
            ]);
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before, "{prefix}");
        }
        let mut out = tokens(&[
            ("20", NUMBER, "ニジュウ"),
            ("日", COUNTER, "ニチ"),
            ("B", "名詞,一般,*,*", "ビー"),
        ]);
        let before = format!("{out:?}");
        apply_counter_readings(&mut out);
        assert_eq!(format!("{out:?}"), before);
        let mut out = tokens(&[("20", NUMBER, "ニジュウ"), ("日", COUNTER, "ニチ")]);
        let before = format!("{out:?}");
        let range = out[0].start..out[0].end;
        apply_counter_readings_with_protection(&mut out, &[range]);
        assert_eq!(format!("{out:?}"), before);
    }

    #[test]
    fn twenty_day_spans_require_safe_numbers_and_standard_known_suffixes() {
        for (surface, reading, expected_r, expected_p) in [
            ("2", "ニ", "フツカカン", "フツカカン"),
            ("3", "サン", "ミッカカン", "ミッカカン"),
            ("4", "ヨン", "ヨッカカン", "ヨッカカン"),
            ("5", "ゴ", "イツカカン", "イツカカン"),
            ("6", "ロク", "ムイカカン", "ムイカカン"),
            ("7", "ナナ", "ナノカカン", "ナノカカン"),
            ("8", "ハチ", "ヨウカカン", "ヨーカカン"),
            ("9", "キュウ", "ココノカカン", "ココノカカン"),
            ("10", "ジュウ", "トオカカン", "トーカカン"),
            ("14", "ジュウヨン", "ジュウヨッカカン", "ジュウヨッカカン"),
            (
                "24",
                "ニジュウヨン",
                "ニジュウヨッカカン",
                "ニジュウヨッカカン",
            ),
        ] {
            let mut out = tokens(&[(surface, NUMBER, reading), ("日間", COUNTER, "ニチカン")]);
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(
                readings(&out),
                (expected_r.into(), expected_p.into()),
                "{surface}"
            );
            assert_eq!(non_reading_fields(&out), fields);
        }
        for (surface, known) in [("20", true), ("20", false), ("二十", false)] {
            let mut out = tokens(&[(surface, NUMBER, "ニジュウ"), ("日間", COUNTER, "ニチカン")]);
            out[0].pronunciation = "ニジュー".into();
            out[0].is_known = known;
            let fields = non_reading_fields(&out);
            apply_counter_readings(&mut out);
            assert_eq!(readings(&out), ("ハツカカン".into(), "ハツカカン".into()));
            assert_eq!(non_reading_fields(&out), fields);
            let once = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), once);
        }
        for (surface, reading, known) in [
            ("二十", "ニジュウ", true),
            ("２０", "ニジュウ", false),
            ("二〇", "ニジュウ", false),
            ("1", "イチ", true),
            ("11", "ジュウイチ", true),
            ("17", "ジュウナナ", true),
            ("21", "ニジュウイチ", true),
            ("30", "サンジュウ", true),
        ] {
            let mut out = tokens(&[(surface, NUMBER, reading), ("日間", COUNTER, "ニチカン")]);
            out[0].is_known = known;
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before, "{surface}");
        }
        for mutation in 0..8 {
            let mut out = tokens(&[("20", NUMBER, "ニジュウ"), ("日間", COUNTER, "ニチカン")]);
            match mutation {
                0 => out[1].is_known = false,
                1 => out[1].pos = "名詞,一般,*,*".into(),
                2 => out[1].reading = "ヒアイ".into(),
                3 => out[1].pronunciation = "ヒアイ".into(),
                4 => out[0].pos = "名詞,固有名詞,人名,名".into(),
                5 => out[0].base_form = "ordinary".into(),
                6 => out[1].surface = "間".into(),
                _ => out[0].reading = "フタトオ".into(),
            }
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            // 原形は数値認識の条件ではなく、変更しないフィールドとして扱う。
            if mutation == 5 {
                assert_eq!(readings(&out), ("ハツカカン".into(), "ハツカカン".into()));
                assert_eq!(out[0].base_form.as_ref(), "ordinary");
            } else {
                assert_eq!(format!("{out:?}"), before, "mutation={mutation}");
            }
        }
        for prefix in ["A", "Ａ", "ID", "第"] {
            let mut out = tokens(&[
                (prefix, "名詞,一般,*,*", "エー"),
                ("20", NUMBER, "ニジュウ"),
                ("日間", COUNTER, "ニチカン"),
            ]);
            let before = format!("{out:?}");
            apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), before, "{prefix}");
        }
        for protected_index in 0..2 {
            let mut out = tokens(&[("20", NUMBER, "ニジュウ"), ("日間", COUNTER, "ニチカン")]);
            let before = format!("{out:?}");
            let range = out[protected_index].start..out[protected_index].end;
            apply_counter_readings_with_protection(&mut out, &[range]);
            assert_eq!(format!("{out:?}"), before);
        }
        let mut out = tokens(&[
            ("20", NUMBER, "ニジュウ"),
            ("日間", COUNTER, "ニチカン"),
            ("B", "名詞,一般,*,*", "ビー"),
        ]);
        let before = format!("{out:?}");
        apply_counter_readings(&mut out);
        assert_eq!(format!("{out:?}"), before);
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

#[cfg(all(test, feature = "build"))]
mod expanded_counter_analyzer_tests {
    use crate::dict::{DictBuilder, DictEntry};
    use crate::{Analyzer, Dictionary};
    const NUMBER: &str = "名詞,数,*,*";
    const COUNTER: &str = "名詞,接尾,助数詞,*";

    fn dictionary() -> Dictionary {
        let mut b = DictBuilder::new();
        for (s, pos, r, p) in [
            ("0", NUMBER, "ゼロ", "ゼロ"),
            ("1", NUMBER, "イチ", "イチ"),
            ("3", NUMBER, "サン", "サン"),
            ("5", NUMBER, "ゴ", "ゴ"),
            ("6", NUMBER, "ロク", "ロク"),
            ("8", NUMBER, "ハチ", "ハチ"),
            ("１", NUMBER, "イチ", "イチ"),
            ("一", NUMBER, "イチ", "イチ"),
            ("十", NUMBER, "ジュウ", "ジュー"),
            ("五", NUMBER, "ゴ", "ゴ"),
            ("零", NUMBER, "レイ", "レイ"),
            ("六", NUMBER, "ロク", "ロク"),
            ("点", COUNTER, "テン", "テン"),
            ("本", COUNTER, "ホン", "ホン"),
            ("拍", COUNTER, "ハク", "ハク"),
            ("票", COUNTER, "ヒョウ", "ヒョー"),
            ("号", "名詞,接尾,一般,*", "ゴウ", "ゴー"),
            ("府", "名詞,接尾,地域,*", "フ", "フ"),
            ("県", "名詞,接尾,地域,*", "ケン", "ケン"),
            ("都", "名詞,接尾,地域,*", "ト", "ト"),
            ("服", "名詞,一般,*,*", "フク", "フク"),
            ("キログラム", COUNTER, "キログラム", "キログラム"),
            ("キロメートル", COUNTER, "キロメートル", "キロメートル"),
            ("センチ", COUNTER, "センチ", "センチ"),
            ("kg", "名詞,一般,*,*", "キログラム", "キログラム"),
            ("㎏", COUNTER, "キログラム", "キログラム"),
            ("校長", "名詞,一般,*,*", "コウチョウ", "コーチョー"),
            ("号令", "名詞,サ変接続,*,*", "ゴウレイ", "ゴーレイ"),
            ("府中", "名詞,固有名詞,地域,一般", "フチュウ", "フチュー"),
            ("県警", "名詞,一般,*,*", "ケンケイ", "ケンケイ"),
            ("服部", "名詞,固有名詞,人名,姓", "ハットリ", "ハットリ"),
            ("A", "名詞,一般,*,*", "エー", "エー"),
            ("ID", "名詞,一般,*,*", "アイディー", "アイディー"),
        ] {
            b.add_entry(DictEntry {
                surface: s.into(),
                left_id: 1,
                right_id: 1,
                cost: 0,
                pos: pos.into(),
                base_form: s.into(),
                reading: r.into(),
                pronunciation: p.into(),
                ..Default::default()
            });
        }
        b.build().unwrap()
    }

    #[test]
    fn real_lattice_quantities_use_new_families_and_keep_decimal_points() {
        let mut a = Analyzer::from_dict(dictionary());
        for (text, r, p) in [
            ("10拍", "ジュッパク", "ジュッパク"),
            ("100票", "ヒャッピョウ", "ヒャッピョー"),
            ("10号", "ジュウゴウ", "ジューゴー"),
            ("1000号", "センゴウ", "センゴー"),
            ("10府", "ジュップ", "ジュップ"),
            ("10県", "ジュッケン", "ジュッケン"),
            ("1都", "イット", "イット"),
            ("六キログラム", "ロッキログラム", "ロッキログラム"),
            ("六キロメートル", "ロッキロメートル", "ロッキロメートル"),
            ("6kg", "ロッキログラム", "ロッキログラム"),
            ("6㎏", "ロッキログラム", "ロッキログラム"),
            ("1センチ", "イッセンチ", "イッセンチ"),
            ("1.5本", "イチテンゴホン", "イチテンゴホン"),
            ("一点五本", "イチテンゴホン", "イチテンゴホン"),
            ("十点零本", "ジュウテンレイホン", "ジューテンレイホン"),
        ] {
            let tokens = a.try_tokenize(text).unwrap();
            assert_eq!(
                tokens
                    .iter()
                    .map(|t| t.reading.as_ref())
                    .collect::<String>(),
                r,
                "{text}: {tokens:?}"
            );
            assert_eq!(
                tokens
                    .iter()
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                p,
                "{text}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
            let sentences = a
                .try_tokenize_sentences(text, &crate::sentence::SplitOptions::default())
                .unwrap();
            assert_eq!(
                sentences
                    .iter()
                    .flat_map(|(_, v)| v)
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                p
            );
        }
    }

    #[test]
    fn identifiers_names_and_noun_beginnings_are_not_quantity_suffixes() {
        let mut a = Analyzer::from_dict(dictionary());
        for (text, word, reading) in [
            ("1校長", "校長", "コウチョウ"),
            ("1号令", "号令", "ゴウレイ"),
            ("1府中", "府中", "フチュウ"),
            ("1県警", "県警", "ケンケイ"),
            ("1服部", "服部", "ハットリ"),
            ("1服", "服", "フク"),
            ("A1拍", "拍", "ハク"),
            ("ID１票", "票", "ヒョウ"),
        ] {
            let v = a.try_tokenize(text).unwrap();
            assert_eq!(
                v.iter()
                    .find(|t| t.surface.as_ref() == word)
                    .unwrap()
                    .reading
                    .as_ref(),
                reading,
                "{text}: {v:?}"
            );
            assert!(
                v.iter()
                    .filter(|t| matches!(t.surface.as_ref(), "1" | "１"))
                    .all(|t| t.reading.as_ref() == "イチ")
            );
        }
    }
}
