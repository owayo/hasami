//! 境界の明確な「年＋整数＋冊/回」の年を、頻度のネンへ補正する。
//!
//! 小学館デジタル大辞泉の「年（ねん）」は、一年を表す名詞。
//! https://kotobank.jp/word/%E5%B9%B4-111642
//! 環境省の公開資料には「年２回」に「ねんにかい」の読みがある。
//! https://www.env.go.jp/park/minamialps/6-1_kekka.pdf
//!
//! 接頭詞へ品詞を変えず、既存の一般名詞の読みと発音だけを補正する。
//! 年を修飾する名詞・連体詞等、暦・年齢、独自読み、識別子は対象にしない。

use super::{ascii, is_numeral, numeral_value, place};
use crate::lattice::Token;
use std::{ops::Range, sync::Arc};

fn boundary(t: &Token) -> bool {
    t.is_known_base_form()
        && ["記号,読点,", "記号,句点,", "記号,括弧開,", "記号,括弧閉,"]
            .iter()
            .any(|p| t.pos.starts_with(p))
}

fn left(t: Option<&Token>) -> bool {
    t.is_none_or(|t| {
        boundary(t)
            || (t.is_known
                && t.pos.as_ref() == "助詞,係助詞,*,*"
                && t.base_form == t.surface
                && t.is_uninflected()
                && matches!(
                    (
                        t.surface.as_ref(),
                        t.reading.as_ref(),
                        t.pronunciation.as_ref()
                    ),
                    ("は", "ハ", "ワ") | ("も", "モ", "モ")
                ))
    })
}

fn right(t: Option<&Token>) -> bool {
    t.is_none_or(|t| {
        boundary(t) || (t.is_known && (t.pos.starts_with("助詞,") || t.pos.starts_with("助動詞,")))
    })
}

