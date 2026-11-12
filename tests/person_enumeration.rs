//! 人の列挙の「外」と空間の「外」を区別する。
#![cfg(feature = "build")]

use hasami::{Analyzer, DictEntry, dict::DictBuilder};

fn fixture() -> Analyzer {
    let mut builder = DictBuilder::new();
    for (surface, pos, reading, pronunciation) in [
        ("佐藤", "名詞,固有名詞,人名,姓", "サトウ", "サトー"),
        ("田中", "名詞,固有名詞,人名,姓", "タナカ", "タナカ"),
        ("太郎", "名詞,固有名詞,人名,名", "タロウ", "タロー"),
        ("さん", "名詞,接尾,人名,*", "サン", "サン"),
        ("氏", "名詞,接尾,人名,*", "シ", "シ"),
        ("課長", "名詞,一般,*,*", "カチョウ", "カチョー"),
        ("敷地", "名詞,一般,*,*", "シキチ", "シキチ"),
        ("区域", "名詞,一般,*,*", "クイキ", "クイキ"),
        ("外", "名詞,接尾,一般,*", "ガイ", "ガイ"),
        ("二", "名詞,数,*,*", "ニ", "ニ"),
        ("三", "名詞,数,*,*", "サン", "サン"),
        ("十", "名詞,数,*,*", "ジュウ", "ジュー"),
        ("２", "名詞,数,*,*", "ニ", "ニ"),
        ("2", "名詞,数,*,*", "ニ", "ニ"),
        ("名", "名詞,接尾,助数詞,*", "メイ", "メイ"),
        ("人", "名詞,接尾,助数詞,*", "ニン", "ニン"),
        ("個", "名詞,接尾,助数詞,*", "コ", "コ"),
        ("が", "助詞,格助詞,一般,*", "ガ", "ガ"),
        ("の", "助詞,連体化,*,*", "ノ", "ノ"),
        ("を", "助詞,格助詞,一般,*", "ヲ", "ヲ"),
        ("。", "記号,句点,*,*", "。", "。"),
        ("、", "記号,読点,*,*", "、", "、"),
    ] {
        builder.add_entry(DictEntry {
            surface: surface.into(),
            pos: pos.into(),
            base_form: surface.into(),
            reading: reading.into(),
            pronunciation: pronunciation.into(),
            cost: -1000,
            ..Default::default()
        });
    }
    Analyzer::from_dict(builder.build().unwrap())
}

#[test]
fn explicit_people_and_reviewed_titles_read_hoka_before_a_person_count() {
    let mut analyzer = fixture();
    for input in [
        "佐藤さん外二名が",
        "田中太郎外三名が",
        "課長外三名が",
        "田中氏外十二名。",
        "佐藤外２名。",
        "課長外2名。",
        "佐藤さん外二人が",
        "敷地の課長外二名。",
    ] {
        let tokens = analyzer.try_tokenize(input).unwrap();
        let outside = tokens.iter().find(|t| t.surface.as_ref() == "外").unwrap();
        assert_eq!(
            (&*outside.reading, &*outside.pronunciation),
            ("ホカ", "ホカ"),
            "{input}: {tokens:?}"
        );
        assert_eq!(&*outside.pos, "名詞,接尾,一般,*");
        assert_eq!(&*outside.base_form, "外");
        assert!(outside.is_known);
        assert_eq!(outside.word_cost, -1000);
        for t in &tokens {
            assert_eq!(input.get(t.start..t.end), Some(t.surface.as_ref()));
        }
        assert_eq!(
            analyzer.contextual_reading_spans(input).unwrap(),
            [hasami::ContextualReadingSpan {
                start: outside.start,
                end: outside.end,
                pronunciation: "ホカ".into(),
            }]
        );
    }
}

#[test]
fn spatial_words_gaps_other_counters_quotes_and_identifiers_keep_gai() {
    let mut analyzer = fixture();
    for input in [
        "敷地外二名を",
        "区域外二名、区域外三名。",
        "外二名。",
        "佐藤さん外三個。",
        "課長外。",
        "田中外の二名。",
        "佐藤さん 外二名。",
        "佐藤さん外 二名。",
        "佐藤さん外二 名。",
        "佐藤 さん外二名。",
        "佐藤さん外二\n名。",
        "ID_佐藤さん外二名。",
        "ID.佐藤さん外二名。",
        "佐藤さん外二名.csv",
        "「佐藤さん外二名」",
        "敷地課長外二名。",
    ] {
        let tokens = analyzer.try_tokenize(input).unwrap();
        for outside in tokens.iter().filter(|t| t.surface.as_ref() == "外") {
            assert_eq!(
                (&*outside.reading, &*outside.pronunciation),
                ("ガイ", "ガイ"),
                "{input}: {tokens:?}"
            );
        }
        assert!(
            analyzer.contextual_reading_spans(input).unwrap().is_empty(),
            "{input}"
        );
    }
}

#[test]
fn document_sentences_and_reused_analyzers_keep_original_byte_ranges() {
    let mut analyzer = fixture();
    let input = "敷地外二名。\n佐藤さん外二名。田中太郎外三名。区域外三名。";
    let direct = analyzer.try_tokenize(input).unwrap();
    let by_sentence: Vec<_> = analyzer
        .try_tokenize_sentences(input, &Default::default())
        .unwrap()
        .into_iter()
        .flat_map(|(_, tokens)| tokens)
        .collect();
    assert_eq!(format!("{direct:?}"), format!("{by_sentence:?}"));
    let spans = analyzer.contextual_reading_spans(input).unwrap();
    assert_eq!(spans.len(), 2);
    for span in spans {
        assert_eq!(&input[span.start..span.end], "外");
        assert_eq!(span.pronunciation, "ホカ");
    }
    assert!(
        analyzer
            .contextual_reading_spans("敷地外二名。")
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_person_enumeration_and_spatial_controls() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (input, expected) in [
            ("佐藤さん外二名が参加した。", "ホカ"),
            ("田中太郎外三名が来た。", "ホカ"),
            ("課長外三名が参加した。", "ホカ"),
            ("田中氏外十二名が来た。", "ホカ"),
            ("佐藤さん外二人が来た。", "ホカ"),
            ("敷地外二名を確認した。", "ガイ"),
            ("区域外二名、区域内一名を確認した。", "ガイ"),
            ("外を歩く。", "ソト"),
        ] {
            let tokens = analyzer.try_tokenize(input).unwrap();
            let outside = tokens.iter().find(|t| t.surface.as_ref() == "外").unwrap();
            assert_eq!(
                (&*outside.reading, &*outside.pronunciation),
                (expected, expected),
                "{name}: {input}: {tokens:?}"
            );
        }
    }
}
