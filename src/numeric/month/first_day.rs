//! 標準の月に直結する分割された1+日だけを暦日のツイタチへまとめる。
//! 全文の境界・引用と選択済み素性を確認し、月・年や既知の全体語は変えない。

use super::MONTHS;
use crate::anchors::Anchors;
use crate::lattice::{ContextualReadingContext, Token};
use std::{ops::Range, sync::Arc};

/// 書き換えるのは「月」で終わる語に直結した「1」「１」「一」と「日」の組だけなので、チャンクに
/// 「月」と「日」の両方が無ければ何もしない
pub(crate) fn may_apply(anchors: Anchors) -> bool {
    anchors.contains(Anchors::MONTH.union(Anchors::DAY))
}

fn value(raw: &str) -> Option<usize> {
    if raw.is_empty() || raw.starts_with(['0', '０', '〇', '零']) {
        return None;
    }
    if raw.is_ascii() {
        return super::super::ascii::integer_value(raw)
            .filter(|n| (1..=12).contains(n))
            .map(|n| n as usize);
    }
    if raw.chars().all(|c| matches!(c, '１'..='９' | '０')) {
        let number = raw.chars().try_fold(0usize, |n, c| {
            n.checked_mul(10)?.checked_add(super::digit(c)? as usize)
        })?;
        return (1..=12).contains(&number).then_some(number);
    }
    MONTHS
        .iter()
        .position(|(forms, _, _)| forms[2] == raw)
        .map(|i| i + 1)
        .or(match raw {
            "一一" => Some(11),
            "一〇" | "一零" => Some(10),
            _ => None,
        })
}

fn empty_conj(t: &Token) -> bool {
    t.is_uninflected()
}

fn month_pos(t: &Token) -> bool {
    ["名詞,一般,*,*", "名詞,副詞可能,*,*", "名詞,固有名詞,一般,*"].contains(&t.pos.as_ref())
}

struct Month {
    start: usize,
    number: Range<usize>,
}

fn before(tokens: &[Token], first: usize, day: usize) -> Option<Month> {
    let last = day.checked_sub(1).filter(|i| *i >= first)?;
    let month = &tokens[last];
    if !month.is_known || !empty_conj(month) || month.end != tokens[day].start {
        return None;
    }
    let raw = month.surface.strip_suffix('月')?;
    if !raw.is_empty() {
        let v = value(raw)?;
        let (_, r, p) = MONTHS[v - 1];
        if !month_pos(month)
            || value(month.base_form.strip_suffix('月')?) != Some(v)
            || month.reading.as_ref() != format!("{r}ガツ")
            || month.pronunciation.as_ref() != format!("{p}ガツ")
        {
            return None;
        }
        return Some(Month {
            start: last,
            number: month.start..month.end - '月'.len_utf8(),
        });
    }
    if month.base_form != month.surface
        || !(month_pos(month) || month.pos.as_ref() == "名詞,接尾,助数詞,*")
        || month.reading.as_ref() != "ガツ"
        || month.pronunciation.as_ref() != "ガツ"
    {
        return None;
    }
    let mut start = last;
    while start > first && last - start < 2 {
        let token = &tokens[start - 1];
        if token.pos.as_ref() != "名詞,数,*,*" || token.end != tokens[start].start {
            break;
        }
        start -= 1;
    }
    if start == last {
        return None;
    }
    let raw: String = tokens[start..last]
        .iter()
        .map(|t| t.surface.as_ref())
        .collect();
    let v = value(&raw)?;
    let (_, r, p) = MONTHS[v - 1];
    if !tokens[start..last]
        .iter()
        .all(|t| t.base_form == t.surface && empty_conj(t))
        || tokens[start..last]
            .iter()
            .map(|t| t.reading.as_ref())
            .collect::<String>()
            != r
        || tokens[start..last]
            .iter()
            .map(|t| t.pronunciation.as_ref())
            .collect::<String>()
            != p
    {
        return None;
    }
    Some(Month {
        start,
        number: tokens[start].start..month.start,
    })
}

