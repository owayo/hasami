use hasami::Analyzer;

#[test]
#[ignore = "NEologd を含む配布辞書が必要"]
fn distributed_science_terms_are_whole_words() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        let cases = [
            (
                "赤外光を測定する。",
                "赤外光",
                "セキガイコウ",
                "セキガイコー",
            ),
            (
                "装置が赤外光を検出した。",
                "赤外光",
                "セキガイコウ",
                "セキガイコー",
            ),
            ("火の炎心を観察する。", "炎心", "エンシン", "エンシン"),
            ("炎心の色を見る。", "炎心", "エンシン", "エンシン"),
            ("赤と青を混ぜる。", "赤", "アカ", "アカ"),
            ("ろうそくの炎が揺れる。", "炎", "ホノオ", "ホノオ"),
            ("人の心を読む。", "心", "ココロ", "ココロ"),
            (
                "赤外線を検出する。",
                "赤外線",
                "セキガイセン",
                "セキガイセン",
            ),
        ];
        let gaikou = (name == "ipadic-neologd-sudachi").then_some((
            "外光が窓から入る。",
            "外光",
            "ガイコウ",
            "ガイコー",
        ));
        for (text, surface, reading, pronunciation) in cases.into_iter().chain(gaikou) {
            let start = text.find(surface).unwrap();
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens
                .iter()
                .find(|t| t.start == start && t.end == start + surface.len())
                .unwrap_or_else(|| panic!("{name}: {text}: {tokens:?}"));
            assert_eq!(token.surface.as_ref(), surface, "{name}: {text}");
            assert_eq!(token.base_form.as_ref(), surface, "{name}: {text}");
            assert_eq!(token.pos.as_ref(), "名詞,一般,*,*", "{name}: {text}");
            assert_eq!(token.reading.as_ref(), reading, "{name}: {text}");
            assert_eq!(
                token.pronunciation.as_ref(),
                pronunciation,
                "{name}: {text}"
            );
            assert!(token.is_known && token.conj_form.is_empty() && token.conj_type.is_empty());
        }
    }
}
