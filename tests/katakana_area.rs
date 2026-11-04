//! 明確な面積構文のカタカナ数量と、同じ綴りを使う名称を区別する。
#![cfg(feature = "build")]

use hasami::{Analyzer, DictEntry, Token, dict::DictBuilder};

fn entry(surface: &str, base: &str, pos: &str, reading: &str, pronunciation: &str) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        base_form: base.into(),
        pos: pos.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
        cost: -1000,
        ..Default::default()
    }
}

fn fixture(custom: Option<DictEntry>) -> Analyzer {
    let mut builder = DictBuilder::new();
    for item in [
        entry("面積", "面積", "名詞,一般,*,*", "メンセキ", "メンセキ"),
        entry("名前", "名前", "名詞,一般,*,*", "ナマエ", "ナマエ"),
        entry("は", "は", "助詞,係助詞,*,*", "ハ", "ワ"),
        entry("が", "が", "助詞,格助詞,一般,*", "ガ", "ガ"),
        entry("も", "も", "助詞,係助詞,*,*", "モ", "モ"),
        entry(
            "ニジュウナナ",
            "27",
            "名詞,固有名詞,一般,*",
            "ニジュウナナ",
            "ニジュウナナ",
        ),
        entry("キュウ", "9", "名詞,固有名詞,一般,*", "キュウ", "キュウ"),
        entry(
            "ヒャクハチ",
            "108",
            "名詞,固有名詞,一般,*",
            "ヒャクハチ",
            "ヒャクハチ",
        ),
        entry(
            "ヘイホー",
            "Hey Ho",
            "名詞,固有名詞,一般,*",
            "ヘイホー",
            "ヘイホー",
        ),
        entry(
            "メートル",
            "メートル",
            "名詞,接尾,助数詞,*",
            "メートル",
            "メートル",
        ),
        entry("。", "。", "記号,句点,*,*", "。", "。"),
        entry("、", "、", "記号,読点,*,*", "、", "、"),
    ] {
        builder.add_entry(item);
    }
    let mut copula = entry("です", "です", "助動詞,*,*,*", "デス", "デス");
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

#[test]
fn complete_area_statement_produces_explicit_synthetic_quantity_metadata() {
    let mut analyzer = fixture(None);
    let text = "面積はニジュウナナヘイホーメートルです。";
    let tokens = analyzer.try_tokenize(text).unwrap();
    let number = focal(&tokens, "ニジュウナナ");
    assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
    assert_eq!(number.base_form.as_ref(), "27");
    assert_eq!(number.reading.as_ref(), "ニジュウナナ");
    assert_eq!(number.pronunciation.as_ref(), "ニジューナナ");
    assert!(!number.is_known);
    assert_eq!(number.word_cost, -1000);
    let unit = focal(&tokens, "ヘイホーメートル");
    assert_eq!(unit.pos.as_ref(), "名詞,接尾,助数詞,*");
    assert_eq!(unit.base_form.as_ref(), "平方メートル");
    assert_eq!(unit.reading.as_ref(), "ヘイホウメートル");
    assert_eq!(unit.pronunciation.as_ref(), "ヘイホーメートル");
    assert_eq!(unit.word_cost, -2000);
    assert!(!unit.is_known);
    assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
    assert!(unit.conj_type.is_empty() && unit.conj_form.is_empty());
    assert_eq!(tokens.len(), 6);
    for token in &tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
}

fn assert_kept_against_name_context(analyzer: &mut Analyzer, text: &str) {
    let control = text.replacen("面積", "名前", 1);
    let expected = analyzer.try_tokenize(&control).unwrap();
    let actual = analyzer.try_tokenize(text).unwrap();
    assert_eq!(actual.len(), expected.len(), "{text}: {actual:?}");
    for (actual, expected) in actual.iter().zip(&expected) {
        if actual.surface.as_ref() != "面積" {
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{text}");
        }
    }
}

#[test]
fn confirmed_aliases_generalize_beyond_one_value_and_leave_public_queries_conservative() {
    let mut analyzer = fixture(None);
    for (surface, base, reading, pronunciation) in [
        ("ニジュウナナ", "27", "ニジュウナナ", "ニジューナナ"),
        ("キュウ", "9", "キュウ", "キュー"),
        ("ヒャクハチ", "108", "ヒャクハチ", "ヒャクハチ"),
    ] {
        for particle in ["は", "が", "も"] {
            let text = format!("面積{particle}{surface}ヘイホーメートルです。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let number = focal(&tokens, surface);
            assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
            assert_eq!(number.base_form.as_ref(), base);
            assert_eq!(number.reading.as_ref(), reading);
            assert_eq!(number.pronunciation.as_ref(), pronunciation);
            assert!(!number.is_known);
            assert_eq!(
                focal(&tokens, "ヘイホーメートル").base_form.as_ref(),
                "平方メートル"
            );
            assert!(analyzer.quantity_spans(&text).unwrap().is_empty());
            assert!(analyzer.contextual_reading_spans(&text).unwrap().is_empty());
            assert_eq!(analyzer.quantity_pronunciation(&text).unwrap(), None);
            let ascii_period = text.replace('。', ".");
            assert_eq!(
                focal(&analyzer.try_tokenize(&ascii_period).unwrap(), surface)
                    .pronunciation
                    .as_ref(),
                pronunciation
            );
        }
        assert_eq!(
            analyzer
                .quantity_pronunciation(&format!("{surface}ヘイホーメートル"))
                .unwrap(),
            None
        );
        let bare = analyzer.try_tokenize(surface).unwrap();
        assert!(bare[0].is_known);
        assert_eq!(bare[0].pos.as_ref(), "名詞,固有名詞,一般,*");
        assert_eq!(bare[0].pronunciation.as_ref(), surface);
    }
    assert!(
        analyzer
            .dictionary()
            .lookup("ヘイホー")
            .unwrap()
            .iter()
            .any(|(_, rows)| rows.iter().any(|e| e.base_form.as_ref() == "Hey Ho"))
    );
    let bare = analyzer.try_tokenize("ヘイホー").unwrap();
    assert_eq!(bare[0].base_form.as_ref(), "Hey Ho");
    assert!(bare[0].is_known);
}

#[test]
fn names_quotes_identifiers_and_incomplete_frames_keep_all_original_fields() {
    let mut analyzer = fixture(None);
    for text in [
        "面積はニジュウナナヘイホーメートル",
        "面積はニジュウナナヘイホーメートルだ。",
        "面積。ニジュウナナヘイホーメートルです。",
        "面積は、ニジュウナナヘイホーメートルです。",
        "面積 はニジュウナナヘイホーメートルです。",
        "面積は ニジュウナナヘイホーメートルです。",
        "面積はニジュウナナ ヘイホーメートルです。",
        "面積はニジュウナナヘイホー メートルです。",
        "面積はニジュウナナヘイホーメートル です。",
        "面積はニジュウナナヘイホーメートルですA",
        "面積はニジュウナナヘイホーメートルです_12",
        "面積はニジュウナナヘイホーメートルです.A",
        "面積はニジュウナナヘイホーメートルですという題名。",
        "ID_面積はニジュウナナヘイホーメートルです。",
        "名称は、面積はニジュウナナヘイホーメートルです。",
        "「前。面積はニジュウナナヘイホーメートルです。」",
        "「前。面積はニジュウナナヘイホーメートルです。",
        "作品名は「前。面積はニジュウナナヘイホーメートルです。」",
        "識別子は『前。面積はニジュウナナヘイホーメートルです。』",
        "\"前。面積はニジュウナナヘイホーメートルです。\"",
        "（前。面積はニジュウナナヘイホーメートルです。）",
    ] {
        assert_kept_against_name_context(&mut analyzer, text);
    }
}

#[test]
fn custom_fields_and_existing_whole_words_are_never_reinterpreted_as_area() {
    for surface in ["面積", "は", "ニジュウナナ", "ヘイホー", "メートル", "です"]
    {
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
                2 => custom.base_form = "独自の原形".into(),
                3 => custom.pos = "名詞,固有名詞,人名,一般".into(),
                4 => custom.conj_type = "独自".into(),
                _ => custom.conj_form = "独自".into(),
            }
            assert_kept_against_name_context(
                &mut fixture(Some(custom)),
                "面積はニジュウナナヘイホーメートルです。",
            );
        }
    }
    for base in ["28", "027", "-27", "27.0", "２７", "27000000000000000"] {
        let mut custom = entry(
            "ニジュウナナ",
            base,
            "名詞,固有名詞,一般,*",
            "ニジュウナナ",
            "ニジュウナナ",
        );
        custom.cost = -30000;
        assert_kept_against_name_context(
            &mut fixture(Some(custom)),
            "面積はニジュウナナヘイホーメートルです。",
        );
    }
    // 選択されなかった既知の全体語でも、独自登録があれば認定を辞退する。
    let mut whole = entry(
        "ニジュウナナヘイホーメートル",
        "作品名",
        "名詞,固有名詞,一般,*",
        "ベツヨミ",
        "ベツヨミ",
    );
    whole.cost = 1000;
    assert_kept_against_name_context(
        &mut fixture(Some(whole)),
        "面積はニジュウナナヘイホーメートルです。",
    );
}

