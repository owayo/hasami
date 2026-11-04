//! 明示された距離と分・秒の列で、mの次の数字を時間へ残す。
//! ソースだけで識別子を分割せず、境界をまたぐ辞書候補の素性も確かめる。

use crate::{DictEntry, DictError, Dictionary};
use std::ops::Range;

pub(super) struct Frame {
    pub(super) label_start: usize,
    pub(super) number: Range<usize>,
    pub(super) unit_end: usize,
    pub(super) minutes: Range<usize>,
    pub(super) end: usize,
}

fn integer_end(input: &str, start: usize, maximum: usize) -> Option<usize> {
    let count = input[start..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if count == 0 || count > maximum || count > 1 && input.as_bytes()[start] == b'0' {
        return None;
    }
    Some(start + count)
}

fn clock_component(input: &str, start: usize) -> Option<usize> {
    let end = integer_end(input, start, 2)?;
    (input[start..end].parse::<u8>().ok()? <= 59).then_some(end)
}

pub(super) fn frames(input: &str, offset: usize) -> Vec<Frame> {
    if !input.contains('m') || !input.contains('分') || !input.contains('秒') {
        return Vec::new();
    }
    let mut frames = Vec::new();
    for (unit_start, _) in input.match_indices('m') {
        let count = input[..unit_start]
            .bytes()
            .rev()
            .take_while(u8::is_ascii_digit)
            .count();
        let number_start = unit_start - count;
        if integer_end(input, number_start, 16) != Some(unit_start) {
            continue;
        }
        let Some(label) = ["長さ", "距離", "全長"]
            .into_iter()
            .find(|label| input[..number_start].ends_with(label))
        else {
            continue;
        };
        let label_start = number_start - label.len();
        let before = input[..label_start].trim_end_matches([' ', '\t', '　']);
        let label_prefix = before.trim_end_matches([' ', '\t', '　', ':', '：', ';', '；']);
        if !crate::numeric::month::eleven_prefix_allowed(label_prefix)
            || ["題名", "書名", "作品名", "タイトル"].iter().any(|label| {
                ["", "は", "も", "が"].iter().any(|tail| {
                    label_prefix
                        .strip_suffix(tail)
                        .is_some_and(|head| head.ends_with(label))
                })
            })
        {
            continue;
        }
        if before.chars().next_back().is_some_and(|c| {
            !matches!(
                c,
                '。' | '、' | '！' | '？' | '!' | '?' | '\n' | '\r' | ':' | '：' | ';' | '；'
            )
        }) {
            continue;
        }
        let minutes_start = unit_start + 1;
        let Some(minutes_end) = clock_component(input, minutes_start) else {
            continue;
        };
        if !input[minutes_end..].starts_with('分') {
            continue;
        }
        let seconds_start = minutes_end + '分'.len_utf8();
        let Some(seconds_end) = clock_component(input, seconds_start) else {
            continue;
        };
        if !input[seconds_end..].starts_with('秒') {
            continue;
        }
        let end = seconds_end + '秒'.len_utf8();
        if input[end..]
            .chars()
            .next()
            .is_some_and(|c| !matches!(c, '。' | '、' | '！' | '？' | '!' | '?' | '\n' | '\r'))
        {
            continue;
        }
        frames.push(Frame {
            label_start: offset + label_start,
            number: offset + number_start..offset + unit_start,
            unit_end: offset + minutes_start,
            minutes: offset + minutes_start..offset + minutes_end,
            end: offset + end,
        });
    }
    frames
}

fn canonical_unit(entry: &DictEntry) -> bool {
    let readings = match entry.surface.as_ref() {
        "m2" => ["", "ヘイホウメートル", "ヘイホーメートル"],
        "m3" => ["", "リッポウメートル", "リッポーメートル"],
        _ => return false,
    };
    entry.pos.as_ref() == "名詞,一般,*,*"
        && entry.base_form == entry.surface
        && matches!(entry.conj_type.as_ref(), "" | "*")
        && matches!(entry.conj_form.as_ref(), "" | "*")
        && readings.contains(&entry.reading.as_ref())
        && readings.contains(&entry.pronunciation.as_ref())
}

fn has_component(
    dict: &Dictionary,
    surface: &str,
    fields: (&str, &str, &str, &str, &str),
) -> Result<bool, DictError> {
    for (end, entries) in dict.lookup(surface)? {
        if end != surface.len() {
            continue;
        }
        if entries.iter().any(|entry| {
            entry.pos.as_ref() == fields.0
                && entry.base_form.as_ref() == fields.1
                && entry.reading.as_ref() == fields.2
                && entry.pronunciation.as_ref() == fields.2
                && (entry.conj_type.as_ref() == fields.3
                    || fields.3.is_empty() && entry.conj_type.as_ref() == "*")
                && (entry.conj_form.as_ref() == fields.4
                    || fields.4.is_empty() && entry.conj_form.as_ref() == "*")
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn canonical_crossings(
    input: &str,
    offset: usize,
    frame: &Frame,
    dict: &Dictionary,
) -> Result<bool, DictError> {
    let boundary = frame.unit_end - offset;
    let label = &input[frame.label_start - offset..frame.number.start - offset];
    let speech = match label {
        "長さ" => ("ナガサ", "ナガサ"),
        "距離" => ("キョリ", "キョリ"),
        "全長" => ("ゼンチョウ", "ゼンチョー"),
        _ => return Ok(false),
    };
    let mut label_exists = false;
    for (end, entries) in dict.lookup(label)? {
        if end != label.len() {
            continue;
        }
        for entry in entries {
            if entry.pos.as_ref() != "名詞,一般,*,*"
                || entry.base_form.as_ref() != label
                || entry.reading.as_ref() != speech.0
                || entry.pronunciation.as_ref() != speech.1
                || !matches!(entry.conj_type.as_ref(), "" | "*")
                || !matches!(entry.conj_form.as_ref(), "" | "*")
            {
                return Ok(false);
            }
            label_exists = true;
        }
    }
    if !label_exists {
        // IPAdicでは「長さ」が標準の形容詞語幹と接尾辞に分かれる。
        if label != "長さ"
            || !has_component(
                dict,
                "長",
                (
                    "形容詞,自立,*,*",
                    "長い",
                    "ナガ",
                    "形容詞・アウオ段",
                    "ガル接続",
                ),
            )?
            || !has_component(dict, "さ", ("名詞,接尾,特殊,*", "さ", "サ", "", ""))?
        {
            return Ok(false);
        }
    }
    // 長い既知語や独自のm3まで壊さない。認定した短い見出しと数値の範囲だけ照合する。
    for (relative, _) in input[frame.label_start - offset..boundary].char_indices() {
        let start = frame.label_start - offset + relative;
        for (end, entries) in dict.lookup(&input[start..])? {
            if start + end <= boundary {
                continue;
            }
            if start != frame.number.end - offset
                || start + end != boundary + 1
                || entries.iter().any(|entry| !canonical_unit(entry))
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
