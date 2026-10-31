//! 純全角の整数数量。入力全体で数詞と一文字助数詞の終端を確認し、既知語を分割しない。

use super::{AsciiPlan, ascii, has_reading, pos_has};
use crate::lattice::Token;
use std::ops::Range;
use std::sync::Arc;

fn digit(c: char) -> Option<u64> {
    matches!(c, '０'..='９').then(|| u64::from(c as u32 - '０' as u32))
}

fn value(raw: &str) -> Option<u64> {
    let mut count = 0;
    let mut value = 0;
    for c in raw.chars() {
        let n = digit(c)?;
        if count == 1 && value == 0 {
            return None;
        }
        count += 1;
        if count > 16 {
            return None;
        }
        value = value * 10 + n;
    }
    (count > 0).then_some(value)
}

fn counter_surface(c: char) -> bool {
    matches!(
        c,
        '本' | '匹'
            | '杯'
            | '回'
            | '個'
            | '冊'
            | '刷'
            | '点'
            | '票'
            | '拍'
            | '泊'
            | '発'
            | '品'
            | '遍'
            | '服'
            | '篇'
            | '編'
            | '筆'
            | '体'
            | '束'
            | '隻'
            | '席'
            | '石'
            | '勝'
            | '戦'
            | '棟'
            | '箱'
            | '皿'
            | '軒'
            | '校'
            | '巻'
            | '課'
            | '機'
            | '階'
            | '件'
            | '期'
            | '曲'
            | '句'
            | '級'
            | '頭'
            | '通'
            | '週'
            | '章'
            | '節'
            | '足'
            | '歩'
            | '口'
            | '分'
            | '人'
            | '日'
            | '時'
            | '尺'
            | '寸'
            | '歳'
            | '才'
            | '都'
            | '府'
            | '県'
            | '号'
            | '年'
            | '円'
            | '枚'
            | '台'
            | '組'
            | '度'
    )
}

/// 位置は入力全体の位置。数詞の途中や、助数詞に英数字が続く位置を認可しない。
pub(super) fn number_starts(
    input: &str,
    offset: usize,
    allowed_prefix: impl Fn(&str) -> bool,
) -> Vec<usize> {
    let mut starts = Vec::new();
    if !input.bytes().any(|b| b == 0xef) {
        return starts;
    }
    let mut chars = input.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
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
        let raw = &input[start..end];
        let Some(counter) = input[end..].chars().next() else {
            continue;
        };
        let after = end + counter.len_utf8();
        if raw.chars().count() < 2 || value(raw).is_none() || !counter_surface(counter)
            || !allowed_prefix(&input[..start])
            || ["日間", "時間", "年度"].iter().any(|suffix| input[end..].starts_with(suffix))
            || input[after..].chars().next().is_some_and(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '^' | '/' | '*' | '+' | '-' | '=' | '²' | '³' | 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９')
            })
        { continue; }
        starts.push(offset + start);
    }
    starts
}

pub(super) fn standard_readings(raw: &str, reading: &str, pronunciation: &str) -> bool {
    value(raw).is_some_and(|n| ascii::standard_readings(&n.to_string(), reading, pronunciation))
}

fn number_token(t: &Token) -> bool {
    pos_has(&t.pos, "名詞,数")
        && !t.surface.is_empty()
        && t.surface.chars().all(|c| digit(c).is_some())
}

fn quantity(t: &Token) -> bool {
    let mut chars = t.surface.chars();
    if !t.is_known || !chars.next().is_some_and(counter_surface) || chars.next().is_some() {
        return false;
    }
    if super::counter(t).is_some() {
        return true;
    }
    pos_has(&t.pos, "名詞,接尾,助数詞")
        && match t.surface.as_ref() {
            "年" => has_reading(t, &["ネン"]),
            "円" => has_reading(t, &["エン"]),
            "枚" => has_reading(t, &["マイ"]),
            "台" => has_reading(t, &["ダイ"]),
            "組" => has_reading(t, &["クミ"]),
            "度" => has_reading(t, &["ド"]),
            _ => false,
        }
}

