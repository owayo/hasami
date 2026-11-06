//! 月日が直結した全角年だけの認可。旧来の年読み・全角数量の条件は変更しない。

use crate::anchors::Anchors;
use std::ops::Range;

/// [`ranges`] が見るのは全角の数字に続く「年」だけ
pub(in crate::numeric) fn may_find(anchors: Anchors) -> bool {
    anchors.contains(Anchors::WIDE_DIGIT.union(Anchors::YEAR))
}

fn component(input: &str, suffix: char, maximum: u32) -> Option<(&str, usize)> {
    let first = input.chars().next()?;
    let base = if first.is_ascii_digit() {
        '0' as u32
    } else if matches!(first, '０'..='９') {
        '０' as u32
    } else {
        return None;
    };
    let mut value = first as u32 - base;
    if value == 0 {
        return None;
    }
    for (count, (index, c)) in (1..).zip(input.char_indices().skip(1).take(2)) {
        if c == suffix {
            let end = index + c.len_utf8();
            return (1..=maximum)
                .contains(&value)
                .then_some((&input[end..], end));
        }
        if count == 2 {
            return None;
        }
        let digit = (c as u32).checked_sub(base)?;
        if digit > 9 {
            return None;
        }
        value = value * 10 + digit;
    }
    None
}

fn named(prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches([' ', '\t', '　', '、', ',', ':', '：']);
    [
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
            prefix
                .strip_suffix(tail)
                .is_some_and(|head| head.ends_with(label))
        })
    }) || !super::super::month::eleven_prefix_allowed(prefix)
}

fn calendar_end(rest: &str) -> Option<usize> {
    // 旧規則は01月/01日も認める。その契約は変更せず、新しい合成には追加で厳格な幅・ゼロを検査する。
    if !super::super::japanese_boundary::calendar_year_tail(rest) {
        return None;
    }
    let (mut tail, consumed) = component(rest.strip_prefix('年')?, '月', 12)?;
    let mut end = '年'.len_utf8() + consumed;
    if tail
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || matches!(c, '０'..='９'))
    {
        let (next, consumed) = component(tail, '日', 31)?;
        tail = next;
        end += consumed;
    }
    if !super::super::month::eleven_right_allowed(tail, true)
        || ["という", "と呼ぶ", "と呼ばれる", "は", "が"]
            .iter()
            .any(|frame| {
                tail.trim_start().strip_prefix(frame).is_some_and(|tail| {
                    ["作品名", "題名", "書名", "タイトル", "人名", "姓"]
                        .iter()
                        .any(|name| tail.starts_with(name))
                })
            })
    {
        return None;
    }
    Some(end)
}

pub(in crate::numeric) fn ranges(input: &str, offset: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if !input.bytes().any(|b| b == 0xef) {
        return ranges;
    }
    let mut quotes = None;
    for (year, _) in input.match_indices('年') {
        let before = input[..year].trim_end_matches(|c| matches!(c, '０'..='９'));
        let start = before.len();
        let raw = &input[start..year];
        if raw.chars().count() < 2
            || super::value(raw).is_none()
            || !super::super::month::eleven_prefix_allowed(before)
            || named(before)
        {
            continue;
        }
        let Some(consumed) = calendar_end(&input[year..]) else {
            continue;
        };
        let quote_ranges = quotes.get_or_insert_with(|| crate::lattice::quotation_ranges(input));
        let index = quote_ranges.partition_point(|range| range.end <= start);
        if quote_ranges
            .get(index)
            .is_some_and(|range| range.start < year + consumed)
        {
            continue;
        }
        ranges.push(offset + start..offset + year + '年'.len_utf8());
    }
    ranges
}

#[cfg(test)]
mod tests {
    use crate::{
        Token,
        numeric::{AsciiPlan, fullwidth},
    };

    fn tokens(offset: usize) -> Vec<Token> {
        "２０１６年"
            .char_indices()
            .zip(["ニ", "ゼロ", "イチ", "ロク", "ネン"])
            .map(|((index, c), speech)| Token {
                surface: c.to_string().into(),
                base_form: c.to_string().into(),
                start: offset + index,
                end: offset + index + c.len_utf8(),
                pos: if c == '年' {
                    "名詞,接尾,助数詞,*"
                } else {
                    "名詞,数,*,*"
                }
                .into(),
                conj_type: "".into(),
                conj_form: "".into(),
                reading: speech.into(),
                pronunciation: speech.into(),
                word_cost: 17,
                is_known: true,
            })
            .collect()
    }

    #[test]
    fn full_input_calendar_proof_controls_year_only_chunks_and_new_strict_fields() {
        let plan = AsciiPlan::new("２０１６年１１月１日", 37);
        let mut valid = tokens(37);
        fullwidth::apply("２０１６年", 37, &mut valid, 0, &plan, &[]);
        assert_eq!(valid.len(), 2);
        assert_eq!(valid[0].surface.as_ref(), "２０１６");
        assert_eq!(valid[0].word_cost, 68);
        for mutation in 0..3 {
            let mut custom = tokens(37);
            match mutation {
                0 => custom.last_mut().unwrap().conj_type = "独自".into(),
                1 => custom[0].base_form = "2".into(),
                _ => custom.last_mut().unwrap().base_form = "別の年".into(),
            }
            let expected = format!("{custom:?}");
            fullwidth::apply("２０１６年", 37, &mut custom, 0, &plan, &[]);
            assert_eq!(format!("{custom:?}"), expected, "mutation {mutation}");
        }
        for (full, offset) in [
            ("２０１６年１１月１日A", 37),
            ("２０１６年０１月１日", 37),
            ("名前は２０１６年１１月１日", 37 + "名前は".len()),
            ("「前。\n２０１６年１１月１日", 37 + "「前。\n".len()),
        ] {
            let mut out = tokens(offset);
            let before = format!("{out:?}");
            fullwidth::apply(
                "２０１６年",
                offset,
                &mut out,
                0,
                &AsciiPlan::new(full, 37),
                &[],
            );
            assert_eq!(format!("{out:?}"), before, "{full}");
        }
    }
}
