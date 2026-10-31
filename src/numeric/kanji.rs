//! 位を省いた漢数字列と、日に直結した二十。標準的な数詞だけを後段へ渡す。

use super::{AsciiPlan, ascii, pos_has, standard_kanji_readings};
use crate::lattice::Token;
use std::ops::Range;
use std::sync::Arc;

fn digit(c: char) -> Option<u64> {
    matches!(
        c,
        '〇' | '零' | '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九'
    )
    .then(|| super::digit(c).unwrap())
}

fn scaled_value(raw: &str) -> Option<u64> {
    let (digits, scale) = raw
        .strip_suffix('万')
        .map(|n| (n, 10_000))
        .or_else(|| raw.strip_suffix('億').map(|n| (n, 100_000_000)))?;
    if !digits.contains(['〇', '零']) {
        return None;
    }
    let n = value(digits)?.checked_mul(scale)?;
    (n < 10_000_000_000_000_000).then_some(n)
}

fn value(raw: &str) -> Option<u64> {
    if raw.ends_with(['万', '億']) {
        return scaled_value(raw);
    }
    if raw == "二十" {
        return Some(20);
    }
    let (mut value, mut count) = (0u64, 0usize);
    for c in raw.chars() {
        let n = digit(c)?;
        if count == 0 && n == 0 {
            return None;
        }
        count += 1;
        if count > 16 {
            return None;
        }
        value = value * 10 + n;
    }
    (count >= 2).then_some(value)
}

fn identifier_neighbor(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || digit(c).is_some()
        || super::place(c).is_some()
        || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９' | '_' | '.' | '．' | '点' | '・' | ',' | '，' | ':' | '：' | '/' | '／' | '∕' | '÷' | '×' | '+' | '＋' | '-' | '－' | '−' | '‐' | '‑' | '‒' | '–' | '—' | '―' | '=' | '*' | '^' | '%' | '第')
}

pub(super) fn identifier_prefix(prefix: &str) -> bool {
    let prefix = prefix
        .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '(' | '[' | '（' | '［'));
    prefix.chars().next_back().is_some_and(identifier_neighbor)
        || [
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
}

fn pair2(raw: &str) -> bool {
    matches!(
        raw,
        "一一" | "一六" | "一八" | "二一" | "三八" | "四一" | "四八"
    )
}

fn pair2_right_boundary(rest: &str) -> bool {
    let Some(c) = rest.chars().next() else {
        return true;
    };
    if matches!(c, ',' | '.') {
        return !rest[c.len_utf8()..]
            .chars()
            .next()
            .is_some_and(identifier_neighbor);
    }
    !identifier_neighbor(c)
}

fn pair2_comma_prefix(prefix: &str) -> bool {
    super::japanese_boundary::comma_prefix(prefix)
}

fn comma_existing_year(prefix: &str, suffix: &str) -> bool {
    super::japanese_boundary::comma_prefix(prefix)
        && super::japanese_boundary::single_year_tail(suffix)
}

pub(super) fn comma_year_starts(input: &str, offset: usize, starts: &[usize]) -> Vec<usize> {
    starts
        .iter()
        .copied()
        .filter(|&start| {
            let start = start - offset;
            let end = input[start..]
                .char_indices()
                .take_while(|&(_, c)| digit(c).is_some())
                .map(|(i, c)| start + i + c.len_utf8())
                .last()
                .unwrap_or(start);
            start < end && comma_existing_year(&input[..start], &input[end..])
        })
        .collect()
}

fn marked_comma_year(plan: &AsciiPlan, start: usize) -> bool {
    plan.comma_year_start(start)
}

fn pair2_suffix(raw: &str) -> Option<(&'static str, &'static str)> {
    [
        ("キログラム", "キログラム"),
        ("キロメートル", "キロメートル"),
        ("キロ", "キロ"),
        ("か月", "カゲツ"),
        ("カ月", "カゲツ"),
        ("ヶ月", "カゲツ"),
        ("歳", "サイ"),
        ("才", "サイ"),
    ]
    .into_iter()
    .find(|(surface, _)| raw.strip_prefix(surface).is_some_and(pair2_right_boundary))
}

fn pair2_global(raw: &str, suffix: &str) -> bool {
    pair2(raw) && pair2_suffix(suffix).is_some()
}

fn pair2_counter(raw: &str, suffix: &Token) -> bool {
    pair2(raw)
        && suffix.is_known
        && pos_has(&suffix.pos, "名詞,接尾,助数詞")
        && suffix.base_form == suffix.surface
        && suffix.conj_type.is_empty()
        && suffix.conj_form.is_empty()
        && pair2_suffix(&suffix.surface).is_some_and(|(surface, reading)| {
            surface == suffix.surface.as_ref()
                && suffix.reading.as_ref() == reading
                && suffix.pronunciation.as_ref() == reading
        })
}

/// 前分割で識別子や小数の前半を失わないよう、入力全体で候補の開始位置を決める。
pub(super) fn number_starts(input: &str, offset: usize) -> Vec<usize> {
    // 純全角数量も同じ全入力planで開始位置を保ち、漢数字の値や適用条件は広げない。
    let mut starts = super::fullwidth::number_starts(input, offset, |prefix| {
        if !identifier_prefix(prefix) {
            return true;
        }

        super::japanese_boundary::comma_prefix(prefix)
    });
    if input.is_ascii() {
        return starts;
    }
    let mut chars = input.char_indices().peekable();
    let mut quotes = Vec::new();
    while let Some((start, c)) = chars.next() {
        if quotes.last() == Some(&c) {
            quotes.pop();
        } else if let Some(close) = super::month::quote_end(c) {
            quotes.push(close);
        }
        // 位付き漢数字全般ではなく、二十日だけを和語の助数詞補正へ渡す。
        // 数字列の途中の二は下の走査が消費するので、三二十日等では開始しない。
        if input[start..].starts_with("二十日") && !identifier_prefix(&input[..start]) {
            starts.push(offset + start);
            continue;
        }
        if digit(c).is_none() {
            continue;
        }
        let mut end = start + c.len_utf8();
        while let Some(&(i, c)) = chars.peek()
            && digit(c).is_some()
        {
            end = i + c.len_utf8();
            chars.next();
        }
        if input[end..].starts_with(['万', '億']) {
            let next_end = end + 3;
            if scaled_value(&input[start..next_end]).is_some() {
                end = next_end;
                chars.next();
            }
        }
        let raw = &input[start..end];
        if scaled_value(raw).is_some()
            && (!quotes.is_empty() || !super::month::eleven_prefix_allowed(&input[..start]))
        {
            continue;
        }
        if value(raw).is_some()
            // ゼロのない二字列は「二、三」のような概数と区別できない。
            && (raw.contains(['〇', '零']) || raw.chars().count() >= 3 || pair2_global(raw, &input[end..]))
            && (!identifier_prefix(&input[..start])
                || (pair2_global(raw, &input[end..]) && pair2_comma_prefix(&input[..start]))
                || comma_existing_year(&input[..start], &input[end..]))
            && !input[end..].chars().next().is_some_and(|c| {
                // 英字・% は実トークンの単位認識と、その末尾の境界で絞る。
                identifier_neighbor(c) && !c.is_ascii_alphabetic() && c != '%'
            })
        {
            starts.push(offset + start);
        }
    }
    starts.sort_unstable();
    starts
}