fn candidate<'a>(
    input: &'a str,
    offset: usize,
    out: &[Token],
    range: Range<usize>,
    protected: &[Range<usize>],
) -> Option<(&'a str, u64)> {
    let first = &out[range.start];
    let last = &out[range.end - 1];
    let raw = input.get(first.start.checked_sub(offset)?..last.end.checked_sub(offset)?)?;
    if raw.chars().count() < 2 || (range.len() == 1 && first.is_known) {
        return None;
    }
    let number = value(raw)?;
    let suffix = out.get(range.end)?;
    let protection = protected.partition_point(|p| p.end <= first.start);
    if last.end != suffix.start
        || !quantity(suffix)
        || protected
            .get(protection)
            .is_some_and(|p| p.start < suffix.end)
        || input.get(suffix.start.checked_sub(offset)?..suffix.end.checked_sub(offset)?)
            != Some(suffix.surface.as_ref())
        || !ascii::unit_boundary(input, offset, suffix)
        || !out[range].iter().all(|t| {
            t.start
                .checked_sub(offset)
                .zip(t.end.checked_sub(offset))
                .and_then(|(start, end)| input.get(start..end))
                == Some(t.surface.as_ref())
                && standard_readings(&t.surface, &t.reading, &t.pronunciation)
                && value(&t.surface).is_some_and(|n| {
                    super::numeral_value(std::iter::once(t.base_form.as_ref())) == Some(n)
                })
        })
    {
        return None;
    }
    Some((raw, number))
}

