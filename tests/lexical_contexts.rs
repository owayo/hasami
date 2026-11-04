//! 単語単体では見落とす読みの欠落・展開と、活用語をまたぐ誤登録の回帰。
#![cfg(feature = "analyzer")]

use hasami::Analyzer;

#[test]
#[ignore = "配布辞書2種が必要"]
fn distributed_lexical_readings_survive_independent_sentence_contexts() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, surface, reading, pronunciation) in [
            (
                "整備士がBMWの車両を点検する。",
                "BMW",
                "ビーエムダブリュー",
                "ビーエムダブリュー",
            ),
            (
                "画面には有機ELを採用する。",
                "有機EL",
                "ユウキイーエル",
                "ユーキイーエル",
            ),
            (
                "担当者が中央労働災害防止協会に問い合わせる。",
                "中央労働災害防止協会",
                "チュウオウロウドウサイガイボウシキョウカイ",
                "チューオーロードーサイガイボーシキョーカイ",
            ),
        ] {
            let start = text.find(surface).unwrap();
            let end = start + surface.len();
            let tokens = analyzer.try_tokenize(text).unwrap();
            let focal: Vec<_> = tokens
                .iter()
                .filter(|token| token.start < end && start < token.end)
                .collect();
            assert_eq!(
                focal.first().map(|t| t.start),
                Some(start),
                "{name}: {tokens:?}"
            );
            assert_eq!(focal.last().map(|t| t.end), Some(end), "{name}: {tokens:?}");
            assert!(focal.iter().all(|t| t.is_known), "{name}: {tokens:?}");
            assert_eq!(
                focal.iter().map(|t| &*t.reading).collect::<String>(),
                reading,
                "{name}: {tokens:?}"
            );
            assert_eq!(
                focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
                pronunciation,
                "{name}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
            assert!(tokens.windows(2).all(|pair| pair[0].end == pair[1].start));
            assert_eq!(tokens.last().unwrap().end, text.len());
        }
        // 読みが埋まるだけでなく、形容詞と後続語の境界も残ること。
        for (text, surface, base, pos) in [
            ("高さが低そうだ。", "低", "低い", "形容詞,"),
            ("計算した値を比べる。", "た", "た", "助動詞,"),
            ("覚えたい事を並べる。", "たい", "たい", "助動詞,"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                tokens.iter().any(|token| {
                    token.surface.as_ref() == surface
                        && token.base_form.as_ref() == base
                        && token.pos.starts_with(pos)
                }),
                "{name}: {text}: {tokens:?}"
            );
        }
        // 車名の通常読みを優先しても、正当な作品名の候補を削除しない。
        assert!(
            analyzer
                .dictionary()
                .lookup("BMW")
                .unwrap()
                .iter()
                .filter(|(end, _)| *end == "BMW".len())
                .flat_map(|(_, entries)| entries)
                .any(|entry| entry.reading.as_ref() == "バトルムーンウォーズ")
        );
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_mode_lexeme_keeps_readings_and_other_uses() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for text in [
                "最頻値",
                "最頻値は一つだ。",
                "最頻値を調べる。",
                "この表では最頻値が二つある。",
                "度数を数えて最頻値を求める。",
            ] {
                let start = text.find("最頻値").unwrap();
                let end = start + "最頻値".len();
                let tokens = analyzer.try_tokenize(text).unwrap();
                let focal: Vec<_> = tokens
                    .iter()
                    .filter(|t| t.start < end && start < t.end)
                    .collect();
                assert_eq!(focal.len(), 1, "{name}: {text}: {tokens:?}");
                let token = focal[0];
                assert_eq!(token.surface.as_ref(), "最頻値", "{name}: {text}");
                assert_eq!((token.start, token.end), (start, end));
                assert_eq!(token.pos.as_ref(), "名詞,一般,*,*");
                assert_eq!(token.base_form, token.surface);
                assert_eq!(token.reading.as_ref(), "サイヒンチ");
                assert_eq!(token.pronunciation.as_ref(), "サイヒンチ");
                assert!(token.is_known);
                assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
                assert!(
                    analyzer
                        .dictionary()
                        .lookup("最頻値")
                        .unwrap()
                        .into_iter()
                        .any(|(end, rows)| {
                            end == "最頻値".len()
                                && rows.iter().any(|entry| {
                                    entry.pos == token.pos
                                        && entry.base_form == token.base_form
                                        && entry.reading == token.reading
                                        && entry.pronunciation == token.pronunciation
                                        && entry.cost == token.word_cost
                                        && matches!(entry.conj_type.as_ref(), "" | "*")
                                        && matches!(entry.conj_form.as_ref(), "" | "*")
                                })
                        }),
                    "{name}: selected metadata must come from a dictionary entry"
                );
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
                assert!(tokens.windows(2).all(|pair| pair[0].end == pair[1].start));
            }
            let tokens = analyzer.try_tokenize("品物の値が上がる。").unwrap();
            let price = tokens.iter().find(|t| t.surface.as_ref() == "値").unwrap();
            assert_eq!(price.reading.as_ref(), "ネ", "{name}: {tokens:?}");
            assert_eq!(price.pronunciation.as_ref(), "ネ");
            assert_eq!(price.base_form.as_ref(), "値");
            assert_eq!(price.pos.as_ref(), "名詞,一般,*,*");
            assert!(price.is_known);
            for (surface, speech) in [("頻", "シキ"), ("頻る", "シキル")] {
                assert!(
                    analyzer
                        .dictionary()
                        .lookup(surface)
                        .unwrap()
                        .into_iter()
                        .any(|(end, rows)| {
                            end == surface.len()
                                && rows.iter().any(|entry| {
                                    entry.base_form.as_ref() == "頻る"
                                        && entry.pos.as_ref() == "動詞,自立,*,*"
                                        && entry.reading.as_ref() == speech
                                        && entry.pronunciation.as_ref() == speech
                                        && entry.conj_type.as_ref() == "五段・ラ行"
                                        && !entry.conj_form.is_empty()
                                })
                        }),
                    "{name}: the healthy verb candidate must remain: {surface}"
                );
            }
        }
    }
}

