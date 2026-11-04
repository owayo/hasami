//! 距離の直後に書いた所要時間を、面積・体積の指数へ取り込ませない。
#![cfg(feature = "build")]

use hasami::{Analyzer, DictEntry, Token, dict::DictBuilder};

fn fixture(extra: Option<DictEntry>) -> Analyzer {
    let mut builder = DictBuilder::new();
    for (surface, pos, reading, pronunciation, cost) in [
        ("長さ", "名詞,一般,*,*", "ナガサ", "ナガサ", -1000),
        ("距離", "名詞,一般,*,*", "キョリ", "キョリ", -1000),
        ("全長", "名詞,一般,*,*", "ゼンチョウ", "ゼンチョー", -1000),
        ("対象", "名詞,一般,*,*", "タイショウ", "タイショー", -1000),
        ("7", "名詞,数,*,*", "ナナ", "ナナ", -100),
        ("2", "名詞,数,*,*", "ニ", "ニ", -100),
        ("0", "名詞,数,*,*", "ゼロ", "ゼロ", -100),
        ("3", "名詞,数,*,*", "サン", "サン", -100),
        ("6", "名詞,数,*,*", "ロク", "ロク", -100),
        ("m", "名詞,一般,*,*", "メートル", "メートル", -100),
        (
            "m3",
            "名詞,一般,*,*",
            "リッポウメートル",
            "リッポウメートル",
            -1000,
        ),
        ("m2", "名詞,一般,*,*", "", "", -1000),
        ("分", "名詞,接尾,助数詞,*", "フン", "フン", -100),
        ("秒", "名詞,接尾,助数詞,*", "ビョウ", "ビョー", -100),
        ("。", "記号,句点,*,*", "。", "。", -100),
    ] {
        builder.add_entry(DictEntry {
            surface: surface.into(),
            pos: pos.into(),
            base_form: surface.into(),
            reading: reading.into(),
            pronunciation: pronunciation.into(),
            cost,
            ..Default::default()
        });
    }
    if let Some(extra) = extra {
        builder.add_entry(extra);
    }
    Analyzer::from_dict(builder.build().unwrap())
}

fn speech(tokens: &[Token]) -> String {
    tokens.iter().map(|t| &*t.pronunciation).collect()
}

#[test]
fn distance_then_minutes_uses_the_real_metre_and_minute_candidates() {
    let mut analyzer = fixture(None);
    let text = "長さ720m3分26秒";
    let tokens = analyzer.try_tokenize(text).unwrap();
    assert_eq!(
        speech(&tokens),
        "ナガサナナヒャクニジューメートルサンプンニジューロクビョー"
    );
    assert_eq!(
        tokens.iter().map(|t| &*t.surface).collect::<Vec<_>>(),
        ["長さ", "720", "m", "3", "分", "26", "秒"]
    );
    for token in &tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
    // 新たに選んだ単位と助数詞は実在する辞書候補の素性を持つ。
    for surface in ["m", "分", "秒"] {
        let token = tokens
            .iter()
            .find(|t| t.surface.as_ref() == surface)
            .unwrap();
        let rows = analyzer.dictionary().lookup(surface).unwrap();
        assert!(
            rows.into_iter().any(|(end, rows)| end == surface.len()
                && rows.iter().any(|row| row.pos == token.pos
                    && row.base_form == token.base_form
                    && row.cost == token.word_cost)),
            "{surface}: {token:?}"
        );
        assert!(token.is_known);
        assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
    }
    let number = &tokens[1];
    assert!(!number.is_known);
    assert_eq!(number.word_cost, -300);
    assert_eq!(number.base_form, number.surface);
    assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
    // 3分は、単独の3分と同じ共通助数詞規則で読む。
    let bare = analyzer.try_tokenize("3分").unwrap();
    assert_eq!(speech(&bare), "サンプン");
    let m3 = analyzer.try_tokenize("m3").unwrap();
    assert_eq!(m3[0].surface.as_ref(), "m3");
    assert_eq!(m3[0].pronunciation.as_ref(), "リッポウメートル");
    // 元の識別子保護範囲を部分数量として再認可する照会は広げない。
    assert!(
        analyzer
            .quantity_spans(text)
            .unwrap()
            .iter()
            .all(|span| span.start >= "長さ720m3分".len())
    );
    assert_eq!(analyzer.quantity_pronunciation(text).unwrap(), None);
}

