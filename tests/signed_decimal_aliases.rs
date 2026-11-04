//! 等号の負小数でだけ、標準の既知prefixを含む全体を数詞として合成する。
#![cfg(feature = "build")]

use hasami::{Analyzer, DictEntry, Token, dict::DictBuilder};

fn entry(
    surface: &str,
    base: &str,
    pos: &str,
    reading: &str,
    pronunciation: &str,
    cost: i16,
) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        base_form: base.into(),
        pos: pos.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
        cost,
        ..Default::default()
    }
}

fn fixture(custom: Option<DictEntry>) -> Analyzer {
    let mut builder = DictBuilder::new();
    for (raw, r, p) in [
        ("0", "ゼロ", "ゼロ"),
        ("1", "イチ", "イチ"),
        ("2", "ニ", "ニ"),
        ("3", "サン", "サン"),
        ("4", "ヨン", "ヨン"),
        ("5", "ゴ", "ゴ"),
        ("6", "ロク", "ロク"),
        ("7", "ナナ", "ナナ"),
        ("8", "ハチ", "ハチ"),
        ("9", "キュウ", "キュー"),
    ] {
        builder.add_entry(entry(raw, raw, "名詞,数,*,*", r, p, -100));
    }
    for (surface, base, speech) in [
        ("-1", "−1", "マイナスイチ"),
        ("-3", "−3", "マイナスサン"),
        ("-4", "−4", "マイナスヨン"),
    ] {
        builder.add_entry(entry(
            surface,
            base,
            "名詞,固有名詞,一般,*",
            speech,
            speech,
            -4000,
        ));
    }
    for (surface, pos, r, p, cost) in [
        ("=", "記号,一般,*,*", "=", "=", -1000),
        ("-", "記号,一般,*,*", "-", "-", -100),
        (".", "記号,一般,*,*", ".", ".", -50),
        ("。", "記号,句点,*,*", "。", "。", -100),
        ("測定値", "名詞,一般,*,*", "ソクテイチ", "ソクテイチ", -100),
        ("名前", "名詞,一般,*,*", "ナマエ", "ナマエ", -100),
        ("は", "助詞,係助詞,*,*", "ハ", "ワ", -100),
        ("本", "名詞,接尾,助数詞,*", "ホン", "ホン", -100),
        ("匹", "名詞,接尾,助数詞,*", "ヒキ", "ヒキ", -100),
        ("年", "名詞,接尾,助数詞,*", "ネン", "ネン", -100),
        ("人", "名詞,接尾,助数詞,*", "ニン", "ニン", -100),
        ("回", "名詞,接尾,助数詞,*", "カイ", "カイ", -100),
        ("章", "名詞,一般,*,*", "ショウ", "ショー", -100),
        (
            "キログラム",
            "名詞,接尾,助数詞,*",
            "キログラム",
            "キログラム",
            -100,
        ),
    ] {
        builder.add_entry(entry(surface, surface, pos, r, p, cost));
    }
    if let Some(custom) = custom {
        builder.add_entry(custom);
    }
    Analyzer::from_dict(builder.build().unwrap())
}

fn focal(tokens: &[Token], surface: &str) -> Token {
    tokens
        .iter()
        .find(|t| t.surface.as_ref() == surface)
        .unwrap_or_else(|| panic!("missing {surface}: {tokens:?}"))
        .clone()
}

fn shifted(mut tokens: Vec<Token>, offset: usize) -> Vec<Token> {
    for token in &mut tokens {
        token.start += offset;
        token.end += offset;
    }
    tokens
}

fn assert_bounds(text: &str, tokens: &[Token]) {
    for token in tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
}

fn assert_raw_kept(analyzer: &mut Analyzer, raw: &str, text: &str) {
    let expected = analyzer.try_tokenize(raw).unwrap();
    let tokens = analyzer.try_tokenize(text).unwrap();
    for (start, _) in text.match_indices(raw) {
        let actual: Vec<_> = tokens
            .iter()
            .filter(|t| start <= t.start && t.end <= start + raw.len())
            .cloned()
            .collect();
        assert_eq!(
            format!("{actual:?}"),
            format!("{:?}", shifted(expected.clone(), start)),
            "{text}: {tokens:?}"
        );
    }
    assert_bounds(text, &tokens);
}