fn prefix_allowed(prefix: &str) -> bool {
    if !super::contiguous::prefix_allowed(prefix) {
        return false;
    }
    // 年がある場合も、その識別用の先頭ゼロを月日の認可へ持ち込まない。
    if let Some(year) = prefix.strip_suffix('年') {
        let before =
            year.trim_end_matches(|c| super::digit(c).is_some() || super::place(c).is_some());
        let raw = &year[before.len()..];
        if raw.is_empty() {
            // 今年・翌年などは数値年の識別子ではない。月の既存境界判定を保つ。
            return true;
        }
        let year_value = if raw.is_ascii() {
            super::super::ascii::integer_value(raw)
        } else if raw.chars().all(|c| matches!(c, '０'..='９'))
            || raw.chars().all(|c| {
                matches!(
                    c,
                    '〇' | '零'
                        | '一'
                        | '二'
                        | '三'
                        | '四'
                        | '五'
                        | '六'
                        | '七'
                        | '八'
                        | '九'
                        | '十'
                        | '百'
                        | '千'
                        | '万'
                        | '億'
                        | '兆'
                )
            })
        {
            super::super::numeral_value(std::iter::once(raw))
        } else {
            None
        };
        if raw.starts_with(['0', '０', '〇', '零']) || !year_value.is_some_and(|v| v > 0) {
            return false;
        }
    }
    true
}

fn right_allowed(rest: &str) -> bool {
    let tail = rest.trim_start_matches([' ', '\t', '　']);
    if [
        "間",
        "目",
        "分",
        "かか",
        "掛か",
        "かけ",
        "掛け",
        "程度",
        "ほど",
        "くらい",
        "ぐらい",
        "以内",
        "以上",
        "以下",
        "あたり",
        "ごと",
        "ずつ",
        "おき",
        "毎",
    ]
    .iter()
    .any(|suffix| tail.starts_with(suffix))
        || ["", "という", "と呼ぶ", "と呼ばれる", "は", "が"]
            .iter()
            .any(|frame| {
                tail.strip_prefix(frame).is_some_and(|name| {
                    [
                        "名前",
                        "名称",
                        "作品名",
                        "題名",
                        "書名",
                        "タイトル",
                        "姓",
                        "人名",
                    ]
                    .iter()
                    .any(|label| name.starts_with(label))
                })
            })
    {
        return false;
    }
    let tail = if tail == "." { "" } else { tail };
    if !super::eleven_right_allowed(tail, true) {
        return false;
    }
    for copula in ["でした", "です", "だった", "だ"] {
        if let Some(after) = tail.strip_prefix(copula) {
            return super::eleven_right_allowed(if after == "." { "" } else { after }, true);
        }
    }
    true
}

fn candidate(
    tokens: &[Token],
    first: usize,
    day: usize,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) -> Option<Token> {
    let number = &tokens[day];
    let suffix = tokens.get(day + 1)?;
    if !["1", "１", "一"].contains(&number.surface.as_ref())
        || number.pos.as_ref() != "名詞,数,*,*"
        || number.base_form != number.surface
        || number.reading.as_ref() != "イチ"
        || number.pronunciation.as_ref() != "イチ"
        || !empty_conj(number)
        || number.end != suffix.start
        || !suffix.is_known
        || suffix.surface.as_ref() != "日"
        || suffix.base_form != suffix.surface
        || suffix.pos.as_ref() != "名詞,接尾,助数詞,*"
        || !empty_conj(suffix)
        || suffix.reading.as_ref() != "ニチ"
        || suffix.pronunciation.as_ref() != "ニチ"
    {
        return None;
    }
    let month = before(tokens, first, day)?;
    let range = tokens[month.start].start..suffix.end;
    let (input, origin) = context.source();
    let start = range.start.checked_sub(origin)?;
    let end = range.end.checked_sub(origin)?;
    if !prefix_allowed(input.get(..start)?)
        || !right_allowed(input.get(end..)?)
        || !tokens[month.start..=day + 1].iter().all(|t| {
            t.start
                .checked_sub(origin)
                .zip(t.end.checked_sub(origin))
                .and_then(|(a, b)| input.get(a..b))
                == Some(t.surface.as_ref())
        })
        || !tokens[month.start..=day + 1]
            .windows(2)
            .all(|w| w[0].end == w[1].start)
        || !context.unquoted(&range)
    {
        return None;
    }
    // 元の数詞だけの保護と、ID等の広い保護範囲を混同しない。
    let index = protected.partition_point(|r| r.end <= range.start);
    if protected[index..]
        .iter()
        .take_while(|r| r.start < range.end)
        .any(|r| *r != month.number && *r != (number.start..number.end))
    {
        return None;
    }
    let surface: Arc<str> = Arc::from(input.get(number.start - origin..end)?);
    Some(Token {
        surface: Arc::clone(&surface),
        base_form: surface,
        start: number.start,
        end: suffix.end,
        pos: Arc::from("名詞,副詞可能,*,*"),
        conj_type: Arc::from(""),
        conj_form: Arc::from(""),
        reading: Arc::from("ツイタチ"),
        pronunciation: Arc::from("ツイタチ"),
        word_cost: (i64::from(number.word_cost) + i64::from(suffix.word_cost))
            .clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
        is_known: false,
    })
}

