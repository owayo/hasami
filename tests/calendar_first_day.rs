//! 月日に直結する初日だけを暦日とし、裸の1日と期間を保持する。
#![cfg(feature = "build")]

use hasami::{Analyzer, ContextualReadingSpan, DictEntry, Token, dict::DictBuilder};

fn entry(surface: &str, pos: &str, reading: &str, pronunciation: &str, cost: i16) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        base_form: surface.into(),
        pos: pos.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
        cost,
        ..Default::default()
    }
}

fn fixture(whole_month: bool, custom: Option<DictEntry>) -> Analyzer {
    let mut builder = DictBuilder::new();
    for (n, (r, p)) in [
        ("ゼロ", "ゼロ"),
        ("イチ", "イチ"),
        ("ニ", "ニ"),
        ("サン", "サン"),
        ("ヨン", "ヨン"),
        ("ゴ", "ゴ"),
        ("ロク", "ロク"),
        ("ナナ", "ナナ"),
        ("ハチ", "ハチ"),
        ("キュウ", "キュー"),
    ]
    .into_iter()
    .enumerate()
    {
        for raw in [
            n.to_string(),
            char::from_u32('０' as u32 + n as u32).unwrap().to_string(),
            "〇一二三四五六七八九".chars().nth(n).unwrap().to_string(),
        ] {
            builder.add_entry(entry(&raw, "名詞,数,*,*", r, p, -100));
        }
    }
    for (s, pos, r, p) in [
        ("十", "名詞,数,*,*", "ジュウ", "ジュー"),
        ("月", "名詞,一般,*,*", "ガツ", "ガツ"),
        ("年", "名詞,接尾,助数詞,*", "ネン", "ネン"),
        (
            "2024",
            "名詞,数,*,*",
            "ニセンニジュウヨン",
            "ニセンニジューヨン",
        ),
        ("日付", "名詞,一般,*,*", "ヒヅケ", "ヒズケ"),
        ("名前", "名詞,一般,*,*", "ナマエ", "ナマエ"),
        ("作品名", "名詞,一般,*,*", "サクヒンメイ", "サクヒンメイ"),
        ("は", "助詞,係助詞,*,*", "ハ", "ワ"),
        ("です", "助動詞,*,*,*", "デス", "デス"),
        ("。", "記号,句点,*,*", "。", "。"),
        ("「", "記号,括弧開,*,*", "「", "「"),
        ("」", "記号,括弧閉,*,*", "」", "」"),
    ] {
        builder.add_entry(entry(s, pos, r, p, -1000));
    }
    builder.add_entry(entry("日", "名詞,接尾,助数詞,*", "ニチ", "ニチ", 50));
    if whole_month {
        for raw in ["11月", "１１月", "十一月"] {
            builder.add_entry(entry(
                raw,
                "名詞,副詞可能,*,*",
                "ジュウイチガツ",
                "ジューイチガツ",
                -3000,
            ));
        }
        for (forms, reading) in [
            (["4月", "４月", "四月"], "シガツ"),
            (["7月", "７月", "七月"], "シチガツ"),
            (["9月", "９月", "九月"], "クガツ"),
        ] {
            for raw in forms {
                builder.add_entry(entry(raw, "名詞,副詞可能,*,*", reading, reading, -3000));
            }
        }
    }
    if let Some(custom) = custom {
        builder.add_entry(custom);
    }
    Analyzer::from_dict(builder.build().unwrap())
}

fn day(tokens: &[Token], surface: &str) -> Token {
    tokens
        .iter()
        .find(|t| t.surface.as_ref() == surface)
        .unwrap_or_else(|| panic!("missing {surface}: {tokens:?}"))
        .clone()
}

#[test]
fn calendar_first_day_is_one_honest_synthetic_token_and_contextual_span() {
    for whole_month in [false, true] {
        let mut analyzer = fixture(whole_month, None);
        let text = "日付は11月1日です。";
        let tokens = analyzer.try_tokenize(text).unwrap();
        let first = day(&tokens, "1日");
        assert_eq!((first.start, first.end), (14, 18));
        assert_eq!(first.base_form, first.surface);
        assert_eq!(first.pos.as_ref(), "名詞,副詞可能,*,*");
        assert_eq!(first.reading.as_ref(), "ツイタチ");
        assert_eq!(first.pronunciation, first.reading);
        assert!(first.conj_type.is_empty() && first.conj_form.is_empty());
        assert_eq!(first.word_cost, -50);
        assert!(!first.is_known);
        assert_eq!(first.mora_count(), 4);
        assert_eq!(
            analyzer.contextual_reading_spans(text).unwrap(),
            [ContextualReadingSpan {
                start: 14,
                end: 18,
                pronunciation: "ツイタチ".into()
            }]
        );
        assert!(
            analyzer
                .quantity_spans(text)
                .unwrap()
                .iter()
                .all(|s| s.end <= 14 || s.start >= 18)
        );
        assert_eq!(
            analyzer.quantity_pronunciation("1日").unwrap().as_deref(),
            Some("イチニチ")
        );
        assert!(analyzer.contextual_reading_spans("1日").unwrap().is_empty());
    }
}