pub(super) fn apply(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first: usize,
    plan: &AsciiPlan,
    protected: &[Range<usize>],
) {
    if !input.bytes().any(|b| b == 0xef) {
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
                && let Some((raw, number)) = candidate(input, offset, out, start..read, protected)
            {
                let (reading, pronunciation) = ascii::integer_readings(number);
                let cost: i64 = out[start..read]
                    .iter()
                    .map(|t| i64::from(t.word_cost))
                    .sum();
                let surface: Arc<str> = Arc::from(raw);
                out[write] = Token {
                    base_form: Arc::clone(&surface),
                    surface,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str, offset: usize) -> Vec<Token> {
        text.char_indices()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(i, c)| {
                let raw = c.to_string();
                let (pos, r, p) = if let Some(n) = super::digit(c) {
                    let (r, p) = ascii::integer_readings(n);
                    ("名詞,数,*,*", r, p)
                } else {
                    let r = match c {
                        '歳' | '才' => "サイ",
                        '回' => "カイ",
                        '本' => "ホン",
                        '票' => "ヒョウ",
                        '年' => "ネン",
                        '円' => "エン",
                        '枚' => "マイ",
                        '台' => "ダイ",
                        '人' => "ニン",
                        '日' => "ニチ",
                        '時' => "ジ",
                        _ => raw.as_str(),
                    };
                    (
                        if counter_surface(c) {
                            "名詞,接尾,助数詞,*"
                        } else {
                            "記号,一般,*,*"
                        },
                        r.into(),
                        r.into(),
                    )
                };
                Token {
                    surface: raw.clone().into(),
                    base_form: raw.into(),
                    start: offset + i,
                    end: offset + i + c.len_utf8(),
                    pos: pos.into(),
                    conj_type: "".into(),
                    conj_form: "".into(),
                    reading: r.into(),
                    pronunciation: p.into(),
                    word_cost: 17,
                    is_known: true,
                }
            })
            .collect()
    }

    fn apply_to(text: &str, offset: usize, out: &mut Vec<Token>, first: usize, plan: &AsciiPlan) {
        let protected = ascii::apply(text, offset, out, first, plan, &mut 0, |_| false);
        apply(text, offset, out, first, plan, &protected);
    }

    fn readings(out: &[Token]) -> (String, String) {
        (
            out.iter().map(|t| t.reading.as_ref()).collect(),
            out.iter().map(|t| t.pronunciation.as_ref()).collect(),
        )
    }

    #[test]
    fn fullwidth_quantities_use_positional_readings_before_shared_counter_rules() {
        for (text, r, p) in [
            ("１８歳", "ジュウハッサイ", "ジューハッサイ"),
            ("２０回", "ニジュッカイ", "ニジュッカイ"),
            ("２４才", "ニジュウヨンサイ", "ニジューヨンサイ"),
            ("１２本", "ジュウニホン", "ジューニホン"),
            ("１００本", "ヒャッポン", "ヒャッポン"),
            ("１０００票", "センビョウ", "センビョウ"),
            ("１２人", "ジュウニニン", "ジューニニン"),
            ("２０日", "ハツカ", "ハツカ"),
            ("１４日", "ジュウヨッカ", "ジューヨッカ"),
            ("２４日", "ニジュウヨッカ", "ニジューヨッカ"),
            ("２０１８年", "ニセンジュウハチネン", "ニセンジューハチネン"),
            ("１２円", "ジュウニエン", "ジューニエン"),
            ("１２枚", "ジュウニマイ", "ジューニマイ"),
            ("１２台", "ジュウニダイ", "ジューニダイ"),
        ] {
            let mut out = tokens(text, 91);
            let original_digits = out.len() - 1;
            apply_to(text, 91, &mut out, 0, &AsciiPlan::new(text, 91));
            assert_eq!(out.len(), 2, "{text}: {out:?}");
            let number = &out[0];
            assert_eq!(number.surface.as_ref(), &text[..text.len() - 3]);
            assert_eq!(number.base_form, number.surface);
            assert_eq!((number.start, number.end), (91, 91 + text.len() - 3));
            assert_eq!(number.word_cost, 17 * original_digits as i16);
            assert!(!number.is_known);
            assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
            assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
            super::super::apply_counter_readings(&mut out);
            assert_eq!(readings(&out), (r.into(), p.into()), "{text}: {out:?}");
            let once = format!("{out:?}");
            apply_to(text, 91, &mut out, 0, &AsciiPlan::new(text, 91));
            super::super::apply_counter_readings(&mut out);
            assert_eq!(format!("{out:?}"), once, "{text}");
        }
    }

    #[test]
    fn fullwidth_identifiers_mixed_widths_zeroes_and_bare_runs_are_preserved() {
        for text in [
            "１８",
            "１８。",
            "０１８歳",
            "００回",
            "０歳",
            "１歳",
            "９歳",
            "１8歳",
            "1８歳",
            "123４歳",
            "１八歳",
            "一８歳",
            "１８〇歳",
            "１８.５歳",
            "１８．５歳",
            "１８,５歳",
            "Ａ１８歳",
            "ID１８歳",
            "ID １８歳",
            "ID (１８歳)",
            "型番１８歳",
            "番号１８回",
            "コード１８歳",
            "-１８歳",
            "+１８歳",
            "_１８歳",
            ".１８歳",
            "A,１８歳",
            "１,１８歳",
            "第１８章",
            "１８歳A",
            "１８歳2",
            "１８kg",
            "１８mAh",
            "１８日間",
            "２０日間",
            "２０時間",
            "２０１８年度",
            "９９９９９９９９９９９９９９９９９歳",
        ] {
            let mut out = tokens(text, 37);
            let mut prior = out.clone();
            let plan = AsciiPlan::new(text, 37);
            ascii::apply(text, 37, &mut prior, 0, &plan, &mut 0, |_| false);
            apply_to(text, 37, &mut out, 0, &plan);
            assert_eq!(format!("{out:?}"), format!("{prior:?}"), "{text}");
        }
        for text in [",１８歳", "ISO ,１８歳", "１ ,１８歳"] {
            let mut out = tokens(text, 0);
            apply_to(text, 0, &mut out, 0, &AsciiPlan::new(text, 0));
            assert!(
                !out.iter()
                    .any(|t| t.surface.as_ref() == "１８" && !t.is_known),
                "{text}: {out:?}"
            );
        }
    }

    #[test]
    fn global_number_and_counter_ends_prevent_partial_chunk_merges() {
        for (full, part, offset) in [
            ("１８２４歳", "８２４歳", 40),
            ("１８歳A", "１８歳", 37),
            ("ID １８歳", "１８歳", 40),
            ("１８日間", "１８日", 37),
        ] {
            let mut out = tokens("前", 0);
            let first = out.len();
            out.extend(tokens(part, offset));
            let before = format!("{out:?}");
            apply_to(part, offset, &mut out, first, &AsciiPlan::new(full, 37));
            assert_eq!(format!("{out:?}"), before, "{full} / {part}");
        }
        let full = "１８歳";
        let offset = 37;
        for which in 0..3 {
            let mut out = tokens(full, offset);
            let before = format!("{out:?}");
            let range = out[which].start..out[which].end;
            apply(
                full,
                offset,
                &mut out,
                0,
                &AsciiPlan::new(full, offset),
                &[range],
            );
            assert_eq!(format!("{out:?}"), before);
        }
    }

    #[test]
    fn known_whole_words_unreviewed_fields_and_counter_readings_are_preserved() {
        for mutation in 0..9 {
            let text = "１８歳";
            let mut out = tokens(text, 0);
            match mutation {
                0 => out[0].pos = "名詞,固有名詞,人名,名".into(),
                1 => out[0].reading = "ヒト".into(),
                2 => out[1].pronunciation = "ヤ".into(),
                3 => out[0].base_form = "ordinary".into(),
                4 => out[2].is_known = false,
                5 => out[2].pos = "名詞,一般,*,*".into(),
                6 => out[2].reading = "トシ".into(),
                7 => out[1].start += 1,
                _ => out[1].end -= 1,
            }
            let before = format!("{out:?}");
            apply(text, 0, &mut out, 0, &AsciiPlan::new(text, 0), &[]);
            assert_eq!(format!("{out:?}"), before, "mutation={mutation}");
        }
        let text = "１８歳";
        let mut out = tokens(text, 0);
        let mut known = out[0].clone();
        known.surface = "１８".into();
        known.base_form = "１８".into();
        known.end = 6;
        known.reading = "ジュウハチ".into();
        known.pronunciation = "ジューハチ".into();
        out.splice(0..2, [known]);
        let before = format!("{out:?}");
        apply(text, 0, &mut out, 0, &AsciiPlan::new(text, 0), &[]);
        assert_eq!(format!("{out:?}"), before);
        let mut whole = out[0].clone();
        whole.surface = text.into();
        whole.end = text.len();
        whole.pos = "名詞,固有名詞,一般,*".into();
        let mut out = vec![whole];
        let before = format!("{out:?}");
        apply(text, 0, &mut out, 0, &AsciiPlan::new(text, 0), &[]);
        assert_eq!(format!("{out:?}"), before);
    }

    #[test]
    fn composite_cost_is_summed_widely_and_original_prefix_output_is_untouched() {
        let text = "１８２歳";
        let mut out = tokens("前", 0);
        let prior = format!("{:?}", out[0]);
        let first = out.len();
        out.extend(tokens(text, 91));
        for (t, cost) in out[first..]
            .iter_mut()
            .zip([i16::MAX, i16::MAX, i16::MIN, 17])
        {
            t.word_cost = cost;
        }
        apply_to(text, 91, &mut out, first, &AsciiPlan::new(text, 91));
        assert_eq!(format!("{:?}", out[0]), prior);
        assert_eq!(out[first].word_cost, 32766);
        assert_eq!(out[first].start, 91);
        assert_eq!(out[first].end, 100);
    }
}

#[cfg(all(test, feature = "build"))]
mod analyzer_tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};
    use crate::{Analyzer, Dictionary};

    fn dictionary(known_numbers: bool) -> Dictionary {
        let mut b = DictBuilder::new();
        for n in 0..10 {
            let surface = char::from_u32('０' as u32 + n).unwrap().to_string();
            let (r, p) = ascii::integer_readings(u64::from(n));
            b.add_entry(DictEntry {
                surface: surface.clone().into(),
                base_form: surface.into(),
                pos: "名詞,数,*,*".into(),
                reading: r.into(),
                pronunciation: p.into(),
                cost: 17,
                ..Default::default()
            });
        }
        for (s, pos, r, p, cost) in [
            ("歳", "名詞,接尾,助数詞,*", "サイ", "サイ", 17),
            ("才", "名詞,接尾,助数詞,*", "サイ", "サイ", 17),
            ("回", "名詞,接尾,助数詞,*", "カイ", "カイ", 17),
            ("本", "名詞,接尾,助数詞,*", "ホン", "ホン", 17),
            ("票", "名詞,接尾,助数詞,*", "ヒョウ", "ヒョー", 17),
            ("日", "名詞,接尾,助数詞,*", "ニチ", "ニチ", 17),
            ("日間", "名詞,接尾,助数詞,*", "ニチカン", "ニチカン", -100),
            ("年", "名詞,接尾,助数詞,*", "ネン", "ネン", 17),
            ("A", "名詞,一般,*,*", "エー", "エー", 17),
            ("ID", "名詞,一般,*,*", "アイディー", "アイディー", 17),
            ("本田", "名詞,固有名詞,人名,姓", "ホンダ", "ホンダ", -100),
            (
                "１８禁",
                "名詞,一般,*,*",
                "ジュウハチキン",
                "ジューハチキン",
                -100,
            ),
        ] {
            b.add_entry(DictEntry {
                surface: s.into(),
                base_form: s.into(),
                pos: pos.into(),
                reading: r.into(),
                pronunciation: p.into(),
                cost,
                ..Default::default()
            });
        }
        if known_numbers {
            b.add_entry(DictEntry {
                surface: "１８".into(),
                base_form: "１８".into(),
                pos: "名詞,数,*,*".into(),
                reading: "イチハチ".into(),
                pronunciation: "イチハチ".into(),
                cost: -100,
                ..Default::default()
            });
            b.add_entry(DictEntry {
                surface: "２４歳".into(),
                base_form: "known-whole".into(),
                pos: "名詞,固有名詞,一般,*".into(),
                reading: "ベツヨミ".into(),
                pronunciation: "ベツヨミ".into(),
                cost: -100,
                ..Default::default()
            });
        }
        b.build().unwrap()
    }

    #[test]
    fn analyzer_fullwidth_quantities_and_sentences_preserve_original_utf8_surfaces() {
        let mut a = Analyzer::from_dict(dictionary(false));
        for _ in 0..2 {
            for (text, r, p) in [
                ("１８歳", "ジュウハッサイ", "ジューハッサイ"),
                ("２０回", "ニジュッカイ", "ニジュッカイ"),
                ("２４才", "ニジュウヨンサイ", "ニジューヨンサイ"),
                ("１００本", "ヒャッポン", "ヒャッポン"),
                ("１０００票", "センビョウ", "センビョー"),
                ("２０日", "ハツカ", "ハツカ"),
                ("２４日", "ニジュウヨッカ", "ニジューヨッカ"),
                ("２０１８年", "ニセンジュウハチネン", "ニセンジューハチネン"),
            ] {
                let out = a.try_tokenize(text).unwrap();
                assert_eq!(
                    out.iter().map(|t| t.reading.as_ref()).collect::<String>(),
                    r,
                    "{text}: {out:?}"
                );
                assert_eq!(
                    out.iter()
                        .map(|t| t.pronunciation.as_ref())
                        .collect::<String>(),
                    p,
                    "{text}"
                );
                assert_eq!(out.len(), 2);
                assert!(!out[0].is_known);
                assert_eq!(out[0].surface, out[0].base_form);
                for t in &out {
                    assert_eq!(&text[t.start..t.end], t.surface.as_ref());
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
        let out = a.try_tokenize("１８").unwrap();
        assert_eq!(
            out.iter().map(|t| t.reading.as_ref()).collect::<String>(),
            "イチハチ"
        );
    }

    #[test]
    fn analyzer_global_ids_known_words_and_multi_character_counters_are_unchanged() {
        let mut a = Analyzer::from_dict(dictionary(false));
        for text in ["ID１８歳", "ID\n１８歳", "１８歳A", "１８本田", "１８日間"] {
            let out = a.try_tokenize(text).unwrap();
            assert!(
                !out.iter()
                    .any(|t| t.surface.as_ref() == "１８" && !t.is_known),
                "{text}: {out:?}"
            );
        }
        let out = a.try_tokenize("１８禁").unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].is_known);
        assert_eq!(out[0].pronunciation.as_ref(), "ジューハチキン");
        let mut a = Analyzer::from_dict(dictionary(true));
        let out = a.try_tokenize("１８歳").unwrap();
        assert_eq!(out[0].surface.as_ref(), "１８");
        assert!(out[0].is_known);
        assert_eq!(out[0].reading.as_ref(), "イチハチ");
        assert_eq!(out[0].word_cost, -100);
        let out = a.try_tokenize("２４歳").unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].is_known);
        assert_eq!(out[0].pronunciation.as_ref(), "ベツヨミ");
        assert_eq!(out[0].base_form.as_ref(), "known-whole");
    }
}
