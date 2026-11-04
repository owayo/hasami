//! 数量が文中に繰り返し現れる場合の読みと、元入力の境界の受け入れ検査。
#![cfg(feature = "analyzer")]

use hasami::{Analyzer, Token};

fn assert_reading(text: &str, tokens: &[Token], surface: &str, reading: &str, speech: &str) {
    let start = text.find(surface).unwrap();
    let end = start + surface.len();
    let focal: Vec<_> = tokens
        .iter()
        .filter(|t| t.start < end && start < t.end)
        .collect();
    assert_eq!(
        focal.first().map(|t| t.start),
        Some(start),
        "{text}: {tokens:?}"
    );
    assert_eq!(focal.last().map(|t| t.end), Some(end), "{text}: {tokens:?}");
    assert_eq!(
        focal.iter().map(|t| &*t.reading).collect::<String>(),
        reading,
        "{text}: {tokens:?}"
    );
    assert_eq!(
        focal.iter().map(|t| &*t.pronunciation).collect::<String>(),
        speech,
        "{text}: {tokens:?}"
    );
    assert!(focal.windows(2).all(|p| p[0].end == p[1].start));
    for token in tokens {
        assert_eq!(
            text.get(token.start..token.end),
            Some(token.surface.as_ref())
        );
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_annual_frequencies_keep_noun_grammar_and_modifier_readings() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for text in [
                "年2回",
                "年十二冊",
                "年0回",
                "回数は年2回だ。冊数も年十二冊だ。",
            ] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                for (quantity, reading, speech) in [
                    ("年2回", "ネンニカイ", "ネンニカイ"),
                    ("年十二冊", "ネンジュウニサツ", "ネンジューニサツ"),
                    ("年0回", "ネンゼロカイ", "ネンゼロカイ"),
                ] {
                    if text.contains(quantity) {
                        assert_reading(text, &tokens, quantity, reading, speech);
                    }
                }
                for year in tokens.iter().filter(|t| t.surface.as_ref() == "年") {
                    assert_eq!(year.pos.as_ref(), "名詞,一般,*,*");
                    assert_eq!(year.base_form.as_ref(), "年");
                    assert!(year.is_known);
                    assert!(year.conj_type.is_empty() && year.conj_form.is_empty());
                    assert!(
                        analyzer
                            .dictionary()
                            .lookup("年")
                            .unwrap()
                            .iter()
                            .any(|(end, rows)| *end == "年".len()
                                && rows.iter().any(|row| row.pos == year.pos
                                    && row.base_form == year.base_form
                                    && row.cost == year.word_cost
                                    && matches!(row.reading.as_ref(), "トシ" | "ネン")
                                    && row.pronunciation == row.reading
                                    && matches!(row.conj_type.as_ref(), "" | "*")
                                    && matches!(row.conj_form.as_ref(), "" | "*"))),
                        "{name}: original year features missing: {year:?}"
                    );
                }
            }
            for text in ["その年2冊を読んだ。", "ある年2回開催した。"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert_reading(text, &tokens, "年", "トシ", "トシ");
                let year = tokens.iter().find(|t| t.surface.as_ref() == "年").unwrap();
                assert_eq!(year.pos.as_ref(), "名詞,一般,*,*");
            }
            for (text, compound, speech) in [
                ("前年2回", "前年", "ゼンネン"),
                ("年末2回", "年末", "ネンマツ"),
            ] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert_reading(text, &tokens, compound, speech, speech);
                assert!(
                    tokens
                        .iter()
                        .any(|t| t.is_known && t.surface.as_ref() == compound)
                );
            }
            let tokens = analyzer.try_tokenize("2000年2月").unwrap();
            let year = tokens.iter().find(|t| t.surface.as_ref() == "年").unwrap();
            assert_eq!(year.pos.as_ref(), "名詞,接尾,助数詞,*");
            assert_eq!(year.pronunciation.as_ref(), "ネン");
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_horsepower_has_katakana_units_and_complete_scalars_in_context() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        let standalone = analyzer.try_tokenize("hp").unwrap();
        assert_reading("hp", &standalone, "hp", "エイチピー", "エイチピー");
        for _ in 0..2 {
            for text in ["12hp", "6.4hp", "出力は12hp。別案は6.4hp。"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                for (number, reading, speech) in [
                    ("12", "ジュウニバリキ", "ジューニバリキ"),
                    ("6.4", "ロクテンヨンバリキ", "ロクテンヨンバリキ"),
                ] {
                    let quantity = format!("{number}hp");
                    if !text.contains(&quantity) {
                        continue;
                    }
                    assert_reading(text, &tokens, &quantity, reading, speech);
                    let start = text.find(&quantity).unwrap();
                    let scalar = tokens.iter().find(|t| t.start == start).unwrap();
                    assert_eq!(scalar.surface.as_ref(), number);
                    assert_eq!(scalar.base_form.as_ref(), number);
                    assert_eq!(scalar.pos.as_ref(), "名詞,数,*,*");
                    assert!(!scalar.is_known);
                    let unit = tokens.iter().find(|t| t.start == scalar.end).unwrap();
                    assert_eq!(unit.surface.as_ref(), "hp");
                    assert_eq!(unit.end, start + quantity.len());
                    assert_eq!(unit.reading.as_ref(), "バリキ");
                    assert_eq!(unit.pronunciation.as_ref(), "バリキ");
                    assert!(unit.conj_type.is_empty() && unit.conj_form.is_empty());
                    assert_eq!(
                        analyzer
                            .quantity_pronunciation(&quantity)
                            .unwrap()
                            .as_deref(),
                        Some(speech)
                    );
                }
            }
            for text in ["ID12hp", "12hpX", "１２hp"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert!(
                    tokens
                        .iter()
                        .all(|t| t.surface.as_ref() != "12" || t.is_known),
                    "{name}: {text}: {tokens:?}"
                );
            }
            assert_eq!(
                format!("{standalone:?}"),
                format!("{:?}", analyzer.try_tokenize("hp").unwrap())
            );
        }
    }
}

