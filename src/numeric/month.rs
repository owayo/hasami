//! 水平空白を挟む暦月と、位を省いた「一一月」の標準読み。

use super::{AsciiPlan, digit, has_reading, is_numeral, place, pos_has, set_readings};
use crate::lattice::Token;
use std::ops::Range;
use std::sync::Arc;

const MONTHS: [([&str; 3], &str, &str); 12] = [
    (["1", "１", "一"], "イチ", "イチ"),
    (["2", "２", "二"], "ニ", "ニ"),
    (["3", "３", "三"], "サン", "サン"),
    (["4", "４", "四"], "シ", "シ"),
    (["5", "５", "五"], "ゴ", "ゴ"),
    (["6", "６", "六"], "ロク", "ロク"),
    (["7", "７", "七"], "シチ", "シチ"),
    (["8", "８", "八"], "ハチ", "ハチ"),
    (["9", "９", "九"], "ク", "ク"),
    (["10", "１０", "十"], "ジュウ", "ジュー"),
    (["11", "１１", "十一"], "ジュウイチ", "ジューイチ"),
    (["12", "１２", "十二"], "ジュウニ", "ジューニ"),
];

fn horizontal(c: char) -> bool {
    matches!(c, ' ' | '\t')
}

const FRAME_LABELS: [&str; 13] = [
    "識別子",
    "コード",
    "型番",
    "番号",
    "ID",
    "id",
    "ＩＤ",
    "ｉｄ",
    "名前",
    "名称",
    "ラベル",
    "変数名",
    "キー",
];

fn quote_end(c: char) -> Option<char> {
    match c {
        '「' => Some('」'),
        '『' => Some('』'),
        '“' => Some('”'),
        '‘' => Some('’'),
        '《' => Some('》'),
        '〈' => Some('〉'),
        '【' => Some('】'),
        '〝' => Some('〟'),
        '"' | '\'' | '`' => Some(c),
        _ => None,
    }
}

fn closing_quote(c: char) -> bool {
    matches!(
        c,
        '」' | '』' | '”' | '’' | '》' | '〉' | '】' | '〟' | '"' | '\'' | '`'
    )
}

fn identifier_neighbor(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || digit(c).is_some()
        || place(c).is_some()
        || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '_' | '.' | '．' | '点' | ',' | '，' | ':' | '：' | '/' | '／' | '∕' | '÷' | '×' | '+' | '＋' | '-' | '－' | '−' | '‐' | '‑' | '‒' | '–' | '—' | '―' | '=' | '*' | '^' | '%' | '第')
}

fn eleven_prefix_allowed(prefix: &str) -> bool {
    let prefix = prefix
        .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '(' | '[' | '（' | '［'));
    !prefix.chars().next_back().is_some_and(identifier_neighbor)
        && ![
            "電話",
            "電話番号",
            "番号",
            "型番",
            "郵便番号",
            "識別子",
            "コード",
            "バージョン",
        ]
        .iter()
        .any(|label| prefix.ends_with(label))
        && !FRAME_LABELS.iter().any(|label| {
            ["", "名", "は", "が", "を", "名は", "名が", "名を"]
                .iter()
                .any(|tail| {
                    prefix
                        .strip_suffix(tail)
                        .is_some_and(|p| p.ends_with(label))
                })
        })
}

/// 続く日は境界の確認にだけ使い、そのトークンや読みは変更しない。
fn following_day(rest: &str) -> Option<(&str, &str)> {
    let end = rest
        .char_indices()
        .take_while(|(_, c)| {
            matches!(
                c,
                '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
            )
        })
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    Some((&rest[..end], rest[end..].strip_prefix('日')?))
}

fn day_value(raw: &str) -> Option<u64> {
    let value = if let Some((a, b)) = raw.split_once('十') {
        if a.chars().count() > 1 || b.chars().count() > 1 {
            return None;
        }
        let tens = if a.is_empty() {
            1
        } else {
            digit(a.chars().next()?)?
        };
        let ones = if b.is_empty() {
            0
        } else {
            digit(b.chars().next()?)?
        };
        tens * 10 + ones
    } else {
        if raw.chars().count() > 2 {
            return None;
        }
        raw.chars().try_fold(0, |n, c| Some(n * 10 + digit(c)?))?
    };
    (1..=31).contains(&value).then_some(value)
}

