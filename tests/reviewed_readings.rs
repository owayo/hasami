//! 配布辞書の読み・発音と、同形の名前・識別子の保持。
#![cfg(feature = "build")]

use hasami::Analyzer;

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_common_and_technical_readings_keep_valid_alternatives() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, reading, pronunciation) in [
            ("不可逆", "フカギャク", "フカギャク"),
            ("世界共通", "セカイキョウツウ", "セカイキョーツー"),
            ("一点集中", "イッテンシュウチュウ", "イッテンシューチュー"),
            ("給料泥棒", "キュウリョウドロボウ", "キューリョードロボー"),
            ("大怪我", "オオケガ", "オオケガ"),
            ("下準備", "シタジュンビ", "シタジュンビ"),
            ("線引き", "センビキ", "センビキ"),
            ("pptx", "ピーピーティーエックス", "ピーピーティーエックス"),
            (
                "Creative Commons",
                "クリエイティブコモンズ",
                "クリエイティブコモンズ",
            ),
            ("Azure", "アジュール", "アジュール"),
            ("有機EL", "ユウキイーエル", "ユーキイーエル"),
            ("VOICEVOX", "ボイスボックス", "ボイスボックス"),
            (
                "Software as a Service",
                "ソフトウェアアズアサービス",
                "ソフトウェアアズアサービス",
            ),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_eq!(
                tokens.iter().map(|t| &*t.reading).collect::<String>(),
                reading,
                "{name}: {text}: {tokens:?}"
            );
            assert_eq!(
                tokens.iter().map(|t| &*t.pronunciation).collect::<String>(),
                pronunciation,
                "{name}: {text}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
        for (surface, reading) in [
            ("Azure", "アズレ"),
            ("BMW", "バトルムーンウォーズ"),
            ("堪能", "カンノウ"),
            ("百本", "ヒャクモト"),
        ] {
            assert!(
                analyzer
                    .dictionary()
                    .lookup(surface)
                    .unwrap()
                    .iter()
                    .filter(|(length, _)| *length == surface.len())
                    .flat_map(|(_, entries)| entries)
                    .any(|entry| entry.reading.as_ref() == reading),
                "{name}: {surface}: {reading}"
            );
        }
        let tokens = analyzer.try_tokenize("Azureさん").unwrap();
        assert_eq!(tokens[0].reading.as_ref(), "アズレ", "{name}: {tokens:?}");
    }
}

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_counter_readings_preserve_offsets_and_nonquantity_names() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (quantity, reading, pronunciation) in [
            ("1つ", "ヒトツ", "ヒトツ"),
            ("二口", "フタクチ", "フタクチ"),
            ("第一章", "ダイイッショウ", "ダイイッショー"),
            ("6頭", "ロクトウ", "ロクトー"),
            ("六曲", "ロッキョク", "ロッキョク"),
            ("8通", "ハッツウ", "ハッツー"),
            ("1週間", "イッシュウカン", "イッシューカン"),
            ("6週間", "ロクシュウカン", "ロクシューカン"),
            ("1点", "イッテン", "イッテン"),
            ("６点", "ロクテン", "ロクテン"),
            ("1か月", "イッカゲツ", "イッカゲツ"),
            ("十階", "ジュッカイ", "ジュッカイ"),
            ("十足", "ジュッソク", "ジュッソク"),
            ("十歳", "ジュッサイ", "ジュッサイ"),
            ("十才", "ジュッサイ", "ジュッサイ"),
            (
                "二〇四二年",
                "ニセンヨンジュウニネン",
                "ニセンヨンジューニネン",
            ),
            ("六〇〇円", "ロッピャクエン", "ロッピャクエン"),
            ("三〇〇本", "サンビャッポン", "サンビャッポン"),
        ] {
            let text = format!("{quantity}を確認した。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let actual = |pronunciation: bool| -> String {
                tokens
                    .iter()
                    .take_while(|t| t.end <= quantity.len())
                    .map(|t| {
                        if pronunciation {
                            t.pronunciation.as_ref()
                        } else {
                            t.reading.as_ref()
                        }
                    })
                    .collect()
            };
            assert_eq!(actual(false), reading, "{name}: {text}: {tokens:?}");
            assert_eq!(actual(true), pronunciation, "{name}: {text}: {tokens:?}");
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
        for text in ["A1つ", "ID1点", "v1.2章"] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                tokens
                    .iter()
                    .all(|t| !matches!(t.reading.as_ref(), "ヒト" | "イッ")),
                "{name}: {text}: {tokens:?}"
            );
        }
    }
}

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_reviewed_readings_keep_names_and_ascii_identifiers() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (word, reading, pronunciation) in [
            ("近日中", "キンジツチュウ", "キンジツチュー"),
            ("正典", "セイテン", "セイテン"),
            ("章", "ショウ", "ショー"),
            ("COO", "シーオーオー", "シーオーオー"),
            ("DEC", "ディーイーシー", "ディーイーシー"),
            ("BMW", "ビーエムダブリュー", "ビーエムダブリュー"),
            ("ppm", "ピーピーエム", "ピーピーエム"),
            ("粗利率", "アラリリツ", "アラリリツ"),
            ("手水舎", "テミズヤ", "テミズヤ"),
            ("小売店", "コウリテン", "コウリテン"),
            ("音信不通", "オンシンフツウ", "オンシンフツー"),
            ("センテンス", "センテンス", "センテンス"),
            ("韓信", "カンシン", "カンシン"),
            ("毛利氏", "モウリシ", "モーリシ"),
            ("北条氏", "ホウジョウシ", "ホージョーシ"),
            ("Language", "ランゲージ", "ランゲージ"),
            ("ＳＮＳ", "エスエヌエス", "エスエヌエス"),
        ] {
            let text = format!("{word}の説明を調べる。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let token = &tokens[0];
            assert_eq!(&*token.surface, word, "{name}: {text}: {tokens:?}");
            assert_eq!(&*token.reading, reading, "{name}: {text}");
            assert_eq!(&*token.pronunciation, pronunciation, "{name}: {text}");
            assert!(token.is_known, "{name}: {text}");
            assert_eq!(&*tokens[1].surface, "の", "{name}: {text}");
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
        for (text, word, reading) in [
            ("山田正典さんが来た。", "正典", "タダノリ"),
            ("田中章さんを紹介した。", "田中章", "タナカアキラ"),
            ("ひとはさんに聞く。", "ひとは", "ヒトハ"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens.iter().find(|t| t.surface.as_ref() == word).unwrap();
            assert_eq!(&*token.reading, reading, "{name}: {text}: {tokens:?}");
            assert!(token.pos.starts_with("名詞,固有名詞,人名"));
        }
        let tokens = analyzer.try_tokenize("ひとは考える。").unwrap();
        assert_eq!(&*tokens[0].surface, "ひと", "{name}: {tokens:?}");
        assert_eq!(&*tokens[1].surface, "は", "{name}: {tokens:?}");
        assert_eq!(&*tokens[1].pronunciation, "ワ", "{name}: {tokens:?}");
        for (year, reading) in [
            ("1997", "センキュウヒャクキュウジュウナナネン"),
            ("1943", "センキュウヒャクヨンジュウサンネン"),
        ] {
            let text = format!("{year}年の記録。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let actual: String = tokens
                .iter()
                .take_while(|t| t.end <= year.len() + "年".len())
                .map(|t| t.reading.as_ref())
                .collect();
            assert_eq!(actual, reading, "{name}: {tokens:?}");
        }
        // 有効な別読みを、通常の読みの優先とは別に辞書内に保持する。
        for (word, reading) in [
            ("音信不通", "インシンフツウ"),
            ("DEC", "ダークエルブンケイブ"),
            ("BMW", "バトルムーンウォーズ"),
            ("COO", "クー"),
            ("ppm", "ピーターポールエンドメアリー"),
        ] {
            assert!(
                analyzer
                    .dictionary()
                    .lookup(word)
                    .unwrap()
                    .iter()
                    .filter(|(length, _)| *length == word.len())
                    .flat_map(|(_, entries)| entries)
                    .any(|entry| entry.reading.as_ref() == reading),
                "{name}: {word}: {reading}"
            );
        }
        for text in ["濃度は10ppmです。", "五万ppmの濃度。"] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens.iter().find(|t| t.surface.as_ref() == "ppm").unwrap();
            assert_eq!(&*token.reading, "ピーピーエム", "{name}: {text}");
            assert_eq!(&*token.pos, "名詞,接尾,助数詞,*");
        }
        for text in [
            "DECQzx",
            "COOQzx",
            "BMWQzx",
            "ppmQzx",
            "DECISION",
            "COOPERATIVE",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_eq!(tokens.len(), 1, "{name}: {text}: {tokens:?}");
            assert_eq!(&*tokens[0].surface, text);
            assert!(!tokens[0].is_known);
        }
        for (text, surface, reading) in [
            ("図9.2を調べる。", "9.2", "キュウテンニ"),
            ("割合は13.2%です。", "13.2", "ジュウサンテンニ"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens
                .iter()
                .find(|t| t.surface.as_ref() == surface)
                .unwrap();
            assert_eq!(&*token.reading, reading, "{name}: {text}: {tokens:?}");
        }
    }
}
