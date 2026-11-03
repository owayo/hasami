//! 数量が文中に繰り返し現れる場合の読みと、元入力の境界の受け入れ検査。
#![cfg(feature = "analyzer")]

use hasami::{Analyzer, Token};

fn assert_reading(text: &str, tokens: &[Token], surface: &str, reading: &str, speech: &str) {
    let start = text.find(surface).unwrap();
    let end = start + surface.len();
    let focal: Vec<_> = tokens
        .iter()
        .filter(|t| t.start < end && start < t.end)
        .collect();
    assert_eq!(
        focal.first().map(|t| t.start),
        Some(start),
        "{text}: {tokens:?}"
    );
    assert_eq!(focal.last().map(|t| t.end), Some(end), "{text}: {tokens:?}");
    assert_eq!(
        focal.iter().map(|t| &*t.reading).collect::<String>(),
        reading,
        "{text}: {tokens:?}"
    );
    assert_eq!(
        focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
        speech,
        "{text}: {tokens:?}"
    );
    assert!(focal.windows(2).all(|p| p[0].end == p[1].start));
    for token in tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
}

#[test]
#[ignore = "配布辞書2種が必要"]
fn distributed_counter_sentences_keep_tens_distinct_from_six_hundred_and_thousand() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            let text = "建物は10階で、資料は10冊ある。靴は10足で、年齢は10歳だ。年齢は10才だ。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading) in [
                ("10階", "ジュッカイ"),
                ("10冊", "ジュッサツ"),
                ("10足", "ジュッソク"),
                ("10歳", "ジュッサイ"),
                ("10才", "ジュッサイ"),
            ] {
                assert_reading(text, &tokens, surface, reading, reading);
            }
            let text = "資料は100冊で、靴は1000足ある。6冊と6足を数える。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading) in [
                ("100冊", "ヒャクサツ"),
                ("1000足", "センソク"),
                ("6冊", "ロクサツ"),
                ("6足", "ロクソク"),
            ] {
                assert_reading(text, &tokens, surface, reading, reading);
            }
            let text = "10.6冊とID10冊を区別する。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_reading(
                text,
                &tokens,
                "10.6冊",
                "ジュウテンロクサツ",
                "ジューテンロクサツ",
            );
            let id_start = text.find("ID10冊").unwrap();
            assert!(
                tokens
                    .iter()
                    .filter(|t| id_start <= t.start)
                    .all(|t| t.reading.as_ref() != "ジュッ")
            );
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_fullwidth_quantities_keep_original_surfaces_and_identifier_guards() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            let text = "年齢は１８歳で、回数は２０回だ。次は２４歳だ。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading, speech) in [
                ("１８歳", "ジュウハッサイ", "ジューハッサイ"),
                ("２０回", "ニジュッカイ", "ニジュッカイ"),
                ("２４歳", "ニジュウヨンサイ", "ニジューヨンサイ"),
            ] {
                assert_reading(text, &tokens, surface, reading, speech);
                let number = surface.trim_end_matches(['歳', '回']);
                let token = tokens
                    .iter()
                    .find(|t| t.surface.as_ref() == number)
                    .unwrap();
                assert_eq!(token.base_form.as_ref(), number);
                assert_eq!(token.pos.as_ref(), "名詞,数,*,*");
                assert!(!token.is_known);
                assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
            }
            // 数量ではない入力を挟んでも、次の呼出しへ保護範囲を持ち越さない。
            for text in ["０１８歳", "ID１８歳", "１８歳A", "１８", "1８歳", "一８歳"]
            {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert!(
                    tokens
                        .iter()
                        .all(|t| !matches!(t.surface.as_ref(), "１８" | "０１８" | "1８" | "一８")),
                    "{name}: {text}: {tokens:?}"
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
}
