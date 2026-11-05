//! 空白のないASCII暦月。全文で認定した1〜12と、選択済みの標準素性だけを扱う。
//! 裸の数詞の保護範囲だけを認定後に除外し、年・日と旧来の暦月規則は変更しない。

use super::{MONTHS, eleven_prefix_allowed, eleven_right_allowed, standard_readings};
use crate::{DictError, Dictionary, Token, numeric::AsciiPlan};
use std::{ops::Range, sync::Arc};

#[derive(Debug)]
pub(crate) struct Frame {
    pub(crate) number: Range<usize>,
    pub(crate) end: usize,
    value: usize,
}

pub(super) fn prefix_allowed(prefix: &str) -> bool {
    let prefix = if let Some(year) = prefix.strip_suffix('年') {
        let before =
            year.trim_end_matches(|c| super::digit(c).is_some() || super::place(c).is_some());
        if before.len() < year.len() {
            before
        } else {
            prefix
        }
    } else {
        prefix
    };
    let named = prefix.trim_end_matches([' ', '\t', '　', '、', ',', ':', '：']);
    ![
        "姓",
        "名字",
        "苗字",
        "氏名",
        "人名",
        "題名",
        "作品名",
        "書名",
        "タイトル",
    ]
    .iter()
    .any(|label| {
        ["", "は", "が", "も", "を"].iter().any(|tail| {
            named
                .strip_suffix(tail)
                .is_some_and(|head| head.ends_with(label))
        })
    }) && eleven_prefix_allowed(named)
        && eleven_prefix_allowed(prefix)
}

fn following_day(rest: &str) -> Option<&str> {
    let first = rest.chars().next()?;
    if first.is_ascii_digit() || matches!(first, '０'..='９') {
        let wide = !first.is_ascii();
        let raw = rest
            .chars()
            .take_while(|c| super::digit(*c).is_some())
            .take(3)
            .collect::<String>();
        if raw.chars().count() > 2
            || raw.starts_with(['0', '０'])
            || !raw.chars().all(|c| {
                if wide {
                    matches!(c, '０'..='９')
                } else {
                    c.is_ascii_digit()
                }
            })
        {
            return None;
        }
        let value = raw
            .chars()
            .try_fold(0u64, |value, c| Some(value * 10 + super::digit(c)?))?;
        if !(1..=31).contains(&value) {
            return None;
        }
        rest[raw.len()..].strip_prefix('日')
    } else {
        let (raw, tail) = super::following_day(rest)?;
        super::day_value(raw)?;
        Some(tail)
    }
}

fn right_allowed(rest: &str) -> bool {
    let has_day = rest
        .chars()
        .next()
        .is_some_and(|c| super::digit(c).is_some() || super::place(c).is_some());
    let rest = if has_day {
        let Some(tail) = following_day(rest) else {
            return false;
        };
        tail
    } else {
        rest
    };
    let rest = if rest == "." { "" } else { rest };
    eleven_right_allowed(rest, has_day)
}

pub(crate) fn frames(input: &str, offset: usize) -> Vec<Frame> {
    let mut frames = Vec::new();
    let mut quotes = None;
    for (month, _) in input.match_indices('月') {
        let digits = input[..month]
            .bytes()
            .rev()
            .take_while(u8::is_ascii_digit)
            .count();
        if !(1..=2).contains(&digits) {
            continue;
        }
        let start = month - digits;
        let raw = &input[start..month];
        let Some(value) = super::super::ascii::integer_value(raw).filter(|n| (1..=12).contains(n))
        else {
            continue;
        };
        let end = month + '月'.len_utf8();
        if !prefix_allowed(&input[..start]) || !right_allowed(&input[end..]) {
            continue;
        }
        let ranges = quotes.get_or_insert_with(|| crate::lattice::quotation_ranges(input));
        let index = ranges.partition_point(|range| range.end <= start);
        if ranges.get(index).is_some_and(|range| range.start < end) {
            continue;
        }
        frames.push(Frame {
            number: offset + start..offset + month,
            end: offset + end,
            value: value as usize,
        });
    }
    frames
}

