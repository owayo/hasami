//! 測定の構文と名称の用法を区別し、選択済みの語の読みだけを補正する。
#![cfg(feature = "build")]

use hasami::{Analyzer, ContextualReadingSpan, DictEntry, dict::DictBuilder};

fn fixture() -> Analyzer {
    fixture_with_target(target())
}

fn target() -> DictEntry {
    DictEntry {
        surface: "震度7".into(),
        pos: "名詞,固有名詞,一般,*".into(),
        base_form: "震度7".into(),
        reading: "シンドセブン".into(),
        pronunciation: "シンドセブン".into(),
        cost: -1000,
        ..Default::default()
    }
}

fn fixture_with_target(target: DictEntry) -> Analyzer {
    let mut builder = DictBuilder::new();
    builder.add_entry(target);
    for (surface, pos, base, reading, kind, form) in [
        ("最大", "名詞,一般,*,*", "最大", "サイダイ", "", ""),
        ("前", "名詞,一般,*,*", "前", "マエ", "", ""),
        ("ここ", "名詞,代名詞,一般,*", "ここ", "ココ", "", ""),
        ("作品名", "名詞,一般,*,*", "作品名", "サクヒンメイ", "", ""),
        ("題名", "名詞,一般,*,*", "題名", "ダイメイ", "", ""),
        ("は", "助詞,係助詞,*,*", "は", "ハ", "", ""),
        ("も", "助詞,係助詞,*,*", "も", "モ", "", ""),
        ("で", "助詞,格助詞,一般,*", "で", "デ", "", ""),
        ("「", "記号,括弧開,*,*", "「", "「", "", ""),
        ("」", "記号,括弧閉,*,*", "」", "」", "", ""),
        ("『", "記号,括弧開,*,*", "『", "『", "", ""),
        ("』", "記号,括弧閉,*,*", "』", "』", "", ""),
        ("を", "助詞,格助詞,一般,*", "を", "ヲ", "", ""),
        ("観測", "名詞,サ変接続,*,*", "観測", "カンソク", "", ""),
        ("記録", "名詞,サ変接続,*,*", "記録", "キロク", "", ""),
        ("し", "動詞,自立,*,*", "する", "シ", "サ変・スル", "連用形"),
        ("た", "助動詞,*,*,*", "た", "タ", "特殊・タ", "基本形"),
        ("。", "記号,句点,*,*", "。", "。", "", ""),
        ("、", "記号,読点,*,*", "、", "、", "", ""),
    ] {
        builder.add_entry(DictEntry {
            surface: surface.into(),
            pos: pos.into(),
            base_form: base.into(),
            reading: reading.into(),
            pronunciation: if surface == "は" { "ワ" } else { reading }.into(),
            conj_type: kind.into(),
            conj_form: form.into(),
            cost: -1000,
            ..Default::default()
        });
    }
    Analyzer::from_dict(builder.build().unwrap())
}

#[test]
fn observed_intensity_changes_only_readings_of_the_selected_known_token() {
    let mut analyzer = fixture();
    let raw = analyzer.try_tokenize("震度7").unwrap();
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].pronunciation.as_ref(), "シンドセブン");
    let text = "最大震度7を観測した。";
    let tokens = analyzer.try_tokenize(text).unwrap();
    let actual = tokens
        .iter()
        .find(|t| t.surface.as_ref() == "震度7")
        .unwrap();
    let mut expected = raw[0].clone();
    expected.start = "最大".len();
    expected.end = expected.start + expected.surface.len();
    expected.reading = "シンドナナ".into();
    expected.pronunciation = "シンドナナ".into();
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}