pub(crate) fn apply(
    out: &mut Vec<Token>,
    first: usize,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) {
    let mut plans = Vec::new();
    for day in first..out.len().saturating_sub(1) {
        if let Some(token) = candidate(out, first, day, protected, context) {
            plans.push((day, token));
        }
    }
    if plans.is_empty() {
        return;
    }
    let (mut read, mut write) = (first, first);
    for (day, token) in plans {
        while read < day {
            if write != read {
                out.swap(write, read);
            }
            read += 1;
            write += 1;
        }
        context.record(&token);
        out[write] = token;
        write += 1;
        read += 2;
    }
    while read < out.len() {
        if write != read {
            out.swap(write, read);
        }
        read += 1;
        write += 1;
    }
    out.truncate(write);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(offset: usize) -> Vec<Token> {
        [
            (
                "11月",
                0,
                "名詞,副詞可能,*,*",
                "ジュウイチガツ",
                "ジューイチガツ",
            ),
            ("1", 5, "名詞,数,*,*", "イチ", "イチ"),
            ("日", 6, "名詞,接尾,助数詞,*", "ニチ", "ニチ"),
        ]
        .into_iter()
        .map(|(surface, start, pos, r, p)| Token {
            surface: surface.into(),
            base_form: surface.into(),
            start: offset + start,
            end: offset + start + surface.len(),
            pos: pos.into(),
            conj_type: "".into(),
            conj_form: "".into(),
            reading: r.into(),
            pronunciation: p.into(),
            word_cost: 137,
            is_known: true,
        })
        .collect()
    }

    #[test]
    fn costs_include_only_the_day_once_and_saturate_at_the_end() {
        for (number, suffix, expected) in [
            (30000, 30000, i16::MAX),
            (-30000, -30000, i16::MIN),
            (30000, -30000, 0),
        ] {
            let mut out = tokens(71);
            out[1].word_cost = number;
            out[2].word_cost = suffix;
            let month = format!("{:?}", out[0]);
            let mut spans = Vec::new();
            {
                let mut context = ContextualReadingContext::new("11月1日", 71, Some(&mut spans));
                apply(&mut out, 0, &[71..73, 76..77], &mut context);
            }
            assert_eq!(out.len(), 2);
            assert_eq!(format!("{:?}", out[0]), month);
            assert_eq!(out[1].word_cost, expected);
            assert!(!out[1].is_known);
            assert_eq!(
                spans,
                [crate::ContextualReadingSpan {
                    start: 76,
                    end: 80,
                    pronunciation: "ツイタチ".into()
                }]
            );
        }
    }

    #[test]
    fn complete_global_context_is_required_even_for_partial_chunks_and_prior_output() {
        for (prefix, tail, allowed) in [
            ("日付は", "です。", true),
            ("ID", "", false),
            ("作品名は", "", false),
            ("「前。\n", "です。」", false),
        ] {
            let input = format!("{prefix}11月1日{tail}");
            let origin = 71;
            let position = origin + prefix.len();
            let mut out = tokens(0);
            let first = out.len();
            let previous = format!("{out:?}");
            out.extend(tokens(position));
            let original = format!("{out:?}");
            let mut context = ContextualReadingContext::new(&input, origin, None);
            apply(&mut out, first, &[], &mut context);
            assert_eq!(format!("{:?}", &out[..first]), previous);
            if allowed {
                assert_eq!(out.len(), first + 2);
                assert_eq!(out[first + 1].surface.as_ref(), "1日");
                assert_eq!(
                    (out[first + 1].start, out[first + 1].end),
                    (position + 5, position + 9)
                );
            } else {
                assert_eq!(format!("{out:?}"), original);
            }
            for mut partial in [
                tokens(position)[..2].to_vec(),
                tokens(position)[1..].to_vec(),
            ] {
                let original = format!("{partial:?}");
                apply(&mut partial, 0, &[], &mut context);
                assert_eq!(format!("{partial:?}"), original);
            }
        }
    }

    #[test]
    fn strong_protection_and_unknown_day_suffix_do_not_get_reauthorized() {
        for protected in [0..9, 0..1, 4..8] {
            let mut out = tokens(0);
            let original = format!("{out:?}");
            let mut context = ContextualReadingContext::new("11月1日", 0, None);
            apply(&mut out, 0, std::slice::from_ref(&protected), &mut context);
            assert_eq!(format!("{out:?}"), original);
        }
        let mut out = tokens(0);
        out[2].is_known = false;
        let original = format!("{out:?}");
        let mut context = ContextualReadingContext::new("11月1日", 0, None);
        apply(&mut out, 0, &[], &mut context);
        assert_eq!(format!("{out:?}"), original);
    }
}
