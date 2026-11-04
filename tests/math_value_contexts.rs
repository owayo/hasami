//! 実際の配布辞書で、数学の値と価格・複合語の読みを区別する。
#![cfg(feature = "analyzer")]

use hasami::{Analyzer, ContextualReadingSpan, DictEntry, Token};

fn matches_source(token: &Token, entry: &DictEntry) -> bool {
    fn conjugation(value: &str) -> &str {
        if value == "*" { "" } else { value }
    }
    token.is_known
        && token.surface == entry.surface
        && token.pos == entry.pos
        && token.base_form == entry.base_form
        && token.conj_type.as_ref() == conjugation(&entry.conj_type)
        && token.conj_form.as_ref() == conjugation(&entry.conj_form)
        && token.reading == entry.reading
        && token.pronunciation == entry.pronunciation
        && token.word_cost == entry.cost
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_mathematical_values_keep_price_compounds_and_source_metadata() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        let bare = format!("{:?}", analyzer.try_tokenize("値").unwrap());
        let cases = [
            ("関数の値を計算する。", "値", "アタイ", true),
            ("f(2)の値を求める。", "値", "アタイ", true),
            ("店で魚の値を聞いた。", "値", "ネ", false),
            ("戻り値を確かめる。", "戻り値", "モドリチ", false),
            ("SKU-関数の値を計算する。", "値", "ネ", false),
        ];
        let initial: Vec<_> = cases
            .iter()
            .map(|(text, _, _, _)| format!("{:?}", analyzer.try_tokenize(text).unwrap()))
            .collect();
        for _ in 0..2 {
            for (index, (text, surface, speech, mathematical)) in cases.iter().enumerate() {
                let start = text.find(surface).unwrap();
                let end = start + surface.len();
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert_eq!(format!("{tokens:?}"), initial[index], "{name}: {text}");
                let focal: Vec<_> = tokens
                    .iter()
                    .filter(|token| token.start < end && start < token.end)
                    .collect();
                assert_eq!(focal.first().unwrap().start, start, "{name}: {text}");
                assert_eq!(focal.last().unwrap().end, end, "{name}: {text}");
                assert_eq!(
                    focal.iter().map(|t| &*t.reading).collect::<String>(),
                    *speech,
                    "{name}: {text}: {tokens:?}"
                );
                assert_eq!(
                    focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
                    *speech,
                    "{name}: {text}: {tokens:?}"
                );
                let spans = analyzer.contextual_reading_spans(text).unwrap();
                if *mathematical {
                    assert_eq!(focal.len(), 1);
                    let token = focal[0];
                    assert_eq!(token.pos.as_ref(), "名詞,一般,*,*");
                    assert_eq!(token.base_form.as_ref(), "値");
                    assert!(token.is_known);
                    assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
                    assert_eq!(
                        spans,
                        [ContextualReadingSpan {
                            start,
                            end,
                            pronunciation: "アタイ".into()
                        }]
                    );
                    // 補正前の実在するネ候補へ戻すと、位置以外の全項目が一致する。
                    let mut original = token.clone();
                    original.reading = "ネ".into();
                    original.pronunciation = "ネ".into();
                    assert!(
                        analyzer
                            .dictionary()
                            .lookup("値")
                            .unwrap()
                            .iter()
                            .any(|(end, rows)| *end == "値".len()
                                && rows.iter().any(|row| matches_source(&original, row))),
                        "{name}: {text}: {token:?}"
                    );
                } else {
                    assert!(spans.is_empty(), "{name}: {text}: {spans:?}");
                    for token in focal {
                        assert!(
                            analyzer
                                .dictionary()
                                .lookup(&token.surface)
                                .unwrap()
                                .iter()
                                .any(|(end, rows)| *end == token.surface.len()
                                    && rows.iter().any(|row| matches_source(token, row))),
                            "{name}: {text}: {token:?}"
                        );
                    }
                }
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
                assert!(tokens.windows(2).all(|pair| pair[0].end == pair[1].start));
                assert_eq!(tokens.last().unwrap().end, text.len());
            }
        }
        // 数学の補正を価格・複合語へ持ち越さず、前分割後も全文の位置で返す。
        let block =
            "関数の値を計算する。\n店で魚の値を聞いた。\nf(2)の値を求める。\n戻り値を確かめる。\n";
        let text = block.repeat(16);
        let mut expected: Vec<_> = ["関数の値", "f(2)の値"]
            .into_iter()
            .flat_map(|phrase| {
                text.match_indices(phrase).map(move |(start, _)| {
                    let start = start + phrase.find('値').unwrap();
                    ContextualReadingSpan {
                        start,
                        end: start + "値".len(),
                        pronunciation: "アタイ".into(),
                    }
                })
            })
            .collect();
        expected.sort_unstable_by_key(|span| span.start);
        assert_eq!(expected.len(), 32);
        assert_eq!(analyzer.contextual_reading_spans(&text).unwrap(), expected);
        let tokens = analyzer.try_tokenize(&text).unwrap();
        let sentences = analyzer
            .try_tokenize_sentences(&text, &Default::default())
            .unwrap();
        let flattened: Vec<_> = sentences
            .into_iter()
            .flat_map(|(_, tokens)| tokens)
            .collect();
        assert_eq!(format!("{tokens:?}"), format!("{flattened:?}"), "{name}");
        assert_eq!(format!("{:?}", analyzer.try_tokenize("値").unwrap()), bare);
    }
}
