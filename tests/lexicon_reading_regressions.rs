//! 確認済みの語彙の読みと、活用語・助詞の境界を配布辞書で保つ。
use hasami::Analyzer;

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_reviewed_lexical_readings() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, expected) in [
            ("記事名", "キジメイ"),
            ("再現度", "サイゲンド"),
            ("進行方向", "シンコウホウコウ"),
            ("非人道的", "ヒジンドウテキ"),
            ("強酸性", "キョウサンセイ"),
            ("燃料電池自動車", "ネンリョウデンチジドウシャ"),
            ("大航海時代", "ダイコウカイジダイ"),
            ("狂牛病", "キョウギュウビョウ"),
            ("臭化銀", "シュウカギン"),
            ("二次面接", "ニジメンセツ"),
            ("伝道師", "デンドウシ"),
            ("行方不明者", "ユクエフメイシャ"),
            ("江風", "カワカゼ"),
            ("備中高松城", "ビッチュウタカマツジョウ"),
            (
                "SDガンダム GGENERATION",
                "エスディーガンダムジージェネレーション",
            ),
            ("光電流", "コウデンリュウ"),
            ("Grand Prix", "グランプリ"),
            ("日経MJ", "ニッケイエムジェイ"),
            ("Wi-Fi", "ワイファイ"),
            ("細川忠興", "ホソカワタダオキ"),
            ("土方歳三", "ヒジカタトシゾウ"),
        ] {
            if text == "Grand Prix" && name == "ipadic-neologd" {
                continue;
            }
            let tokens = analyzer.try_tokenize(text).unwrap();
            let reading: String = tokens.iter().map(|token| &*token.reading).collect();
            assert_eq!(reading, expected, "{name}: {text}: {tokens:?}");
        }
        for (text, rejected) in [
            ("国ごとに比べる。", "国ご"),
            ("服も洗う。", "服も"),
            ("家で休む。", "家で"),
            ("困らないよう水を用意する。", "よう水"),
            ("昨日模写した絵を見せる。", "した絵"),
            ("海のない国を訪ねる。", "ない国"),
            ("初回のみ予約できる。", "回のみ"),
            ("太郎は兵を集めた。", "は兵"),
            ("弱い者いじめを許さない。", "い者"),
            ("検査した回数を数える。", "した回"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                !tokens.iter().any(|token| &*token.surface == rejected),
                "{name}: {text}: {tokens:?}"
            );
        }
        for text in [
            "用水",
            "揚水",
            "下絵",
            "内国",
            "家出",
            "服喪",
            "回し飲み",
            "医者",
            "歯医者",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_eq!(tokens.len(), 1, "{name}: {text}: {tokens:?}");
            assert!(tokens[0].is_known, "{name}: {text}");
        }
    }
}

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_reviewed_pronunciations_and_missing_reading_tails() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, expected) in [
            ("入れ子構造", "イレココーゾー"),
            ("君の名は。", "キミノナワ"),
            ("386BSD", "サンハチロクビーエスディー"),
            ("4.4BSD-Lite", "ヨンテンヨンビーエスディーライト"),
            ("愛聴", "アイチョー"),
            ("ラジオ第1放送", "ラジオダイイチホーソー"),
            ("ニコニコ生放送", "ニコニコナマホーソー"),
            ("大喜利", "オーギリ"),
            ("表計算ソフト", "ヒョーケイサンソフト"),
            ("採卵鶏", "サイランケイ"),
            (
                "チャーリーとチョコレート工場",
                "チャーリートチョコレートコージョー",
            ),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let pronunciation: String = tokens.iter().map(|token| &*token.pronunciation).collect();
            assert_eq!(pronunciation, expected, "{name}: {text}: {tokens:?}");
        }
        for (text, ending) in [
            ("ベンジャミン・バトン 数奇な人生", "ジンセイ"),
            ("滋賀県立彦根東高等学校", "ガッコウ"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let reading: String = tokens.iter().map(|token| &*token.reading).collect();
            assert!(reading.ends_with(ending), "{name}: {text}: {tokens:?}");
        }
    }
}

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_common_dormitory_closure_reading_and_boundaries() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (text, following, following_pos) in [
            ("学生寮は来月閉寮する。", "する", "動詞,自立,*,*"),
            ("閉寮の予定を知らせる。", "の", "助詞,連体化,*,*"),
            ("閉寮を延期する。", "を", "助詞,格助詞,一般,*"),
            ("今月は閉寮しない。", "し", "動詞,自立,*,*"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let index = tokens
                .iter()
                .position(|token| &*token.surface == "閉寮")
                .unwrap_or_else(|| panic!("{name}: {text}: {tokens:?}"));
            let noun = &tokens[index];
            assert!(noun.is_known, "{name}: {text}: {tokens:?}");
            assert_eq!(&*noun.reading, "ヘイリョウ", "{name}: {text}");
            assert_eq!(&*noun.pronunciation, "ヘイリョー", "{name}: {text}");
            assert_eq!(&*noun.pos, "名詞,サ変接続,*,*", "{name}: {text}");
            assert_eq!(&*noun.base_form, "閉寮", "{name}: {text}");
            assert!(noun.conj_type.is_empty(), "{name}: {text}");
            assert!(noun.conj_form.is_empty(), "{name}: {text}");
            assert_eq!(Some(noun.start), text.find("閉寮"), "{name}: {text}");
            assert_eq!(noun.end - noun.start, "閉寮".len(), "{name}: {text}");
            let next = &tokens[index + 1];
            assert_eq!(&*next.surface, following, "{name}: {text}: {tokens:?}");
            assert_eq!(&*next.pos, following_pos, "{name}: {text}: {tokens:?}");
            assert_eq!(noun.end, next.start, "{name}: {text}: {tokens:?}");
            if following == "し" {
                assert_eq!(&*next.base_form, "する", "{name}: {text}");
                assert_eq!(&*tokens[index + 2].surface, "ない", "{name}: {text}");
                assert!(tokens[index + 2].is_negation(), "{name}: {text}");
            }
        }
        for (text, reading, pronunciation, pos) in [
            ("閉校", "ヘイコウ", "ヘイコー", "名詞,サ変接続,*,*"),
            ("閉館", "ヘイカン", "ヘイカン", "名詞,サ変接続,*,*"),
            ("入寮", "ニュウリョウ", "ニューリョー", "名詞,サ変接続,*,*"),
            ("寮", "リョウ", "リョー", "名詞,一般,*,*"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_eq!(tokens.len(), 1, "{name}: {text}: {tokens:?}");
            let token = &tokens[0];
            assert!(token.is_known, "{name}: {text}");
            assert_eq!(&*token.surface, text, "{name}: {text}");
            assert_eq!(&*token.reading, reading, "{name}: {text}");
            assert_eq!(&*token.pronunciation, pronunciation, "{name}: {text}");
            assert_eq!(&*token.pos, pos, "{name}: {text}");
        }
    }
}