#[test]
fn equal_negative_decimal_with_known_prefix_becomes_one_honest_synthetic_number() {
    let mut analyzer = fixture(None);
    let text = "=-123.045";
    let tokens = analyzer.try_tokenize(text).unwrap();
    let number = focal(&tokens, "-123.045");
    assert_eq!(number.base_form, number.surface);
    assert_eq!((number.start, number.end), (1, text.len()));
    assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
    assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
    assert_eq!(
        number.reading.as_ref(),
        "マイナスヒャクニジュウサンテンゼロヨンゴ"
    );
    assert_eq!(
        number.pronunciation.as_ref(),
        "マイナスヒャクニジューサンテンゼロヨンゴ"
    );
    assert_eq!(number.word_cost, -4550);
    assert!(!number.is_known);
    let equal = analyzer.try_tokenize("=").unwrap();
    assert_eq!(format!("{:?}", &tokens[..1]), format!("{equal:?}"));
    for token in &tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
}

#[test]
fn canonical_prefix_values_work_in_prose_and_preserve_surrounding_fields() {
    let mut analyzer = fixture(None);
    for (raw, r, p) in [
        ("-3.5", "マイナスサンテンゴ", "マイナスサンテンゴ"),
        (
            "-47.25",
            "マイナスヨンジュウナナテンニゴ",
            "マイナスヨンジューナナテンニゴ",
        ),
        (
            "-1.045",
            "マイナスイチテンゼロヨンゴ",
            "マイナスイチテンゼロヨンゴ",
        ),
    ] {
        for (prefix, suffix) in [
            ("=", ""),
            ("測定値は=", "。"),
            ("測定値は \t=　\t", "　です。"),
            ("前。=", "。次"),
        ] {
            let text = format!("{prefix}{raw}{suffix}");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let number = focal(&tokens, raw);
            assert_eq!(
                (number.start, number.end),
                (prefix.len(), prefix.len() + raw.len())
            );
            assert_eq!(number.reading.as_ref(), r, "{text}: {tokens:?}");
            assert_eq!(number.pronunciation.as_ref(), p);
            assert_eq!(number.base_form, number.surface);
            assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
            assert!(!number.is_known);
            let expected_before = analyzer.try_tokenize(prefix).unwrap();
            let before: Vec<_> = tokens
                .iter()
                .filter(|t| t.end <= prefix.len())
                .cloned()
                .collect();
            assert_eq!(format!("{before:?}"), format!("{expected_before:?}"));
            let expected_after = shifted(
                analyzer.try_tokenize(suffix).unwrap(),
                prefix.len() + raw.len(),
            );
            let after: Vec<_> = tokens
                .iter()
                .filter(|t| t.start >= prefix.len() + raw.len())
                .cloned()
                .collect();
            assert_eq!(format!("{after:?}"), format!("{expected_after:?}"));
            assert_bounds(&text, &tokens);
        }
    }
    // 辞書の標準句点候補も許す。独自の読みを許す根拠にはしない。
    let mut analyzer = fixture(Some(entry(".", "．", "記号,句点,*,*", "．", "．", -2000)));
    assert_eq!(
        focal(&analyzer.try_tokenize("=-3.5").unwrap(), "-3.5").word_cost,
        -6100
    );
}

#[test]
fn bare_names_identifiers_quotations_and_malformed_values_keep_all_fields() {
    let mut analyzer = fixture(None);
    let raw = "-123.045";
    for text in [
        "-123.045",
        "Ａ＝-123.045",
        "＝-123.045",
        "A=-123.045",
        "Ａ=-123.045",
        "1=-123.045",
        "一=-123.045",
        "名前は=-123.045",
        "作品名は=-123.045",
        "識別子は=-123.045",
        "型番=-123.045",
        "数列は=-123.045",
        "ID =-123.045",
        "ＩＤは =-123.045",
        "=-123.045A",
        "=-123.045Ａ",
        "=-123.045_",
        "=-123.045/2",
        "=-123.045 A",
        "=-123.045さん",
        "=-123.045という名前",
        "=-123.045は作品名",
        "「=-123.045」",
        "作品名は「前。=-123.045。」",
        "「前\n=-123.045",
        "（=-123.045）",
        "[=-123.045]",
        "=-\n123.045",
    ] {
        if text.contains(raw) {
            assert_raw_kept(&mut analyzer, raw, text);
        }
    }
    for raw in [
        "-0123.045",
        "-123.",
        "-123..045",
        "-123.045.6",
        "-123.０４５",
        "-1",
        "-3",
        "-47",
    ] {
        assert_raw_kept(&mut analyzer, raw, &format!("={raw}"));
    }
    let tokens = analyzer.try_tokenize("=-\n123.045").unwrap();
    assert!(!tokens.iter().any(|t| t.surface.starts_with("-123")));
}