#[test]
fn custom_crossing_entries_and_long_known_words_keep_all_selected_fields() {
    for field in 0..8 {
        let mut custom = DictEntry {
            surface: "m3".into(),
            pos: "名詞,一般,*,*".into(),
            base_form: "m3".into(),
            reading: "リッポウメートル".into(),
            pronunciation: "リッポウメートル".into(),
            cost: -30000,
            ..Default::default()
        };
        match field {
            0 => custom.reading = "カスタム".into(),
            1 => custom.pronunciation = "カスタム".into(),
            2 => custom.pos = "名詞,固有名詞,一般,*".into(),
            3 => custom.base_form = "別の原形".into(),
            4 => custom.conj_type = "独自".into(),
            5 => custom.conj_form = "独自".into(),
            _ => {
                custom.surface = if field == 6 {
                    "720m3"
                } else {
                    "720m3分26秒。続き"
                }
                .into();
                custom.base_form = custom.surface.clone();
            }
        }
        let mut analyzer = fixture(Some(custom));
        let suffix = if field == 7 {
            "720m3分26秒。続き"
        } else {
            "720m3分26秒"
        };
        let control = analyzer.try_tokenize(&format!("対象{suffix}")).unwrap();
        let tokens = analyzer.try_tokenize(&format!("長さ{suffix}")).unwrap();
        assert_eq!(
            format!("{:?}", &tokens[1..]),
            format!("{:?}", &control[1..]),
            "field {field}"
        );
    }
}

#[test]
fn custom_number_metadata_survives_the_new_distance_and_time_boundaries() {
    for surface in ["720", "23"] {
        for field in 0..6 {
            let mut custom = DictEntry {
                surface: surface.into(),
                pos: "名詞,数,*,*".into(),
                base_form: surface.into(),
                reading: if surface == "720" {
                    "ナナヒャクニジュウ"
                } else {
                    "ニジュウサン"
                }
                .into(),
                pronunciation: if surface == "720" {
                    "ナナヒャクニジュー"
                } else {
                    "ニジューサン"
                }
                .into(),
                cost: -30000,
                ..Default::default()
            };
            match field {
                0 => custom.reading = "カスタム".into(),
                1 => custom.pronunciation = "カスタム".into(),
                2 => custom.pos = "名詞,固有名詞,一般,*".into(),
                3 => custom.base_form = "別の原形".into(),
                4 => custom.conj_type = "独自".into(),
                _ => custom.conj_form = "独自".into(),
            }
            let mut analyzer = fixture(Some(custom));
            let mut expected = analyzer.try_tokenize(surface).unwrap().remove(0);
            let text = "長さ720m23分26秒";
            expected.start = text.find(surface).unwrap();
            expected.end = expected.start + surface.len();
            let tokens = analyzer.try_tokenize(text).unwrap();
            let actual = tokens
                .iter()
                .find(|t| t.surface.as_ref() == surface)
                .unwrap();
            assert_eq!(
                format!("{actual:?}"),
                format!("{expected:?}"),
                "{surface}: field {field}"
            );
        }
    }
}

#[test]
fn quoted_identifiers_and_names_remain_protected_across_chunks() {
    let mut analyzer = fixture(None);
    for text in [
        "識別子は「前。長さ720m3分26秒。」",
        "作品名は「前。長さ720m3分26秒。」",
        "『前。長さ720m3分26秒。』",
        "\"前。長さ720m3分26秒。\"",
        "「前。長さ720m3分26秒。",
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        assert!(
            tokens.iter().any(|t| t.surface.as_ref() == "m3"),
            "{text}: {tokens:?}"
        );
        assert!(analyzer.quantity_spans(text).unwrap().iter().all(|span| {
            !text[span.start..span.end].ends_with('m')
                && !text[span.start..span.end].ends_with('分')
        }));
    }
    let block = "「前。長さ720m3分26秒。」。長さ720m3分26秒。";
    let tokens = analyzer.try_tokenize(&block.repeat(64)).unwrap();
    assert_eq!(
        tokens.iter().filter(|t| t.surface.as_ref() == "m3").count(),
        64
    );
    assert_eq!(
        tokens.iter().filter(|t| t.surface.as_ref() == "m").count(),
        64
    );
}