#[test]
#[ignore = "配布辞書2種が必要"]
fn distributed_horse_lengths_keep_half_in_all_thirty_registered_numbers() {
    const NUMBERS: [&str; 30] = [
        "イチ",
        "ニ",
        "サン",
        "ヨン",
        "ゴ",
        "ロク",
        "ナナ",
        "ハチ",
        "キュウ",
        "ジュウ",
        "ジュウイチ",
        "ジュウニ",
        "ジュウサン",
        "ジュウヨン",
        "ジュウゴ",
        "ジュウロク",
        "ジュウナナ",
        "ジュウハチ",
        "ジュウキュウ",
        "ニジュウ",
        "ニジュウイチ",
        "ニジュウニ",
        "ニジュウサン",
        "ニジュウヨン",
        "ニジュウゴ",
        "ニジュウロク",
        "ニジュウナナ",
        "ニジュウハチ",
        "ニジュウキュウ",
        "サンジュウ",
    ];
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (index, number) in NUMBERS.into_iter().enumerate() {
            for suffix in ["", "差"] {
                let quantity = format!("{}馬身半{suffix}", index + 1);
                let old_reading = format!("{number}バシンサ");
                for (end, rows) in analyzer.dictionary().lookup(&quantity).unwrap() {
                    if end == quantity.len() {
                        assert!(
                            rows.iter()
                                .all(|row| !(row.pos.as_ref() == "名詞,固有名詞,一般,*"
                                    && row.reading.as_ref() == old_reading
                                    && row.pronunciation.as_ref() == old_reading)),
                            "{name}: {quantity}: half-less registration remains"
                        );
                    }
                }
                let reading = format!(
                    "{number}バシンハン{}",
                    if suffix.is_empty() { "" } else { "サ" }
                );
                let speech = reading
                    .replace("ジュウ", "ジュー")
                    .replace("キュウ", "キュー");
                for text in [quantity.clone(), format!("着差は{quantity}で確定した。")] {
                    let tokens = analyzer.try_tokenize(&text).unwrap();
                    let start = text.find(&quantity).unwrap();
                    let end = start + quantity.len();
                    let actual_speech = tokens
                        .iter()
                        .filter(|t| start <= t.start && t.end <= end)
                        .map(|t| &*t.pronunciation)
                        .collect::<String>();
                    // 既知の複合数量は長音を「ウ」で書く辞書の発音も保持する。
                    let expected_speech = if actual_speech == reading {
                        &reading
                    } else {
                        &speech
                    };
                    assert_reading(&text, &tokens, &quantity, &reading, expected_speech);
                }
            }
        }
        let text = "差は二馬身半差。次は2馬身差。別案は2馬身。差は2.5馬身。最後は31馬身半。";
        let tokens = analyzer.try_tokenize(text).unwrap();
        for (quantity, reading, speech) in [
            ("二馬身半差", "ニバシンハンサ", "ニバシンハンサ"),
            ("2馬身差", "ニバシンサ", "ニバシンサ"),
            ("2馬身。", "ニバシン。", "ニバシン。"),
            ("2.5馬身", "ニテンゴバシン", "ニテンゴバシン"),
            (
                "31馬身半",
                "サンジュウイチバシンハン",
                "サンジューイチバシンハン",
            ),
        ] {
            assert_reading(text, &tokens, quantity, reading, speech);
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_unknown_ascii_units_are_not_organizations_in_quantities() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for text in ["75kW", "出力は75kW。幅は64bit。速度は4.5mph。"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert_reading(
                    text,
                    &tokens,
                    "75kW",
                    "ナナジュウゴキロワット",
                    "ナナジューゴキロワット",
                );
                let unit = tokens.iter().find(|t| t.surface.as_ref() == "kW").unwrap();
                assert!(!unit.is_known, "{name}: {text}: {tokens:?}");
                assert!(!unit.pos.contains(",組織,"), "{name}: {text}: {tokens:?}");
                assert_eq!(unit.base_form.as_ref(), "kW");
                assert!(unit.conj_type.is_empty() && unit.conj_form.is_empty());
                for unit in tokens
                    .iter()
                    .filter(|t| !t.is_known && matches!(t.surface.as_ref(), "bit" | "mph"))
                {
                    assert!(!unit.pos.contains(",組織,"), "{name}: {text}: {tokens:?}");
                }
            }
            for (text, speech) in [
                ("10cm", "ジュッセンチメートル"),
                ("170cm", "ヒャクナナジュッセンチメートル"),
                ("6kg", "ロッキログラム"),
            ] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                // 一語の既知語や既知の単位は辞書の読みを保つ。未知の単位の経路を検査する。
                if tokens
                    .last()
                    .is_some_and(|t| !t.is_known && matches!(t.surface.as_ref(), "cm" | "kg"))
                {
                    assert_reading(text, &tokens, text, speech, speech);
                    assert_eq!(
                        analyzer.quantity_pronunciation(text).unwrap().as_deref(),
                        Some(speech)
                    );
                }
            }
            for text in ["ID75kW", "型番75kW", "75kWX", "75kW_2", "75kWＡ", "75kW－2"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert!(
                    tokens
                        .iter()
                        .filter(|t| !t.is_known && t.surface.as_ref() == "kW")
                        .all(|t| t.pos.as_ref() != "名詞,接尾,助数詞,*"),
                    "{name}: {text}: {tokens:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_ascii_units_keep_complete_unit_boundaries_and_scalar_readings() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            let text = "幅は64bit。速さは37mph。速度は4.5mph。熱量は1,500kcal。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (number, unit, reading, speech) in [
                ("64", "bit", "ロクジュウヨンビット", "ロクジューヨンビット"),
                (
                    "37",
                    "mph",
                    "サンジュウナナエムピーエイチ",
                    "サンジューナナエムピーエイチ",
                ),
                (
                    "4.5",
                    "mph",
                    "ヨンテンゴエムピーエイチ",
                    "ヨンテンゴエムピーエイチ",
                ),
                (
                    "1,500",
                    "kcal",
                    "センゴヒャクキロカロリー",
                    "センゴヒャクキロカロリー",
                ),
            ] {
                let surface = format!("{number}{unit}");
                assert_reading(text, &tokens, &surface, reading, speech);
                let start = text.find(&surface).unwrap();
                let scalar = tokens.iter().find(|t| t.start == start).unwrap();
                assert_eq!(scalar.surface.as_ref(), number);
                assert_eq!(scalar.base_form.as_ref(), number);
                assert_eq!(scalar.pos.as_ref(), "名詞,数,*,*");
                assert!(!scalar.is_known);
                let suffix = tokens.iter().find(|t| t.start == scalar.end).unwrap();
                assert_eq!(suffix.surface.as_ref(), unit);
                assert_eq!(suffix.end, start + surface.len());
            }
            for (text, number, reading) in [
                ("ID64bit", "64", "ロクジュウヨン"),
                ("64bitX", "64", "ロクジュウヨン"),
                ("37mph_2", "37", "サンジュウナナ"),
                ("６４bit", "６４", "ロクジュウヨン"),
                ("64ｂｉｔ", "64", "ロクジュウヨン"),
                ("1,500kcalX", "1,500", "センゴヒャク"),
            ] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert!(
                    tokens.iter().all(|t| t.surface.as_ref() != number
                        || t.is_known
                        || t.reading.as_ref() != reading),
                    "{name}: {text}: {tokens:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_zero_digits_before_large_units_keep_positional_readings_in_sentences() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            let text = "予算は一〇〇〇万円。次は二〇億円だ。別案は一零零零万円だ。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading, speech) in [
                ("一〇〇〇万円", "センマンエン", "センマンエン"),
                ("二〇億円", "ニジュウオクエン", "ニジューオクエン"),
                ("一零零零万円", "センマンエン", "センマンエン"),
            ] {
                assert_reading(text, &tokens, surface, reading, speech);
                let raw = surface.strip_suffix('円').unwrap();
                let number = tokens.iter().find(|t| t.surface.as_ref() == raw).unwrap();
                assert_eq!(number.base_form.as_ref(), raw);
                assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
                assert!(!number.is_known);
                assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
            }
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_terminal_four_years_keep_the_reading_before_calendar_months() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            for text in [
                "開始は2024年11月1日です。次は2024年11月です。",
                "開始は2024年１１月１日です。",
                "開始は2024年。次は二〇二四年十一月です。",
            ] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                let speech = tokens
                    .iter()
                    .find(|t| t.is_known && t.surface.as_ref() == "2024年")
                    .map_or("ニセンニジューヨネン", |t| {
                        t.pronunciation.as_ref()
                    });
                // 一語の既知語は辞書の発音を保つ。数詞から合成した年は長音の発音を使う。
                assert!(matches!(
                    speech,
                    "ニセンニジュウヨネン" | "ニセンニジューヨネン"
                ));
                assert_reading(text, &tokens, "2024年", "ニセンニジュウヨネン", speech);
                if text.contains("二〇二四年") {
                    assert_reading(
                        text,
                        &tokens,
                        "二〇二四年",
                        "ニセンニジュウヨネン",
                        "ニセンニジューヨネン",
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "配布辞書2種が必要"]
fn distributed_counter_sentences_keep_tens_distinct_from_six_hundred_and_thousand() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            let text = "建物は10階で、資料は10冊ある。靴は10足で、年齢は10歳だ。年齢は10才だ。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading) in [
                ("10階", "ジュッカイ"),
                ("10冊", "ジュッサツ"),
                ("10足", "ジュッソク"),
                ("10歳", "ジュッサイ"),
                ("10才", "ジュッサイ"),
            ] {
                assert_reading(text, &tokens, surface, reading, reading);
            }
            let text = "資料は100冊で、靴は1000足ある。6冊と6足を数える。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading) in [
                ("100冊", "ヒャクサツ"),
                ("1000足", "センソク"),
                ("6冊", "ロクサツ"),
                ("6足", "ロクソク"),
            ] {
                assert_reading(text, &tokens, surface, reading, reading);
            }
            let text = "10.6冊とID10冊を区別する。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_reading(
                text,
                &tokens,
                "10.6冊",
                "ジュウテンロクサツ",
                "ジューテンロクサツ",
            );
            let id_start = text.find("ID10冊").unwrap();
            assert!(
                tokens
                    .iter()
                    .filter(|t| id_start <= t.start)
                    .all(|t| t.reading.as_ref() != "ジュッ")
            );
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_fullwidth_quantities_keep_original_surfaces_and_identifier_guards() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            let text = "年齢は１８歳で、回数は２０回だ。次は２４歳だ。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading, speech) in [
                ("１８歳", "ジュウハッサイ", "ジューハッサイ"),
                ("２０回", "ニジュッカイ", "ニジュッカイ"),
                ("２４歳", "ニジュウヨンサイ", "ニジューヨンサイ"),
            ] {
                assert_reading(text, &tokens, surface, reading, speech);
                let number = surface.trim_end_matches(['歳', '回']);
                let token = tokens
                    .iter()
                    .find(|t| t.surface.as_ref() == number)
                    .unwrap();
                assert_eq!(token.base_form.as_ref(), number);
                assert_eq!(token.pos.as_ref(), "名詞,数,*,*");
                assert!(!token.is_known);
                assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
            }
            // 数量ではない入力を挟んでも、次の呼出しへ保護範囲を持ち越さない。
            for text in ["０１８歳", "ID１８歳", "１８歳A", "１８", "1８歳", "一８歳"]
            {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert!(
                    tokens
                        .iter()
                        .all(|t| !matches!(t.surface.as_ref(), "１８" | "０１８" | "1８" | "一８")),
                    "{name}: {text}: {tokens:?}"
                );
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_eleven_month_keeps_known_dates_quotes_and_name_contexts() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for text in [
            "一一月",
            "締切は一一月です。",
            "確認した。締切は一一月です。",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_reading(text, &tokens, "一一月", "ジュウイチガツ", "ジューイチガツ");
        }
        for text in ["「一一月」という識別子。", "名前は一一月。", "ID一一月"]
        {
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_reading(text, &tokens, "一一月", "イチイチガツ", "イチイチガツ");
        }
        let text = "締切は十一月です。";
        let tokens = analyzer.try_tokenize(text).unwrap();
        assert_reading(text, &tokens, "十一月", "ジュウイチガツ", "ジューイチガツ");
        if name != "ipadic" {
            // 既知の「一月一日」を切り直さず、日付の読みを含めて保持する。
            let text = "発売は一一月一日です。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_reading(
                text,
                &tokens,
                "一一月一日",
                "ジュウイチガツツイタチ",
                "ジューイチガツツイタチ",
            );
            let day_word = tokens
                .iter()
                .find(|t| t.surface.as_ref() == "一月一日")
                .unwrap();
            assert!(day_word.is_known);
            assert_eq!(day_word.reading.as_ref(), "イチガツツイタチ");
            assert_eq!(day_word.pronunciation.as_ref(), "イチガツツイタチ");
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_finite_kanji_pairs_keep_approximate_days_and_identifier_contexts() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for _ in 0..2 {
            let text = "期間は一八か月で、年齢は二一歳だ。一一歳も記録した。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            for (surface, reading, speech) in [
                ("一八か月", "ジュウハッカゲツ", "ジューハッカゲツ"),
                ("二一歳", "ニジュウイッサイ", "ニジューイッサイ"),
                ("一一歳", "ジュウイッサイ", "ジューイッサイ"),
            ] {
                assert_reading(text, &tokens, surface, reading, speech);
            }
            let text = "二三日で終わる。";
            let tokens = analyzer.try_tokenize(text).unwrap();
            assert_reading(text, &tokens, "二三日", "ニサンニチ", "ニサンニチ");
            for text in ["ID二一歳", "二一歳A", "二一", "〇一歳", "1一歳"] {
                let tokens = analyzer.try_tokenize(text).unwrap();
                assert!(
                    tokens.iter().all(|t| {
                        t.is_known
                            || t.pos.as_ref() != "名詞,数,*,*"
                            || !matches!(t.surface.as_ref(), "二一" | "〇一" | "1一")
                    }),
                    "{name}: {text}: {tokens:?}"
                );
                for token in &tokens {
                    assert_eq!(
                        text.get(token.start..token.end),
                        Some(token.surface.as_ref())
                    );
                }
            }
        }
    }
}