#[test]
fn custom_prefix_and_remainder_fields_and_known_whole_words_are_preserved() {
    for field in 0..6 {
        for (surface, base, pos, r, p) in [
            (
                "-1",
                "−1",
                "名詞,固有名詞,一般,*",
                "マイナスイチ",
                "マイナスイチ",
            ),
            ("2", "2", "名詞,数,*,*", "ニ", "ニ"),
            (".", ".", "記号,一般,*,*", ".", "."),
        ] {
            let mut custom = entry(surface, base, pos, r, p, -30000);
            match field {
                0 => custom.pos = "名詞,一般,*,*".into(),
                1 => custom.base_form = "独自原形".into(),
                2 => custom.conj_type = "独自活用型".into(),
                3 => custom.conj_form = "独自活用形".into(),
                4 => custom.reading = "ドクジヨミ".into(),
                _ => custom.pronunciation = "ドクジハツオン".into(),
            }
            let mut analyzer = fixture(Some(custom));
            assert_raw_kept(&mut analyzer, "-123.045", "測定値は=-123.045。");
        }
    }
    for (surface, base, pos, speech, raw) in [
        (
            "-123.045",
            "独自全体語",
            "名詞,固有名詞,一般,*",
            "サクヒン",
            "-123.045",
        ),
        (
            "2.0",
            "2.0",
            "名詞,固有名詞,一般,*",
            "ニーテンゼロ",
            "-12.045",
        ),
    ] {
        let mut analyzer = fixture(Some(entry(surface, base, pos, speech, speech, -30000)));
        assert_raw_kept(&mut analyzer, raw, &format!("={raw}"));
    }
    // 標準の整数aliasそのものは、等号があっても新しい数詞にしない。
    let mut analyzer = fixture(None);
    assert_raw_kept(&mut analyzer, "-1", "=-1");
    assert!(focal(&analyzer.try_tokenize("-1").unwrap(), "-1").is_known);
}

#[test]
fn repeated_source_offsets_append_and_public_quantity_contracts_stay_local() {
    let mut analyzer = fixture(None);
    let text = "「前。=-123.045」。測定値は=-123.045。=-3.5。".repeat(48);
    for _ in 0..2 {
        let tokens = analyzer.try_tokenize(&text).unwrap();
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.surface.as_ref() == "-123.045")
                .count(),
            48
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.surface.as_ref() == "-3.5")
                .count(),
            48
        );
        assert_bounds(&text, &tokens);
        let by_sentence: Vec<_> = analyzer
            .try_tokenize_sentences(&text, &Default::default())
            .unwrap()
            .into_iter()
            .flat_map(|(_, t)| t)
            .collect();
        assert_eq!(format!("{tokens:?}"), format!("{by_sentence:?}"));
    }
    let mut out = analyzer.try_tokenize("名前").unwrap();
    let previous = format!("{out:?}");
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    workspace
        .tokenize_into("=-123.045", analyzer.dictionary(), 71, &mut out)
        .unwrap();
    assert_eq!(format!("{:?}", &out[..1]), previous);
    assert_eq!(
        format!("{:?}", &out[1..]),
        format!(
            "{:?}",
            shifted(analyzer.try_tokenize("=-123.045").unwrap(), 71)
        )
    );
    for text in [
        "=-123.045",
        "測定値は=-3.5。",
        "名前は=-123.045",
        "A=-123.045",
    ] {
        assert_eq!(analyzer.quantity_pronunciation(text).unwrap(), None);
        assert!(analyzer.quantity_spans(text).unwrap().is_empty());
        assert!(analyzer.contextual_reading_spans(text).unwrap().is_empty());
    }
    let mut custom = entry(
        "-1",
        "−1",
        "名詞,固有名詞,一般,*",
        "マイナスイチ",
        "マイナスイチ",
        -32700,
    );
    custom.conj_type = "*".into();
    assert_eq!(
        focal(
            &fixture(Some(custom)).try_tokenize("=-123.045").unwrap(),
            "-123.045"
        )
        .word_cost,
        i16::MIN
    );
}