/// 合成後の位取り読みを後段の助数詞補正で認可する。字ごとの元の読みの検査とは分ける。
pub(super) fn standard_readings(raw: &str, reading: &str, pronunciation: &str) -> bool {
    let Some(value) = value(raw) else {
        return false;
    };
    ascii::standard_readings(&value.to_string(), reading, pronunciation)
}

fn number_token(t: &Token) -> bool {
    pos_has(&t.pos, "名詞,数")
        && !t.surface.is_empty()
        && (t.surface.chars().all(|c| digit(c).is_some())
            || matches!(t.surface.as_ref(), "十" | "二十" | "万" | "億"))
}

fn quantity(t: &Token, unit: impl Fn(&str) -> bool) -> bool {
    // 位付き漢数字と番号・型番は、この処理で数量の位取りへ解釈し直さない。
    !t.surface
        .chars()
        .next()
        .is_some_and(|c| super::place(c).is_some() || matches!(c, '番' | '号' | '版' | '型'))
        && ascii::quantity(t, unit)
}

fn candidate<'a>(
    input: &'a str,
    offset: usize,
    out: &[Token],
    range: Range<usize>,
    protected: &[Range<usize>],
    plan: &AsciiPlan,
    unit: impl Fn(&str) -> bool,
) -> Option<(&'a str, u64)> {
    let first = &out[range.start];
    let comma_year = marked_comma_year(plan, first.start);
    let last = &out[range.end - 1];
    let raw = input.get(first.start.checked_sub(offset)?..last.end.checked_sub(offset)?)?;
    let value = value(raw)?;
    let scaled = scaled_value(raw).is_some();
    // 明示された万・億で終わる数詞は元入力の末尾だけ認可する。
    // 切り出したチャンクの末尾では認可せず、位を伴わない裸の数字列は変更しない。
    if scaled
        && range.end == out.len()
        && last.end == plan.input_end()
        && !(range.len() == 1 && first.is_known)
        && !protected
            .iter()
            .any(|p| p.start < last.end && first.start < p.end)
        && out[range.clone()].iter().all(|t| {
            let (Some(start), Some(end)) = (t.start.checked_sub(offset), t.end.checked_sub(offset))
            else {
                return false;
            };
            input.get(start..end) == Some(t.surface.as_ref())
                && t.conj_type.is_empty()
                && t.conj_form.is_empty()
                && standard_kanji_readings(&t.surface, &t.reading, &t.pronunciation)
                && super::numeral_value(std::iter::once(t.surface.as_ref())).is_some_and(|n| {
                    super::numeral_value(std::iter::once(t.base_form.as_ref())) == Some(n)
                })
        })
    {
        return Some((raw, value));
    }
    let suffix = out.get(range.end)?;
    let finite_pair = pair2_counter(raw, suffix);
    if scaled
        && (!plan.scaled_unit_end_allowed(suffix.end)
            || !(suffix.is_known
                && suffix.base_form == suffix.surface
                && suffix.conj_type.is_empty()
                && suffix.conj_form.is_empty()
                && (super::counter(suffix).is_some()
                    || (pos_has(&suffix.pos, "名詞,接尾,助数詞")
                        && matches!(suffix.surface.as_ref(), "円" | "エン")
                        && super::has_reading(suffix, &["エン"])))))
    {
        return None;
    }
    if comma_year
        && !(suffix.is_known
            && suffix.surface.as_ref() == "年"
            && suffix.base_form.as_ref() == "年"
            && pos_has(&suffix.pos, "名詞,接尾,助数詞")
            && super::has_reading(suffix, &["ネン"])
            && suffix.conj_type.is_empty()
            && suffix.conj_form.is_empty())
    {
        return None;
    }
    let native_day20 = raw == "二十"
        && pos_has(&suffix.pos, "名詞,接尾,助数詞")
        && match suffix.surface.as_ref() {
            "日" => super::has_reading(suffix, &["ニチ", "カ"]),
            "日間" => suffix.is_known && super::has_reading(suffix, &["ニチカン"]),
            _ => false,
        }
        && input.get(suffix.start.checked_sub(offset)?..suffix.end.checked_sub(offset)?)
            == Some(suffix.surface.as_ref());
    if (raw == "二十" && !native_day20) || (range.len() == 1 && first.is_known && !native_day20) {
        return None;
    }
    let protected_end = if native_day20 || finite_pair || comma_year || scaled {
        suffix.end
    } else {
        last.end
    };
    let protection = protected.partition_point(|p| p.end <= first.start);
    if protected
        .get(protection)
        .is_some_and(|p| p.start < protected_end)
    {
        return None;
    }
    if last.end != suffix.start
        // ゼロのない桁列は年・年度だけ。例外は標準の日・日間に直結した二十だけ。
        || (!raw.contains(['〇', '零'])
            && !matches!(suffix.surface.as_ref(), "年" | "年度")
            && !native_day20 && !finite_pair)
        || !quantity(suffix, &unit)
        || !ascii::unit_boundary(input, offset, suffix)
        || ((finite_pair || comma_year) && (suffix.start.checked_sub(offset).and_then(|start| suffix.end.checked_sub(offset).and_then(|end| input.get(start..end))) != Some(suffix.surface.as_ref())))
        || !out[range.clone()].iter().all(|t| {
            let (Some(start), Some(end)) = (t.start.checked_sub(offset), t.end.checked_sub(offset))
            else {
                return false;
            };
            input.get(start..end) == Some(t.surface.as_ref())
                && standard_kanji_readings(&t.surface, &t.reading, &t.pronunciation)
                && (!(native_day20 || finite_pair || comma_year || scaled)
                    || super::numeral_value(std::iter::once(t.surface.as_ref())).is_some_and(
                        |value| {
                            super::numeral_value(std::iter::once(t.base_form.as_ref())) == Some(value)
                        },
                    ))
                && (!(finite_pair || comma_year || scaled) || (t.conj_type.is_empty() && t.conj_form.is_empty()))
        })
    {
        return None;
    }
    Some((raw, value))
}