fn trailing_frame(rest: &str) -> bool {
    let rest = following_day(rest).map_or(rest, |(_, tail)| tail);
    let rest = rest
        .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ')' | ']' | '）' | '］'));
    rest.starts_with("と名乗る")
        || ["という", "と呼ぶ", "と呼ばれる", "と名付けた", "は", "が"]
            .iter()
            .any(|frame| {
                rest.strip_prefix(frame)
                    .is_some_and(|tail| FRAME_LABELS.iter().any(|label| tail.starts_with(label)))
            })
}

fn eleven_right_allowed(rest: &str, includes_day: bool) -> bool {
    if trailing_frame(rest)
        || rest.trim_start().chars().next().is_some_and(closing_quote)
        || [
            "さん",
            "様",
            "君",
            "氏",
            "ちゃん",
            "間",
            "曜",
            "額",
            "見",
            "島",
            "極",
        ]
        .iter()
        .any(|suffix| rest.starts_with(suffix))
    {
        return false;
    }
    if rest.chars().next().is_some_and(identifier_neighbor) {
        if includes_day {
            return false;
        }
        let Some((day, tail)) = following_day(rest) else {
            return false;
        };
        // TTS の文末ピリオドだけは識別子の継続と区別する。
        let tail = if tail == "." { "" } else { tail };
        if day_value(day).is_none()
            || tail.chars().next().is_some_and(identifier_neighbor)
            || ["さん", "様", "君", "氏", "ちゃん"]
                .iter()
                .any(|suffix| tail.starts_with(suffix))
        {
            return false;
        }
    }
    let after_close = rest.trim_start_matches([')', ']', '）', '］']);
    !(after_close.len() != rest.len()
        && after_close.chars().next().is_some_and(|c| {
            c.is_ascii_alphanumeric() || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９' | '_')
        }))
}

/// 引用状態は候補間を一度ずつ走査する。チャンクの外側にある ID や引用も失わない。
fn add_eleven_starts(input: &str, offset: usize, starts: &mut Vec<usize>) {
    let (mut cursor, mut quotes) = (0, Vec::new());
    for (start, _) in input.match_indices("一一月") {
        for c in input[cursor..start].chars() {
            if quotes.last() == Some(&c) {
                quotes.pop();
            } else if let Some(end) = quote_end(c) {
                quotes.push(end);
            }
        }
        cursor = start;
        if quotes.is_empty()
            && eleven_prefix_allowed(&input[..start])
            && eleven_right_allowed(&input[start + "一一月".len()..], false)
        {
            starts.push(offset + start);
        }
    }
}

/// 前分割で小数点などの前半を失わないよう、入力全体で月名の数詞の開始位置を確定する。
pub(super) fn number_starts(input: &str, offset: usize) -> Vec<usize> {
    let mut starts = Vec::new();
    for (i, _) in input.match_indices('月') {
        let left = input[..i].trim_end_matches(horizontal);
        if left.len() == i {
            continue;
        }
        let prefix = left.trim_end_matches(|c| digit(c).is_some() || place(c).is_some());
        let raw = &left[prefix.len()..];
        if !MONTHS.iter().any(|(forms, _, _)| forms.contains(&raw)) {
            continue;
        }
        // 小数・番号・英字の末尾だけを拾わない。括弧を挟む識別子の前置も守る。
        let previous = prefix
            .trim_end_matches(|c| horizontal(c) || matches!(c, '(' | '[' | '（' | '［'))
            .chars()
            .next_back();
        if previous.is_some_and(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '_' | '.' | '．' | '点' | ',' | '，' | ':' | '/' | '／' | '∕' | '÷' | '×' | '+' | '＋' | '-' | '－' | '−' | '‐' | '‑' | '‒' | '–' | '—' | '―' | '=' | '*' | '^' | '%')
                || digit(c).is_some()
                || place(c).is_some()
        }) {
            continue;
        }
        starts.push(offset + prefix.len());
    }
    add_eleven_starts(input, offset, &mut starts);
    starts.sort_unstable();
    starts.dedup();
    starts
}

