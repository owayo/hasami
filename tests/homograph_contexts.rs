use hasami::Analyzer;

#[test]
#[ignore = "全部入りの配布辞書が必要"]
fn distributed_homographs_use_the_readings_supported_by_their_context() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    let mut analyzer = Analyzer::load(format!("{dir}/ipadic-neologd-sudachi.hsd")).unwrap();

    for (text, surface, reading, pronunciation, pieces) in [
        (
            "数合の米を炊く。",
            "数合",
            "スウゴウ",
            "スーゴー",
            &["数", "合"][..],
        ),
        (
            "二重に確認する。",
            "二重",
            "ニジュウ",
            "ニジュー",
            &["二", "重"],
        ),
        (
            "小さな石かと思った。",
            "石か",
            "イシカ",
            "イシカ",
            &["石", "か"],
        ),
        (
            "金星が夜空に見える。",
            "金星",
            "キンセイ",
            "キンセイ",
            &["金星"],
        ),
        (
            "長江は中国を流れる。",
            "長江",
            "チョウコウ",
            "チョーコー",
            &["長江"],
        ),
        (
            "待ち時間が長ければ休む。",
            "長けれ",
            "ナガケレ",
            "ナガケレ",
            &["長けれ"],
        ),
    ] {
        let start = text.find(surface).unwrap();
        let end = start + surface.len();
        let tokens = analyzer.try_tokenize(text).unwrap();
        let part: Vec<_> = tokens
            .iter()
            .filter(|t| t.start < end && start < t.end)
            .collect();
        assert_eq!(part.first().unwrap().start, start, "{text}");
        assert_eq!(part.last().unwrap().end, end, "{text}");
        assert!(part.windows(2).all(|pair| pair[0].end == pair[1].start));
        assert!(
            part.iter()
                .all(|t| t.is_known && !t.pos.starts_with("名詞,固有名詞,人名,")),
            "{text}: {part:?}"
        );
        assert_eq!(
            part.iter().map(|t| t.reading.as_ref()).collect::<String>(),
            reading,
            "{text}"
        );
        assert_eq!(
            part.iter()
                .map(|t| t.pronunciation.as_ref())
                .collect::<String>(),
            pronunciation,
            "{text}"
        );
        assert_eq!(
            part.iter().map(|t| t.surface.as_ref()).collect::<Vec<_>>(),
            pieces,
            "{text}"
        );
        let spans = analyzer.contextual_reading_spans(text).unwrap();
        let corrected: Vec<_> = spans
            .iter()
            .filter(|span| start <= span.start && span.end <= end)
            .map(|span| (span.start, span.end, span.pronunciation.as_str()))
            .collect();
        let expected: Vec<_> = part
            .iter()
            .map(|t| (t.start, t.end, t.pronunciation.as_ref()))
            .collect();
        assert_eq!(corrected, expected, "{text}");
        if surface == "長けれ" {
            assert_eq!(part[0].base_form.as_ref(), "長い");
            assert_eq!(part[0].conj_form.as_ref(), "仮定形");
        }
    }

    for (text, surface, reading, pos, base) in [
        (
            "数合さんに会う。",
            "数合",
            "スゴウ",
            "名詞,固有名詞,人名,姓",
            "数合",
        ),
        (
            "二重さんに会う。",
            "二重",
            "フタエ",
            "名詞,固有名詞,人名,姓",
            "二重",
        ),
        (
            "金星さんに会う。",
            "金星",
            "カナボシ",
            "名詞,固有名詞,人名,姓",
            "金星",
        ),
        (
            "金星を挙げる。",
            "金星",
            "キンボシ",
            "名詞,一般,*,*",
            "金星",
        ),
        (
            "長江さんに会う。",
            "長江",
            "ナガエ",
            "名詞,固有名詞,人名,姓",
            "長江",
        ),
        (
            "木が石化する。",
            "石化",
            "セキカ",
            "名詞,サ変接続,*,*",
            "石化",
        ),
        (
            "芸に長ければ採用する。",
            "長けれ",
            "タケレ",
            "動詞,自立,*,*",
            "長ける",
        ),
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        let token = tokens
            .iter()
            .find(|t| t.surface.as_ref() == surface)
            .unwrap();
        assert_eq!(token.reading.as_ref(), reading, "{text}");
        assert_eq!(token.pos.as_ref(), pos, "{text}");
        assert_eq!(token.base_form.as_ref(), base, "{text}");
    }
}