#[test]
#[ignore = "配布辞書2種が必要"]
fn distributed_playoff_abbreviation_keeps_source_metadata_and_identifier_digits() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        let cases = [
            "挑決",
            "挑決を観戦する。",
            "挑決の結果を待つ。",
            "次の挑決は来週だ。",
            "ID_挑決_12",
        ];
        let initial: Vec<_> = cases
            .iter()
            .map(|text| format!("{:?}", analyzer.try_tokenize(text).unwrap()))
            .collect();
        for _ in 0..2 {
            for (index, text) in cases.iter().enumerate() {
                let start = text.find("挑決").unwrap();
                let end = start + "挑決".len();
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert_eq!(format!("{tokens:?}"), initial[index], "{name}: {text}");
                let focal: Vec<_> = tokens
                    .iter()
                    .filter(|t| t.start < end && start < t.end)
                    .collect();
                assert_eq!(focal.len(), 1, "{name}: {text}: {tokens:?}");
                let token = focal[0];
                assert_eq!(token.surface.as_ref(), "挑決");
                assert_eq!((token.start, token.end), (start, end));
                assert_eq!(token.pos.as_ref(), "名詞,一般,*,*");
                assert_eq!(token.base_form, token.surface);
                assert_eq!(token.reading.as_ref(), "チョウケツ");
                assert_eq!(token.pronunciation.as_ref(), "チョーケツ");
                assert!(token.is_known);
                assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
                assert!(
                    analyzer
                        .dictionary()
                        .lookup("挑決")
                        .unwrap()
                        .iter()
                        .any(|(end, rows)| *end == "挑決".len()
                            && rows.iter().any(|row| {
                                row.surface == token.surface
                                    && row.pos == token.pos
                                    && row.base_form == token.base_form
                                    && row.reading == token.reading
                                    && row.pronunciation == token.pronunciation
                                    && row.cost == token.word_cost
                                    && matches!(row.conj_type.as_ref(), "" | "*")
                                    && matches!(row.conj_form.as_ref(), "" | "*")
                            })),
                    "{name}: selected metadata must match a dictionary entry"
                );
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
                assert_eq!(tokens.first().unwrap().start, 0);
                assert_eq!(tokens.last().unwrap().end, text.len());
                assert!(tokens.windows(2).all(|pair| pair[0].end == pair[1].start));
                assert!(analyzer.contextual_reading_spans(text).unwrap().is_empty());
                if text.starts_with("ID_") {
                    let digits: Vec<_> = tokens.iter().filter(|t| t.start > end).collect();
                    assert_eq!(
                        digits
                            .iter()
                            .map(|t| t.surface.as_ref())
                            .collect::<Vec<_>>(),
                        ["1", "2"]
                    );
                    assert_eq!(
                        digits
                            .iter()
                            .map(|t| t.pronunciation.as_ref())
                            .collect::<String>(),
                        "イチニ"
                    );
                    assert!(analyzer.quantity_spans(text).unwrap().is_empty());
                    assert_eq!(analyzer.quantity_pronunciation(text).unwrap(), None);
                    assert_eq!(analyzer.normalize_identifier(text).unwrap(), None);
                }
            }
        }
    }
}
