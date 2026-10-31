//! 標準の整数数量と既知の年/ネン。末尾4をヨネンへそろえ、識別子と別読みを保持する。
use super::{AsciiPlan, ascii, has_reading, is_numeral, pos_has};
use crate::lattice::Token;
use std::ops::Range;

fn scalar(raw: &str) -> Option<u64> {
    if raw.is_ascii() {
        return ascii::integer_value(raw);
    }
    let fw = raw.chars().all(|c| matches!(c, '０'..='９'));
    let han = raw.chars().all(|c| {
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
    });
    if raw.is_empty()
        || !(fw || han)
        || (raw.chars().count() > 1 && raw.starts_with(['０', '〇', '零']))
    {
        return None;
    }
    super::numeral_value(std::iter::once(raw))
}

fn numeric_char(c: char) -> bool {
    c.is_ascii_digit()
        || matches!(
            c,
            '０'
                ..='９'
                    | '〇'
                    | '零'
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
}

fn comma_boundary(prefix: &str) -> bool {
    super::japanese_boundary::comma_prefix(prefix)
}

pub(super) fn number_starts(
    input: &str,
    offset: usize,
    allowed: impl Fn(&str) -> bool,
) -> Vec<usize> {
    let mut out = Vec::new();
    if !input.contains('年') {
        return out;
    }
    let mut chars = input.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if !numeric_char(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        while let Some(&(i, c)) = chars.peek()
            && (numeric_char(c) || c == ',')
        {
            end = i + c.len_utf8();
            chars.next();
        }
        let raw = &input[start..end];
        let rest = &input[end..];
        if !scalar(raw).is_some_and(|n| n % 10 == 4)
            || !super::japanese_boundary::single_year_tail(rest)
            || !(allowed(&input[..start]) || comma_boundary(&input[..start]))
        {
            continue;
        }
        out.push(offset + start);
    }
    out
}

pub(crate) fn apply(
    input: &str,
    offset: usize,
    out: &mut [Token],
    plan: &AsciiPlan,
    protected: &[Range<usize>],
) {
    let starts = plan.year_number_starts();
    if starts.is_empty() {
        return;
    }
    for i in 1..out.len() {
        let t = &out[i];
        if !t.is_known
            || t.surface.as_ref() != "年"
            || t.base_form.as_ref() != "年"
            || !pos_has(&t.pos, "名詞,接尾,助数詞")
            || !has_reading(t, &["ネン"])
            || !t.conj_type.is_empty()
            || !t.conj_form.is_empty()
        {
            continue;
        }
        let mut start = i;
        while start > 0 && out[start - 1].end == out[start].start && is_numeral(&out[start - 1]) {
            start -= 1;
        }
        if start == i || starts.binary_search(&out[start].start).is_err() {
            continue;
        }
        let Some(left) = out[start].start.checked_sub(offset) else {
            continue;
        };
        let Some(right) = out[i - 1].end.checked_sub(offset) else {
            continue;
        };
        let Some(raw) = input.get(left..right) else {
            continue;
        };
        let Some(n) = scalar(raw) else { continue };
        if n % 10 != 4 {
            continue;
        }
        let end = out[i].end;
        if protected
            .iter()
            .any(|r| r.start < end && out[start].start < r.end)
        {
            continue;
        }
        if !out[start..=i].iter().all(|t| {
            t.start
                .checked_sub(offset)
                .zip(t.end.checked_sub(offset))
                .and_then(|(a, b)| input.get(a..b))
                == Some(t.surface.as_ref())
        }) {
            continue;
        }
        if !out[start..i].iter().all(|t| {
            scalar(&t.surface).is_some_and(|v| scalar(&t.base_form) == Some(v))
                && !t.reading.is_empty()
                && !t.pronunciation.is_empty()
                && t.conj_type.is_empty()
                && t.conj_form.is_empty()
        }) {
            continue;
        }
        let r: String = out[start..i].iter().map(|t| t.reading.as_ref()).collect();
        let p: String = out[start..i]
            .iter()
            .map(|t| t.pronunciation.as_ref())
            .collect();
        if !ascii::standard_readings(&n.to_string(), &r, &p)
            && !(raw.chars().any(|c| super::place(c).is_some())
                && super::standard_kanji_readings(raw, &r, &p))
        {
            continue;
        }
        super::replace_tail(&mut out[i - 1], &["ヨン"], "ヨ", "ヨ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(raw: &str, reading: &str, pronunciation: &str, offset: usize) -> Vec<Token> {
        vec![
            Token {
                surface: raw.into(),
                start: offset,
                end: offset + raw.len(),
                pos: "名詞,数,*,*".into(),
                conj_type: "".into(),
                conj_form: "".into(),
                base_form: raw.into(),
                reading: reading.into(),
                pronunciation: pronunciation.into(),
                word_cost: 137,
                is_known: true,
            },
            Token {
                surface: "年".into(),
                start: offset + raw.len(),
                end: offset + raw.len() + 3,
                pos: "名詞,接尾,助数詞,*".into(),
                conj_type: "".into(),
                conj_form: "".into(),
                base_form: "年".into(),
                reading: "ネン".into(),
                pronunciation: "ネン".into(),
                word_cost: -23,
                is_known: true,
            },
        ]
    }

    #[test]
    fn standard_terminal_four_changes_only_its_two_readings() {
        for (raw, r, p) in [
            ("4", "ヨン", "ヨン"),
            ("14", "ジュウヨン", "ジューヨン"),
            ("24", "ニジュウヨン", "ニジューヨン"),
            ("2024", "ニセンニジュウヨン", "ニセンニジューヨン"),
            ("四", "ヨン", "ヨン"),
            ("十四", "ジュウヨン", "ジューヨン"),
            ("二十四", "ニジュウヨン", "ニジューヨン"),
            ("二〇二四", "ニセンニジュウヨン", "ニセンニジューヨン"),
            ("２０２４", "ニセンニジュウヨン", "ニセンニジューヨン"),
            ("三千四", "サンゼンヨン", "サンゼンヨン"),
            ("六百四", "ロッピャクヨン", "ロッピャクヨン"),
        ] {
            let text = format!("{raw}年");
            let mut out = tokens(raw, r, p, 31);
            let before = out.clone();
            apply(&text, 31, &mut out, &AsciiPlan::new(&text, 31), &[]);
            assert_eq!(
                out[0].reading.as_ref(),
                r.strip_suffix('ン').unwrap(),
                "{text}"
            );
            assert_eq!(
                out[0].pronunciation.as_ref(),
                p.strip_suffix('ン').unwrap(),
                "{text}"
            );
            let mut normalized = out.clone();
            for (a, b) in normalized.iter_mut().zip(&before) {
                a.reading = b.reading.clone();
                a.pronunciation = b.pronunciation.clone();
            }
            assert_eq!(format!("{normalized:?}"), format!("{before:?}"));
        }
    }

    #[test]
    fn whole_run_fields_counter_fields_and_protection_must_be_standard() {
        for mutation in 0..15 {
            let mut out = tokens("24", "ニジュウヨン", "ニジューヨン", 0);
            let mut protected = Vec::new();
            match mutation {
                0 => out[0].reading = "ニジュウシ".into(),
                1 => out[0].pronunciation = "ニジューヨ".into(),
                2 => out[0].base_form = "ordinary".into(),
                3 => out[0].base_form = "14".into(),
                4 => out[0].pos = "名詞,固有名詞,人名,名".into(),
                5 => out[0].conj_type = "fixture".into(),
                6 => out[1].is_known = false,
                7 => out[1].base_form = "ordinary".into(),
                8 => out[1].pos = "名詞,一般,*,*".into(),
                9 => out[1].reading = "トシ".into(),
                10 => out[1].conj_form = "fixture".into(),
                11 => protected.push(0..1),
                12 => protected.push(2..3),
                13 => out[0].end -= 1,
                14 => out[1].start += 1,
                _ => unreachable!(),
            }
            let before = format!("{out:?}");
            apply("24年", 0, &mut out, &AsciiPlan::new("24年", 0), &protected);
            assert_eq!(format!("{out:?}"), before, "mutation {mutation}");
        }
        for (raw, r, p) in [
            ("二四", "ニヨン", "ニヨン"),
            ("三四", "サンヨン", "サンヨン"),
            ("二〇二四", "ニレイニヨン", "ニレイニヨン"),
        ] {
            let text = format!("{raw}年");
            let mut out = tokens(raw, r, p, 0);
            let before = format!("{out:?}");
            apply(&text, 0, &mut out, &AsciiPlan::new(&text, 0), &[]);
            assert_eq!(format!("{out:?}"), before);
        }
    }

    #[test]
    fn full_input_plan_keeps_identifiers_multisuffixes_and_partial_chunks() {
        for (full, local, offset) in [
            ("A4年", "4年", 1),
            ("ISO ,4年", "4年", 5),
            ("A,4年", "4年", 2),
            ("コード4年", "4年", 9),
            ("第4年", "4年", 3),
            ("4年A", "4年", 0),
            ("4年.2", "4年", 0),
            ("4年度", "4年", 0),
            ("4年次", "4年", 0),
            ("4年間", "4年", 0),
            ("4年生", "4年", 0),
            ("日本語,二〇二四年A", "二〇二四年", 10),
            ("日本語,二〇二四年度", "二〇二四年", 10),
        ] {
            let raw = local.strip_suffix('年').unwrap();
            let (r, p) = if raw == "4" {
                ("ヨン", "ヨン")
            } else {
                ("ニセンニジュウヨン", "ニセンニジューヨン")
            };
            let mut out = tokens(raw, r, p, offset);
            let before = format!("{out:?}");
            apply(local, offset, &mut out, &AsciiPlan::new(full, 0), &[]);
            assert_eq!(format!("{out:?}"), before, "{full}");
        }
        let text = "4年";
        let mut out = tokens("4", "ヨン", "ヨン", 51);
        let previous = out[0].clone();
        out.insert(0, previous.clone());
        apply(text, 51, &mut out[1..], &AsciiPlan::new(text, 51), &[]);
        assert_eq!(format!("{:?}", out[0]), format!("{previous:?}"));
        assert_eq!(out[1].reading.as_ref(), "ヨ");
    }
}

#[cfg(all(test, feature = "build"))]
mod analyzer_tests {
    use crate::{
        Analyzer, Dictionary,
        dict::{DictBuilder, DictEntry},
    };

    fn dictionary(whole_word: bool) -> Dictionary {
        let mut builder = DictBuilder::new();
        for (n, (r, p)) in [
            ("ゼロ", "ゼロ"),
            ("イチ", "イチ"),
            ("ニ", "ニ"),
            ("サン", "サン"),
            ("ヨン", "ヨン"),
            ("ゴ", "ゴ"),
            ("ロク", "ロク"),
            ("ナナ", "ナナ"),
            ("ハチ", "ハチ"),
            ("キュウ", "キュー"),
        ]
        .into_iter()
        .enumerate()
        {
            for surface in [
                n.to_string(),
                char::from_u32('０' as u32 + n as u32).unwrap().to_string(),
                "〇一二三四五六七八九".chars().nth(n).unwrap().to_string(),
            ] {
                builder.add_entry(DictEntry {
                    surface: surface.clone().into(),
                    base_form: surface.into(),
                    pos: "名詞,数,*,*".into(),
                    reading: r.into(),
                    pronunciation: p.into(),
                    cost: 0,
                    ..Default::default()
                });
            }
        }
        for (surface, pos, r, p) in [
            ("十", "名詞,数,*,*", "ジュウ", "ジュー"),
            ("年", "名詞,接尾,助数詞,*", "ネン", "ネン"),
            ("歳", "名詞,接尾,助数詞,*", "サイ", "サイ"),
            ("日本語", "名詞,一般,*,*", "ニホンゴ", "ニホンゴ"),
            (
                "ISO",
                "名詞,固有名詞,組織,*",
                "アイエスオー",
                "アイエスオー",
            ),
            (",", "記号,一般,*,*", ",", ","),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                base_form: surface.into(),
                pos: pos.into(),
                reading: r.into(),
                pronunciation: p.into(),
                cost: 0,
                ..Default::default()
            });
        }
        if whole_word {
            builder.add_entry(DictEntry {
                surface: "二〇二四年".into(),
                base_form: "二〇二四年".into(),
                pos: "名詞,固有名詞,一般,*".into(),
                reading: "コユウゴ".into(),
                pronunciation: "コユーゴ".into(),
                cost: -30000,
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    #[test]
    fn analyzer_composes_positional_quantities_then_year_and_age_readings() {
        let mut analyzer = Analyzer::from_dict(dictionary(false));
        for (text, r, p) in [
            ("4年", "ヨネン", "ヨネン"),
            ("14年", "ジュウヨネン", "ジューヨネン"),
            ("24年", "ニジュウヨネン", "ニジューヨネン"),
            ("2024年", "ニセンニジュウヨネン", "ニセンニジューヨネン"),
            ("２０２４年", "ニセンニジュウヨネン", "ニセンニジューヨネン"),
            ("二〇二四年", "ニセンニジュウヨネン", "ニセンニジューヨネン"),
            (
                "日本語,二〇二四年",
                "ニホンゴニセンニジュウヨネン",
                "ニホンゴニセンニジューヨネン",
            ),
            ("一一歳", "ジュウイッサイ", "ジューイッサイ"),
            ("二八歳", "ニハチサイ", "ニハチサイ"),
            (
                "日本語,１８歳",
                "ニホンゴジュウハッサイ",
                "ニホンゴジューハッサイ",
            ),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            let actual: Vec<_> = out.iter().filter(|t| !t.pos.starts_with("記号")).collect();
            assert_eq!(
                actual
                    .iter()
                    .map(|t| t.reading.as_ref())
                    .collect::<String>(),
                r,
                "{text}: {out:?}"
            );
            assert_eq!(
                actual
                    .iter()
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                p,
                "{text}: {out:?}"
            );
        }
    }

    #[test]
    fn analyzer_keeps_known_words_and_restores_the_old_identifier_readings() {
        let mut analyzer = Analyzer::from_dict(dictionary(false));
        for text in ["ISO ,２０２４年", "ISO ,１８歳"] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert!(
                !out.iter()
                    .any(|t| matches!(t.surface.as_ref(), "２０２４" | "１８")),
                "{text}: {out:?}"
            );
            if text.ends_with('年') {
                assert_eq!(
                    out.iter()
                        .find(|t| t.surface.as_ref() == "４")
                        .unwrap()
                        .reading
                        .as_ref(),
                    "ヨン"
                );
            } else {
                // 拒否したIDの音価は旧経路へ戻す。既存の末尾だけの音便は今回広げて直さない。
                assert_eq!(
                    out.iter()
                        .find(|t| t.surface.as_ref() == "８")
                        .unwrap()
                        .reading
                        .as_ref(),
                    "ハッ"
                );
            }
        }
        let mut analyzer = Analyzer::from_dict(dictionary(true));
        let out = analyzer.try_tokenize("二〇二四年").unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].is_known);
        assert_eq!(out[0].reading.as_ref(), "コユウゴ");
        assert_eq!(out[0].pronunciation.as_ref(), "コユーゴ");
        assert_eq!(out[0].word_cost, -30000);
        assert_eq!(out[0].base_form.as_ref(), "二〇二四年");
    }
}