fn selected(token: &Token, surface: &str, pos: &str, reading: &str) -> bool {
    token.surface.as_ref() == surface
        && token.base_form.as_ref() == surface
        && token.pos.as_ref() == pos
        && token.is_known
        && token.conj_type.is_empty()
        && token.conj_form.is_empty()
        && token.reading.as_ref() == reading
        && token.pronunciation.as_ref() == reading
}

fn eleven_candidate(
    input: &str,
    offset: usize,
    tokens: &[Token],
    start: usize,
    plan: &AsciiPlan,
    protected: &[Range<usize>],
) -> Option<()> {
    let first = tokens.get(start)?;
    if !plan.month_number_start(first.start) || !selected(first, "一", "名詞,数,*,*", "イチ")
    {
        return None;
    }
    let second = tokens.get(start + 1)?;
    if first.end != second.start {
        return None;
    }
    let (last, includes_day) = if selected(second, "一月", "名詞,副詞可能,*,*", "イチガツ")
    {
        (start + 1, false)
    } else if selected(
        second,
        "一月一日",
        "名詞,固有名詞,一般,*",
        "イチガツツイタチ",
    ) {
        (start + 1, true)
    } else {
        let month = tokens.get(start + 2)?;
        if !selected(second, "一", "名詞,数,*,*", "イチ")
            || second.end != month.start
            || !["名詞,一般,*,*", "名詞,副詞可能,*,*", "名詞,接尾,助数詞,*"]
                .iter()
                .any(|pos| selected(month, "月", pos, "ガツ"))
        {
            return None;
        }
        (start + 2, false)
    };
    let end = tokens[last].end;
    let from = first.start.checked_sub(offset)?;
    let to = end.checked_sub(offset)?;
    if input.get(from..to)?
        != if includes_day {
            "一一月一日"
        } else {
            "一一月"
        }
        || protected
            .iter()
            .any(|p| p.start < end && first.start < p.end)
        || !eleven_right_allowed(input.get(to..)?, includes_day)
        || !tokens[start..=last].iter().all(|t| {
            let (Some(a), Some(b)) = (t.start.checked_sub(offset), t.end.checked_sub(offset))
            else {
                return false;
            };
            input.get(a..b) == Some(t.surface.as_ref())
        })
    {
        return None;
    }
    Some(())
}

fn standard_readings(token: &Token) -> bool {
    // 数詞として確定した未知語だけは、欠けた読みを救済できる。
    if !token.is_known
        && token.surface.is_ascii()
        && token.reading.is_empty()
        && token.pronunciation.is_empty()
    {
        return true;
    }
    let spelling: String = token
        .surface
        .chars()
        .map(|c| {
            digit(c).map_or(c, |n| {
                ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'][n as usize]
            })
        })
        .collect();
    // 既に一語の 10〜12 は位取りで、分かれた各桁は元の桁読みで照合する。
    let allowed = match spelling.as_str() {
        "一〇" => &["ジュウ", "ジュー"][..],
        "一一" | "十一" => &["ジュウイチ", "ジューイチ"],
        "一二" | "十二" => &["ジュウニ", "ジューニ"],
        _ => {
            return super::standard_kanji_readings(&spelling, &token.reading, &token.pronunciation);
        }
    };
    has_reading(token, allowed)
}

fn candidate<'a>(
    input: &'a str,
    offset: usize,
    tokens: &[Token],
    start: usize,
    end: usize,
    plan: &AsciiPlan,
) -> Option<(&'a str, &'static str, &'static str)> {
    let month = tokens.get(end)?;
    if month.surface.as_ref() != "月"
        || !(pos_has(&month.pos, "名詞,一般") || pos_has(&month.pos, "名詞,副詞可能"))
        || !has_reading(month, &["ツキ", "ガツ"])
    {
        return None;
    }
    let from = tokens[start].start.checked_sub(offset)?;
    let to = tokens[end - 1].end.checked_sub(offset)?;
    let month_start = month.start.checked_sub(offset)?;
    let gap = input.get(to..month_start)?;
    if gap.is_empty() || !gap.chars().all(horizontal) {
        return None;
    }
    let raw = input.get(from..to)?;
    let (_, reading, pronunciation) = MONTHS.iter().find(|(forms, _, _)| forms.contains(&raw))?;
    if !plan.month_number_start(tokens[start].start) {
        return None;
    }
    if raw.is_ascii() && !plan.bare_integer(tokens[start].start, tokens[end - 1].end) {
        return None;
    }
    // 「月間」「月曜」が別トークンでも期間・曜日のままにする。
    if input
        .get(month.end.checked_sub(offset)?..)?
        .starts_with(['間', '曜'])
        || !tokens[start..end].iter().all(standard_readings)
    {
        return None;
    }
    Some((raw, reading, pronunciation))
}

