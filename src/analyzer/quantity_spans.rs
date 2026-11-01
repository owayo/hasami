//! 全文で選択された数詞と単位を、既存の数量検証へ相対位置で渡す。

use crate::Token;

/// 全文の解析で認可された単一数量。位置は入力全体のUTF-8バイト位置。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuantitySpan {
    /// 数詞の先頭（この位置を含む）。
    pub start: usize,
    /// 単位の末尾（この位置を含まない）。
    pub end: usize,
    /// 共通数詞規則と一致した、選択済みトークンの発音。
    pub pronunciation: String,
}

pub(super) fn collect(input: &str, tokens: &[Token]) -> Vec<QuantitySpan> {
    let mut spans = Vec::new();
    // 全文で一度だけ構文候補を作り、孤立した末尾だけを数量へ再解釈しない。
    let plan = crate::numeric::AsciiPlan::new(input, 0);
    for pair in tokens.windows(2) {
        let [number, suffix] = pair else {
            unreachable!("windows(2)は常に2要素");
        };
        // 単一数量の検証器はASCII数値だけを読む。非数詞のArc複製を先に避ける。
        if !number
            .surface
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_digit)
        {
            continue;
        }
        if number.end < number.start || number.end != suffix.start || suffix.end < suffix.start {
            continue;
        }
        if !plan.scalar_candidate(number.start, number.end) {
            continue;
        }
        let Some(raw) = input.get(number.start..suffix.end) else {
            continue;
        };
        let Some(prefix) = input.get(..number.start) else {
            continue;
        };
        let Some(tail) = input.get(suffix.end..) else {
            continue;
        };
        // 先頭ゼロは識別用の桁数を表す可能性がある。0.xのゼロ一字は通常の小数。
        if number.surface.starts_with('0')
            && number
                .surface
                .as_bytes()
                .get(1)
                .is_some_and(u8::is_ascii_digit)
        {
            continue;
        }
        let previous = prefix.trim_end().chars().next_back();
        if previous.is_some_and(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '−' | '－' | '﹣' | '⁻')
        }) || tail.chars().next().is_some_and(|c| {
            c.is_ascii_alphabetic() || c == '_' || suffix.surface.is_ascii() && c.is_ascii_digit()
        }) {
            continue;
        }
        // 単位を含む一語の一部分へ当てない。空白をまたぐ組も既存検証で拒否する。
        let mut relative = [number.clone(), suffix.clone()];
        for token in &mut relative {
            token.start -= number.start;
            token.end -= number.start;
        }
        let Some(pronunciation) = crate::numeric::units::quantity_pronunciation(raw, &relative)
        else {
            continue;
        };
        spans.push(QuantitySpan {
            start: number.start,
            end: suffix.end,
            pronunciation,
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn token(surface: &str, start: usize, pos: &str, reading: &str) -> Token {
        Token {
            surface: Arc::from(surface),
            start,
            end: start + surface.len(),
            pos: Arc::from(pos),
            conj_type: Arc::from(""),
            conj_form: Arc::from(""),
            base_form: Arc::from(surface),
            reading: Arc::from(reading),
            pronunciation: Arc::from(reading),
            word_cost: 2000,
            is_known: true,
        }
    }

    #[test]
    fn rejects_partial_decimal_and_identifier_suffixes_but_keeps_complete_decimal() {
        for (text, start, digit, pronunciation) in [
            ("01.2本", 3, "2", "ニ"),
            ("1.2.3本", 4, "3", "サン"),
            ("一2本", 3, "2", "ニ"),
            ("Ａ2本", 3, "2", "ニ"),
            ("ZX.2本", 3, "2", "ニ"),
        ] {
            let tokens = [
                token(digit, start, "名詞,数,*,*", pronunciation),
                token("本", start + digit.len(), "名詞,接尾,助数詞,*", "ホン"),
            ];
            let before = format!("{tokens:?}");
            assert!(collect(text, &tokens).is_empty(), "{text}");
            assert_eq!(format!("{tokens:?}"), before);
        }
        let tokens = [
            token("1.2", 0, "名詞,数,*,*", "イチテンニ"),
            token("本", 3, "名詞,接尾,助数詞,*", "ホン"),
        ];
        assert_eq!(
            collect("1.2本", &tokens),
            [QuantitySpan {
                start: 0,
                end: 6,
                pronunciation: "イチテンニホン".into()
            }]
        );
    }

    #[test]
    fn keeps_global_utf8_positions_and_selected_standard_suffixes() {
        let text = "前2割と8代。";
        let tokens = [
            token("前", 0, "名詞,一般,*,*", "マエ"),
            token("2", 3, "名詞,数,*,*", "ニ"),
            token("割", 4, "名詞,接尾,助数詞,*", "ワリ"),
            token("と", 7, "助詞,並立助詞,*,*", "ト"),
            token("8", 10, "名詞,数,*,*", "ハチ"),
            token("代", 11, "名詞,接尾,助数詞,*", "ダイ"),
        ];
        let before = tokens.clone();
        assert_eq!(
            collect(text, &tokens),
            [
                QuantitySpan {
                    start: 3,
                    end: 7,
                    pronunciation: "ニワリ".into()
                },
                QuantitySpan {
                    start: 10,
                    end: 14,
                    pronunciation: "ハチダイ".into()
                }
            ]
        );
        assert_eq!(format!("{tokens:?}"), format!("{before:?}"));
    }

    #[test]
    fn rejects_whole_words_partial_units_noncanonical_and_gaps() {
        assert!(collect("2割", &[token("2割", 0, "名詞,一般,*,*", "フタツワリ")]).is_empty());
        assert!(
            collect(
                "10階層",
                &[
                    token("10", 0, "名詞,数,*,*", "ジュウ"),
                    token("階層", 2, "名詞,一般,*,*", "カイソウ")
                ]
            )
            .is_empty()
        );
        assert!(
            collect(
                "6本",
                &[
                    token("6", 0, "名詞,数,*,*", "ロク"),
                    token("本", 1, "名詞,接尾,助数詞,*", "ホン")
                ]
            )
            .is_empty()
        );
        assert!(
            collect(
                "2 割",
                &[
                    token("2", 0, "名詞,数,*,*", "ニ"),
                    token("割", 2, "名詞,接尾,助数詞,*", "ワリ")
                ]
            )
            .is_empty()
        );
        for (text, start, number) in [
            ("-2割", 1, "2"),
            ("−2割", 3, "2"),
            ("X2割", 1, "2"),
            ("02割", 0, "02"),
            ("2割X", 0, "2"),
        ] {
            let pair = [
                token(number, start, "名詞,数,*,*", "ニ"),
                token("割", start + number.len(), "名詞,接尾,助数詞,*", "ワリ"),
            ];
            assert!(collect(text, &pair).is_empty(), "{text}");
        }
    }

    #[test]
    fn known_compound_counter_reuses_existing_euphony() {
        let k = [
            token("1", 0, "名詞,数,*,*", "イッ"),
            token("回戦", 1, "名詞,接尾,助数詞,*", "カイセン"),
        ];
        assert_eq!(collect("1回戦", &k)[0].pronunciation, "イッカイセン");
        let h = [
            token("3", 0, "名詞,数,*,*", "サン"),
            token("本塁打", 1, "名詞,一般,*,*", "ボンルイダ"),
        ];
        assert_eq!(collect("3本塁打", &h)[0].pronunciation, "サンボンルイダ");
        for pair in [k, h] {
            let mut wrong = pair.clone();
            wrong[1].base_form = Arc::from("別語");
            assert!(collect(&format!("{}{}", pair[0].surface, pair[1].surface), &wrong).is_empty());
        }
    }

    #[test]
    // 本塁打は選択済みの読みを保つ検証。唯一の標準発音を規定する表ではない。
    fn compound_units_accept_only_known_canonical_forms() {
        for (number, reading, pronunciation, unit, unit_reading, expected) in [
            (
                "1",
                "イチ",
                "イチ",
                "本塁打",
                "ホンルイダ",
                "イチホンルイダ",
            ),
            (
                "3",
                "サン",
                "サン",
                "本塁打",
                "ボンルイダ",
                "サンボンルイダ",
            ),
            (
                "6",
                "ロク",
                "ロク",
                "本塁打",
                "ホンルイダ",
                "ロクホンルイダ",
            ),
            (
                "8",
                "ハチ",
                "ハチ",
                "本塁打",
                "ホンルイダ",
                "ハチホンルイダ",
            ),
            (
                "10",
                "ジュウ",
                "ジュー",
                "本塁打",
                "ホンルイダ",
                "ジューホンルイダ",
            ),
            (
                "100",
                "ヒャク",
                "ヒャク",
                "本塁打",
                "ホンルイダ",
                "ヒャクホンルイダ",
            ),
            ("6", "ロッ", "ロッ", "回戦", "カイセン", "ロッカイセン"),
            ("8", "ハッ", "ハッ", "回戦", "カイセン", "ハッカイセン"),
            (
                "10",
                "ジュッ",
                "ジュッ",
                "回戦",
                "カイセン",
                "ジュッカイセン",
            ),
        ] {
            let pos = if unit == "本塁打" {
                "名詞,一般,*,*"
            } else {
                "名詞,接尾,助数詞,*"
            };
            let mut pair = [
                token(number, 0, "名詞,数,*,*", reading),
                token(unit, number.len(), pos, unit_reading),
            ];
            pair[0].pronunciation = pronunciation.into();
            let text = format!("{number}{unit}");
            assert_eq!(
                collect(&text, &pair)
                    .first()
                    .map(|span| span.pronunciation.as_str()),
                Some(expected),
                "{text}"
            );
            for mode in 0..4 {
                let mut invalid = pair.clone();
                match mode {
                    0 => invalid[1].is_known = false,
                    1 => invalid[1].pos = Arc::from("名詞,固有名詞,一般,*"),
                    2 => invalid[1].conj_type = Arc::from("形容詞・アウオ段"),
                    _ => invalid[1].reading = Arc::from("ベツヨミ"),
                }
                assert!(collect(&text, &invalid).is_empty(), "{text}: {mode}");
            }
        }
    }

    #[cfg(feature = "build")]
    #[test]
    fn analyzer_uses_whole_context_once_and_preserves_normal_tokens() {
        use crate::{Analyzer, DictEntry, dict::DictBuilder};
        let mut builder = DictBuilder::new();
        for (surface, pos, reading) in [
            ("冊子", "名詞,一般,*,*", "サッシ"),
            ("7", "名詞,数,*,*", "ナナ"),
            ("巻", "名詞,接尾,助数詞,*", "カン"),
            ("1", "名詞,数,*,*", "イチ"),
            ("回戦", "名詞,接尾,助数詞,*", "カイセン"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: 0,
                right_id: 0,
                cost: 0,
                pos: pos.into(),
                base_form: surface.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
                ..Default::default()
            });
        }
        let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
        let text = "冊子7巻1回戦";
        let before = analyzer.try_tokenize(text).unwrap();
        let spans = analyzer.quantity_spans(text).unwrap();
        assert_eq!(
            spans
                .iter()
                .map(|v| v.pronunciation.as_str())
                .collect::<Vec<_>>(),
            ["ナナカン", "イッカイセン"]
        );
        assert_eq!(
            format!("{:?}", analyzer.try_tokenize(text).unwrap()),
            format!("{before:?}")
        );
        assert!(analyzer.quantity_spans("").unwrap().is_empty());
    }
}
