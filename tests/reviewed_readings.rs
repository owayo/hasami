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
            ("瞬く間", "マタタクマ", "マタタクマ"),
            ("敵いません", "カナイマセン", "カナイマセン"),
            ("三十歩神社", "サンジュウブジンジャ", "サンジューブジンジャ"),
            (
                "三十歩子どもまつり",
                "サンジュウブコドモマツリ",
                "サンジューブコドモマツリ",
            ),
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
            ("瞬く", "マバタク"),
            ("等", "ヒトシ"),
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
        let tokens = analyzer.try_tokenize("瞬く間隔").unwrap();
        assert!(
            tokens.iter().any(|token| token.surface.as_ref() == "間隔")
                && !tokens
                    .iter()
                    .any(|token| token.surface.as_ref() == "瞬く間"),
            "{name}: {tokens:?}"
        );
        let tokens = analyzer.try_tokenize("大間違いさ。").unwrap();
        assert_eq!(tokens[0].reading.as_ref(), "オオマチガイ", "{name}");
        // 「さ」の品詞は既存の文脈判定の課題として残し、ここでは読みを検証する。
        assert!(
            tokens.iter().any(|token| {
                token.surface.as_ref() == "さ" && token.reading.as_ref() == "サ"
            }),
            "{name}: {tokens:?}"
        );
        let tokens = analyzer
            .try_tokenize("審判の判定は大間違いで、試合はやり直しになった。")
            .unwrap();
        assert!(
            tokens.iter().any(|token| {
                token.surface.as_ref() == "で"
                    && token.pos.starts_with("助動詞,")
                    && token.base_form.as_ref() == "だ"
                    && token.conj_form.as_ref() == "連用形"
            }),
            "{name}: {tokens:?}"
        );
        for (text, surface, pos) in [
            (
                "日本経済新聞出版社",
                "日本経済新聞出版社",
                "名詞,固有名詞,組織,",
            ),
            ("一言一句", "一言一句", "名詞,一般,"),
            ("Fintechを活用する。", "Fintech", "名詞,一般,"),
            ("Fintech社", "Fintech", "名詞,固有名詞,"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                tokens.iter().any(|token| {
                    token.surface.as_ref() == surface && token.pos.starts_with(pos)
                }),
                "{name}: {text}: {tokens:?}"
            );
        }
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
            ("10階", "ジュッカイ", "ジュッカイ"),
            ("10冊", "ジュッサツ", "ジュッサツ"),
            ("10足", "ジュッソク", "ジュッソク"),
            ("10歳", "ジュッサイ", "ジュッサイ"),
            ("10才", "ジュッサイ", "ジュッサイ"),
            ("１８歳", "ジュウハッサイ", "ジューハッサイ"),
            ("一八か月", "ジュウハッカゲツ", "ジューハッカゲツ"),
            ("二一歳", "ニジュウイッサイ", "ニジューイッサイ"),
            ("4年", "ヨネン", "ヨネン"),
            ("14年", "ジュウヨネン", "ジューヨネン"),
            ("一一月", "ジュウイチガツ", "ジューイチガツ"),
            ("6冊", "ロクサツ", "ロクサツ"),
            ("6足", "ロクソク", "ロクソク"),
            ("6歳", "ロクサイ", "ロクサイ"),
            ("6才", "ロクサイ", "ロクサイ"),
            ("100冊", "ヒャクサツ", "ヒャクサツ"),
            ("1000冊", "センサツ", "センサツ"),
            ("100足", "ヒャクソク", "ヒャクソク"),
            ("1000足", "センソク", "センソク"),
            ("100本", "ヒャッポン", "ヒャッポン"),
            ("1000本", "センボン", "センボン"),
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
            ("1軒", "イッケン", "イッケン"),
            ("3軒", "サンゲン", "サンゲン"),
            ("6軒", "ロッケン", "ロッケン"),
            ("10軒", "ジュッケン", "ジュッケン"),
            ("1校", "イッコウ", "イッコー"),
            ("6校", "ロッコウ", "ロッコー"),
            ("10校", "ジュッコウ", "ジュッコー"),
            ("10巻", "ジュッカン", "ジュッカン"),
            ("10課", "ジュッカ", "ジュッカ"),
            ("10機", "ジュッキ", "ジュッキ"),
            ("1箇所", "イッカショ", "イッカショ"),
            ("6箇所", "ロッカショ", "ロッカショ"),
            ("10箇所", "ジュッカショ", "ジュッカショ"),
            ("1拍", "イッパク", "イッパク"),
            ("3拍", "サンパク", "サンパク"),
            ("6拍", "ロッパク", "ロッパク"),
            ("8拍", "ハッパク", "ハッパク"),
            ("10拍", "ジュッパク", "ジュッパク"),
            ("100拍", "ヒャッパク", "ヒャッパク"),
            ("1票", "イッピョウ", "イッピョー"),
            ("3票", "サンピョウ", "サンピョー"),
            ("6票", "ロッピョウ", "ロッピョー"),
            ("10票", "ジュッピョウ", "ジュッピョー"),
            ("100票", "ヒャッピョウ", "ヒャッピョー"),
            ("1俵", "イッピョウ", "イッピョー"),
            ("3俵", "サンビョウ", "サンビョー"),
            ("10俵", "ジュッピョウ", "ジュッピョー"),
            ("1遍", "イッペン", "イッペン"),
            ("3遍", "サンベン", "サンベン"),
            ("10遍", "ジュッペン", "ジュッペン"),
            ("1品", "イッピン", "イッピン"),
            ("3品", "サンピン", "サンピン"),
            ("10品", "ジュッピン", "ジュッピン"),
            ("1筆", "イッピツ", "イッピツ"),
            ("2筆", "ニヒツ", "ニヒツ"),
            ("3筆", "サンピツ", "サンピツ"),
            ("10筆", "ジュッピツ", "ジュッピツ"),
            ("1府", "イップ", "イップ"),
            ("3府", "サンプ", "サンプ"),
            ("10府", "ジュップ", "ジュップ"),
            ("1都", "イット", "イット"),
            ("10都", "ジュット", "ジュット"),
            ("1県", "イッケン", "イッケン"),
            ("10県", "ジュッケン", "ジュッケン"),
            ("10号", "ジュウゴウ", "ジューゴー"),
            ("1000号", "センゴウ", "センゴー"),
            ("1体", "イッタイ", "イッタイ"),
            ("8体", "ハッタイ", "ハッタイ"),
            ("10体", "ジュッタイ", "ジュッタイ"),
            ("1隻", "イッセキ", "イッセキ"),
            ("10席", "ジュッセキ", "ジュッセキ"),
            ("1勝", "イッショウ", "イッショー"),
            ("10戦", "ジュッセン", "ジュッセン"),
            ("1センチ", "イッセンチ", "イッセンチ"),
            ("6センチ", "ロクセンチ", "ロクセンチ"),
            ("8センチ", "ハッセンチ", "ハッセンチ"),
            ("10センチ", "ジュッセンチ", "ジュッセンチ"),
            ("1キロ", "イチキロ", "イチキロ"),
            ("6キロ", "ロッキロ", "ロッキロ"),
            ("8キロ", "ハチキロ", "ハチキロ"),
            ("10キロ", "ジュッキロ", "ジュッキロ"),
            ("100キロ", "ヒャッキロ", "ヒャッキロ"),
            ("六キログラム", "ロッキログラム", "ロッキログラム"),
            ("六キロメートル", "ロッキロメートル", "ロッキロメートル"),
            ("1箱", "ヒトハコ", "ヒトハコ"),
            ("2箱", "フタハコ", "フタハコ"),
            ("10箱", "ジュッパコ", "ジュッパコ"),
            ("6箱", "ロクハコ", "ロクハコ"),
            ("1皿", "ヒトサラ", "ヒトサラ"),
            ("2皿", "フタサラ", "フタサラ"),
            ("1束", "ヒトタバ", "ヒトタバ"),
            ("2束", "フタタバ", "フタタバ"),
            ("一点五本", "イチテンゴホン", "イチテンゴホン"),
            ("十点零本", "ジュウテンレイホン", "ジューテンレイホン"),
            ("2日間", "フツカカン", "フツカカン"),
            ("3日間", "ミッカカン", "ミッカカン"),
            ("4日間", "ヨッカカン", "ヨッカカン"),
            ("5日間", "イツカカン", "イツカカン"),
            ("6日間", "ムイカカン", "ムイカカン"),
            ("7日間", "ナノカカン", "ナノカカン"),
            ("8日間", "ヨウカカン", "ヨーカカン"),
            ("9日間", "ココノカカン", "ココノカカン"),
            ("10日間", "トオカカン", "トーカカン"),
            ("14日間", "ジュウヨッカカン", "ジューヨッカカン"),
            ("24日間", "ニジュウヨッカカン", "ニジューヨッカカン"),
            ("20日間", "ハツカカン", "ハツカカン"),
            ("二十日間", "ハツカカン", "ハツカカン"),
            ("二日間", "フツカカン", "フツカカン"),
            ("十四日間", "ジュウヨッカカン", "ジューヨッカカン"),
            ("二十四日間", "ニジュウヨッカカン", "ニジューヨッカカン"),
            ("一日間", "イチニチカン", "イチニチカン"),
            ("1000拍", "センパク", "センパク"),
            ("10000拍", "イチマンパク", "イチマンパク"),
            ("1000泊", "センパク", "センパク"),
            ("10000泊", "イチマンパク", "イチマンパク"),
            ("1000筆", "センピツ", "センピツ"),
            ("10000筆", "イチマンピツ", "イチマンピツ"),
            ("1000品", "センピン", "センピン"),
            ("10000品", "イチマンピン", "イチマンピン"),
            ("1000波", "センパ", "センパ"),
            ("10000波", "イチマンパ", "イチマンパ"),
            ("1000敗", "センパイ", "センパイ"),
            ("10000敗", "イチマンパイ", "イチマンパイ"),
            ("1000府", "センプ", "センプ"),
            ("10000府", "イチマンプ", "イチマンプ"),
            ("1000発", "センパツ", "センパツ"),
            ("10000発", "イチマンパツ", "イチマンパツ"),
            ("1000票", "センビョウ", "センビョー"),
            ("10000票", "イチマンビョウ", "イチマンビョー"),
            ("1000俵", "センビョウ", "センビョー"),
            ("10000俵", "イチマンビョウ", "イチマンビョー"),
            ("1000000票", "ヒャクマンビョウ", "ヒャクマンビョー"),
            ("1000遍", "センベン", "センベン"),
            ("10000遍", "イチマンベン", "イチマンベン"),
            ("1篇", "イッペン", "イッペン"),
            ("6篇", "ロッペン", "ロッペン"),
            ("10篇", "ジュッペン", "ジュッペン"),
            ("1編", "イッペン", "イッペン"),
            ("6編", "ロッペン", "ロッペン"),
            ("10編", "ジュッペン", "ジュッペン"),
            ("二十日", "ハツカ", "ハツカ"),
            ("20日", "ハツカ", "ハツカ"),
            ("二十日後", "ハツカゴ", "ハツカゴ"),
            ("20日後", "ハツカゴ", "ハツカゴ"),
            (
                "41都府県警",
                "ヨンジュウイチトフケンケイ",
                "ヨンジューイチトフケンケイ",
            ),
            ("1府県", "イチフケン", "イチフケン"),
            ("6県", "ロッケン", "ロッケン"),
            ("8県", "ハッケン", "ハッケン"),
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

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_person_name_spacing_variants_keep_given_name_readings() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for text in ["Daniel H.Pink", "Daniel H. Pink"] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let reading = tokens.iter().map(|t| &*t.reading).collect::<String>();
            let pronunciation = tokens.iter().map(|t| &*t.pronunciation).collect::<String>();
            // 空白なしの表層は自然な分割を許し、名の音節欠落だけを検出する。
            assert!(
                reading.starts_with("ダニエル"),
                "{name}: {text}: {tokens:?}"
            );
            assert!(
                pronunciation.starts_with("ダニエル"),
                "{name}: {text}: {tokens:?}"
            );
            if text == "Daniel H. Pink" {
                assert_eq!(reading, "ダニエルエイチピンク", "{name}: {tokens:?}");
                assert_eq!(pronunciation, "ダニエルエイチピンク", "{name}: {tokens:?}");
            }
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
    }
}