fn candidate(
    input: &str,
    offset: usize,
    out: &[Token],
    start: usize,
    frame: &Frame,
    protected: &[Range<usize>],
) -> Option<usize> {
    if out[start].start != frame.number.start {
        return None;
    }
    let mut end = start;
    while end < out.len() && out[end].end <= frame.number.end {
        let token = &out[end];
        if token.start
            != if end == start {
                frame.number.start
            } else {
                out[end - 1].end
            }
            || token.base_form != token.surface
            || token.pos.as_ref() != "名詞,数,*,*"
            || !token.is_uninflected()
            || !standard_readings(token)
        {
            return None;
        }
        end += 1;
    }
    let month = out.get(end)?;
    if end == start
        || out[end - 1].end != frame.number.end
        || month.start != frame.number.end
        || month.end != frame.end
        || month.surface.as_ref() != "月"
        || month.base_form != month.surface
        || !month.is_known
        || !month.is_uninflected()
        || !["名詞,一般,*,*", "名詞,副詞可能,*,*", "名詞,接尾,助数詞,*"]
            .contains(&month.pos.as_ref())
        || month.reading != month.pronunciation
        || !["ツキ", "ガツ"].contains(&month.reading.as_ref())
    {
        return None;
    }
    for token in &out[start..=end] {
        if token
            .start
            .checked_sub(offset)
            .zip(token.end.checked_sub(offset))
            .and_then(|(a, b)| input.get(a..b))
            != Some(token.surface.as_ref())
        {
            return None;
        }
    }
    let first = protected.partition_point(|range| range.end <= frame.number.start);
    if protected[first..]
        .iter()
        .take_while(|range| range.start < frame.end)
        .any(|range| *range != frame.number)
    {
        return None;
    }
    Some(end)
}

pub(crate) fn apply(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first: usize,
    dict: &Dictionary,
    plan: &AsciiPlan,
    protected: &[Range<usize>],
) -> Result<(), DictError> {
    let frames = plan.contiguous_months_in(offset, offset + input.len());
    if frames.is_empty() {
        return Ok(());
    }
    let (mut read, mut write, mut next) = (first, first, 0);
    while read < out.len() {
        while next < frames.len() && frames[next].number.start < out[read].start {
            next += 1;
        }
        let mut matched = None;
        if let Some(frame) = frames.get(next)
            && frame.number.start == out[read].start
        {
            next += 1;
            if let Some(end) = candidate(input, offset, out, read, frame, protected) {
                let a = frame.number.start - offset;
                let b = frame.end - offset;
                if !dict
                    .lookup(&input[a..b])?
                    .iter()
                    .any(|(end, rows)| *end == b - a && !rows.is_empty())
                {
                    matched = Some((frame, end));
                }
            }
        }
        if let Some((frame, end)) = matched {
            let (_, reading, pronunciation) = MONTHS[frame.value - 1];
            let mut number = if end - read == 1 {
                out[read].clone()
            } else {
                let surface: Arc<str> =
                    Arc::from(&input[frame.number.start - offset..frame.number.end - offset]);
                Token {
                    surface: Arc::clone(&surface),
                    base_form: surface,
                    start: frame.number.start,
                    end: frame.number.end,
                    pos: Arc::from("名詞,数,*,*"),
                    conj_type: Arc::from(""),
                    conj_form: Arc::from(""),
                    reading: Arc::from(""),
                    pronunciation: Arc::from(""),
                    word_cost: out[read..end]
                        .iter()
                        .map(|t| i64::from(t.word_cost))
                        .sum::<i64>()
                        .clamp(i64::from(i16::MIN), i64::from(i16::MAX))
                        as i16,
                    is_known: false,
                }
            };
            number.reading = Arc::from(reading);
            number.pronunciation = Arc::from(pronunciation);
            let mut month = out[end].clone();
            month.reading = Arc::from("ガツ");
            month.pronunciation = Arc::from("ガツ");
            out[write] = number;
            out[write + 1] = month;
            write += 2;
            read = end + 1;
        } else {
            if write != read {
                out.swap(write, read);
            }
            write += 1;
            read += 1;
        }
    }
    out.truncate(write);
    Ok(())
}
