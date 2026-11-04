//! 公開の識別子APIが辞書の断片展開と区別され、既定の解析を変更しないことを確認する。
#![cfg(feature = "analyzer")]

use hasami::Analyzer;

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_numeric_literal_ids_avoid_partial_minus_without_changing_tokens() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for (text, expected) in [
                ("123-4567", "イチニサンヨンゴロクナナ"),
                ("03-1234-5678", "ゼロサンイチニサンヨンゴロクナナハチ"),
                (
                    "090-1234-5678",
                    "ゼロキューゼロイチニサンヨンゴロクナナハチ",
                ),
            ] {
                let before = analyzer.try_tokenize(text).unwrap();
                if name == "ipadic-neologd" && text == "123-4567" {
                    assert!(before.iter().any(|t| t.is_known
                        && t.surface.as_ref() == "-4"
                        && t.pronunciation.as_ref() == "マイナスヨン"));
                }
                assert_eq!(
                    analyzer.normalize_identifier(text).unwrap().as_deref(),
                    Some(expected),
                    "{name}: {text}: {before:?}"
                );
                assert_eq!(
                    format!("{before:?}"),
                    format!("{:?}", analyzer.try_tokenize(text).unwrap())
                );
            }
            for text in [
                "2025-01-09",
                "978-4-1234-5678-0",
                "12-34",
                "123",
                "123-4567+8",
                "１２３-４５６７",
                "123−4567",
            ] {
                let before = analyzer.try_tokenize(text).unwrap();
                assert_eq!(
                    analyzer.normalize_identifier(text).unwrap(),
                    None,
                    "{name}: {text}"
                );
                assert_eq!(
                    format!("{before:?}"),
                    format!("{:?}", analyzer.try_tokenize(text).unwrap())
                );
            }
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_identifiers_keep_selected_english_digits_beside_word_expansions() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for (text, tail) in [
                ("ZXP2D-7Q", "ナナキュー"),
                ("ZXP2D_1B_7Q", "イチビーナナキュー"),
            ] {
                let before = analyzer.try_tokenize(text).unwrap();
                let digit = if name == "ipadic" {
                    "ニ"
                } else {
                    assert!(before.iter().any(|t| t.is_known
                        && t.surface.as_ref() == "2D"
                        && t.pronunciation.as_ref() == "ツーディー"));
                    if text.contains("1B") {
                        assert!(before.iter().any(|t| t.is_known
                            && t.surface.as_ref() == "1B"
                            && t.pronunciation.as_ref() == "ファーストベースマン"));
                    }
                    "ツー"
                };
                let expected = format!("ゼットエックスピー{digit}ディー{tail}");
                assert_eq!(
                    analyzer.normalize_identifier(text).unwrap().as_deref(),
                    Some(expected.as_str()),
                    "{name}: {text}: {before:?}"
                );
                assert_eq!(
                    format!("{before:?}"),
                    format!("{:?}", analyzer.try_tokenize(text).unwrap())
                );
            }
            if name != "ipadic" {
                assert_eq!(
                    analyzer.normalize_identifier("2D").unwrap().as_deref(),
                    Some("2D")
                );
            }
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_literal_identifiers_keep_default_tokens_and_quantity_guards() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for (text, expected) in [
                ("ZX1B-90Q", "ゼットエックスイチビーキューゼロキュー"),
                ("ZX-M4-190Q", "ゼットエックスエムヨンイチキューゼロキュー"),
                (
                    "ISBN978-4-1234-5678-0",
                    "アイエスビーエヌキューナナハチヨンイチニサンヨンゴロクナナハチゼロ",
                ),
            ] {
                let before = analyzer.try_tokenize(text).unwrap();
                // M4 が字母と英語の数字音だけで登録されている場合は、その音を保つ。
                let expected = if text == "ZX-M4-190Q"
                    && before.iter().any(|t| {
                        t.is_known
                            && t.surface.as_ref() == "M4"
                            && t.pronunciation.as_ref() == "エムフォー"
                    }) {
                    "ゼットエックスエムフォーイチキューゼロキュー"
                } else {
                    expected
                };
                assert_eq!(
                    analyzer.normalize_identifier(text).unwrap().as_deref(),
                    Some(expected),
                    "{name}: {text}: {before:?}"
                );
                let after = analyzer.try_tokenize(text).unwrap();
                assert_eq!(
                    format!("{before:?}"),
                    format!("{after:?}"),
                    "{name}: {text}"
                );
                for token in &after {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
            }
            if name == "ipadic-neologd" {
                let before = analyzer.try_tokenize("M4").unwrap();
                assert!(
                    before
                        .iter()
                        .any(|t| t.is_known && t.surface.as_ref() == "M4")
                );
                assert_eq!(
                    analyzer.normalize_identifier("M4").unwrap().as_deref(),
                    Some("M4")
                );
                assert_eq!(
                    format!("{before:?}"),
                    format!("{:?}", analyzer.try_tokenize("M4").unwrap())
                );
            }
            for text in [
                "4.5mph",
                "64bit",
                "1,500kcal",
                "AB12+CD34",
                "https://AB12",
                "ＡＢ12Z",
            ] {
                let before = analyzer.try_tokenize(text).unwrap();
                assert_eq!(
                    analyzer.normalize_identifier(text).unwrap(),
                    None,
                    "{name}: {text}"
                );
                assert_eq!(
                    format!("{before:?}"),
                    format!("{:?}", analyzer.try_tokenize(text).unwrap())
                );
            }
        }
    }
}
