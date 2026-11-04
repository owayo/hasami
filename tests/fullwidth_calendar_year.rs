//! 全角の年の位取りだけを補い、後続の月日を保持する。
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
    for (ascii, wide, r, p) in [
        ("0", "０", "ゼロ", "ゼロ"),
        ("1", "１", "イチ", "イチ"),
        ("2", "２", "ニ", "ニ"),
        ("3", "３", "サン", "サン"),
        ("4", "４", "ヨン", "ヨン"),
        ("5", "５", "ゴ", "ゴ"),
        ("6", "６", "ロク", "ロク"),
        ("7", "７", "ナナ", "ナナ"),
        ("8", "８", "ハチ", "ハチ"),
        ("9", "９", "キュウ", "キュー"),
    ] {
        for surface in [ascii, wide] {
            builder.add_entry(entry(surface, "名詞,数,*,*", r, p));
        }
    }
    for (s, pos, r, p) in [
        ("年", "名詞,接尾,助数詞,*", "ネン", "ネン"),
        (
            "１１月",
            "名詞,副詞可能,*,*",
            "ジュウイチガツ",
            "ジューイチガツ",
        ),
        ("月", "名詞,一般,*,*", "ツキ", "ツキ"),
        ("日", "名詞,接尾,助数詞,*", "ニチ", "ニチ"),
        ("名前", "名詞,一般,*,*", "ナマエ", "ナマエ"),
        ("日付", "名詞,一般,*,*", "ヒヅケ", "ヒヅケ"),
        ("は", "助詞,係助詞,*,*", "ハ", "ワ"),
        ("。", "記号,句点,*,*", "。", "。"),
    ] {
        builder.add_entry(entry(s, pos, r, p));
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

#[test]
fn fullwidth_year_before_month_and_day_uses_existing_positional_composition() {
    let mut analyzer = fixture(None);
    let text = "２０１６年１１月１日";
    let tokens = analyzer.try_tokenize(text).unwrap();
    let number = focal(&tokens, "２０１６");
    assert_eq!(number.reading.as_ref(), "ニセンジュウロク");
    assert_eq!(number.pronunciation.as_ref(), "ニセンジューロク");
    assert_eq!(number.base_form, number.surface);
    assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
    assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
    assert_eq!(number.word_cost, -4000);
    assert!(!number.is_known);
    assert_eq!((number.start, number.end), (0, "２０１６".len()));
    for token in &tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
}

fn shifted(mut tokens: Vec<Token>, offset: usize) -> Vec<Token> {
    for token in &mut tokens {
        token.start += offset;
        token.end += offset;
    }
    tokens
}

#[test]
fn each_calendar_component_can_have_its_own_uniform_width_without_changing_month_or_day() {
    let mut analyzer = fixture(None);
    for tail in [
        "１１月１日",
        "11月1日",
        "11月１日",
        "１１月1日",
        "１１月",
        "11月",
    ] {
        let expected = shifted(
            analyzer.try_tokenize(tail).unwrap(),
            "日付は２０１６年".len(),
        );
        let text = format!("日付は２０１６年{tail}");
        let tokens = analyzer.try_tokenize(&text).unwrap();
        let number = focal(&tokens, "２０１６");
        assert_eq!(number.pronunciation.as_ref(), "ニセンジューロク", "{text}");
        let actual: Vec<_> = tokens
            .iter()
            .filter(|t| t.start >= "日付は２０１６年".len())
            .cloned()
            .collect();
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{text}");
        assert!(analyzer.contextual_reading_spans(&text).unwrap().is_empty());
        assert_eq!(analyzer.quantity_pronunciation(&text).unwrap(), None);
        assert!(
            analyzer
                .quantity_spans(&text)
                .unwrap()
                .iter()
                .all(|span| span.start >= "日付は２０１６年".len())
        );
    }
    for (year, speech) in [
        ("２０１４", "ニセンジューヨ"),
        ("２０２４", "ニセンニジューヨ"),
        ("２０１８", "ニセンジューハチ"),
        ("１２", "ジューニ"),
    ] {
        let text = format!("{year}年１１月１日");
        let tokens = analyzer.try_tokenize(&text).unwrap();
        assert_eq!(
            focal(&tokens, year).pronunciation.as_ref(),
            speech,
            "{text}: {tokens:?}"
        );
        assert_eq!(focal(&tokens, "年").pronunciation.as_ref(), "ネン");
    }
    let standalone = analyzer.try_tokenize("２０１６年").unwrap();
    assert_eq!(
        focal(&standalone, "２０１６").pronunciation.as_ref(),
        "ニセンジューロク"
    );
}

fn assert_kept_in_name_context(analyzer: &mut Analyzer, text: &str) {
    let prefixed = format!("名前は{text}");
    let mut expected: Vec<_> = analyzer
        .try_tokenize(&prefixed)
        .unwrap()
        .into_iter()
        .filter(|t| t.start >= "名前は".len())
        .collect();
    for token in &mut expected {
        token.start -= "名前は".len();
        token.end -= "名前は".len();
    }
    let actual = analyzer.try_tokenize(text).unwrap();
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{text}");
}

#[test]
fn invalid_calendar_ids_names_quotes_and_mixed_components_keep_all_fields() {
    let mut analyzer = fixture(None);
    for text in [
        "０１６年１１月１日",
        "００１６年１１月１日",
        "２016年１１月１日",
        "2０１６年１１月１日",
        "二２０１６年１１月１日",
        "２０１６年０１月１日",
        "２０１６年01月1日",
        "２０１６年１１月０１日",
        "２０１６年11月01日",
        "２０１６年1１月1日",
        "２０１６年１１月1１日",
        "２０１６年０月１日",
        "２０１６年１３月１日",
        "２０１６年１１月０日",
        "２０１６年１１月３２日",
        "２０１６年１１月１日A",
        "２０１６年１１月１日Ａ",
        "２０１６年１１月１日_1",
        "２０１６年１１月A",
        "２０１６年１１月1",
        "ID２０１６年１１月１日",
        "ID\n２０１６年１１月１日",
        "A.２０１６年１１月１日",
        "第２０１６年１１月１日",
        "型番は２０１６年１１月１日",
        "名前は２０１６年１１月１日",
        "作品名は２０１６年１１月１日",
        "氏名は、２０１６年１１月１日",
        "２０１６年１１月１日という名前",
        "２０１６年１１月１日という作品名",
        "２０１６年１１月１日さん",
        "「前。２０１６年１１月１日」",
        "「前。\n２０１６年１１月１日",
        "（前。２０１６年１１月１日）",
        "\"前。２０１６年１１月１日\"",
    ] {
        assert_kept_in_name_context(&mut analyzer, text);
    }
}

#[test]
fn custom_selected_fields_and_known_whole_numbers_or_year_words_are_preserved() {
    for surface in ["２", "年"] {
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
                3 => custom.pos = "名詞,固有名詞,人名,一般".into(),
                4 => custom.conj_type = "独自".into(),
                _ => custom.conj_form = "独自".into(),
            }
            assert_kept_in_name_context(&mut fixture(Some(custom)), "２０１６年１１月１日");
        }
    }
    for (surface, pos, speech) in [
        ("２０１６", "名詞,数,*,*", "ニセンジューロク"),
        ("２０１６年", "名詞,固有名詞,一般,*", "サクヒンメイ"),
    ] {
        let mut custom = entry(surface, pos, speech, speech);
        custom.cost = -30000;
        assert_kept_in_name_context(&mut fixture(Some(custom)), "２０１６年１１月１日");
    }
    // 旧単独年は新しいcalendar用のconj条件へ巻き込まない。
    let mut old_year = entry("年", "名詞,接尾,助数詞,*", "ネン", "ネン");
    old_year.conj_type = "独自".into();
    old_year.cost = -30000;
    let mut analyzer = fixture(Some(old_year));
    assert_eq!(
        focal(&analyzer.try_tokenize("２０１６年").unwrap(), "２０１６")
            .pronunciation
            .as_ref(),
        "ニセンジューロク"
    );
}

