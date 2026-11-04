//! 空白のないASCII暦月を補い、月以外の位置と素性を保持する。
#![cfg(feature = "build")]

use hasami::{Analyzer, DictEntry, Token, dict::DictBuilder};

fn entry(surface: &str, pos: &str, reading: &str, pronunciation: &str) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        base_form: surface.into(),
        pos: pos.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
        cost: -1000,
        ..Default::default()
    }
}

fn fixture(custom: Option<DictEntry>) -> Analyzer {
    let mut builder = DictBuilder::new();
    for (number, speech) in [
        ("0", "ゼロ"),
        ("1", "イチ"),
        ("2", "ニ"),
        ("3", "サン"),
        ("4", "ヨン"),
        ("5", "ゴ"),
        ("6", "ロク"),
        ("7", "ナナ"),
        ("8", "ハチ"),
        ("9", "キュウ"),
    ] {
        builder.add_entry(entry(number, "名詞,数,*,*", speech, speech));
    }
    for (surface, pos, speech) in [
        ("月", "名詞,一般,*,*", "ツキ"),
        ("年", "名詞,接尾,助数詞,*", "ネン"),
        ("1日", "名詞,接尾,助数詞,*", "イチニチ"),
        ("名前", "名詞,一般,*,*", "ナマエ"),
        ("日付", "名詞,一般,*,*", "ヒヅケ"),
        ("は", "助詞,係助詞,*,*", "ハ"),
        ("も", "助詞,係助詞,*,*", "モ"),
        ("。", "記号,句点,*,*", "。"),
        ("、", "記号,読点,*,*", "、"),
    ] {
        builder.add_entry(entry(
            surface,
            pos,
            speech,
            if surface == "は" { "ワ" } else { speech },
        ));
    }
    let mut copula = entry("です", "助動詞,*,*,*", "デス", "デス");
    copula.conj_type = "特殊・デス".into();
    copula.conj_form = "基本形".into();
    builder.add_entry(copula);
    if let Some(custom) = custom {
        builder.add_entry(custom);
    }
    Analyzer::from_dict(builder.build().unwrap())
}

fn focal(tokens: &[Token], surface: &str) -> Token {
    tokens
        .iter()
        .find(|t| t.surface.as_ref() == surface)
        .unwrap()
        .clone()
}

const MONTHS: [(&str, &str); 12] = [
    ("イチ", "イチ"),
    ("ニ", "ニ"),
    ("サン", "サン"),
    ("シ", "シ"),
    ("ゴ", "ゴ"),
    ("ロク", "ロク"),
    ("シチ", "シチ"),
    ("ハチ", "ハチ"),
    ("ク", "ク"),
    ("ジュウ", "ジュー"),
    ("ジュウイチ", "ジューイチ"),
    ("ジュウニ", "ジューニ"),
];