#[test]
fn following_counters_keep_original_tokens_and_counter_euphony() {
    let mut analyzer = fixture(None);
    for raw in [
        "-3.8本",
        "-47.18匹",
        "-1.8年",
        "-1.8人",
        "-1.8回",
        "-3.8章",
        "-3.8 章",
        "-3.8キログラム",
        "-3.8 本",
        "-47.18\t匹",
        "-3.8　キログラム",
    ] {
        for text in [format!("={raw}"), format!("測定値は={raw}。")] {
            assert_raw_kept(&mut analyzer, raw, &text);
        }
    }
    let tokens = analyzer.try_tokenize("=-3.8本").unwrap();
    assert_eq!(focal(&tokens, "8").pronunciation.as_ref(), "ハッ");
    assert_eq!(focal(&tokens, "本").pronunciation.as_ref(), "ポン");
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_following_counters_preserve_every_field_and_euphony() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for raw in [
            "-3.8本",
            "-47.18匹",
            "-1.8年",
            "-1.8人",
            "-1.8回",
            "-3.8章",
            "-3.8 章",
            "-3.8キログラム",
            "-3.8 本",
            "-47.18\t匹",
            "-3.8　キログラム",
        ] {
            assert_raw_kept(&mut analyzer, raw, &format!("={raw}"));
        }
        let tokens = analyzer.try_tokenize("=-3.8本").unwrap();
        assert_eq!(
            focal(&tokens, "8").pronunciation.as_ref(),
            "ハッ",
            "{name}: {tokens:?}"
        );
        assert_eq!(focal(&tokens, "本").pronunciation.as_ref(), "ポン");
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_equal_signed_decimals_use_places_without_disguising_known_aliases() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (raw, speech) in [
            ("-123.045", "マイナスヒャクニジューサンテンゼロヨンゴ"),
            ("-3.5", "マイナスサンテンゴ"),
            ("-1.045", "マイナスイチテンゼロヨンゴ"),
        ] {
            let text = format!("測定値は={raw}。次は={raw}。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            for (start, _) in text.match_indices(raw) {
                let selected: Vec<_> = tokens
                    .iter()
                    .filter(|t| start <= t.start && t.end <= start + raw.len())
                    .collect();
                assert_eq!(
                    selected
                        .iter()
                        .map(|t| t.pronunciation.as_ref())
                        .collect::<String>(),
                    speech,
                    "{name}: {tokens:?}"
                );
                assert_eq!(selected.first().unwrap().start, start);
                assert_eq!(selected.last().unwrap().end, start + raw.len());
                if selected.len() == 1 {
                    let number = selected[0];
                    assert_eq!(number.surface.as_ref(), raw);
                    assert_eq!(number.base_form, number.surface);
                    assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
                    assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
                    assert!(!number.is_known);
                }
            }
            assert_bounds(&text, &tokens);
            if name == "ipadic-neologd" || name == "ipadic-neologd-sudachi" && raw != "-123.045" {
                assert_eq!(
                    tokens.iter().filter(|t| t.surface.as_ref() == raw).count(),
                    2,
                    "{name}: {tokens:?}"
                );
            }
        }
        if name != "ipadic" {
            assert_raw_kept(&mut analyzer, "-3.5", "名前は=-3.5");
            assert_raw_kept(&mut analyzer, "-3.5", "「前。=-3.5」");
            // 既知残部「2.0」が標準数詞でないため、この形は新規合成の対象外。
            if name == "ipadic-neologd" {
                assert_raw_kept(&mut analyzer, "-12.045", "=-12.045");
            }
        }
    }
}
