//! 単語単体では見落とす読みの欠落・展開と、活用語をまたぐ誤登録の回帰。
#![cfg(feature = "analyzer")]

use hasami::Analyzer;

#[test]
#[ignore = "配布辞書2種が必要"]
fn distributed_lexical_readings_survive_independent_sentence_contexts() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, surface, reading, pronunciation) in [
            (
                "整備士がBMWの車両を点検する。",
                "BMW",
                "ビーエムダブリュー",
                "ビーエムダブリュー",
            ),
            (
                "画面には有機ELを採用する。",
                "有機EL",
                "ユウキイーエル",
                "ユーキイーエル",
            ),
            (
                "担当者が中央労働災害防止協会に問い合わせる。",
                "中央労働災害防止協会",
                "チュウオウロウドウサイガイボウシキョウカイ",
                "チューオーロードーサイガイボーシキョーカイ",
            ),
        ] {
            let start = text.find(surface).unwrap();
            let end = start + surface.len();
            let tokens = analyzer.try_tokenize(text).unwrap();
            let focal: Vec<_> = tokens
                .iter()
                .filter(|token| token.start < end && start < token.end)
                .collect();
            assert_eq!(
                focal.first().map(|t| t.start),
                Some(start),
                "{name}: {tokens:?}"
            );
            assert_eq!(focal.last().map(|t| t.end), Some(end), "{name}: {tokens:?}");
            assert!(focal.iter().all(|t| t.is_known), "{name}: {tokens:?}");
            assert_eq!(
                focal.iter().map(|t| &*t.reading).collect::<String>(),
                reading,
                "{name}: {tokens:?}"
            );
            assert_eq!(
                focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
                pronunciation,
                "{name}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
            assert!(tokens.windows(2).all(|pair| pair[0].end == pair[1].start));
            assert_eq!(tokens.last().unwrap().end, text.len());
        }
        // 読みが埋まるだけでなく、形容詞と後続語の境界も残ること。
        for (text, surface, base, pos) in [
            ("高さが低そうだ。", "低", "低い", "形容詞,"),
            ("計算した値を比べる。", "た", "た", "助動詞,"),
            ("覚えたい事を並べる。", "たい", "たい", "助動詞,"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                tokens.iter().any(|token| {
                    token.surface.as_ref() == surface
                        && token.base_form.as_ref() == base
                        && token.pos.starts_with(pos)
                }),
                "{name}: {text}: {tokens:?}"
            );
        }
        // 車名の通常読みを優先しても、正当な作品名の候補を削除しない。
        assert!(
            analyzer
                .dictionary()
                .lookup("BMW")
                .unwrap()
                .iter()
                .filter(|(end, _)| *end == "BMW".len())
                .flat_map(|(_, entries)| entries)
                .any(|entry| entry.reading.as_ref() == "バトルムーンウォーズ")
        );
    }
}
