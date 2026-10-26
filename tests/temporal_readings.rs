//! 配布辞書の時間表現と、同形の「先々（サキザキ）」の読み。
#![cfg(feature = "build")]

use hasami::Analyzer;

#[test]
#[ignore = "配布辞書が必要"]
fn test_distributed_temporal_expressions_preserve_sakizaki() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (word, reading, pronunciation) in [
            ("先々週", "センセンシュウ", "センセンシュー"),
            ("先々月", "センセンゲツ", "センセンゲツ"),
            ("先々年", "センセンネン", "センセンネン"),
            ("先週", "センシュウ", "センシュー"),
            ("先月", "センゲツ", "センゲツ"),
            ("先年", "センネン", "センネン"),
        ] {
            for (text, start) in [
                (word.to_owned(), 0),
                (format!("{word}の予定を確認する。"), 0),
                (format!("会議は{word}に開かれた。"), "会議は".len()),
            ] {
                let tokens = analyzer.try_tokenize(&text).unwrap();
                let token = tokens.iter().find(|t| t.start == start).unwrap();
                assert_eq!(token.surface.as_ref(), word, "{name}: {text}: {tokens:?}");
                assert_eq!(token.end, start + word.len(), "{name}: {text}");
                assert_eq!(token.base_form.as_ref(), word, "{name}: {text}");
                assert_eq!(token.pos.as_ref(), "名詞,副詞可能,*,*", "{name}: {text}");
                assert_eq!(token.reading.as_ref(), reading, "{name}: {text}");
                assert_eq!(
                    token.pronunciation.as_ref(),
                    pronunciation,
                    "{name}: {text}"
                );
                assert!(token.is_known, "{name}: {text}");
                for token in &tokens {
                    assert_eq!(&text[token.start..token.end], token.surface.as_ref());
                }
            }
        }
        for (text, start) in [
            ("先々", 0),
            ("先々のことを考える。", 0),
            ("今から先々のことを考える。", "今から".len()),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens.iter().find(|t| t.start == start).unwrap();
            assert_eq!(token.surface.as_ref(), "先々", "{name}: {text}");
            assert_eq!(token.reading.as_ref(), "サキザキ", "{name}: {text}");
            assert_eq!(token.pronunciation.as_ref(), "サキザキ", "{name}: {text}");
        }
    }
}