// 独立に作成した例で、限定削除後の合成と正しい同形語の保持を確認する。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_reviewed_calendar_and_compound_counters() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (quantity, reading, pronunciation) in [
            ("4月上旬", "シガツジョウジュン", "シガツジョージュン"),
            ("4月中旬", "シガツチュウジュン", "シガツチュージュン"),
            ("4月下旬", "シガツゲジュン", "シガツゲジュン"),
            ("9月上旬", "クガツジョウジュン", "クガツジョージュン"),
            ("9月中旬", "クガツチュウジュン", "クガツチュージュン"),
            ("9月下旬", "クガツゲジュン", "クガツゲジュン"),
            ("2馬身半", "ニバシンハン", "ニバシンハン"),
            ("2馬身半差", "ニバシンハンサ", "ニバシンハンサ"),
            ("2馬身差", "ニバシンサ", "ニバシンサ"),
            ("1杯目", "イッパイメ", "イッパイメ"),
            ("3杯目", "サンバイメ", "サンバイメ"),
            ("6杯目", "ロッパイメ", "ロッパイメ"),
            ("100杯目", "ヒャッパイメ", "ヒャッパイメ"),
            ("年2回", "ネンニカイ", "ネンニカイ"),
            ("年0回", "ネンゼロカイ", "ネンゼロカイ"),
        ] {
            let text = format!("{quantity}を記録した。");
            let tokens = analyzer.try_tokenize(&text).unwrap();
            let actual: Vec<_> = tokens
                .iter()
                .take_while(|t| t.end <= quantity.len())
                .collect();
            assert_eq!(
                actual.iter().map(|t| &*t.reading).collect::<String>(),
                reading,
                "{name}: {text}: {tokens:?}"
            );
            assert_eq!(
                actual.iter().map(|t| &*t.pronunciation).collect::<String>(),
                pronunciation,
                "{name}: {text}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
        for text in ["その年2回を記録した。", "ある年2回を記録した。"] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let year = tokens.iter().find(|t| t.surface.as_ref() == "年").unwrap();
            assert_eq!(&*year.reading, "トシ", "{name}: {text}: {tokens:?}");
            assert_eq!(&*year.pronunciation, "トシ", "{name}: {text}: {tokens:?}");
        }
    }
}

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_lexical_repairs_keep_plural_sounds_and_grammatical_boundaries() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (surface, reading, pronunciation) in [
            ("潰散", "カイサン", "カイサン"),
            ("失冠", "シッカン", "シッカン"),
            ("界磁", "カイジ", "カイジ"),
            ("蹄球炎", "テイキュウエン", "テイキューエン"),
            ("安青錦", "アオニシキ", "アオニシキ"),
            ("Stories", "ストーリーズ", "ストーリーズ"),
            ("Associates", "アソシエーツ", "アソシエーツ"),
            ("ストーリー", "ストーリー", "ストーリー"),
            ("アソシエート", "アソシエート", "アソシエート"),
            ("アトール", "アトール", "アトール"),
            ("多値", "タチ", "タチ"),
            ("医事", "イジ", "イジ"),
        ] {
            for text in [surface.to_owned(), format!("{surface}を記す。")] {
                let tokens = analyzer.try_tokenize(&text).unwrap();
                let actual: Vec<_> = tokens
                    .iter()
                    .take_while(|token| token.end <= surface.len())
                    .collect();
                assert_eq!(
                    actual.iter().map(|t| &*t.reading).collect::<String>(),
                    reading,
                    "{name}: {text}: {tokens:?}"
                );
                assert_eq!(
                    actual.iter().map(|t| &*t.pronunciation).collect::<String>(),
                    pronunciation,
                    "{name}: {text}: {tokens:?}"
                );
                for token in &tokens {
                    assert_eq!(&text[token.start..token.end], token.surface.as_ref());
                }
            }
        }
        for (text, reading, pronunciation) in [
            (
                "学習到達度調査",
                "ガクシュウトウタツドチョウサ",
                "ガクシュートータツドチョーサ",
            ),
            ("ルート権", "ルートケン", "ルートケン"),
            ("ルート権限", "ルートケンゲン", "ルートケンゲン"),
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
        // 著者省略表記を別語のアトールへ展開せず、唯一のカナ表記はここで決めない。
        let text = "著者一覧を et al. と表す。";
        let tokens = analyzer.try_tokenize(text).unwrap();
        assert!(
            tokens.iter().all(|token| {
                token.reading.as_ref() != "アトール" && token.pronunciation.as_ref() != "アトール"
            }),
            "{name}: {text}: {tokens:?}"
        );
        for token in &tokens {
            assert_eq!(&text[token.start..token.end], token.surface.as_ref());
        }
        // 真の多値・医事とは別に、助動詞と名詞の境界を保つ。
        // 値の同形異音語の選択を、この境界検査だけで正読と認定しない。
        for (text, auxiliary, noun, wrong_surface, wrong_base) in [
            ("計算した値を比べる。", "た", "値", "た値", "多値"),
            ("覚えたい事を並べる。", "たい", "事", "い事", "医事"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                tokens.iter().all(|token| {
                    token.surface.as_ref() != wrong_surface
                        || token.base_form.as_ref() != wrong_base
                }),
                "{name}: {text}: {tokens:?}"
            );
            assert!(
                tokens.iter().any(|token| {
                    token.surface.as_ref() == auxiliary && token.pos.starts_with("助動詞,")
                }),
                "{name}: {text}: {tokens:?}"
            );
            assert!(
                tokens.iter().any(|token| {
                    token.surface.as_ref() == noun && token.base_form.as_ref() == noun
                }),
                "{name}: {text}: {tokens:?}"
            );
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
    }
}

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_choketsu_readings_keep_general_noun_and_particle_boundaries() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, surfaces) in [
            ("挑決", &["挑決"][..]),
            (
                "挑決を観戦する。",
                &["挑決", "を", "観戦", "する", "。"][..],
            ),
            (
                "挑決の結果を待つ。",
                &["挑決", "の", "結果", "を", "待つ", "。"][..],
            ),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_eq!(
                tokens
                    .iter()
                    .map(|token| token.surface.as_ref())
                    .collect::<Vec<_>>(),
                surfaces,
                "{name}: {text}: {tokens:?}"
            );
            let word = &tokens[0];
            assert!(word.is_known, "{name}: {text}: {tokens:?}");
            assert_eq!(word.reading.as_ref(), "チョウケツ", "{name}: {text}");
            assert_eq!(word.pronunciation.as_ref(), "チョーケツ", "{name}: {text}");
            assert_eq!(word.pos.as_ref(), "名詞,一般,*,*", "{name}: {text}");
            assert_eq!(word.base_form.as_ref(), "挑決", "{name}: {text}");
            assert!(word.conj_type.is_empty(), "{name}: {text}: {tokens:?}");
            assert!(word.conj_form.is_empty(), "{name}: {text}: {tokens:?}");
            assert_eq!(word.start, 0, "{name}: {text}");
            assert_eq!(word.end, "挑決".len(), "{name}: {text}");
            if let Some(particle) = tokens.get(1) {
                assert!(
                    particle.pos.starts_with("助詞,"),
                    "{name}: {text}: {tokens:?}"
                );
            }
            for token in &tokens {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
            for pair in tokens.windows(2) {
                assert_eq!(pair[0].end, pair[1].start, "{name}: {text}: {tokens:?}");
            }
            assert_eq!(tokens.last().unwrap().end, text.len(), "{name}: {text}");
        }
    }
}
