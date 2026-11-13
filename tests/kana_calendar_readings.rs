//! 読み上げ前の正規化が出すかな表記の暦月・暦日と、助詞を取り込む登録の不在。
#![cfg(feature = "analyzer")]

use hasami::Analyzer;

fn analyzer(name: &str) -> Analyzer {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    Analyzer::load(format!("{dir}/{name}.hsd")).unwrap()
}

/// かな表記の暦月は、助詞や動詞に割れず 1 語の副詞可能名詞になる。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_kana_calendar_months_stay_single_nouns() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (surface, reading, pronunciation) in [
            ("いちがつ", "イチガツ", "イチガツ"),
            ("にがつ", "ニガツ", "ニガツ"),
            ("さんがつ", "サンガツ", "サンガツ"),
            ("しがつ", "シガツ", "シガツ"),
            ("ごがつ", "ゴガツ", "ゴガツ"),
            ("ろくがつ", "ロクガツ", "ロクガツ"),
            ("しちがつ", "シチガツ", "シチガツ"),
            ("はちがつ", "ハチガツ", "ハチガツ"),
            ("くがつ", "クガツ", "クガツ"),
            ("じゅうがつ", "ジュウガツ", "ジューガツ"),
            ("じゅういちがつ", "ジュウイチガツ", "ジューイチガツ"),
            ("じゅうにがつ", "ジュウニガツ", "ジューニガツ"),
        ] {
            let text = format!("{surface}ふつかに出発する。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let token = &tokens[0];
            assert_eq!(
                token.surface.as_ref(),
                surface,
                "{name}: {text}: {tokens:?}"
            );
            assert_eq!(token.pos.as_ref(), "名詞,副詞可能,*,*", "{name}: {text}");
            assert_eq!(token.reading.as_ref(), reading, "{name}: {text}");
            assert_eq!(
                token.pronunciation.as_ref(),
                pronunciation,
                "{name}: {text}"
            );
        }
    }
}

/// かな表記の暦日は、形容詞の活用形や人名に割れず 1 語の副詞可能名詞になる。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_kana_calendar_days_stay_single_nouns() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (surface, reading, pronunciation) in [
            ("ついたち", "ツイタチ", "ツイタチ"),
            ("ふつか", "フツカ", "フツカ"),
            ("みっか", "ミッカ", "ミッカ"),
            ("よっか", "ヨッカ", "ヨッカ"),
            ("いつか", "イツカ", "イツカ"),
            ("むいか", "ムイカ", "ムイカ"),
            ("ようか", "ヨウカ", "ヨーカ"),
            ("ここのか", "ココノカ", "ココノカ"),
            ("とおか", "トオカ", "トーカ"),
            ("はつか", "ハツカ", "ハツカ"),
        ] {
            let text = format!("くがつ{surface}に放送された。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let token = &tokens[1];
            assert_eq!(
                token.surface.as_ref(),
                surface,
                "{name}: {text}: {tokens:?}"
            );
            assert_eq!(token.pos.as_ref(), "名詞,副詞可能,*,*", "{name}: {text}");
            assert_eq!(token.reading.as_ref(), reading, "{name}: {text}");
            assert_eq!(
                token.pronunciation.as_ref(),
                pronunciation,
                "{name}: {text}"
            );
        }
    }
}

/// 助詞と自立語をまとめる生成エントリを除いても、同じかなの正しい語形は残る。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_particle_absorbing_entries_are_removed() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (text, absorbed, expected) in [
            ("関節があり上関節面を形作る。", "があ", vec!["が", "あり"]),
            (
                "横紋ともいい、骨格筋に見られる。",
                "ともいい",
                vec!["とも", "いい"],
            ),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let surfaces: Vec<&str> = tokens.iter().map(|t| t.surface.as_ref()).collect();
            assert!(
                !surfaces.contains(&absorbed),
                "{name}: {text}: {absorbed} must not be one token: {tokens:?}"
            );
            assert!(
                surfaces.windows(expected.len()).any(|w| w == &expected[..]),
                "{name}: {text}: {expected:?} must stay separate: {tokens:?}"
            );
        }
        // 正しい活用形と、助動詞を伴う文末の形は残す。
        let tokens = analyzer.try_tokenize("強うございます。").unwrap();
        assert_eq!(tokens[0].surface.as_ref(), "強う", "{name}: {tokens:?}");
        assert_eq!(tokens[0].pos.as_ref(), "形容詞,自立,*,*", "{name}");
        let tokens = analyzer.try_tokenize("そうなのか、よく分かった。").unwrap();
        let surfaces: Vec<&str> = tokens.iter().map(|t| t.surface.as_ref()).collect();
        assert_eq!(
            &surfaces[..4],
            &["そう", "な", "の", "か"],
            "{name}: {tokens:?}"
        );
    }
}

/// 解説文に出る語の読みが空のままにならない。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_explanatory_terms_have_readings() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (text, surface, reading, pronunciation) in [
            (
                "単純閉曲線を疆界とする。",
                "疆界",
                "キョウカイ",
                "キョーカイ",
            ),
            ("上関節窩と下関節窩がある。", "窩", "カ", "カ"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens
                .iter()
                .find(|t| t.surface.as_ref() == surface)
                .unwrap_or_else(|| panic!("{name}: {text}: {tokens:?}"));
            assert_eq!(token.reading.as_ref(), reading, "{name}: {text}");
            assert_eq!(
                token.pronunciation.as_ref(),
                pronunciation,
                "{name}: {text}"
            );
        }
    }
}