#[test]
fn all_ascii_calendar_months_keep_single_token_metadata_and_merge_only_separate_digits() {
    let mut analyzer = fixture(None);
    let original_month = focal(&analyzer.try_tokenize("月").unwrap(), "月");
    for (index, (reading, pronunciation)) in MONTHS.iter().enumerate() {
        let number = (index + 1).to_string();
        for prefix in ["", "日付は"] {
            let text = format!("{prefix}{number}月");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let actual_month = focal(&tokens, "月");
            let mut expected_month = original_month.clone();
            expected_month.start = prefix.len() + number.len();
            expected_month.end = expected_month.start + "月".len();
            expected_month.reading = "ガツ".into();
            expected_month.pronunciation = "ガツ".into();
            assert_eq!(
                format!("{actual_month:?}"),
                format!("{expected_month:?}"),
                "{text}"
            );
            let actual_number = focal(&tokens, &number);
            assert_eq!(actual_number.reading.as_ref(), *reading, "{text}");
            assert_eq!(
                actual_number.pronunciation.as_ref(),
                *pronunciation,
                "{text}"
            );
            assert_eq!(actual_number.base_form, actual_number.surface);
            assert_eq!(
                actual_number.word_cost,
                if number.len() == 1 { -1000 } else { -2000 }
            );
            assert_eq!(actual_number.is_known, number.len() == 1);
            assert!(analyzer.quantity_spans(&text).unwrap().is_empty(), "{text}");
            assert!(
                analyzer.contextual_reading_spans(&text).unwrap().is_empty(),
                "{text}"
            );
            assert_eq!(
                analyzer.quantity_pronunciation(&text).unwrap(),
                None,
                "{text}"
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

fn assert_kept_in_name_context(analyzer: &mut Analyzer, text: &str) {
    let control = format!("名前は{text}");
    let expected = analyzer.try_tokenize(&control).unwrap();
    let actual = analyzer.try_tokenize(text).unwrap();
    let mut body: Vec<_> = expected
        .into_iter()
        .filter(|t| t.start >= "名前は".len())
        .collect();
    for token in &mut body {
        token.start -= "名前は".len();
        token.end -= "名前は".len();
    }
    assert_eq!(format!("{actual:?}"), format!("{body:?}"), "{text}");
}

#[test]
fn independent_dates_correct_only_the_month_and_keep_the_original_day_token() {
    let mut analyzer = fixture(None);
    let bare_day = focal(&analyzer.try_tokenize("1日").unwrap(), "1日");
    for (index, (reading, pronunciation)) in MONTHS.iter().enumerate() {
        let value = index + 1;
        for text in [
            format!("日付は{value}月1日です。"),
            format!("2024年{value}月1日"),
        ] {
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let number = focal(&tokens, &value.to_string());
            assert_eq!(number.reading.as_ref(), *reading, "{text}");
            assert_eq!(number.pronunciation.as_ref(), *pronunciation, "{text}");
            assert_eq!(focal(&tokens, "月").pronunciation.as_ref(), "ガツ");
            let mut expected = bare_day.clone();
            expected.start = text.find("1日").unwrap();
            expected.end = expected.start + "1日".len();
            assert_eq!(
                format!("{:?}", focal(&tokens, "1日")),
                format!("{expected:?}"),
                "{text}"
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

#[test]
fn names_ids_quotes_out_of_range_and_other_month_words_remain_unchanged() {
    let mut analyzer = fixture(None);
    for text in [
        "0月",
        "00月",
        "01月",
        "13月",
        "111月",
        "1１月",
        "１１月",
        "-11月",
        "+11月",
        "1.11月",
        "1/11月",
        "A11月",
        "A_11月",
        "Ａ11月",
        "第11月",
        "ID\n11月",
        "名前は11月です。",
        "姓は11月です。",
        "氏名は、11月です。",
        "作品名は11月です。",
        "名前は2024年11月1日です。",
        "型番A2024年11月1日",
        "11月という名前",
        "11月さん",
        "11月間",
        "11月曜日",
        "11月額",
        "11月見",
        "11月A",
        "11月１A",
        "11月1",
        "11月0日",
        "11月32日",
        "11月01日",
        "11月1日A",
        "11月1日という名前",
        "「前。11月です。」",
        "「前。11月です。",
        "作品名は『前。11月1日です。』",
        "\"前。\n11月です。\"",
        "（前。11月です。）",
    ] {
        assert_kept_in_name_context(&mut analyzer, text);
    }
}

#[test]
fn custom_month_number_fields_and_unselected_whole_entries_are_preserved() {
    for surface in ["1", "月"] {
        for field in 0..6 {
            let baseline = fixture(None);
            let mut custom = baseline
                .dictionary()
                .lookup(surface)
                .unwrap()
                .into_iter()
                .find(|(end, _)| *end == surface.len())
                .unwrap()
                .1
                .remove(0);
            custom.cost = -30000;
            match field {
                0 => custom.reading = "カスタム".into(),
                1 => custom.pronunciation = "カスタム".into(),
                2 => custom.base_form = "別の原形".into(),
                3 => custom.pos = "名詞,固有名詞,人名,姓".into(),
                4 => custom.conj_type = "独自".into(),
                _ => custom.conj_form = "独自".into(),
            }
            assert_kept_in_name_context(&mut fixture(Some(custom)), "11月");
        }
    }
    let mut whole = entry("11月", "名詞,固有名詞,一般,*", "カスタム", "カスタム");
    whole.cost = 1000;
    assert_kept_in_name_context(&mut fixture(Some(whole)), "11月");
    let mut number = entry("11", "名詞,数,*,*", "ジュウイチ", "ジュウイチ");
    number.cost = -30000;
    let mut analyzer = fixture(Some(number));
    let mut expected = focal(&analyzer.try_tokenize("11").unwrap(), "11");
    expected.reading = "ジュウイチ".into();
    expected.pronunciation = "ジューイチ".into();
    assert_eq!(
        format!("{:?}", focal(&analyzer.try_tokenize("11月").unwrap(), "11")),
        format!("{expected:?}")
    );
}

#[test]
fn repeated_frames_keep_global_offsets_and_leave_prior_workspace_output_untouched() {
    let mut analyzer = fixture(None);
    let block = "「前。11月です。」。11月です。";
    let text = block.repeat(64);
    let tokens = analyzer.try_tokenize(&text).unwrap();
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.surface.as_ref() == "月" && t.pronunciation.as_ref() == "ガツ")
            .count(),
        64
    );
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.surface.as_ref() == "月" && t.pronunciation.as_ref() == "ツキ")
            .count(),
        64
    );
    let sentences = analyzer
        .try_tokenize_sentences(&text, &Default::default())
        .unwrap();
    let from_sentences: Vec<_> = sentences
        .into_iter()
        .flat_map(|(_, tokens)| tokens)
        .collect();
    assert_eq!(format!("{tokens:?}"), format!("{from_sentences:?}"));
    for token in &tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    let mut out = analyzer.try_tokenize("名前").unwrap();
    let previous = format!("{out:?}");
    workspace
        .tokenize_into("11月", analyzer.dictionary(), 73, &mut out)
        .unwrap();
    assert_eq!(format!("{:?}", &out[..1]), previous);
    let mut expected = analyzer.try_tokenize("11月").unwrap();
    for token in &mut expected {
        token.start += 73;
        token.end += 73;
    }
    assert_eq!(format!("{:?}", &out[1..]), format!("{expected:?}"));
}

#[test]
fn merging_two_month_digits_saturates_the_cost_only_once() {
    let mut one = entry("1", "名詞,数,*,*", "イチ", "イチ");
    one.cost = -30000;
    let mut analyzer = fixture(Some(one));
    let tokens = analyzer.try_tokenize("11月").unwrap();
    let number = focal(&tokens, "11");
    assert_eq!(number.word_cost, i16::MIN);
    assert!(!number.is_known);
    assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
    assert_eq!(number.base_form.as_ref(), "11");
    assert_eq!(number.pronunciation.as_ref(), "ジューイチ");
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_ascii_calendar_months_preserve_existing_known_month_names() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (index, (reading, pronunciation)) in MONTHS.iter().enumerate() {
            let value = index + 1;
            for text in [
                format!("{value}月"),
                format!("日付は{value}月1日です。"),
                format!("2024年{value}月1日"),
            ] {
                let start = text.rfind(&format!("{value}月")).unwrap();
                let end = start + value.to_string().len() + "月".len();
                let tokens = analyzer.try_tokenize(&text).unwrap();
                let focal: Vec<_> = tokens
                    .iter()
                    .filter(|t| t.start < end && start < t.end)
                    .collect();
                assert_eq!(
                    focal.iter().map(|t| &*t.reading).collect::<String>(),
                    format!("{reading}ガツ"),
                    "{name}: {text}: {tokens:?}"
                );
                assert_eq!(
                    focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
                    format!("{pronunciation}ガツ"),
                    "{name}: {text}: {tokens:?}"
                );
                if name == "ipadic" {
                    assert_eq!(focal.len(), 2);
                } else {
                    assert_eq!(focal.len(), 1);
                    assert!(focal[0].is_known);
                }
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
                assert!(analyzer.contextual_reading_spans(&text).unwrap().is_empty());
            }
        }
        let known = analyzer.try_tokenize("１１月").unwrap();
        assert_eq!(known.len(), 1);
        assert!(known[0].is_known);
        assert_eq!(known[0].pronunciation.as_ref(), "ジューイチガツ");
    }
}