#[test]
fn year_cost_saturates_once_and_global_offsets_survive_repeated_dates() {
    let mut digit = entry("２", "名詞,数,*,*", "ニ", "ニ");
    digit.cost = -32000;
    let mut analyzer = fixture(Some(digit));
    let text = "２０１６年１１月１日";
    assert_eq!(
        focal(&analyzer.try_tokenize(text).unwrap(), "２０１６").word_cost,
        i16::MIN
    );
    let block = format!("「前。{text}」。{text}。");
    let input = block.repeat(64);
    let tokens = analyzer.try_tokenize(&input).unwrap();
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.surface.as_ref() == "２０１６")
            .count(),
        64
    );
    let sentences = analyzer
        .try_tokenize_sentences(&input, &Default::default())
        .unwrap();
    let from_sentences: Vec<_> = sentences
        .into_iter()
        .flat_map(|(_, tokens)| tokens)
        .collect();
    assert_eq!(format!("{tokens:?}"), format!("{from_sentences:?}"));
    for token in &tokens {
        assert_eq!(
            input.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
    let mut out = analyzer.try_tokenize("名前").unwrap();
    let prior = format!("{out:?}");
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    workspace
        .tokenize_into(text, analyzer.dictionary(), 73, &mut out)
        .unwrap();
    assert_eq!(format!("{:?}", &out[..1]), prior);
    let expected = shifted(analyzer.try_tokenize(text).unwrap(), 73);
    assert_eq!(format!("{:?}", &out[1..]), format!("{expected:?}"));
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_fullwidth_years_keep_the_selected_calendar_suffix_tokens() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (year, speech) in [
            ("２０１６", "ニセンジューロク"),
            ("２０１４", "ニセンジューヨ"),
            ("２０２４", "ニセンニジューヨ"),
        ] {
            let text = format!("{year}年１１月１日");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let number = focal(&tokens, year);
            assert_eq!(number.pronunciation.as_ref(), speech, "{name}: {tokens:?}");
            assert_eq!(number.base_form, number.surface);
            assert!(!number.is_known);
            assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
            let month = focal(&tokens, "１１月");
            assert!(month.is_known);
            assert_eq!(month.pronunciation.as_ref(), "ジューイチガツ");
            for token in &tokens {
                assert_eq!(
                    text.get(token.start..token.end),
                    Some(token.surface.as_ref())
                );
            }
        }
        for text in [
            "名前は２０１６年１１月１日",
            "「前。２０１６年１１月１日」",
            "２０１６年０１月１日",
        ] {
            assert!(
                !analyzer
                    .try_tokenize(text)
                    .unwrap()
                    .iter()
                    .any(|t| t.surface.as_ref() == "２０１６"),
                "{name}: {text}"
            );
        }
    }
}