/// 今回のチャンクに追加された範囲だけを圧縮し、助数詞の音便は後段の共通処理に任せる。
pub(crate) fn apply(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first: usize,
    plan: &AsciiPlan,
    protected: &[Range<usize>],
    unit: impl Fn(&str) -> bool,
) {
    if !plan.has_kanji_in(offset, offset + input.len()) {
        return;
    }
    let (mut read, mut write) = (first, first);
    while read < out.len() {
        let start = read;
        read += 1;
        if number_token(&out[start]) {
            while read < out.len()
                && out[read - 1].end == out[read].start
                && number_token(&out[read])
            {
                read += 1;
            }
            if plan.kanji_number_start(out[start].start)
                && let Some((raw, value)) =
                    candidate(input, offset, out, start..read, protected, plan, &unit)
            {
                let (reading, pronunciation) = ascii::integer_readings(value);
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
                write += 1;
                continue;
            }
        }
        for i in start..read {
            out.swap(write, i);
            write += 1;
        }
    }
    out.truncate(write);
    super::fullwidth::apply(input, offset, out, first, plan, protected);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair_tokens(text: &str, offset: usize) -> Vec<Token> {
        let mut out = tokens(text, offset);
        for t in &mut out {
            t.base_form = Arc::clone(&t.surface);
            t.conj_type = "".into();
            t.conj_form = "".into();
            if t.surface.as_ref() == "歳" {
                t.pos = "名詞,接尾,助数詞,*".into();
                t.reading = "サイ".into();
                t.pronunciation = "サイ".into();
            }
        }
        out
    }

    #[test]
    fn pair_guards_and_global_boundaries() {
        for (full, local, offset) in [("一八歳", "一八歳", 0), ("一八歳", "一八歳", 51)]
        {
            let mut out = pair_tokens(local, offset);
            let plan = AsciiPlan::new(full, offset);
            apply(local, offset, &mut out, 0, &plan, &[], |_| false);
            assert_eq!(out.len(), 2);
            assert_eq!(out[0].surface.as_ref(), "一八");
            assert_eq!(out[0].base_form.as_ref(), "一八");
            assert_eq!((out[0].start, out[0].end), (offset, offset + 6));
            assert_eq!(out[0].word_cost, 200);
            assert!(!out[0].is_known);
            assert_eq!(out[0].reading.as_ref(), "ジュウハチ");
        }
        for full in ["日本語,一八歳", "日本語,一八歳,です", "日本語,一八歳."] {
            let mut out = pair_tokens("一八歳", 10);
            let plan = AsciiPlan::new(full, 0);
            apply("一八歳", 10, &mut out, 0, &plan, &[], |_| false);
            assert_eq!(out.len(), 2);
            assert_eq!(out[0].surface.as_ref(), "一八");
            assert_eq!((out[0].start, out[0].end), (10, 16));
        }
        for (full, offset) in [
            ("ISO ,一八歳", 5),
            ("A,一八歳", 2),
            ("1,一八歳", 2),
            ("１,一八歳", 4),
            ("一,一八歳", 4),
            (",,一八歳", 2),
            ("コード,一八歳", 10),
            ("電話番号,一八歳", 13),
            ("日本語,一八歳A", 10),
            ("日本語,一八歳.2", 10),
            ("日本語,一八歳,A", 10),
            ("日本語,一八歳,,です", 10),
        ] {
            let mut out = pair_tokens("一八歳", offset);
            let before = format!("{out:?}");
            let plan = AsciiPlan::new(full, 0);
            apply("一八歳", offset, &mut out, 0, &plan, &[], |_| false);
            assert_eq!(format!("{out:?}"), before, "{full}");
        }
        for (full, local, offset) in [
            ("A 一八歳", "一八歳", 2),
            ("一八歳A", "一八歳", 0),
            ("一八歳Ａ", "一八歳", 0),
            ("一八歳/", "一八歳", 0),
            ("一八歳", "八歳", 3),
            ("一八歳", "一八", 0),
            ("一八歳", "一八 歳", 0),
            ("一八歳", "一 八歳", 0),
            ("一一八歳", "一八歳", 3),
            ("一八〇歳", "一八〇", 0),
        ] {
            let mut out = pair_tokens(local, offset);
            let before = out.clone();
            let plan = AsciiPlan::new(full, 0);
            apply(local, offset, &mut out, 0, &plan, &[], |_| false);
            assert_eq!(
                format!("{out:?}"),
                format!("{before:?}"),
                "{full:?}/{local:?}"
            );
        }
    }

    #[test]
    fn pair_standard_fields_known_whole_and_protection() {
        for mutation in 0..14 {
            let mut out = pair_tokens("一八歳", 0);
            let mut protected = Vec::new();
            match mutation {
                0 => out[0].base_form = "ordinary".into(),
                1 => out[0].base_form = "二".into(),
                2 => out[0].reading = "ヒト".into(),
                3 => out[1].pronunciation = "ハッ".into(),
                4 => out[0].conj_type = "fixture".into(),
                5 => out[1].pos = "名詞,固有名詞,人名,名".into(),
                6 => out[2].is_known = false,
                7 => out[2].pos = "名詞,一般,*,*".into(),
                8 => out[2].base_form = "ordinary".into(),
                9 => out[2].reading = "トシ".into(),
                10 => out[2].conj_form = "fixture".into(),
                11 => protected.push(0..1),
                12 => protected.push(6..7),
                13 => out[1].end -= 1,
                _ => unreachable!(),
            }
            let before = out.clone();
            let plan = AsciiPlan::new("一八歳", 0);
            apply("一八歳", 0, &mut out, 0, &plan, &protected, |_| false);
            assert_eq!(
                format!("{out:?}"),
                format!("{before:?}"),
                "mutation {mutation}"
            );
        }
        let mut out = pair_tokens("一八歳", 0);
        out[0].surface = "一八".into();
        out[0].base_form = "一八".into();
        out[0].end = 6;
        out[0].reading = "ジュウハチ".into();
        out[0].pronunciation = "ジューハチ".into();
        out.remove(1);
        let before = out.clone();
        let plan = AsciiPlan::new("一八歳", 0);
        apply("一八歳", 0, &mut out, 0, &plan, &[], |_| false);
        assert_eq!(format!("{out:?}"), format!("{before:?}"));
    }

    #[test]
    fn pair_saturating_cost_and_prior_output() {
        for cost in [i16::MAX, i16::MIN] {
            let mut out = pair_tokens("一八歳", 70);
            out[0].word_cost = cost;
            out[1].word_cost = cost;
            let previous = pair_tokens("前", 0).remove(0);
            out.insert(0, previous.clone());
            let suffix = out.last().unwrap().clone();
            let plan = AsciiPlan::new("一八歳", 70);
            apply("一八歳", 70, &mut out, 1, &plan, &[], |_| false);
            assert_eq!(format!("{:?}", out[0]), format!("{previous:?}"));
            assert_eq!(format!("{:?}", out[2]), format!("{suffix:?}"));
            assert_eq!(out[1].word_cost, cost);
        }
    }

    #[test]
    fn japanese_comma_year_requires_a_complete_standard_frame() {
        let raw = "二〇二四年";
        let full = format!("日本語,{raw}");
        let plan = AsciiPlan::new(&full, 0);
        for mutation in 0..12 {
            let mut out = pair_tokens(raw, 10);
            let mut protected = Vec::new();
            match mutation {
                0 => {}
                1 => out[4].is_known = false,
                2 => out[4].reading = "トシ".into(),
                3 => out[4].pos = "名詞,一般,*,*".into(),
                4 => out[4].base_form = "ordinary".into(),
                5 => out[4].conj_type = "fixture".into(),
                6 => out[0].base_form = "ordinary".into(),
                7 => out[0].base_form = "三".into(),
                8 => out[0].conj_form = "fixture".into(),
                9 => protected.push(22..23),
                10 => protected.push(10..11),
                11 => out[0].reading = "ナマエ".into(),
                _ => unreachable!(),
            }
            let before = format!("{out:?}");
            apply(raw, 10, &mut out, 0, &plan, &protected, |_| false);
            if mutation == 0 {
                assert_eq!(out.len(), 2);
                assert_eq!(out[0].reading.as_ref(), "ニセンニジュウヨン");
                assert_eq!(out[0].word_cost, 400);
                assert_eq!(out[0].base_form.as_ref(), "二〇二四");
            } else {
                assert_eq!(format!("{out:?}"), before, "mutation {mutation}");
            }
        }
        for full in [
            "ISO ,二〇二四年",
            "A,二〇二四年",
            "1,二〇二四年",
            "１,二〇二四年",
            "一,二〇二四年",
            ",,二〇二四年",
            "日本語,二〇二四年A",
            "日本語,二〇二四年.2",
            "日本語,二〇二四年度",
            "日本語,二〇二四年間",
        ] {
            let start = full.find("二〇二四").unwrap();
            let mut out = pair_tokens(raw, start);
            let before = format!("{out:?}");
            let plan = AsciiPlan::new(full, 0);
            apply(raw, start, &mut out, 0, &plan, &[], |_| false);
            assert_eq!(format!("{out:?}"), before, "{full}");
        }
        for (full, local, start) in [
            ("日本語,二〇二四年", "〇二四年", 13),
            ("日本語,二〇二四年", "二〇二四", 10),
            ("日本語,二〇二四年A", "二〇二四年", 10),
        ] {
            let mut out = pair_tokens(local, start);
            let before = format!("{out:?}");
            let plan = AsciiPlan::new(full, 0);
            apply(local, start, &mut out, 0, &plan, &[], |_| false);
            assert_eq!(format!("{out:?}"), before, "{full}/{local}");
        }
    }

    #[test]
    fn fullwidth_comma_accepts_japanese_and_keeps_identifier_contexts() {
        for text in [
            "日本語,１８歳",
            "かな,１８歳",
            "カナ,１８歳",
            "日本語、１８歳",
        ] {
            let start = text.find('１').unwrap();
            assert!(AsciiPlan::new(text, 0).kanji_number_start(start), "{text}");
        }
        for text in [
            "ISO ,１８歳",
            "ISO , １８歳",
            "A ,１８歳",
            "ＩＤ,１８歳",
            "ＩＤ ,１８歳",
            "コード,１８歳",
            "型番 ,１８歳",
            "番号,１８歳",
            "1,１８歳",
            "１,１８歳",
            "一,１８歳",
            ",,１８歳",
            ",１８歳",
        ] {
            let starts = AsciiPlan::new(text, 0);
            let last_start = text.rfind("１８").unwrap();
            assert!(!starts.kanji_number_start(last_start), "{text}");
        }
    }

    fn tokens(text: &str, offset: usize) -> Vec<Token> {
        text.char_indices()
            .filter(|&(_, c)| !c.is_whitespace())
            .map(|(i, c)| {
                let surface = c.to_string();
                let (pos, reading, pronunciation) = if digit(c).is_some() {
                    let r = super::super::numeral_glyph_readings(None, c, None)[0];
                    ("名詞,数,*,*", r, if r == "キュウ" { "キュー" } else { r })
                } else {
                    match c {
                        '十' => ("名詞,数,*,*", "ジュウ", "ジュー"),
                        '円' => ("名詞,接尾,助数詞,*", "エン", "エン"),
                        '年' => ("名詞,接尾,助数詞,*", "ネン", "ネン"),
                        '本' => ("名詞,接尾,助数詞,*", "ホン", "ホン"),
                        '匹' => ("名詞,接尾,助数詞,*", "ヒキ", "ヒキ"),
                        '人' => ("名詞,接尾,助数詞,*", "ニン", "ニン"),
                        '社' => ("名詞,接尾,助数詞,*", "シャ", "シャ"),
                        '日' => ("名詞,接尾,助数詞,*", "ニチ", "ニチ"),
                        '章' => ("名詞,接尾,助数詞,*", "ショウ", "ショー"),
                        '番' => ("名詞,接尾,助数詞,*", "バン", "バン"),
                        '号' => ("名詞,接尾,助数詞,*", "ゴウ", "ゴー"),
                        '版' => ("名詞,接尾,助数詞,*", "ハン", "ハン"),
                        _ => ("記号,一般,*,*", "", ""),
                    }
                };
                Token {
                    surface: surface.into(),
                    start: offset + i,
                    end: offset + i + c.len_utf8(),
                    pos: pos.into(),
                    base_form: format!("base:{c}").into(),
                    conj_type: "fixture-type".into(),
                    conj_form: "fixture-form".into(),
                    reading: reading.into(),
                    pronunciation: pronunciation.into(),
                    word_cost: 100,
                    is_known: true,
                }
            })
            .collect()
    }

    fn apply_to(text: &str, out: &mut Vec<Token>) {
        apply(text, 0, out, 0, &AsciiPlan::new(text, 0), &[], |_| false);
    }

    fn twenty_tokens(text: &str, offset: usize, single: bool) -> Vec<Token> {
        let mut out = tokens(text, offset);
        for t in &mut out {
            if number_token(t) {
                t.base_form = Arc::clone(&t.surface);
            }
        }
        if single {
            let mut number = out[0].clone();
            number.surface = "二十".into();
            number.base_form = Arc::clone(&number.surface);
            number.end = offset + "二十".len();
            number.reading = "ニジュウ".into();
            number.pronunciation = "ニジュー".into();
            out.splice(0..2, [number]);
        }
        out
    }

    fn day_span_tokens(text: &str, offset: usize, single: bool) -> Vec<Token> {
        let mut out = twenty_tokens(text, offset, single);
        let day = out.iter().position(|t| t.surface.as_ref() == "日").unwrap();
        let mut suffix = out[day].clone();
        suffix.surface = "日間".into();
        suffix.base_form = "known:日間".into();
        suffix.end = out[day + 1].end;
        suffix.reading = "ニチカン".into();
        suffix.pronunciation = "ニチカン".into();
        out.splice(day..day + 2, [suffix]);
        out
    }

    #[test]
    fn exact_twenty_day_span_keeps_the_known_counter_whole() {
        for single in [false, true] {
            for tail in ["", "後"] {
                let text = format!("二十日間{tail}");
                let mut out = day_span_tokens(&text, 0, single);
                let original_suffix = out[if single { 1 } else { 2 }].clone();
                apply_to(&text, &mut out);
                assert_eq!(out[0].surface.as_ref(), "二十");
                assert!(!out[0].is_known);
                assert_eq!(out[1].surface.as_ref(), "日間");
                assert_eq!(format!("{:?}", out[1]), format!("{original_suffix:?}"));
                super::super::apply_counter_readings(&mut out);
                assert_eq!(out[0].reading.as_ref(), "ハツ");
                let mut expected_suffix = original_suffix;
                expected_suffix.reading = "カカン".into();
                expected_suffix.pronunciation = "カカン".into();
                assert_eq!(format!("{:?}", out[1]), format!("{expected_suffix:?}"));
                let once = format!("{out:?}");
                apply_to(&text, &mut out);
                super::super::apply_counter_readings(&mut out);
                assert_eq!(format!("{out:?}"), once);
            }
        }
    }

    #[test]
    fn twenty_day_span_rejects_ids_other_values_unreviewed_suffixes_and_protection() {
        for text in [
            "第二十日間",
            "コード二十日間",
            "A二十日間B",
            "ID 二十日間",
            "二十日間A",
            "二十零日間",
            "三二十日間",
            "二 十日間",
            "二十 日間",
            "十四日間",
            "二十四日間",
        ] {
            let mut out = day_span_tokens(text, 0, false);
            let before = format!("{out:?}");
            apply_to(text, &mut out);
            assert_eq!(format!("{out:?}"), before, "{text}");
        }
        for defect in 0..6 {
            let mut out = day_span_tokens("二十日間", 0, false);
            match defect {
                0 => out[2].is_known = false,
                1 => out[2].pos = "名詞,一般,*,*".into(),
                2 => out[2].pos = "名詞,固有名詞,人名,姓".into(),
                3 => out[2].reading = "ヒマ".into(),
                4 => out[2].pronunciation = "ジツカン".into(),
                5 => out[2].end -= 3,
                _ => unreachable!(),
            }
            let before = format!("{out:?}");
            apply_to("二十日間", &mut out);
            assert_eq!(format!("{out:?}"), before, "defect {defect}");
        }
        let offset = 29;
        for protected in [
            offset + 3..offset + 6,
            offset + 6..offset + 9,
            offset + 9..offset + 12,
        ] {
            let mut out = day_span_tokens("二十日間", offset, false);
            let before = format!("{out:?}");
            apply(
                "二十日間",
                offset,
                &mut out,
                0,
                &AsciiPlan::new("二十日間", offset),
                std::slice::from_ref(&protected),
                |_| false,
            );
            assert_eq!(format!("{out:?}"), before, "{protected:?}");
        }
    }

    #[test]
    fn exact_twenty_day_synthesizes_one_safe_number_before_native_reading() {
        for tail in ["", "間", "後"] {
            for single in [false, true] {
                for day in ["ニチ", "カ"] {
                    let text = format!("二十日{tail}");
                    let mut out = twenty_tokens(&text, 0, single);
                    let day_index = if single { 1 } else { 2 };
                    out[day_index].reading = day.into();
                    out[day_index].pronunciation = day.into();
                    let original_day = out[day_index].clone();
                    apply_to(&text, &mut out);
                    let number = &out[0];
                    assert_eq!(number.surface.as_ref(), "二十");
                    assert_eq!(number.base_form, number.surface);
                    assert_eq!((number.start, number.end), (0, 6));
                    assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
                    assert_eq!(number.reading.as_ref(), "ニジュウ");
                    assert_eq!(number.pronunciation.as_ref(), "ニジュー");
                    assert_eq!(number.word_cost, if single { 100 } else { 200 });
                    assert!(!number.is_known);
                    assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
                    assert_eq!(format!("{:?}", out[1]), format!("{original_day:?}"));
                    super::super::apply_counter_readings(&mut out);
                    assert_eq!(out[0].reading.as_ref(), "ハツ");
                    assert_eq!(out[0].pronunciation.as_ref(), "ハツ");
                    let mut expected_day = original_day;
                    expected_day.reading = "カ".into();
                    expected_day.pronunciation = "カ".into();
                    assert_eq!(format!("{:?}", out[1]), format!("{expected_day:?}"));
                    let once = format!("{out:?}");
                    apply_to(&text, &mut out);
                    super::super::apply_counter_readings(&mut out);
                    assert_eq!(format!("{out:?}"), once, "{text}, single={single}");
                }
            }
        }
    }

    #[test]
    fn twenty_day_plan_does_not_expand_to_other_places_units_or_identifiers() {
        assert_eq!(value("二十"), Some(20));
        for raw in ["十", "二十一", "二十零", "三二十", "一二十", "二 十"] {
            assert_eq!(value(raw), None, "{raw}");
        }
        for text in [
            "第二十日",
            "コード二十日",
            "A二十日B",
            "ID 二十日",
            "二十日A",
            "二十人",
            "二十円",
            "二十年",
            "二十零日",
            "三二十日",
            "二 十日",
            "二\t十日",
            "二十 日",
            "２０日",
            "Ａ二十日",
            "二十日Ｂ",
        ] {
            let mut out = tokens(text, 0);
            let before = format!("{out:?}");
            apply_to(text, &mut out);
            assert_eq!(format!("{out:?}"), before, "{text}");
        }
    }

    #[test]
    fn twenty_day_requires_standard_pos_reading_and_complete_adjacent_spans() {
        for single in [false, true] {
            for defect in 0..10 {
                let mut out = twenty_tokens("二十日", 0, single);
                let day = out.len() - 1;
                match defect {
                    0 => out[0].pos = "名詞,固有名詞,人名,姓".into(),
                    1 => out[0].reading = "ニイ".into(),
                    2 => out[day - 1].pronunciation = "テン".into(),
                    3 => out[day].pos = "名詞,一般,*,*".into(),
                    4 => out[day].reading = "ジツ".into(),
                    5 => out[day].pronunciation = "ジツ".into(),
                    6 => out[day].start += 1,
                    7 => out[day].end -= 1,
                    8 => out[0].base_form = "人名の原形".into(),
                    9 => out[0].base_form = "三".into(),
                    _ => unreachable!(),
                }
                let before = format!("{out:?}");
                apply_to("二十日", &mut out);
                assert_eq!(
                    format!("{out:?}"),
                    before,
                    "single={single}, defect={defect}"
                );
            }
        }
        for (surface, pos, reading) in [
            ("二十日", "名詞,固有名詞,一般,*", "ハツカ"),
            ("二十", "名詞,固有名詞,人名,姓", "ニジュウ"),
        ] {
            let mut out = tokens("二十日", 0);
            let limit = if surface == "二十日" { 3 } else { 2 };
            let mut known = out[0].clone();
            known.surface = surface.into();
            known.end = surface.len();
            known.pos = pos.into();
            known.reading = reading.into();
            known.pronunciation = reading.into();
            out.splice(0..limit, [known]);
            let before = format!("{out:?}");
            apply_to("二十日", &mut out);
            assert_eq!(format!("{out:?}"), before, "{surface}, {pos}");
        }
    }

    #[test]
    fn twenty_day_preserves_global_plan_guards_offsets_prior_output_and_protection() {
        let full = "前。二十日後";
        let offset = 37;
        let start = full.find('二').unwrap();
        let chunk = &full[start..];
        let mut out = tokens("前", 0);
        let prior = format!("{:?}", out[0]);
        out.extend(twenty_tokens(chunk, offset + start, false));
        apply(
            chunk,
            offset + start,
            &mut out,
            1,
            &AsciiPlan::new(full, offset),
            &[],
            |_| false,
        );
        assert_eq!(format!("{:?}", out[0]), prior);
        assert_eq!(out[1].surface.as_ref(), "二十");
        assert_eq!(
            (out[1].start, out[1].end),
            (offset + start, offset + start + 6)
        );
        assert!(!out[1].is_known);
        for t in &out[1..] {
            assert_eq!(&full[t.start - offset..t.end - offset], t.surface.as_ref());
        }
        for prefix in ["ID ", "ID.\n", "コード（\n", "第"] {
            let full = format!("{prefix}二十日");
            let start = full.find('二').unwrap();
            let chunk = &full[start..];
            let mut out = tokens(chunk, offset + start);
            let before = format!("{out:?}");
            apply(
                chunk,
                offset + start,
                &mut out,
                0,
                &AsciiPlan::new(&full, offset),
                &[],
                |_| false,
            );
            assert_eq!(format!("{out:?}"), before, "{full}");
        }
        for protected in [
            offset..offset + 3,
            offset + 3..offset + 6,
            offset + 6..offset + 9,
        ] {
            let mut out = twenty_tokens("二十日", offset, false);
            let before = format!("{out:?}");
            apply(
                "二十日",
                offset,
                &mut out,
                0,
                &AsciiPlan::new("二十日", offset),
                std::slice::from_ref(&protected),
                |_| false,
            );
            assert_eq!(format!("{out:?}"), before, "{protected:?}");
        }
    }

    #[test]
    fn quantities_use_shared_positional_readings_and_following_counter_euphony() {
        for (text, reading, pronunciation) in [
            (
                "二〇四二年",
                "ニセンヨンジュウニネン",
                "ニセンヨンジューニネン",
            ),
            ("六〇〇円", "ロッピャクエン", "ロッピャクエン"),
            (
                "一九八七年",
                "センキュウヒャクハチジュウナナネン",
                "センキューヒャクハチジューナナネン",
            ),
            ("三〇〇本", "サンビャッポン", "サンビャッポン"),
            ("六〇〇匹", "ロッピャッピキ", "ロッピャッピキ"),
            ("八〇〇〇円", "ハッセンエン", "ハッセンエン"),
            ("一〇〇〇〇〇円", "ジュウマンエン", "ジューマンエン"),
            (
                "一九八八年",
                "センキュウヒャクハチジュウハチネン",
                "センキューヒャクハチジューハチネン",
            ),
        ] {
            let mut out = tokens(text, 0);
            apply_to(text, &mut out);
            super::super::apply_counter_readings(&mut out);
            assert_eq!(out.len(), 2, "{text}: {out:?}");
            assert_eq!(
                out.iter().map(|t| t.reading.as_ref()).collect::<String>(),
                reading,
                "{text}"
            );
            assert_eq!(
                out.iter()
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                pronunciation,
                "{text}"
            );
            for token in &out {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
    }

    #[test]
    fn synthetic_span_cost_and_prior_output_are_preserved() {
        let text = "一〇三円六〇〇本";
        let offset = 91;
        let mut out = tokens("前", 0);
        out.extend(tokens(text, offset));
        let prior = format!("{:?}", out[0]);
        for (t, cost) in out[1..4].iter_mut().zip([30000, 30000, -30000]) {
            t.word_cost = cost;
        }
        let suffix = format!("{:?}", out[4]);
        apply(
            text,
            offset,
            &mut out,
            1,
            &AsciiPlan::new(text, offset),
            &[],
            |_| false,
        );
        assert_eq!(format!("{:?}", out[0]), prior);
        assert_eq!(format!("{:?}", out[2]), suffix);
        assert_eq!(out[1].surface.as_ref(), "一〇三");
        assert_eq!((out[1].start, out[1].end), (offset, offset + 9));
        assert_eq!(out[1].word_cost, 30000);
        assert_eq!(out[1].base_form, out[1].surface);
        assert_eq!(out[1].pos.as_ref(), "名詞,数,*,*");
        assert!(!out[1].is_known);
        assert!(out[1].conj_type.is_empty() && out[1].conj_form.is_empty());
        assert_eq!(out[3].surface.as_ref(), "六〇〇");
        let once = format!("{out:?}");
        apply(
            text,
            offset,
            &mut out,
            1,
            &AsciiPlan::new(text, offset),
            &[],
            |_| false,
        );
        assert_eq!(format!("{out:?}"), once);
        for t in &out[1..] {
            assert_eq!(&text[t.start - offset..t.end - offset], t.surface.as_ref());
        }
    }

    #[test]
    fn bare_numbers_identifiers_and_malformed_runs_remain_unchanged() {
        for text in [
            "一二三",
            "二三人",
            "三五社",
            "一二三社",
            "五六年",
            "四四人",
            "一四日",
            "〇〇年",
            "〇一年",
            "零一円",
            "版番号一二三円",
            "一二三番",
            "一二三号",
            "一二三版",
            "第一二章",
            "型番一二円",
            "電話一二三年",
            "番号一二年",
            "ID一二年",
            "ID 一二年",
            "v一二年",
            "ＩＤ一二年",
            "2一二円",
            "２一二円",
            "一2二円",
            "一２二円",
            "千二三円",
            "一二千円",
            "一二百円",
            "二点一二円",
            "一二.三円",
            "一二・三円",
            "一二三-四五年",
            "一 二年",
            "一\t二円",
            "二〇三一 年",
            "一二円ID",
            "一二円²",
            "一二三四五六七八九一二三四五六七八円",
        ] {
            let mut out = tokens(text, 0);
            let before = format!("{out:?}");
            apply_to(text, &mut out);
            assert_eq!(format!("{out:?}"), before, "{text}");
        }
    }

    #[test]
    fn wrong_pos_readings_and_byte_spans_reject_the_entire_run() {
        let text = "二〇四二年";
        for defect in 0..5 {
            let mut out = tokens(text, 0);
            match defect {
                0 => out[0].pos = "名詞,固有名詞,人名,姓".into(),
                1 => out[1].reading = "マル".into(),
                2 => out[2].pronunciation = "アズマ".into(),
                3 => out[2].start += 1,
                4 => out[4].pos = "名詞,一般,*,*".into(),
                _ => unreachable!(),
            }
            let before = format!("{out:?}");
            apply_to(text, &mut out);
            assert_eq!(format!("{out:?}"), before, "defect {defect}");
        }
    }

    #[test]
    fn a_known_single_number_or_quantity_keeps_all_dictionary_fields() {
        for (surface, pos, reading) in [
            ("一二三", "名詞,数,*,*", "イチニサン"),
            ("一二三", "名詞,数,*,*", "ヒフミ"),
            ("一二三円", "名詞,固有名詞,一般,*", "ヒフミエン"),
        ] {
            let text = if surface.ends_with('円') {
                surface.into()
            } else {
                format!("{surface}円")
            };
            let mut out = tokens(&text, 0);
            let end = surface.len();
            let limit = out.iter().take_while(|t| t.end <= end).count();
            let mut first = out[0].clone();
            first.surface = surface.into();
            first.end = end;
            first.pos = pos.into();
            first.reading = reading.into();
            first.pronunciation = reading.into();
            out.splice(0..limit, [first]);
            let before = format!("{out:?}");
            apply_to(&text, &mut out);
            assert_eq!(format!("{out:?}"), before, "{text}");
        }
    }

    #[test]
    fn full_input_guards_survive_chunk_boundaries_and_protected_ranges() {
        for separator in [" ", "\n", ".\n", "（\n"] {
            let full = format!("ISO{separator}二〇四二年");
            let offset = 37;
            let start = full.find('二').unwrap();
            let chunk = &full[start..];
            let mut out = tokens(chunk, offset + start);
            let before = format!("{out:?}");
            apply(
                chunk,
                offset + start,
                &mut out,
                0,
                &AsciiPlan::new(&full, offset),
                &[],
                |_| false,
            );
            assert_eq!(format!("{out:?}"), before, "{full}");
        }
        let text = "二〇四二年、六〇〇円";
        let mut out = tokens(text, 0);
        let protected = 3..9;
        apply(
            text,
            0,
            &mut out,
            0,
            &AsciiPlan::new(text, 0),
            std::slice::from_ref(&protected),
            |_| false,
        );
        assert_eq!(out[0].surface.as_ref(), "二");
        assert!(out.iter().any(|t| t.surface.as_ref() == "六〇〇"));
    }

    #[test]
    fn recognized_units_require_the_whole_unit_boundary() {
        for (tail, accepted) in [
            ("", true),
            ("2", false),
            ("²", false),
            ("/s", false),
            ("ID", false),
        ] {
            let text = format!("六〇〇kg{tail}");
            let mut out = tokens(&text, 0);
            let mut unit = out[3].clone();
            unit.surface = "kg".into();
            unit.end += 1;
            unit.pos = "名詞,一般,*,*".into();
            unit.reading = "キログラム".into();
            unit.pronunciation = "キログラム".into();
            out.splice(3..5, [unit]);
            let before = format!("{out:?}");
            apply(&text, 0, &mut out, 0, &AsciiPlan::new(&text, 0), &[], |s| {
                s == "kg"
            });
            if accepted {
                assert_eq!(out[0].surface.as_ref(), "六〇〇");
                assert_eq!(out[0].pronunciation.as_ref(), "ロッピャク");
                assert_eq!(out[1].surface.as_ref(), "kg");
            } else {
                assert_eq!(format!("{out:?}"), before, "{text}");
            }
        }
    }
}

#[cfg(all(test, feature = "build"))]
mod analyzer_tests {
    use crate::dict::{DictBuilder, DictEntry};
    use crate::{Analyzer, Dictionary};

    fn day_dictionary(extra: Option<(&str, &str, &str, &str)>) -> Dictionary {
        let mut builder = DictBuilder::new();
        for (surface, pos, reading, pronunciation) in [
            ("二", "名詞,数,*,*", "ニ", "ニ"),
            ("十", "名詞,数,*,*", "ジュウ", "ジュー"),
            ("2", "名詞,数,*,*", "ニ", "ニ"),
            ("0", "名詞,数,*,*", "ゼロ", "ゼロ"),
            ("日", "名詞,接尾,助数詞,*", "ニチ", "ニチ"),
            ("間", "名詞,接尾,一般,*", "カン", "カン"),
            ("後", "名詞,接尾,副詞可能,*", "ゴ", "ゴ"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                reading: reading.into(),
                pronunciation: pronunciation.into(),
                base_form: if pos == "名詞,数,*,*" {
                    surface.into()
                } else {
                    format!("base:{surface}").into()
                },
                conj_type: "fixture-type".into(),
                conj_form: "fixture-form".into(),
                ..Default::default()
            });
        }
        // 実辞書と同じく、日+間より既知の複合助数詞を優先させる。
        builder.add_entry(DictEntry {
            surface: "日間".into(),
            pos: "名詞,接尾,助数詞,*".into(),
            reading: "ニチカン".into(),
            pronunciation: "ニチカン".into(),
            base_form: "base:日間".into(),
            conj_type: "fixture-type".into(),
            conj_form: "fixture-form".into(),
            cost: -100,
            ..Default::default()
        });
        if let Some((surface, pos, reading, pronunciation)) = extra {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                reading: reading.into(),
                pronunciation: pronunciation.into(),
                base_form: if pos == "名詞,数,*,*"
                    && reading == "ニジュウ"
                    && pronunciation == "ニジュー"
                {
                    surface.into()
                } else {
                    format!("known:{surface}").into()
                },
                conj_type: "known-type".into(),
                conj_form: "known-form".into(),
                cost: -1000,
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    #[test]
    fn analyzer_native_twenty_day_uses_one_number_for_split_and_known_numerals() {
        for extra in [None, Some(("二十", "名詞,数,*,*", "ニジュウ", "ニジュー"))] {
            let mut analyzer = Analyzer::from_dict(day_dictionary(extra));
            for (text, expected) in [
                ("二十日", "ハツカ"),
                ("二十日間", "ハツカカン"),
                ("二十日後", "ハツカゴ"),
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                assert_eq!(out[0].surface.as_ref(), "二十");
                assert_eq!(out[0].reading.as_ref(), "ハツ");
                assert!(!out[0].is_known);
                let day_span = text == "二十日間";
                assert_eq!(
                    out[1].surface.as_ref(),
                    if day_span { "日間" } else { "日" }
                );
                assert_eq!(
                    out[1].base_form.as_ref(),
                    if day_span { "base:日間" } else { "base:日" }
                );
                assert_eq!(out[1].conj_type.as_ref(), "fixture-type");
                assert_eq!(out[1].conj_form.as_ref(), "fixture-form");
                assert_eq!(
                    out[1].pronunciation.as_ref(),
                    if day_span { "カカン" } else { "カ" }
                );
                assert!(out[1].is_known);
                assert_eq!(
                    out.iter()
                        .map(|t| t.pronunciation.as_ref())
                        .collect::<String>(),
                    expected,
                    "{text}: {out:?}"
                );
                for t in &out {
                    assert_eq!(&text[t.start..t.end], t.surface.as_ref());
                }
            }
            for text in [
                "第二十日",
                "コード二十日",
                "A二十日B",
                "ID 二十日",
                "二十日A",
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                assert!(
                    !out.iter()
                        .any(|t| t.surface.as_ref() == "二十" && !t.is_known),
                    "{text}: {out:?}"
                );
                assert!(
                    !out.iter().any(|t| t.reading.as_ref() == "ハツ"),
                    "{text}: {out:?}"
                );
            }
            let text = "前。二十日後";
            let sentences = analyzer
                .try_tokenize_sentences(text, &crate::sentence::SplitOptions::default())
                .unwrap();
            let number = sentences
                .iter()
                .flat_map(|(_, tokens)| tokens)
                .find(|t| t.surface.as_ref() == "二十")
                .unwrap();
            assert_eq!(number.start, "前。".len());
            assert_eq!(number.end, "前。二十".len());
            assert_eq!(number.reading.as_ref(), "ハツ");
            assert!(!number.is_known);
        }
    }

    #[test]
    fn analyzer_native_twenty_day_does_not_reclassify_known_words_or_names() {
        for (surface, pos, reading, pronunciation) in [
            ("二十日", "名詞,副詞可能,*,*", "ハツカ", "ハツカ"),
            ("二十日", "名詞,固有名詞,人名,姓", "フタトカ", "フタトカ"),
            ("二十", "名詞,固有名詞,人名,姓", "ニジュウ", "ニジュー"),
            ("二十", "名詞,数,*,*", "フタソ", "フタソ"),
        ] {
            let mut analyzer =
                Analyzer::from_dict(day_dictionary(Some((surface, pos, reading, pronunciation))));
            let out = analyzer.try_tokenize("二十日").unwrap();
            let first = &out[0];
            assert_eq!(first.surface.as_ref(), surface);
            assert_eq!(first.pos.as_ref(), pos);
            assert_eq!(first.reading.as_ref(), reading);
            assert_eq!(first.pronunciation.as_ref(), pronunciation);
            assert_eq!(first.base_form.as_ref(), format!("known:{surface}"));
            assert_eq!(first.conj_type.as_ref(), "known-type");
            assert_eq!(first.conj_form.as_ref(), "known-form");
            assert_eq!(first.word_cost, -1000);
            assert!(first.is_known);
        }
    }

    fn dictionary() -> Dictionary {
        let mut builder = DictBuilder::new();
        for c in "〇零一二三四五六七八九".chars() {
            let reading = super::super::numeral_glyph_readings(None, c, None)[0];
            builder.add_entry(DictEntry {
                surface: c.to_string().into(),
                pos: "名詞,数,*,*".into(),
                reading: reading.into(),
                pronunciation: if reading == "キュウ" {
                    "キュー"
                } else {
                    reading
                }
                .into(),
                ..Default::default()
            });
        }
        for (surface, pos, reading, cost) in [
            ("年", "名詞,接尾,助数詞,*", "ネン", 0),
            ("年度", "名詞,一般,*,*", "ネンド", 0),
            ("円", "名詞,接尾,助数詞,*", "エン", 0),
            ("本", "名詞,接尾,助数詞,*", "ホン", 0),
            ("号", "名詞,接尾,助数詞,*", "ゴウ", 0),
            ("社", "名詞,接尾,助数詞,*", "シャ", 0),
            ("人", "名詞,接尾,助数詞,*", "ニン", 0),
            ("一一", "名詞,数,*,*", "ジュウイチ", -100),
            ("一二三", "名詞,固有名詞,人名,姓", "ヒフミ", -100),
            ("二四年", "名詞,固有名詞,一般,*", "ニシネン", -100),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
                base_form: format!("base:{surface}").into(),
                conj_type: "fixture-type".into(),
                conj_form: "fixture-form".into(),
                cost,
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    #[test]
    fn analyzer_merges_quantities_but_preserves_names_and_known_words() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for (text, expected) in [
            ("二〇四二年", "ニセンヨンジューニネン"),
            ("一九八七年", "センキューヒャクハチジューナナネン"),
            ("一九九一年度", "センキューヒャクキュージューイチネンド"),
            ("六〇〇円", "ロッピャクエン"),
            ("三〇〇本", "サンビャッポン"),
            ("一二三円", "ヒフミエン"),
            ("二四年", "ニシネン"),
            ("二三人", "ニサンニン"),
            ("三五社", "サンゴシャ"),
            ("五六年", "ゴロクネン"),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert_eq!(
                out.iter()
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                expected,
                "{text}: {out:?}"
            );
            for token in &out {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
        let out = analyzer.try_tokenize("一一年").unwrap();
        let number = &out[0];
        assert_eq!(number.surface.as_ref(), "一一");
        assert_eq!(number.base_form.as_ref(), "base:一一");
        assert_eq!(number.conj_type.as_ref(), "fixture-type");
        assert_eq!(number.conj_form.as_ref(), "fixture-form");
        assert_eq!(number.word_cost, -100);
        assert!(number.is_known);
    }

    #[test]
    fn analyzer_keeps_full_input_identifier_protection_in_each_sentence() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for text in ["ID 二〇四二年", "ISO.\n二〇四二年", "〇一年", "一二号"] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert!(
                !out.iter().any(|t| !t.is_known
                    && t.surface.chars().count() > 1
                    && t.pos.as_ref() == "名詞,数,*,*"),
                "{text}: {out:?}"
            );
            let sentences = analyzer
                .try_tokenize_sentences(text, &crate::sentence::SplitOptions::default())
                .unwrap();
            assert!(
                !sentences
                    .iter()
                    .flat_map(|(_, tokens)| tokens)
                    .any(|t| !t.is_known
                        && t.surface.chars().count() > 1
                        && t.pos.as_ref() == "名詞,数,*,*"),
                "sentences: {text}"
            );
        }
    }
}