fn shifted(mut tokens: Vec<Token>, offset: usize) -> Vec<Token> {
    for token in &mut tokens {
        token.start += offset;
        token.end += offset;
    }
    tokens
}

fn assert_bounds(text: &str, tokens: &[Token]) {
    for token in tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
}

#[test]
fn standard_months_widths_and_years_keep_all_surrounding_fields() {
    let mut analyzer = fixture(false, None);
    for month in 1..=12 {
        let text = format!("{month}月1日");
        let tokens = analyzer.try_tokenize(&text).unwrap();
        assert_eq!(
            day(&tokens, "1日").pronunciation.as_ref(),
            "ツイタチ",
            "{text}"
        );
    }
    for whole_month in [false, true] {
        let mut analyzer = fixture(whole_month, None);
        for (text, control, raw) in [
            ("日付は11月1日です。", "日付は11月2日です。", "1日"),
            ("十一月一日", "十一月二日", "一日"),
            ("2024年11月１日", "2024年11月２日", "１日"),
            ("2024年１１月１日", "2024年１１月２日", "１日"),
            ("2024年十一月一日", "2024年十一月二日", "一日"),
            ("４月１日", "４月２日", "１日"),
            ("７月１日", "７月２日", "１日"),
            ("９月１日", "９月２日", "１日"),
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            // 全角のsplit月に既存の認可がない場合、新しい日規則で月を補わない。
            if !whole_month && text.contains(['１', '４', '７', '９']) && !text.contains("11月")
            {
                continue;
            }
            let first = day(&tokens, raw);
            assert_eq!(first.pronunciation.as_ref(), "ツイタチ");
            let previous = analyzer.try_tokenize(control).unwrap();
            let outer = |tokens: Vec<Token>| {
                tokens
                    .into_iter()
                    .filter(|t| t.end <= first.start || t.start >= first.end)
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                format!("{:?}", outer(tokens.clone())),
                format!("{:?}", outer(previous)),
                "{text}"
            );
            assert_bounds(text, &tokens);
            assert_eq!(
                analyzer.contextual_reading_spans(text).unwrap(),
                [ContextualReadingSpan {
                    start: first.start,
                    end: first.end,
                    pronunciation: "ツイタチ".into()
                }]
            );
            assert_eq!(analyzer.quantity_pronunciation(text).unwrap(), None);
        }
    }
}

#[test]
fn durations_names_identifiers_quotes_and_unrecognized_months_keep_the_split_day() {
    let mut analyzer = fixture(true, None);
    for text in [
        "1日",
        "一日かかる。",
        "11月1日間",
        "11月1日目",
        "11月1日かかった。",
        "11月1日 以内",
        "毎月1日",
        "11か月1日",
        "0月1日",
        "13月1日",
        "01月1日",
        "011月1日",
        "11月01日",
        "11月 1日",
        "11月\n1日",
        "11月1 日",
        "11月。1日",
        "11月がある。1日",
        "ID11月1日",
        "型番11月1日",
        "名前は11月1日",
        "作品名は11月1日",
        "11月1日という名前",
        "11月1日A",
        "11月1日Ａ",
        "11月1日_2",
        "11月1日ですA",
        "11月1日.A",
        "「11月1日」",
        "「前。11月1日です。」",
        "作品名は「前\n11月1日です。」",
        "「前。11月1日",
        "（前。11月1日。）",
        "四月一日さん",
        "02024年11月1日",
        "2０24年11月1日",
    ] {
        let tokens = analyzer.try_tokenize(text).unwrap();
        assert!(
            !tokens
                .iter()
                .any(|t| t.pronunciation.as_ref() == "ツイタチ"),
            "{text}: {tokens:?}"
        );
        assert!(
            analyzer.contextual_reading_spans(text).unwrap().is_empty(),
            "{text}"
        );
        assert_bounds(text, &tokens);
    }
    for raw in ["1日", "一日"] {
        let expected = analyzer.try_tokenize(raw).unwrap();
        for text in [
            format!("11月{raw}目"),
            format!("「11月{raw}」"),
            format!("作品名は11月{raw}"),
        ] {
            let start = text.find(raw).unwrap();
            let actual: Vec<_> = analyzer
                .try_tokenize(&text)
                .unwrap()
                .into_iter()
                .filter(|t| start <= t.start && t.end <= start + raw.len())
                .collect();
            assert_eq!(
                format!("{actual:?}"),
                format!("{:?}", shifted(expected.clone(), start)),
                "{text}"
            );
        }
    }
    assert_eq!(
        analyzer.quantity_pronunciation("1日").unwrap().as_deref(),
        Some("イチニチ")
    );
    let before = analyzer.quantity_spans("1日").unwrap();
    assert_eq!(before[0].pronunciation, "イチニチ");
    analyzer.try_tokenize("11月1日").unwrap();
    assert_eq!(analyzer.quantity_spans("1日").unwrap(), before);
}

