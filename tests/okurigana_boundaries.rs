//! 送り仮名を省いた別表記が、次の動詞の読みを重複させないことを確かめる。
use hasami::{Analyzer, Dictionary};

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_okoshi_preserves_the_noun_and_separate_kosu_inflections() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let dictionary = Dictionary::load(format!("{dir}/{name}.hsd")).unwrap();
        let entries = dictionary.lookup("お越し").unwrap();
        let canonical = entries
            .iter()
            .filter(|(end, _)| *end == "お越し".len())
            .flat_map(|(_, entries)| entries)
            .find(|entry| {
                entry.base_form.as_ref() == "お越し"
                    && entry.pos.as_ref() == "名詞,一般,*,*"
                    && entry.reading.as_ref() == "オコシ"
                    && entry.pronunciation.as_ref() == "オコシ"
            })
            .unwrap();
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for text in [
            "お越しください。",
            "またお越しください。",
            "お越しになる。",
            "お越しを待つ。",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let start = text.find("お越し").unwrap();
            let end = start + "お越し".len();
            let focal: Vec<_> = tokens
                .iter()
                .filter(|t| t.start < end && start < t.end)
                .collect();
            assert_eq!(focal.first().unwrap().start, start, "{name}: {text}");
            assert_eq!(focal.last().unwrap().end, end, "{name}: {text}");
            assert!(focal.windows(2).all(|pair| pair[0].end == pair[1].start));
            assert_eq!(
                focal.iter().map(|t| t.reading.as_ref()).collect::<String>(),
                "オコシ",
                "{name}: {text}: {tokens:?}"
            );
            assert_eq!(
                focal
                    .iter()
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                "オコシ",
                "{name}: {text}: {tokens:?}"
            );
            assert!(focal.iter().all(|t| t.is_known));
            // 「お」＋動詞「越し」の正しい分割も残す。
            if focal.len() == 1 {
                let token = focal[0];
                assert_eq!(token.surface.as_ref(), "お越し");
                assert_eq!(token.reading, canonical.reading);
                assert_eq!(token.pronunciation, canonical.pronunciation);
                assert_eq!(token.pos, canonical.pos);
                assert_eq!(token.word_cost, canonical.cost);
                assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
            } else {
                assert_eq!(focal.len(), 2);
                assert_eq!(focal[0].surface.as_ref(), "お");
                assert_eq!(focal[0].pos.as_ref(), "接頭詞,名詞接続,*,*");
                assert_eq!(focal[1].surface.as_ref(), "越し");
                assert_eq!(focal[1].base_form.as_ref(), "越す");
                assert_eq!(focal[1].conj_form.as_ref(), "連用形");
            }
            if text.ends_with("ください。") {
                let reading: String = tokens.iter().map(|t| t.reading.as_ref()).collect();
                assert!(
                    reading.ends_with("オコシクダサイ。"),
                    "{name}: {text}: {reading}"
                );
            }
        }
        for (text, reading, base, pos, kind, form) in [
            (
                "川を越して進む。",
                "コシ",
                "越す",
                "動詞,自立,*,*",
                "五段・サ行",
                "連用形",
            ),
            (
                "三年越しの計画。",
                "ゴシ",
                "越し",
                "名詞,接尾,一般,*",
                "",
                "",
            ),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens
                .iter()
                .find(|t| t.surface.as_ref() == "越し")
                .unwrap();
            assert_eq!(token.reading.as_ref(), reading, "{name}: {text}");
            assert_eq!(token.pronunciation.as_ref(), reading);
            assert_eq!(token.base_form.as_ref(), base);
            assert_eq!(token.pos.as_ref(), pos);
            assert_eq!(token.conj_type.as_ref(), kind);
            assert_eq!(token.conj_form.as_ref(), form);
            assert!(token.is_known);
        }
    }
}