/// 今回のチャンクで追加した範囲だけを線形に圧縮する。前の出力・チャンクには触れない。
pub(crate) fn apply(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first: usize,
    plan: &AsciiPlan,
    protected: &[Range<usize>],
) {
    if !plan.has_month_in(offset, offset + input.len()) {
        return;
    }
    // 位を省いた暦月は境界を変えず、先頭「一」の r/p だけを補う。
    for start in first..out.len() {
        if eleven_candidate(input, offset, out, start, plan, protected).is_some() {
            set_readings(&mut out[start], "ジュウ", "ジュー");
        }
    }
    let (mut read, mut write) = (first, first);
    while read < out.len() {
        let start = read;
        read += 1;
        if is_numeral(&out[start]) {
            while read < out.len() && out[read - 1].end == out[read].start && is_numeral(&out[read])
            {
                read += 1;
            }
            if let Some((raw, reading, pronunciation)) =
                candidate(input, offset, out, start, read, plan)
            {
                if read - start == 1 {
                    set_readings(&mut out[start], reading, pronunciation);
                    out.swap(write, start);
                } else {
                    let cost: i64 = out[start..read]
                        .iter()
                        .map(|t| i64::from(t.word_cost))
                        .sum();
                    let surface: Arc<str> = Arc::from(raw);
                    out[write] = Token {
                        surface: Arc::clone(&surface),
                        base_form: surface,
                        start: out[start].start,
                        end: out[read - 1].end,
                        pos: Arc::from("名詞,数,*,*"),
                        conj_type: Arc::from(""),
                        conj_form: Arc::from(""),
                        reading: Arc::from(reading),
                        pronunciation: Arc::from(pronunciation),
                        word_cost: cost.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
                        is_known: false,
                    };
                }
                write += 1;
                set_readings(&mut out[read], "ガツ", "ガツ");
                out.swap(write, read);
                write += 1;
                read += 1;
                continue;
            }
        }
        for i in start..read {
            out.swap(write, i);
            write += 1;
        }
    }
    out.truncate(write);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(number: &str, reading: &str, pronunciation: &str) -> Vec<Token> {
        [
            (number, "名詞,数,*,*", reading, pronunciation),
            ("月", "名詞,一般,*,*", "ツキ", "ツキ"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (surface, pos, reading, pronunciation))| Token {
            surface: surface.into(),
            start: if i == 0 { 37 } else { 38 + number.len() },
            end: if i == 0 {
                37 + number.len()
            } else {
                41 + number.len()
            },
            pos: pos.into(),
            base_form: format!("base:{surface}").into(),
            conj_type: "fixture-type".into(),
            conj_form: "fixture-form".into(),
            reading: reading.into(),
            pronunciation: pronunciation.into(),
            word_cost: 321,
            is_known: true,
        })
        .collect()
    }

    #[test]
    fn single_number_and_month_keep_every_non_reading_field_and_are_idempotent() {
        for (number, reading, pronunciation, expected_r, expected_p) in [
            ("9", "キュウ", "キュー", "ク", "ク"),
            ("９", "キュウ", "キュー", "ク", "ク"),
            ("九", "キュウ", "キュー", "ク", "ク"),
            ("4", "ヨン", "ヨン", "シ", "シ"),
            ("7", "ナナ", "ナナ", "シチ", "シチ"),
            ("12", "ジュウニ", "ジューニ", "ジュウニ", "ジューニ"),
        ] {
            let text = format!("{number} 月");
            let mut out = tokens(number, reading, pronunciation);
            let before = out.clone();
            let plan = AsciiPlan::new(&text, 37);
            apply(&text, 37, &mut out, 0, &plan, &[]);
            assert_eq!(out[0].reading.as_ref(), expected_r);
            assert_eq!(out[0].pronunciation.as_ref(), expected_p);
            assert_eq!(out[1].reading.as_ref(), "ガツ");
            let once = format!("{out:?}");
            apply(&text, 37, &mut out, 0, &plan, &[]);
            assert_eq!(format!("{out:?}"), once);
            for (token, original) in out.iter_mut().zip(before) {
                token.reading = Arc::clone(&original.reading);
                token.pronunciation = Arc::clone(&original.pronunciation);
                assert_eq!(format!("{token:?}"), format!("{original:?}"));
            }
        }
    }

    #[test]
    fn unknown_ascii_numbers_can_gain_readings_but_known_unusual_readings_are_kept() {
        let text = "9 月";
        let plan = AsciiPlan::new(text, 37);
        let mut out = tokens("9", "", "");
        out[0].is_known = false;
        apply(text, 37, &mut out, 0, &plan, &[]);
        assert_eq!(out[0].reading.as_ref(), "ク");
        assert!(!out[0].is_known);
        for (reading, pronunciation) in [("", ""), ("キュウ", "ベツ"), ("ナイン", "ナイン")]
        {
            let mut out = tokens("9", reading, pronunciation);
            let before = format!("{out:?}");
            apply(text, 37, &mut out, 0, &plan, &[]);
            assert_eq!(format!("{out:?}"), before);
        }
        for index in [0, 1] {
            let mut out = tokens("9", "キュウ", "キュー");
            out[index].pos = "名詞,固有名詞,人名,名".into();
            let before = format!("{out:?}");
            apply(text, 37, &mut out, 0, &plan, &[]);
            assert_eq!(format!("{out:?}"), before);
        }
    }

    fn calendar_tokens(suffix: &str, pos: &str, reading: &str, offset: usize) -> Vec<Token> {
        [("一", "名詞,数,*,*", "イチ"), (suffix, pos, reading)]
            .into_iter()
            .scan(offset, |start, (surface, pos, reading)| {
                let token = Token {
                    surface: surface.into(),
                    start: *start,
                    end: *start + surface.len(),
                    pos: pos.into(),
                    conj_type: "".into(),
                    conj_form: "".into(),
                    base_form: surface.into(),
                    reading: reading.into(),
                    pronunciation: reading.into(),
                    word_cost: 321,
                    is_known: true,
                };
                *start = token.end;
                Some(token)
            })
            .collect()
    }

    fn split_calendar_tokens(pos: &str, offset: usize) -> Vec<Token> {
        let mut out = calendar_tokens("一", "名詞,数,*,*", "イチ", offset);
        out.push(Token {
            surface: "月".into(),
            start: offset + 6,
            end: offset + 9,
            pos: pos.into(),
            conj_type: "".into(),
            conj_form: "".into(),
            base_form: "月".into(),
            reading: "ガツ".into(),
            pronunciation: "ガツ".into(),
            word_cost: -17,
            is_known: true,
        });
        out
    }

    #[test]
    fn calendar_eleven_changes_only_first_reading_and_preserves_every_other_field() {
        for (suffix, pos, reading) in [
            ("一月", "名詞,副詞可能,*,*", "イチガツ"),
            ("一月一日", "名詞,固有名詞,一般,*", "イチガツツイタチ"),
        ] {
            let text = format!("一{suffix}");
            let plan = AsciiPlan::new(&text, 37);
            let mut out = calendar_tokens(suffix, pos, reading, 37);
            let before = out.clone();
            apply(&text, 37, &mut out, 0, &plan, &[]);
            assert_eq!(out.len(), before.len());
            assert_eq!(out[0].reading.as_ref(), "ジュウ");
            assert_eq!(out[0].pronunciation.as_ref(), "ジュー");
            assert_eq!(format!("{:?}", &out[1..]), format!("{:?}", &before[1..]));
            let once = format!("{out:?}");
            apply(&text, 37, &mut out, 0, &plan, &[]);
            assert_eq!(format!("{out:?}"), once);
            out[0].reading = Arc::clone(&before[0].reading);
            out[0].pronunciation = Arc::clone(&before[0].pronunciation);
            assert_eq!(format!("{out:?}"), format!("{before:?}"));
        }
        for pos in ["名詞,一般,*,*", "名詞,副詞可能,*,*", "名詞,接尾,助数詞,*"] {
            let mut out = split_calendar_tokens(pos, 37);
            let before = out.clone();
            apply(
                "一一月",
                37,
                &mut out,
                0,
                &AsciiPlan::new("一一月", 37),
                &[],
            );
            assert_eq!(out[0].pronunciation.as_ref(), "ジュー");
            out[0].reading = Arc::clone(&before[0].reading);
            out[0].pronunciation = Arc::clone(&before[0].pronunciation);
            assert_eq!(format!("{out:?}"), format!("{before:?}"));
        }
    }

    #[test]
    fn calendar_eleven_global_plan_preserves_quotes_explicit_frames_and_identifiers() {
        for text in [
            "「一一月」",
            "『一一月』",
            "\"一一月\"",
            "'一一月'",
            "`一一月`",
            "“一一月”",
            "‘一一月’",
            "《一一月》",
            "〈一一月〉",
            "【一一月】",
            "〝一一月〟",
            "「 一一月 」",
            "「日付は一一月」",
            "名前は一一月。",
            "名称は一一月。",
            "識別子は一一月。",
            "コードは一一月。",
            "IDは一一月。",
            "変数名は一一月。",
            "ラベルは（一一月）。",
            "一一月という識別子。",
            "一一月という名前。",
            "一一月という名称。",
            "一一月というコード。",
            "一一月というID。",
            "一一月という変数名。",
            "一一月と呼ぶ識別子。",
            "一一月と呼ばれる識別子。",
            "一一月と名付けたラベル。",
            "一一月と名乗る。",
            "一一月は識別子です。",
            "一一月一九日という名前。",
            "（一一月）という識別子。",
            "A一一月",
            "A 一一月",
            "A　一一月",
            "Ａ 一一月",
            "A（\n一一月）",
            "コード　（一一月）",
            "第一一月",
            "第 一一月",
            "番号一一月",
            "型番 一一月",
            "コード（一一月）",
            "一.一一月",
            "一点一一月",
            "一一一月",
            "〇一一月",
            "零一一月",
            "十一一月",
            "一一月A",
            "一一月Ａ",
            "一一月2",
            "一一月２",
            "一一月間",
            "一一月曜日",
            "一一月額",
            "一一月見",
            "一一月島",
            "一一月極",
            "一一月さん",
            "一一月」",
        ] {
            assert!(number_starts(text, 0).is_empty(), "{text}");
        }
        for text in [
            "一一月",
            "年一一月",
            "一一月に発売する。",
            "締切は（一一月）です。",
            "「資料」年一一月",
            "年一一月に「資料」を出す。",
            "一一月という月。",
            "一一月一九日。",
            "一一月一九日.",
        ] {
            assert_eq!(
                number_starts(text, 37),
                vec![37 + text.find("一一月").unwrap()],
                "{text}"
            );
        }
        assert_eq!(number_starts("「一一月」一一月「一一月」", 0), vec![15]);
    }

    #[test]
    fn calendar_eleven_requires_every_selected_standard_field_including_month_conjugation() {
        let mutations: [fn(&mut Token); 7] = [
            |t| t.pos = "名詞,固有名詞,人名,名".into(),
            |t| t.base_form = "別名".into(),
            |t| t.reading = "ベツヨミ".into(),
            |t| t.pronunciation = "ベツヨミ".into(),
            |t| t.is_known = false,
            |t| t.conj_type = "nonstandard-type".into(),
            |t| t.conj_form = "nonstandard-form".into(),
        ];
        let plan = AsciiPlan::new("一一月", 37);
        for index in 0..3 {
            for mutate in mutations {
                let mut out = split_calendar_tokens("名詞,一般,*,*", 37);
                mutate(&mut out[index]);
                let before = format!("{out:?}");
                apply("一一月", 37, &mut out, 0, &plan, &[]);
                assert_eq!(format!("{out:?}"), before, "index={index}");
            }
        }
        for (surface, reading) in [("一月", "ヒトツキ"), ("一月一日", "イチガツイチジツ")]
        {
            let text = format!("一{surface}");
            let mut out = calendar_tokens(surface, "名詞,固有名詞,一般,*", reading, 37);
            let before = format!("{out:?}");
            apply(&text, 37, &mut out, 0, &AsciiPlan::new(&text, 37), &[]);
            assert_eq!(format!("{out:?}"), before);
        }
        let mut out = split_calendar_tokens("名詞,一般,*,*", 37);
        out[2].reading = "ツキ".into();
        out[2].pronunciation = "ツキ".into();
        let before = format!("{out:?}");
        apply("一一月", 37, &mut out, 0, &plan, &[]);
        assert_eq!(format!("{out:?}"), before);
    }

    #[test]
    fn calendar_eleven_keeps_protected_suffixes_previous_output_and_incomplete_chunks() {
        let text = "一一月一日";
        let plan = AsciiPlan::new(text, 37);
        for range in [37..40, 40..46, 46..52] {
            let mut out =
                calendar_tokens("一月一日", "名詞,固有名詞,一般,*", "イチガツツイタチ", 37);
            let before = format!("{out:?}");
            apply(text, 37, &mut out, 0, &plan, &[range]);
            assert_eq!(format!("{out:?}"), before);
        }
        let mut out = calendar_tokens("一月", "名詞,副詞可能,*,*", "イチガツ", 37);
        let before = format!("{out:?}");
        apply(
            "一一月",
            37,
            &mut out,
            2,
            &AsciiPlan::new("一一月", 37),
            &[],
        );
        assert_eq!(format!("{out:?}"), before);
        let full = "一一月。一一月";
        let mut out = calendar_tokens("一月", "名詞,副詞可能,*,*", "イチガツ", 37);
        let previous = format!("{out:?}");
        out.extend(calendar_tokens("一月", "名詞,副詞可能,*,*", "イチガツ", 49));
        apply("一一月", 49, &mut out, 2, &AsciiPlan::new(full, 37), &[]);
        assert_eq!(format!("{:?}", &out[..2]), previous);
        assert_eq!(out[2].reading.as_ref(), "ジュウ");
        assert_eq!(out[2].start, 49);
        assert_eq!(out[3].end, 58);
        for full in ["A（\n一一月）", "「\n一一月」", "一一月という識別子。"] {
            let start = full.find("一一月").unwrap();
            let mut out = calendar_tokens("一月", "名詞,副詞可能,*,*", "イチガツ", 37 + start);
            let before = format!("{out:?}");
            apply(
                "一一月",
                37 + start,
                &mut out,
                0,
                &AsciiPlan::new(full, 37),
                &[],
            );
            assert_eq!(format!("{out:?}"), before);
        }
        for len in 0..3 {
            let mut out = split_calendar_tokens("名詞,一般,*,*", 37);
            out.truncate(len);
            let before = format!("{out:?}");
            apply(
                "一一月",
                37,
                &mut out,
                0,
                &AsciiPlan::new("一一月", 37),
                &[],
            );
            assert_eq!(format!("{out:?}"), before);
        }
        let mut out = calendar_tokens("一月", "名詞,副詞可能,*,*", "イチガツ", 37);
        let before = format!("{out:?}");
        apply("一一", 37, &mut out, 0, &AsciiPlan::new("一一月", 37), &[]);
        assert_eq!(format!("{out:?}"), before);
        out[0].start += 1;
        let before = format!("{out:?}");
        apply(
            "一一月",
            37,
            &mut out,
            0,
            &AsciiPlan::new("一一月", 37),
            &[],
        );
        assert_eq!(format!("{out:?}"), before);
    }
}