#[test]
fn source_reading_and_quantity_contract_stay_available_after_contextual_calls() {
    let mut analyzer = fixture();
    for _ in 0..2 {
        for text in [
            "最大震度7を観測した。",
            "震度7を記録した。",
            "ここで最大震度7を観測した。",
            "　最大震度7を観測した。",
        ] {
            let start = text.find("震度7").unwrap();
            assert_eq!(
                analyzer.contextual_reading_spans(text).unwrap(),
                [ContextualReadingSpan {
                    start,
                    end: start + "震度7".len(),
                    pronunciation: "シンドナナ".into()
                }]
            );
            assert!(analyzer.quantity_spans(text).unwrap().is_empty());
        }
        assert_eq!(analyzer.quantity_pronunciation("震度7").unwrap(), None);
        assert!(
            analyzer
                .contextual_reading_spans("震度7")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            analyzer.try_tokenize("震度7").unwrap()[0]
                .pronunciation
                .as_ref(),
            "シンドセブン"
        );
        assert!(
            analyzer
                .dictionary()
                .lookup("震度7")
                .unwrap()
                .iter()
                .any(|(end, rows)| *end == "震度7".len()
                    && rows
                        .iter()
                        .any(|row| row.reading.as_ref() == "シンドセブン"))
        );
    }
}

#[test]
fn quotations_across_sentence_boundaries_and_name_labels_keep_the_selected_reading() {
    let mut analyzer = fixture();
    for text in [
        "「震度7」を記録した。",
        "作品名は震度7を記録した。",
        "作品名は、最大震度7を記録した。",
        "題名も最大震度7を観測した。",
        "「前。最大震度7を観測した。」",
        "『前。震度7を記録した。』",
        "「前。最大震度7を観測した。",
        "\"前。最大震度7を観測した。\"",
        "（前。最大震度7を観測した。）",
        "`前。最大震度7を観測した。`",
        "\"前\\\"。最大震度7を観測した。\"",
        "ID-最大震度7を観測した。",
        "IDは最大震度7を観測した。",
        "最大震度7 を観測した。",
        "最大震度7を観測 した。",
        "最大震度7を観測したという題名",
        "最大震度7を観測したA",
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        let value = tokens
            .iter()
            .find(|t| t.surface.as_ref() == "震度7")
            .unwrap();
        assert_eq!(
            value.pronunciation.as_ref(),
            "シンドセブン",
            "{text}: {tokens:?}"
        );
        assert!(
            analyzer.contextual_reading_spans(text).unwrap().is_empty(),
            "{text}"
        );
    }
    let text = "「前。」。最大震度7を観測した。";
    let tokens = analyzer.try_tokenize(text).unwrap();
    assert_eq!(
        tokens
            .iter()
            .find(|t| t.surface.as_ref() == "震度7")
            .unwrap()
            .pronunciation
            .as_ref(),
        "シンドナナ"
    );
    // 引用の範囲を、次の独立した解析へ持ち越さない。
    assert_eq!(
        analyzer
            .contextual_reading_spans("最大震度7を観測した。")
            .unwrap()
            .len(),
        1
    );
    let text = "don't。最大震度7を観測した。";
    assert_eq!(analyzer.contextual_reading_spans(text).unwrap().len(), 1);
}

#[test]
fn whole_input_quote_ranges_and_global_offsets_survive_repeated_clauses() {
    let mut analyzer = fixture();
    let block = "「前。最大震度7を観測した。」。震度7を記録した。";
    let text = block.repeat(64);
    let spans = analyzer.contextual_reading_spans(&text).unwrap();
    let local = block.rfind("震度7").unwrap();
    assert_eq!(spans.len(), 64);
    for (index, span) in spans.iter().enumerate() {
        assert_eq!(span.start, index * block.len() + local);
        assert_eq!(span.end, span.start + "震度7".len());
        assert_eq!(span.pronunciation, "シンドナナ");
    }
    let sentences = analyzer
        .try_tokenize_sentences(&text, &hasami::sentence::SplitOptions::default())
        .unwrap();
    let full = analyzer.try_tokenize(&text).unwrap();
    let from_sentences: Vec<_> = sentences
        .into_iter()
        .flat_map(|(_, tokens)| tokens)
        .collect();
    assert_eq!(format!("{full:?}"), format!("{from_sentences:?}"));
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    let mut out = analyzer.try_tokenize("前").unwrap();
    let prior = format!("{out:?}");
    workspace
        .tokenize_into("最大震度7を観測した。", analyzer.dictionary(), 71, &mut out)
        .unwrap();
    assert_eq!(format!("{:?}", &out[..1]), prior);
    let value = out.iter().find(|t| t.surface.as_ref() == "震度7").unwrap();
    assert_eq!(
        (value.start, value.end),
        (71 + "最大".len(), 71 + "最大震度7".len())
    );
    assert_eq!(value.pronunciation.as_ref(), "シンドナナ");
}