#[test]
fn ascii_sentence_boundaries_admit_japanese_text_and_protect_alphanumeric_ids() {
    let mut analyzer = fixture(None);
    for (prefix, tail) in [
        ("前文.", "次の文。"),
        ("かな.", "かな。"),
        ("カナ.", "カナ。"),
        ("ｶﾅ.", "ｶﾅ。"),
    ] {
        let text = format!("{prefix}面積はニジュウナナヘイホーメートルです.{tail}");
        let tokens = analyzer.try_tokenize(&text).unwrap();
        assert_eq!(
            focal(&tokens, "ニジュウナナ").pos.as_ref(),
            "名詞,数,*,*",
            "{text}"
        );
        assert_eq!(
            focal(&tokens, "ヘイホーメートル").base_form.as_ref(),
            "平方メートル"
        );
    }
    for prefix in ["A.", "Ａ.", "1.", "１."] {
        assert_kept_against_name_context(
            &mut analyzer,
            &format!("{prefix}面積はニジュウナナヘイホーメートルです。"),
        );
    }
    for tail in ["A", "Ａ", "1", "１", "_12"] {
        assert_kept_against_name_context(
            &mut analyzer,
            &format!("面積はニジュウナナヘイホーメートルです.{tail}"),
        );
    }
}

#[test]
fn unit_cost_saturates_once_and_offsets_survive_repeated_frames() {
    let mut expensive = entry(
        "メートル",
        "メートル",
        "名詞,接尾,助数詞,*",
        "メートル",
        "メートル",
    );
    expensive.cost = -32000;
    let mut analyzer = fixture(Some(expensive));
    let text = "面積はニジュウナナヘイホーメートルです。";
    assert_eq!(
        focal(&analyzer.try_tokenize(text).unwrap(), "ヘイホーメートル").word_cost,
        i16::MIN
    );
    let block = format!("「前。{text}」。{text}");
    let input = block.repeat(64);
    let tokens = analyzer.try_tokenize(&input).unwrap();
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.surface.as_ref() == "ヘイホーメートル")
            .count(),
        64
    );
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.surface.as_ref() == "ヘイホー")
            .count(),
        64
    );
    for token in &tokens {
        assert_eq!(
            input.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
    let sentences = analyzer
        .try_tokenize_sentences(&input, &Default::default())
        .unwrap();
    let from_sentences: Vec<_> = sentences
        .into_iter()
        .flat_map(|(_, tokens)| tokens)
        .collect();
    assert_eq!(format!("{tokens:?}"), format!("{from_sentences:?}"));
    let mut out = analyzer.try_tokenize("名前").unwrap();
    let previous = format!("{out:?}");
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    workspace
        .tokenize_into(text, analyzer.dictionary(), 71, &mut out)
        .unwrap();
    assert_eq!(format!("{:?}", &out[..1]), previous);
    let mut expected = analyzer.try_tokenize(text).unwrap();
    for token in &mut expected {
        token.start += 71;
        token.end += 71;
    }
    assert_eq!(format!("{:?}", &out[1..]), format!("{expected:?}"));
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_katakana_area_has_numeric_features_without_guessing_unknown_ipa_words() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        let text = "面積はニジュウナナヘイホーメートルです。";
        let tokens = analyzer.try_tokenize(text).unwrap();
        if name == "ipadic" {
            let unknown = focal(&tokens, "ニジュウナナヘイホーメートル");
            assert!(!unknown.is_known);
            assert_eq!(unknown.pos.as_ref(), "名詞,一般,*,*");
            assert_eq!(unknown.base_form, unknown.surface);
            assert_eq!(unknown.pronunciation, unknown.surface);
        } else {
            let number = focal(&tokens, "ニジュウナナ");
            assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
            assert_eq!(number.base_form.as_ref(), "27");
            assert_eq!(number.pronunciation.as_ref(), "ニジューナナ");
            assert!(!number.is_known);
            let unit = focal(&tokens, "ヘイホーメートル");
            assert_eq!(unit.base_form.as_ref(), "平方メートル");
            assert_eq!(unit.pos.as_ref(), "名詞,接尾,助数詞,*");
            assert_eq!(unit.reading.as_ref(), "ヘイホウメートル");
            assert_eq!(unit.pronunciation.as_ref(), "ヘイホーメートル");
            assert!(!unit.is_known);
            assert_eq!(
                analyzer
                    .quantity_pronunciation("ニジュウナナヘイホーメートル")
                    .unwrap(),
                None
            );
        }
        assert!(analyzer.quantity_spans(text).unwrap().is_empty());
        assert!(analyzer.contextual_reading_spans(text).unwrap().is_empty());
        for token in &tokens {
            assert_eq!(
                text.get(token.start..token.end),
                Some(token.surface.as_ref())
            );
        }
        for text in [
            "ニジュウナナヘイホーメートル",
            "名前はニジュウナナヘイホーメートルです。",
            "「前。面積はニジュウナナヘイホーメートルです。」",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            if name != "ipadic" {
                assert!(focal(&tokens, "ニジュウナナ").is_known);
                assert_eq!(focal(&tokens, "ヘイホー").base_form.as_ref(), "Hey Ho");
            }
        }
        for text in [
            "面積は二十七平方メートルです。",
            "面積は27平方メートルです。",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(focal(&tokens, "平方メートル").is_known);
            assert!(
                !tokens
                    .iter()
                    .any(|t| t.surface.as_ref() == "ヘイホーメートル")
            );
        }
    }
}