#[cfg(all(test, feature = "build"))]
mod analyzer_tests {
    use crate::dict::{DictBuilder, DictEntry};
    use crate::{Analyzer, Dictionary};

    fn dictionary(extra: Option<(&str, &str, &str)>) -> Dictionary {
        let mut builder = DictBuilder::new();
        for (surface, pos, reading, cost) in [
            ("一", "名詞,数,*,*", "イチ", 0),
            ("月", "名詞,一般,*,*", "ガツ", 0),
            ("年", "名詞,接尾,助数詞,*", "ネン", 0),
            ("日", "名詞,接尾,助数詞,*", "ニチ", 0),
            ("一月", "名詞,副詞可能,*,*", "イチガツ", -100),
            ("一月一日", "名詞,固有名詞,一般,*", "イチガツツイタチ", -200),
            ("一日", "名詞,副詞可能,*,*", "ツイタチ", -100),
            ("十一月", "名詞,副詞可能,*,*", "ジュウイチガツ", -1000),
            ("11月", "名詞,副詞可能,*,*", "ジュウイチガツ", -1000),
            ("１１月", "名詞,副詞可能,*,*", "ジュウイチガツ", -1000),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
                base_form: surface.into(),
                cost,
                ..Default::default()
            });
        }
        if let Some((surface, pos, reading)) = extra {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
                base_form: surface.into(),
                cost: -2000,
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    #[test]
    fn analyzer_calendar_eleven_keeps_boundaries_and_known_calendar_day() {
        let mut analyzer = Analyzer::from_dict(dictionary(None));
        for text in ["一一月", "年一一月", "一一月一日", "年一一月一日"] {
            let out = analyzer.try_tokenize(text).unwrap();
            let first = out.iter().position(|t| t.surface.as_ref() == "一").unwrap();
            assert_eq!(out[first].reading.as_ref(), "ジュウ");
            assert_eq!(out[first].pronunciation.as_ref(), "ジュー");
            assert_eq!(out[first].pos.as_ref(), "名詞,数,*,*");
            assert_eq!(out[first].base_form.as_ref(), "一");
            assert_eq!(out[first].word_cost, 0);
            assert!(out[first].is_known);
            let tail = &out[first + 1];
            assert!(matches!(tail.surface.as_ref(), "一月" | "一月一日"));
            assert_eq!(
                tail.reading.as_ref(),
                if text.ends_with('日') {
                    "イチガツツイタチ"
                } else {
                    "イチガツ"
                }
            );
            assert_eq!(tail.pronunciation, tail.reading);
            assert_eq!(tail.start, out[first].end);
            assert_eq!(tail.end, text.len());
        }
        for (text, reading) in [
            ("十一月", "ジュウイチガツ"),
            ("11月", "ジュウイチガツ"),
            ("１１月", "ジュウイチガツ"),
            ("一月", "イチガツ"),
            ("一日", "ツイタチ"),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].surface.as_ref(), text);
            assert_eq!(out[0].reading.as_ref(), reading);
        }
        for text in [
            "「一一月」",
            "A 一一月",
            "一一月という識別子。",
            "名前は一一月。",
            "年一一月という名前。",
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert!(
                !out.iter()
                    .any(|t| t.surface.as_ref() == "一" && t.reading.as_ref() == "ジュウ"),
                "{text}"
            );
        }
    }

    #[test]
    fn analyzer_calendar_eleven_preserves_selected_names_whole_words_and_explicit_readings() {
        for (surface, pos, reading) in [
            ("一一月", "名詞,固有名詞,人名,名", "ベツヨミ"),
            ("一", "名詞,固有名詞,人名,名", "ハジメ"),
            ("一月", "名詞,固有名詞,人名,姓", "イチガツ"),
            ("一月", "名詞,副詞可能,*,*", "ヒトツキ"),
            ("一月一日", "名詞,固有名詞,一般,*", "イチガツイチジツ"),
        ] {
            let text = if surface == "一月一日" {
                "一一月一日"
            } else {
                "一一月"
            };
            let mut analyzer = Analyzer::from_dict(dictionary(Some((surface, pos, reading))));
            let out = analyzer.try_tokenize(text).unwrap();
            let selected = out.iter().find(|t| t.surface.as_ref() == surface).unwrap();
            assert_eq!(selected.reading.as_ref(), reading);
            assert_eq!(selected.pronunciation.as_ref(), reading);
            assert_eq!(selected.pos.as_ref(), pos);
            assert_eq!(selected.word_cost, -2000);
            assert!(selected.is_known);
            assert!(
                !out.iter()
                    .any(|t| t.surface.as_ref() == "一" && t.reading.as_ref() == "ジュウ")
            );
        }
    }
}
