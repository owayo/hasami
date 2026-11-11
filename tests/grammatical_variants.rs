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
            ("同じく別室で待っている。", "く別", &["同じく", "別室"][..]),
            (
                "ここにいた事を思い出す。",
                "いた事",
                &["い", "た", "事"][..],
            ),
            ("すごく楽になった。", "ごく楽", &["すごく", "楽"][..]),
            ("念押しして確認する。", "念押", &["念", "押し", "し"][..]),
            ("入ろうと思う。", "入ろう", &["入ろ", "う"][..]),
            ("どうしたらいいだろう。", "らいい", &["たら", "いい"][..]),
            ("もういいです。", "ういい", &["もう", "いい"][..]),
            ("提示した手を使う。", "した手", &["し", "た", "手"][..]),
            ("安定した打率を保つ。", "した打", &["し", "た", "打率"][..]),
            ("美しく心を表す。", "く心", &["美しく", "心"][..]),
            ("ウッド用の塗料を選ぶ。", "ド用", &["ウッド", "用"][..]),
            ("会話がない部室にいる。", "ない部", &["ない", "部室"][..]),
            ("弟が残した書を読む。", "した書", &["残し", "た", "書"][..]),
            ("小さな船で島へ渡る。", "船で", &["船", "で"][..]),
            ("平易な言葉に直す。", "平易な", &["平易", "な"][..]),
            ("赤か黒かで選ぶ。", "か黒か", &["赤", "か", "黒", "か"][..]),
            ("白か青かを比べる。", "白か", &["白", "か", "青", "か"][..]),
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
            ("異体", &["イタイ"][..]),
            ("露骨", &["ロコツ"][..]),
            ("着意", &["チャクイ"][..]),
            ("区別", &["クベツ"][..]),
            ("多産", &["タサン"][..]),
            ("下手", &["シタテ", "ヘタ"][..]),
            ("極楽", &["ゴクラク"][..]),
            ("舌打ち", &["シタウチ"][..]),
            ("入牢", &["ニュウロウ"][..]),
            ("苦心", &["クシン"][..]),
            ("土用", &["ドヨウ"][..]),
            ("来意", &["ライイ"][..]),
            ("内部", &["ナイブ"][..]),
            ("出迎え", &["デムカエ"][..]),
            ("徒長", &["トチョウ"][..]),
            ("船出", &["フナデ"][..]),
            ("同視", &["ドウシ"][..]),
            ("下書き", &["シタガキ"][..]),
            ("着い", &["ツイ"][..]),
            ("赤かっ", &["アカカッ"][..]),
            ("白かっ", &["シロカッ"][..]),
            ("青かっ", &["アオカッ"][..]),
            ("黒かっ", &["クロカッ"][..]),
            ("湊や", &["ミナトヤ"][..]),
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
