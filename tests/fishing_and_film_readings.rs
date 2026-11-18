use hasami::Analyzer;

#[test]
#[ignore = "全部入りの配布辞書が必要"]
fn distributed_fishing_and_film_readings_preserve_their_other_senses() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    let mut analyzer = Analyzer::load(format!("{dir}/ipadic-neologd-sudachi.hsd")).unwrap();

    for (text, net) in [
        ("深海底引き網で漁獲される", "底引き網"),
        ("深海底びき網で調査する。", "底びき網"),
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        let deep = &tokens[0];
        assert_eq!(deep.surface.as_ref(), "深海");
        assert_eq!(deep.pos.as_ref(), "名詞,一般,*,*");
        assert_eq!(deep.reading.as_ref(), "シンカイ");
        assert_eq!(deep.pronunciation.as_ref(), "シンカイ");
        assert_eq!(deep.base_form.as_ref(), "深海");
        assert!(deep.is_known);
        assert_eq!(deep.start, 0);
        assert_eq!(deep.end, "深海".len());
        assert_eq!(tokens[1].surface.as_ref(), net);
        assert_eq!(tokens[1].base_form.as_ref(), "底引き網");
        assert_eq!(tokens[1].reading.as_ref(), "ソコビキアミ");
        assert_eq!(tokens[1].start, deep.end);
    }

    let text = "深海底引き網で漁獲される";
    let tokens = analyzer.try_tokenize(text).unwrap();
    assert_eq!(
        tokens
            .iter()
            .map(|t| t.reading.as_ref())
            .collect::<String>(),
        "シンカイソコビキアミデギョカクサレル"
    );

    let company = "勝プロダクション";
    let tokens = analyzer.try_tokenize("勝プロダクション製作").unwrap();
    assert_eq!(tokens[0].surface.as_ref(), company);
    assert_eq!(tokens[0].base_form.as_ref(), company);
    assert_eq!(tokens[0].pos.as_ref(), "名詞,固有名詞,組織,*");
    assert_eq!(tokens[0].pronunciation.as_ref(), "カツプロダクション");
    assert_eq!((tokens[0].start, tokens[0].end), (0, company.len()));
    assert!(tokens[0].is_known);
    assert_eq!(tokens[1].surface.as_ref(), "製作");
    assert_eq!(
        tokens
            .iter()
            .map(|t| t.reading.as_ref())
            .collect::<String>(),
        "カツプロダクションセイサク"
    );

    for (text, surface, reading, pos) in [
        (
            "深海さんに話を聞く。",
            "深海",
            "フカウミ",
            "名詞,固有名詞,人名,姓",
        ),
        ("勝を重ねる。", "勝", "カチ", "名詞,一般,*,*"),
        (
            "プロダクションで働く。",
            "プロダクション",
            "プロダクション",
            "名詞,一般,*,*",
        ),
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        assert_eq!(tokens[0].surface.as_ref(), surface);
        assert_eq!(tokens[0].reading.as_ref(), reading);
        assert_eq!(tokens[0].pos.as_ref(), pos);
        assert!(tokens[0].is_known);
    }
}