#[test]
fn custom_day_and_month_fields_and_selected_whole_words_are_preserved() {
    for (surface, pos, r, p) in [
        ("1", "名詞,数,*,*", "イチ", "イチ"),
        ("日", "名詞,接尾,助数詞,*", "ニチ", "ニチ"),
        (
            "11月",
            "名詞,副詞可能,*,*",
            "ジュウイチガツ",
            "ジューイチガツ",
        ),
    ] {
        for field in 0..6 {
            let mut custom = entry(surface, pos, r, p, -30000);
            match field {
                0 => custom.pos = "名詞,固有名詞,人名,姓".into(),
                1 => custom.base_form = "独自原形".into(),
                2 => custom.reading = "ドクジヨミ".into(),
                3 => custom.pronunciation = "ドクジハツオン".into(),
                4 => custom.conj_type = "独自活用型".into(),
                _ => custom.conj_form = "独自活用形".into(),
            }
            let mut analyzer = fixture(true, Some(custom));
            let raw = analyzer.try_tokenize("11月1日").unwrap();
            assert!(
                !raw.iter().any(|t| t.surface.as_ref() == "1日"),
                "{surface}/{field}: {raw:?}"
            );
            assert_eq!(day(&raw, surface).word_cost, -30000);
            assert!(
                analyzer
                    .contextual_reading_spans("11月1日")
                    .unwrap()
                    .is_empty()
            );
            let reference = analyzer.try_tokenize(surface).unwrap();
            let actual = day(&raw, surface);
            let offset = actual.start;
            assert_eq!(
                format!("{actual:?}"),
                format!("{:?}", shifted(reference, offset)[0]),
                "{surface}/{field}"
            );
        }
    }
    for (surface, pos, r) in [
        ("1日", "名詞,副詞可能,*,*", "イチニチ"),
        ("1日", "名詞,副詞可能,*,*", "ツイタチ"),
        ("11月1日", "名詞,固有名詞,一般,*", "キネンビ"),
    ] {
        let mut analyzer = fixture(true, Some(entry(surface, pos, r, r, -30000)));
        let tokens = analyzer.try_tokenize("11月1日").unwrap();
        let selected = day(&tokens, surface);
        assert_eq!(selected.pronunciation.as_ref(), r);
        assert!(selected.is_known);
        assert_eq!(selected.word_cost, -30000);
        assert!(
            analyzer
                .contextual_reading_spans("11月1日")
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn repeated_dates_sentence_api_and_offset_append_keep_the_same_ranges() {
    let mut analyzer = fixture(true, None);
    let block = "「前。11月1日です。」。日付は11月1日です。";
    let text = block.repeat(64);
    for _ in 0..2 {
        let tokens = analyzer.try_tokenize(&text).unwrap();
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.surface.as_ref() == "1日")
                .count(),
            64
        );
        let spans = analyzer.contextual_reading_spans(&text).unwrap();
        assert_eq!(spans.len(), 64);
        for (i, span) in spans.iter().enumerate() {
            assert_eq!(span.start, i * block.len() + block.rfind("1日").unwrap());
        }
        let sentences: Vec<_> = analyzer
            .try_tokenize_sentences(&text, &Default::default())
            .unwrap()
            .into_iter()
            .flat_map(|(_, t)| t)
            .collect();
        assert_eq!(format!("{tokens:?}"), format!("{sentences:?}"));
        assert_bounds(&text, &tokens);
    }
    let mut output = analyzer.try_tokenize("名前").unwrap();
    let previous = format!("{output:?}");
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    workspace
        .tokenize_into("11月1日", analyzer.dictionary(), 71, &mut output)
        .unwrap();
    assert_eq!(format!("{:?}", &output[..1]), previous);
    assert_eq!(
        format!("{:?}", &output[1..]),
        format!(
            "{:?}",
            shifted(analyzer.try_tokenize("11月1日").unwrap(), 71)
        )
    );
    let mut suffix = Vec::new();
    workspace
        .tokenize_into("1日", analyzer.dictionary(), 71, &mut suffix)
        .unwrap();
    assert_eq!(suffix.len(), 2);
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_calendar_first_day_follows_month_without_changing_bare_duration() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for text in [
            "日付は11月1日です。",
            "2024年１１月１日",
            "十一月一日",
            "４月１日",
            "７月１日",
            "９月１日",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert!(
                tokens.iter().any(|t| t.pronunciation.ends_with("ツイタチ")),
                "{name}: {text}: {tokens:?}"
            );
        }
        for text in ["1日", "一日かかる。"] {
            assert!(
                !analyzer
                    .try_tokenize(text)
                    .unwrap()
                    .iter()
                    .any(|t| t.pronunciation.as_ref() == "ツイタチ"),
                "{name}: {text}"
            );
            assert!(analyzer.contextual_reading_spans(text).unwrap().is_empty());
        }
    }
}
