//! 辞書の分割に依存しない負小数の符号と、識別子・演算子の境界。
#![cfg(feature = "analyzer")]

use hasami::Analyzer;

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_negative_signs_survive_sentence_offsets_and_analyzer_reuse() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for text in ["=-47.25", "測定値は=-47.25。次は=-0.5。"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                for (start, _) in text.match_indices('-') {
                    let end = start
                        + 1
                        + text[start + 1..]
                            .bytes()
                            .take_while(|byte| byte.is_ascii_digit() || *byte == b'.')
                            .count();
                    let focal: Vec<_> = tokens
                        .iter()
                        .filter(|t| t.start < end && start < t.end)
                        .collect();
                    assert_eq!(
                        focal.first().map(|t| t.start),
                        Some(start),
                        "{name}: {tokens:?}"
                    );
                    assert_eq!(focal.last().map(|t| t.end), Some(end), "{name}: {tokens:?}");
                    for speech in [
                        focal.iter().map(|t| &*t.reading).collect::<String>(),
                        focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
                    ] {
                        assert!(speech.starts_with("マイナス"), "{name}: {text}: {speech}");
                        assert_eq!(speech.matches("マイナス").count(), 1, "{name}: {text}");
                    }
                    if focal[0].surface.as_ref() == "-" {
                        assert!(focal[0].pos.starts_with("記号,一般,"));
                        assert_eq!(focal[0].base_form.as_ref(), "-");
                        assert!(focal[0].conj_type.is_empty() && focal[0].conj_form.is_empty());
                    }
                }
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
            }
            // 既知の「-4」等は保存する。分離された記号に新しく符号読みを付けないことを検査。
            for text in ["A-47.25", "47.25-6.5", "1-47.25"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert!(
                    tokens
                        .iter()
                        .filter(|t| t.surface.as_ref() == "-")
                        .all(|t| t.reading.as_ref() != "マイナス"
                            && t.pronunciation.as_ref() != "マイナス"),
                    "{name}: {text}: {tokens:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_negative_decimal_magnitudes_keep_places_points_and_fraction_digits() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            // NEologd の既知語「-1」が先頭を覆うケースは #92 で別に追う。
            let texts = if name == "ipadic-neologd" {
                [
                    "-0.5",
                    "=-0.045",
                    "測定値は=-0.045。次は=-0.5。最後は=-0.045。",
                ]
            } else {
                [
                    "-0.5",
                    "=-123.045",
                    "測定値は=-123.045。次は=-0.5。最後は=-123.045。",
                ]
            };
            for text in texts {
                let tokens = analyzer.try_tokenize(text).unwrap();
                for (surface, reading, speech) in [
                    ("0.5", "ゼロテンゴ", "ゼロテンゴ"),
                    ("0.045", "ゼロテンゼロヨンゴ", "ゼロテンゼロヨンゴ"),
                    (
                        "123.045",
                        "ヒャクニジュウサンテンゼロヨンゴ",
                        "ヒャクニジューサンテンゼロヨンゴ",
                    ),
                ] {
                    for (start, _) in text.match_indices(surface) {
                        let i = tokens.iter().position(|t| t.start == start).unwrap();
                        let body = &tokens[i];
                        assert_eq!(body.surface.as_ref(), surface, "{name}: {text}: {tokens:?}");
                        assert_eq!(body.base_form.as_ref(), surface);
                        assert_eq!(body.end, start + surface.len());
                        assert_eq!(body.pos.as_ref(), "名詞,数,*,*");
                        assert!(!body.is_known);
                        assert!(body.conj_type.is_empty() && body.conj_form.is_empty());
                        assert_eq!(body.reading.as_ref(), reading);
                        assert_eq!(body.pronunciation.as_ref(), speech);
                        let sign = &tokens[i - 1];
                        assert_eq!(sign.end, start);
                        assert_eq!(sign.surface.as_ref(), "-");
                        assert_eq!(sign.reading.as_ref(), "マイナス");
                        assert_eq!(sign.pronunciation.as_ref(), "マイナス");
                    }
                }
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
            }
        }
    }
}