/// 既存の数詞列・助数詞の照合後、音便を変える前に呼ぶ。0も標準整数として認める。
pub(super) fn apply(
    tokens: &mut [Token],
    number_start: usize,
    counter: usize,
    protected: &[Range<usize>],
) {
    let Some(year) = number_start.checked_sub(1) else {
        return;
    };
    let t = &tokens[year];
    if t.surface.as_ref() != "年"
        || t.base_form.as_ref() != "年"
        || t.pos.as_ref() != "名詞,一般,*,*"
        || !t.is_known
        || t.reading.as_ref() != "トシ"
        || t.pronunciation.as_ref() != "トシ"
        || !t.is_uninflected()
        || !left(year.checked_sub(1).map(|i| &tokens[i]))
        || !right(tokens.get(counter + 1))
        || !tokens[year..=counter]
            .windows(2)
            .all(|w| w[0].end == w[1].start)
        || protected
            .iter()
            .any(|r| r.start < tokens[counter].end && t.start < r.end)
    {
        return;
    }
    let suffix = &tokens[counter];
    if !suffix.is_known
        || suffix.pos.as_ref() != "名詞,接尾,助数詞,*"
        || suffix.base_form != suffix.surface
        || !suffix.is_uninflected()
        || !matches!(
            (
                suffix.surface.as_ref(),
                suffix.reading.as_ref(),
                suffix.pronunciation.as_ref()
            ),
            ("冊", "サツ", "サツ") | ("回", "カイ", "カイ")
        )
    {
        return;
    }
    let numbers = &tokens[number_start..counter];
    if numbers.is_empty()
        || !numbers.iter().all(|t| {
            is_numeral(t)
                && t.pos.as_ref() == "名詞,数,*,*"
                && t.base_form == t.surface
                && t.is_uninflected()
        })
    {
        return;
    }
    let raw: String = numbers.iter().map(|t| t.surface.as_ref()).collect();
    let value = if raw.is_ascii() {
        ascii::integer_value(&raw)
    } else if raw.chars().all(|c| {
        matches!(
            c,
            '〇' | '零' | '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九'
        ) || place(c).is_some()
    }) && (raw.chars().count() == 1
        || (!raw.starts_with(['〇', '零']) && raw.chars().any(|c| place(c).is_some())))
    {
        // 位のない複数桁漢数字は概数・語彙義との区別をこの補正で増やさない。
        numeral_value(std::iter::once(raw.as_str()))
    } else {
        None
    };
    let Some(value) = value else { return };
    let reading: String = numbers.iter().map(|t| t.reading.as_ref()).collect();
    let pronunciation: String = numbers.iter().map(|t| t.pronunciation.as_ref()).collect();
    if !ascii::standard_readings(&value.to_string(), &reading, &pronunciation) {
        return;
    }
    tokens[year].reading = Arc::from("ネン");
    tokens[year].pronunciation = Arc::from("ネン");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(surface: &str, pos: &str, reading: &str, pronunciation: &str) -> Token {
        Token {
            surface: surface.into(),
            start: 0,
            end: surface.len(),
            pos: pos.into(),
            base_form: surface.into(),
            reading: reading.into(),
            pronunciation: pronunciation.into(),
            conj_type: "".into(),
            conj_form: "".into(),
            is_known: true,
            word_cost: 7055,
        }
    }

    fn quantity(raw: &str, reading: &str, pronunciation: &str, counter: &str) -> Vec<Token> {
        let mut out = vec![
            token("年", "名詞,一般,*,*", "トシ", "トシ"),
            token(raw, "名詞,数,*,*", reading, pronunciation),
            token(counter, "名詞,接尾,助数詞,*", "カイ", "カイ"),
        ];
        if counter == "冊" {
            out[2].reading = Arc::from("サツ");
            out[2].pronunciation = Arc::from("サツ");
        }
        place_tokens(&mut out);
        out
    }

    fn place_tokens(tokens: &mut [Token]) {
        let mut offset = 71;
        for t in tokens {
            t.start = offset;
            offset += t.surface.len();
            t.end = offset;
        }
    }

    fn unchanged(mut input: Vec<Token>, start: usize, counter: usize) {
        let before = format!("{input:?}");
        apply(&mut input, start, counter, &[]);
        assert_eq!(format!("{input:?}"), before);
    }

    #[test]
    fn annual_rate_changes_only_the_two_readings_of_the_existing_noun() {
        for (raw, r, p, suffix) in [
            ("0", "ゼロ", "ゼロ", "回"),
            ("2", "ニ", "ニ", "回"),
            ("12", "ジュウニ", "ジューニ", "冊"),
            ("1,000", "セン", "セン", "回"),
            ("三", "サン", "サン", "回"),
            ("十二", "ジュウニ", "ジューニ", "冊"),
        ] {
            let mut out = quantity(raw, r, p, suffix);
            let mut expected = out.clone();
            expected[0].reading = Arc::from("ネン");
            expected[0].pronunciation = Arc::from("ネン");
            apply(&mut out, 1, 2, &[]);
            assert_eq!(format!("{out:?}"), format!("{expected:?}"), "{raw}{suffix}");
        }
    }

    #[test]
    fn noun_modifiers_and_other_quantity_categories_keep_the_selected_reading() {
        for (surface, pos) in [
            ("その", "連体詞,*,*,*"),
            ("ある", "連体詞,*,*,*"),
            ("の", "助詞,連体化,*,*"),
            ("た", "助動詞,*,*,*"),
            ("良い", "形容詞,自立,*,*"),
            ("読む", "動詞,自立,*,*"),
            ("翌", "名詞,一般,*,*"),
            ("_", "記号,一般,*,*"),
        ] {
            let mut out = quantity("2", "ニ", "ニ", "回");
            out.insert(0, token(surface, pos, "ア", "ア"));
            place_tokens(&mut out);
            unchanged(out, 2, 3);
        }
        for suffix in ["月", "歳", "年", "日", "時", "回転"] {
            unchanged(quantity("2", "ニ", "ニ", suffix), 1, 2);
        }
    }

    #[test]
    fn annual_rate_requires_canonical_integer_features_and_complete_boundaries() {
        for (raw, r, p) in [
            ("02", "ニ", "ニ"),
            ("-2", "ニ", "ニ"),
            ("2.5", "ニテンゴ", "ニテンゴ"),
            ("二三", "ニジュウサン", "ニジューサン"),
            ("〇十", "ジュウ", "ジュー"),
            ("２", "ニ", "ニ"),
            ("2", "フタ", "フタ"),
        ] {
            unchanged(quantity(raw, r, p, "回"), 1, 2);
        }
        for field in 0..10 {
            let mut out = quantity("2", "ニ", "ニ", "回");
            match field {
                0 => out[0].pos = Arc::from("名詞,固有名詞,人名,名"),
                1 => out[0].base_form = Arc::from("年次"),
                2 => out[0].reading = Arc::from("ネン"),
                3 => out[0].is_known = false,
                4 => out[1].base_form = Arc::from("二"),
                5 => out[1].conj_type = Arc::from("独自"),
                6 => out[2].is_known = false,
                7 => out[2].pronunciation = Arc::from("ガイ"),
                8 => out[2].base_form = Arc::from("別語"),
                _ => out[1].start += 1,
            }
            unchanged(out, 1, 2);
        }
        let mut out = quantity("2", "ニ", "ニ", "回");
        out.push(token("転", "名詞,接尾,一般,*", "テン", "テン"));
        place_tokens(&mut out);
        unchanged(out, 1, 2);
        let mut out = quantity("2", "ニ", "ニ", "回");
        let before = format!("{out:?}");
        let protected = out[1].start..out[2].end;
        apply(&mut out, 1, 2, &[protected]);
        assert_eq!(format!("{out:?}"), before);
    }

    #[cfg(feature = "build")]
    #[test]
    fn real_lattice_uses_the_noun_reading_without_changing_its_grammar_or_cost() {
        use crate::{Analyzer, DictEntry, dict::DictBuilder};
        let mut builder = DictBuilder::new();
        for (surface, pos, r, p) in [
            ("年", "名詞,一般,*,*", "トシ", "トシ"),
            ("0", "名詞,数,*,*", "ゼロ", "ゼロ"),
            ("二", "名詞,数,*,*", "ニ", "ニ"),
            ("十二", "名詞,数,*,*", "ジュウニ", "ジューニ"),
            ("回", "名詞,接尾,助数詞,*", "カイ", "カイ"),
            ("冊", "名詞,接尾,助数詞,*", "サツ", "サツ"),
            ("その", "連体詞,*,*,*", "ソノ", "ソノ"),
            ("は", "助詞,係助詞,*,*", "ハ", "ワ"),
            ("も", "助詞,係助詞,*,*", "モ", "モ"),
            ("を", "助詞,格助詞,一般,*", "ヲ", "ヲ"),
            ("、", "記号,読点,*,*", "", ""),
            ("「", "記号,括弧開,*,*", "", ""),
            ("」", "記号,括弧閉,*,*", "", ""),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                base_form: surface.into(),
                reading: r.into(),
                pronunciation: p.into(),
                cost: 3123,
                ..Default::default()
            });
        }
        let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
        for (text, expected) in [
            ("年0回", "ネンゼロカイ"),
            ("年二回", "ネンニカイ"),
            ("年十二冊を", "ネンジューニサツヲ"),
            ("は年二回", "ワネンニカイ"),
            ("も年二回", "モネンニカイ"),
            ("、年二回", "ネンニカイ"),
            ("「年二回」", "ネンニカイ"),
            ("その年二回", "ソノトシニカイ"),
            ("年", "トシ"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_eq!(
                tokens
                    .iter()
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                expected,
                "{text}"
            );
            let year = tokens.iter().find(|t| t.surface.as_ref() == "年").unwrap();
            assert_eq!(year.pos.as_ref(), "名詞,一般,*,*");
            assert_eq!(year.base_form.as_ref(), "年");
            assert_eq!(year.word_cost, 3123);
            assert!(year.is_known);
            assert_eq!(&text[year.start..year.end], "年");
            assert!(year.conj_type.is_empty() && year.conj_form.is_empty());
        }
    }
}