#[test]
fn incomplete_frames_identifiers_and_explicit_powers_keep_the_unit_boundary() {
    let mut analyzer = fixture(None);
    for text in [
        "長さ720m3",
        "長さ720m3分",
        "長さ720m3分60秒",
        "長さ720m03分26秒",
        "長さ0720m3分26秒",
        "長さ720m3分026秒",
        "長さ720m3分26秒A",
        "ID長さ720m3分26秒",
        "型番は長さ720m3分26秒",
        "ID:長さ720m3分26秒",
        "作品名：長さ720m3分26秒",
        "長さ720m3/秒",
        "長さ720m3分の26秒",
        "長さ720m3分26秒分の一",
        "長さ７２０m3分26秒",
        "長さ720m３分26秒",
        "長さ720m3 分26秒",
        "長さ720m 3分26秒",
        "長さ720m3分 26秒",
        "長さ720m3分26秒_7",
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        let control = text.replacen("長さ", "対象", 1);
        let expected = analyzer.try_tokenize(&control).unwrap();
        assert_eq!(tokens.len(), expected.len(), "{text}");
        for (actual, expected) in tokens.iter().zip(&expected) {
            if actual.surface.as_ref() != "長さ" {
                assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{text}");
            }
        }
    }
    for text in ["長さ720m²3分26秒", "長さ720m³3分26秒"] {
        let control = text.replacen("長さ", "対象", 1);
        let expected = analyzer.try_tokenize(&control).unwrap();
        let tokens = analyzer.try_tokenize(text).unwrap();
        assert_eq!(
            format!("{:?}", &tokens[1..]),
            format!("{:?}", &expected[1..]),
            "{text}"
        );
    }
}

#[test]
fn repeated_clauses_and_workspace_offsets_use_the_original_source() {
    let mut analyzer = fixture(None);
    let text = "長さ720m3分26秒。全長720m2分26秒。".repeat(64);
    let tokens = analyzer.try_tokenize(&text).unwrap();
    assert_eq!(
        tokens.iter().filter(|t| t.surface.as_ref() == "m").count(),
        128
    );
    assert!(
        !tokens
            .iter()
            .any(|t| matches!(t.surface.as_ref(), "m2" | "m3"))
    );
    for token in &tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
    let sentences = analyzer
        .try_tokenize_sentences(&text, &Default::default())
        .unwrap();
    let from_sentences: Vec<_> = sentences
        .into_iter()
        .flat_map(|(_, tokens)| tokens)
        .collect();
    assert_eq!(format!("{tokens:?}"), format!("{from_sentences:?}"));
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    let mut out = analyzer.try_tokenize("全長").unwrap();
    let previous = format!("{out:?}");
    workspace
        .tokenize_into("長さ720m3分26秒", analyzer.dictionary(), 47, &mut out)
        .unwrap();
    assert_eq!(format!("{:?}", &out[..1]), previous);
    let mut expected = analyzer.try_tokenize("長さ720m3分26秒").unwrap();
    for token in &mut expected {
        token.start += 47;
        token.end += 47;
    }
    assert_eq!(format!("{:?}", &out[1..]), format!("{expected:?}"));
    // 別の入力に、直前の距離フレームの保護解除を持ち越さない。
    assert!(
        analyzer
            .try_tokenize("長さ720m3")
            .unwrap()
            .iter()
            .any(|t| t.surface.as_ref() == "m3")
    );
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_distance_then_minutes_keeps_time_and_place_values() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for text in ["長さ720m3分26秒", "距離720m3分26秒。", "全長720m2分26秒"] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                speech(&tokens).contains("ナナヒャクニジュ"),
                "{name}: {tokens:?}"
            );
            assert!(
                speech(&tokens).contains(if text.contains("m2") {
                    "メートルニフンニジューロクビョー"
                } else {
                    "メートルサンプンニジューロクビョー"
                }),
                "{name}: {tokens:?}"
            );
            assert!(
                tokens
                    .iter()
                    .all(|t| !matches!(t.surface.as_ref(), "m2" | "m3")),
                "{name}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(
                    text.get(token.start..token.end),
                    Some(token.surface.as_ref())
                );
            }
        }
    }
}
