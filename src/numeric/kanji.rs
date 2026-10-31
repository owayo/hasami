//! 位を省いた漢数字列。数量に直結する標準的な数詞だけを、ASCII 数量と同じ位取りで合成する。

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

fn value(raw: &str) -> Option<u64> {
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

fn identifier_prefix(prefix: &str) -> bool {
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

/// 前分割で識別子や小数の前半を失わないよう、入力全体で候補の開始位置を決める。
pub(super) fn number_starts(input: &str, offset: usize) -> Vec<usize> {
    let mut starts = Vec::new();
    if input.is_ascii() {
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
        if value(raw).is_some()
            // ゼロのない二字列は「二、三」のような概数と区別できない。
            && (raw.contains(['〇', '零']) || raw.chars().count() >= 3)
            && !identifier_prefix(&input[..start])
            && !input[end..].chars().next().is_some_and(|c| {
                // 英字・% は実トークンの単位認識と、その末尾の境界で絞る。
                identifier_neighbor(c) && !c.is_ascii_alphabetic() && c != '%'
            })
        {
            starts.push(offset + start);
        }
    }
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
        && t.surface.chars().all(|c| digit(c).is_some())
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
    unit: impl Fn(&str) -> bool,
) -> Option<(&'a str, u64)> {
    let first = &out[range.start];
    let last = &out[range.end - 1];
    if range.len() == 1 && first.is_known {
        return None;
    }
    let raw = input.get(first.start.checked_sub(offset)?..last.end.checked_sub(offset)?)?;
    let value = value(raw)?;
    let protection = protected.partition_point(|p| p.end <= first.start);
    if protected
        .get(protection)
        .is_some_and(|p| p.start < last.end)
    {
        return None;
    }
    let suffix = out.get(range.end)?;
    if last.end != suffix.start
        // ゼロのない桁列は、西暦の年・年度だけを位取りとして扱う。
        || (!raw.contains(['〇', '零'])
            && !matches!(suffix.surface.as_ref(), "年" | "年度"))
        || !quantity(suffix, &unit)
        || !ascii::unit_boundary(input, offset, suffix)
        || !out[range.clone()].iter().all(|t| {
            let (Some(start), Some(end)) = (t.start.checked_sub(offset), t.end.checked_sub(offset))
            else {
                return false;
            };
            input.get(start..end) == Some(t.surface.as_ref())
                && standard_kanji_readings(&t.surface, &t.reading, &t.pronunciation)
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
                    candidate(input, offset, out, start..read, protected, &unit)
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
