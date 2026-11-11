//! 非標準の表記ゆれが助詞・活用語を取り込まないことを配布辞書で確かめる。
#![cfg(feature = "build")]

use hasami::Analyzer;

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_grammatical_variants_keep_particles_and_inflections() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, forbidden, required) in [
            ("「遠景」がよく見える。", "がよく", &["が", "よく"][..]),
            ("見ていただくと助かる。", "だくと", &["いただく", "と"][..]),
            (
                "箱の長さとは無関係だ。",
                "長さと",
                &["長さ", "と", "は"][..],
            ),
            (
                "昨夜あの店にいたことを思い出した。",
                "いたこと",
                &["い", "た", "こと"][..],
            ),
            (
                "参加してきた方に尋ねる。",
                "きた方",
                &["き", "た", "方"][..],
            ),
            ("一体どう進めるのか。", "体どう", &["一体", "どう"][..]),
            ("うちには時計がある。", "うちに", &["うち", "に", "は"][..]),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let surfaces: Vec<_> = tokens.iter().map(|t| t.surface.as_ref()).collect();
            assert!(!surfaces.contains(&forbidden), "{name}: {text}: {tokens:?}");
            assert!(
                surfaces
                    .windows(required.len())
                    .any(|part| part == required),
                "{name}: {text}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }

        // かな・混ぜ書きだけを除き、標準表記と異なる正しい読みを保持する。
        for (surface, readings) in [
            ("我欲", &["ガヨク"][..]),
            ("ダクト", &["ダクト"][..]),
            ("痛事", &["イタゴト"][..]),
            ("長里", &["ナガサト"][..]),
            ("北方", &["ホッポウ", "キタガタ", "キタカタ", "ボツケ"][..]),
            ("打荷", &["ウチニ"][..]),
        ] {
            let entries: Vec<_> = analyzer
                .dictionary()
                .lookup(surface)
                .unwrap()
                .into_iter()
                .filter(|(end, _)| *end == surface.len())
                .flat_map(|(_, rows)| rows)
                .collect();
            for reading in readings {
                assert!(
                    entries
                        .iter()
                        .any(|entry| entry.reading.as_ref() == *reading),
                    "{name}: {surface}: {reading}: {entries:?}"
                );
            }
        }
        // 体動は Sudachi 由来なので、収録されている辞書で保持を検査する。
        if name == "ipadic-neologd-sudachi" {
            assert!(
                analyzer
                    .dictionary()
                    .lookup("体動")
                    .unwrap()
                    .iter()
                    .filter(|(end, _)| *end == "体動".len())
                    .flat_map(|(_, entries)| entries)
                    .any(|entry| entry.reading.as_ref() == "タイドウ")
            );
        }
    }
}
