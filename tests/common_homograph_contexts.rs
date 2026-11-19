use hasami::Analyzer;

#[test]
#[ignore = "全部入りの配布辞書が必要"]
fn distributed_common_homographs_preserve_other_readings() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    let mut analyzer = Analyzer::load(format!("{dir}/ipadic-neologd-sudachi.hsd")).unwrap();

    for (text, surface, reading, pronunciation, pos) in [
        (
            "祖国を治めた方です。",
            "方",
            "カタ",
            "カタ",
            "名詞,非自立,一般,*",
        ),
        (
            "米の値上がりに驚く。",
            "米",
            "コメ",
            "コメ",
            "名詞,一般,*,*",
        ),
        (
            "電話は後にしてね。",
            "後",
            "アト",
            "アト",
            "名詞,副詞可能,*,*",
        ),
        (
            "カップの縁が欠けています。",
            "縁",
            "フチ",
            "フチ",
            "名詞,一般,*,*",
        ),
        (
            "そちらの方が近い。",
            "方",
            "ホウ",
            "ホー",
            "名詞,非自立,一般,*",
        ),
        (
            "日米の値上がりを比較する。",
            "米",
            "ベイ",
            "ベイ",
            "名詞,固有名詞,地域,国",
        ),
        (
            "予約は会議後に取る。",
            "後",
            "ゴ",
            "ゴ",
            "名詞,接尾,副詞可能,*",
        ),
        (
            "縁があって出会った。",
            "縁",
            "エン",
            "エン",
            "名詞,一般,*,*",
        ),
        (
            "十分に時間がある。",
            "十分",
            "ジュウブン",
            "ジューブン",
            "名詞,形容動詞語幹,*,*",
        ),
        ("図形を描く。", "図形", "ズケイ", "ズケイ", "名詞,一般,*,*"),
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        let token = tokens
            .iter()
            .find(|t| t.surface.as_ref() == surface)
            .unwrap();
        assert_eq!(token.reading.as_ref(), reading, "{text}");
        assert_eq!(token.pronunciation.as_ref(), pronunciation, "{text}");
        assert_eq!(token.pos.as_ref(), pos, "{text}");
        assert!(token.is_known);
    }

    let text = "徒歩での移動は十分程度。";
    let start = text.find("十分").unwrap();
    let tokens = analyzer.try_tokenize(text).unwrap();
    let i = tokens
        .iter()
        .position(|token| token.start == start)
        .unwrap();
    let [ten, minutes] = &tokens[i..i + 2] else {
        unreachable!()
    };
    assert_eq!(ten.surface.as_ref(), "十");
    assert_eq!(ten.pos.as_ref(), "名詞,数,*,*");
    assert_eq!(minutes.surface.as_ref(), "分");
    assert_eq!(minutes.pos.as_ref(), "名詞,接尾,助数詞,*");
    assert_eq!(ten.end, minutes.start);
    assert_eq!(minutes.end, start + "十分".len());
    assert_eq!(format!("{}{}", ten.reading, minutes.reading), "ジュップン");
    assert_eq!(
        format!("{}{}", ten.pronunciation, minutes.pronunciation),
        "ジュップン"
    );
    let spans = analyzer.contextual_reading_spans(text).unwrap();
    assert_eq!(spans.len(), 2);
    for (span, token) in spans.iter().zip([ten, minutes]) {
        assert_eq!((span.start, span.end), (token.start, token.end));
        assert_eq!(span.pronunciation, token.pronunciation.as_ref());
    }
}

#[test]
#[ignore = "全部入りの配布辞書が必要"]
fn distributed_directional_he_to_preserves_nominal_pairs() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    let mut analyzer = Analyzer::load(format!("{dir}/ipadic-neologd-sudachi.hsd")).unwrap();

    for text in [
        "船は「函館」,へと津軽海峡を渡った。",
        "一行は「函館」、へと北海道を南下した。",
        "一行は「大津」,へと琵琶湖を渡った。",
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        let start = text.find("へと").unwrap();
        let index = tokens.iter().position(|t| t.start == start).unwrap();
        let [he, to] = &tokens[index..index + 2] else {
            unreachable!()
        };
        for (token, surface, pos, reading, pronunciation) in [
            (he, "へ", "助詞,格助詞,一般,*", "ヘ", "エ"),
            (to, "と", "助詞,格助詞,引用,*", "ト", "ト"),
        ] {
            assert_eq!(token.surface.as_ref(), surface, "{text}");
            assert_eq!(token.base_form.as_ref(), surface, "{text}");
            assert_eq!(token.pos.as_ref(), pos, "{text}");
            assert_eq!(token.reading.as_ref(), reading, "{text}");
            assert_eq!(token.pronunciation.as_ref(), pronunciation, "{text}");
            assert_eq!(&text[token.start..token.end], surface, "{text}");
            assert!(token.is_known);
        }
        assert_eq!(he.end, to.start, "{text}");
    }

    for text in [
        "識別札の「に」,へと東京を並べた。",
        "船は「函館」,へと東京を並べた。",
        "船は「函館」,へと 北海道を進んだ。",
        "一行は「『函館』,へと北海道を進んだ」と書いた。",
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        let start = text.find("へと").unwrap();
        let index = tokens.iter().position(|t| t.start == start).unwrap();
        assert_eq!(tokens[index].pos.as_ref(), "名詞,一般,*,*", "{text}");
        assert_eq!(
            tokens[index + 1].pos.as_ref(),
            "助詞,並立助詞,*,*",
            "{text}"
        );
    }
}