#[test]
fn custom_target_fields_do_not_authorize_the_canonical_measurement_reading() {
    for field in 0..7 {
        let mut custom = target();
        match field {
            0 => custom.reading = "カスタム".into(),
            1 => custom.pronunciation = "カスタム".into(),
            2 => custom.base_form = "別の原形".into(),
            3 => custom.pos = "名詞,一般,*,*".into(),
            4 => custom.conj_type = "独自".into(),
            5 => custom.conj_form = "独自".into(),
            _ => {
                custom.surface = "震度７".into();
                custom.base_form = "震度７".into();
            }
        }
        let surface = custom.surface.to_string();
        let text = format!("最大{surface}を観測した。");
        let mut analyzer = fixture_with_target(custom);
        let mut expected = analyzer.try_tokenize(&surface).unwrap().remove(0);
        expected.start = "最大".len();
        expected.end = expected.start + surface.len();
        let tokens = analyzer.try_tokenize(&text).unwrap();
        let actual = tokens
            .iter()
            .find(|t| t.surface.as_ref() == surface)
            .unwrap();
        assert_eq!(
            format!("{actual:?}"),
            format!("{expected:?}"),
            "field {field}"
        );
        assert!(analyzer.contextual_reading_spans(&text).unwrap().is_empty());
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_seismic_measurements_keep_name_candidates_and_original_metadata() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        let bare = analyzer.try_tokenize("震度7").unwrap();
        for _ in 0..2 {
            for text in [
                "最大震度7を観測した。",
                "震度7を記録した。",
                "ここで最大震度7を観測した。",
            ] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                let start = text.find("震度7").unwrap();
                let end = start + "震度7".len();
                let focal: Vec<_> = tokens
                    .iter()
                    .filter(|t| t.start < end && start < t.end)
                    .collect();
                assert_eq!(focal.first().unwrap().start, start);
                assert_eq!(focal.last().unwrap().end, end);
                assert_eq!(
                    focal.iter().map(|t| &*t.reading).collect::<String>(),
                    "シンドナナ",
                    "{name}: {text}: {tokens:?}"
                );
                assert_eq!(
                    focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
                    "シンドナナ",
                    "{name}: {text}: {tokens:?}"
                );
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
                let spans = analyzer.contextual_reading_spans(text).unwrap();
                if name == "ipadic" {
                    assert!(spans.is_empty());
                } else {
                    assert_eq!(bare.len(), 1);
                    assert_eq!(bare[0].pronunciation.as_ref(), "シンドセブン");
                    assert_eq!(
                        spans,
                        [ContextualReadingSpan {
                            start,
                            end,
                            pronunciation: "シンドナナ".into()
                        }]
                    );
                    let mut expected = bare[0].clone();
                    expected.start = start;
                    expected.end = end;
                    expected.reading = "シンドナナ".into();
                    expected.pronunciation = "シンドナナ".into();
                    assert_eq!(format!("{:?}", focal[0]), format!("{expected:?}"));
                }
            }
            for text in [
                "小説「震度7」を読んだ。",
                "作品名は震度7を記録した。",
                "「前。最大震度7を観測した。」",
            ] {
                assert!(
                    analyzer.contextual_reading_spans(text).unwrap().is_empty(),
                    "{name}: {text}"
                );
            }
            assert_eq!(
                format!("{bare:?}"),
                format!("{:?}", analyzer.try_tokenize("震度7").unwrap())
            );
            assert_eq!(analyzer.quantity_pronunciation("震度7").unwrap(), None);
        }
    }
}
