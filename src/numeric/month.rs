//! 水平空白を挟む暦月。数詞と「月」の範囲を保ち、数詞が複数トークンなら数量と同じ形に合成する。

use super::{AsciiPlan, digit, has_reading, is_numeral, place, pos_has, set_readings};
use crate::lattice::Token;
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
    starts
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
) {
    if !plan.has_month_in(offset, offset + input.len()) {
        return;
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
            apply(&text, 37, &mut out, 0, &plan);
            assert_eq!(out[0].reading.as_ref(), expected_r);
            assert_eq!(out[0].pronunciation.as_ref(), expected_p);
            assert_eq!(out[1].reading.as_ref(), "ガツ");
            let once = format!("{out:?}");
            apply(&text, 37, &mut out, 0, &plan);
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
        apply(text, 37, &mut out, 0, &plan);
        assert_eq!(out[0].reading.as_ref(), "ク");
        assert!(!out[0].is_known);
        for (reading, pronunciation) in [("", ""), ("キュウ", "ベツ"), ("ナイン", "ナイン")]
        {
            let mut out = tokens("9", reading, pronunciation);
            let before = format!("{out:?}");
            apply(text, 37, &mut out, 0, &plan);
            assert_eq!(format!("{out:?}"), before);
        }
        for index in [0, 1] {
            let mut out = tokens("9", "キュウ", "キュー");
            out[index].pos = "名詞,固有名詞,人名,名".into();
            let before = format!("{out:?}");
            apply(text, 37, &mut out, 0, &plan);
            assert_eq!(format!("{out:?}"), before);
        }
    }
}
