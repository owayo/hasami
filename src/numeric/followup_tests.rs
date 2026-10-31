use super::*;

fn selected(surface: &str, start: usize, pos: &str, r: &str, p: &str) -> Token {
    Token {
        surface: surface.into(),
        base_form: surface.into(),
        start,
        end: start + surface.len(),
        pos: pos.into(),
        reading: r.into(),
        pronunciation: p.into(),
        conj_type: "".into(),
        conj_form: "".into(),
        word_cost: 123,
        is_known: true,
    }
}

fn chars(text: &str, offset: usize) -> Vec<Token> {
    text.char_indices()
        .map(|(i, c)| {
            let s = c.to_string();
            let (pos, r, p) = match c {
                '万' => ("名詞,数,*,*", "マン", "マン"),
                '億' => ("名詞,数,*,*", "オク", "オク"),
                '円' => ("名詞,接尾,助数詞,*", "エン", "エン"),
                c if digit(c).is_some() => {
                    let r = numeral_glyph_readings(None, c, None)
                        .first()
                        .copied()
                        .unwrap_or(
                            [
                                "ゼロ",
                                "イチ",
                                "ニ",
                                "サン",
                                "ヨン",
                                "ゴ",
                                "ロク",
                                "ナナ",
                                "ハチ",
                                "キュウ",
                            ][digit(c).unwrap() as usize],
                        );
                    ("名詞,数,*,*", r, r)
                }
                _ => ("記号,一般,*,*", "", ""),
            };
            selected(&s, offset + i, pos, r, p)
        })
        .collect()
}

#[test]
fn zero_scale_composition_keeps_utf8_metadata_and_cost_and_rejects_changed_guards() {
    for (text, expected) in [
        ("一〇〇〇万円", "センマン"),
        ("四零零零万円", "ヨンセンマン"),
        ("二〇億円", "ニジューオク"),
    ] {
        let mut ts = chars(text, 83);
        let suffix = ts.last().unwrap().clone();
        let count = ts.len() - 1;
        kanji::apply(text, 83, &mut ts, 0, &AsciiPlan::new(text, 83), &[], |_| {
            false
        });
        assert_eq!(ts.len(), 2, "{text}");
        assert_eq!(ts[0].pronunciation.as_ref(), expected);
        assert_eq!(ts[0].base_form, ts[0].surface);
        assert_eq!((ts[0].start, ts[0].end), (83, 83 + text.len() - 3));
        assert_eq!(ts[0].word_cost, 123 * count as i16);
        assert!(!ts[0].is_known);
        assert_eq!(format!("{:?}", ts[1]), format!("{suffix:?}"));
    }
    let text = "一〇〇〇万円";
    for change in 0..6 {
        let mut ts = chars(text, 0);
        match change {
            0 => ts[0].base_form = "氏名".into(),
            1 => ts[0].reading = "ヒト".into(),
            2 => ts[0].conj_type = "custom".into(),
            3 => ts[4].reading = "ヨロズ".into(),
            4 => ts[5].reading = "マル".into(),
            _ => ts[5].is_known = false,
        }
        let before = format!("{ts:?}");
        kanji::apply(text, 0, &mut ts, 0, &AsciiPlan::new(text, 0), &[], |_| {
            false
        });
        assert_eq!(format!("{ts:?}"), before, "guard {change}");
    }
    for text in [
        "〇一〇万円",
        "一二万円",
        "一二三万円",
        "一0〇万円",
        "ID 一〇万円",
        "コード一〇万円",
        "第一〇万円",
        "「一〇万円」",
        "名称は一〇万円",
    ] {
        let mut ts = chars(text, 0);
        let before = format!("{ts:?}");
        kanji::apply(text, 0, &mut ts, 0, &AsciiPlan::new(text, 0), &[], |_| {
            false
        });
        assert_eq!(format!("{ts:?}"), before, "{text}");
    }
}

#[test]
fn zero_scale_global_tail_protection_and_known_whole_survive_chunks() {
    let mut bare = chars("四〇〇〇万", 53);
    kanji::apply(
        "四〇〇〇万",
        53,
        &mut bare,
        0,
        &AsciiPlan::new("四〇〇〇万", 53),
        &[],
        |_| false,
    );
    assert_eq!(bare.len(), 1);
    assert_eq!(bare[0].pronunciation.as_ref(), "ヨンセンマン");
    let mut cropped = chars("四〇〇〇万", 53);
    let before = format!("{cropped:?}");
    kanji::apply(
        "四〇〇〇万",
        53,
        &mut cropped,
        0,
        &AsciiPlan::new("四〇〇〇万円", 53),
        &[],
        |_| false,
    );
    assert_eq!(format!("{cropped:?}"), before);
    let full = "一〇〇〇万円A";
    let part = "一〇〇〇万円";
    let mut ts = chars(part, 53);
    let before = format!("{ts:?}");
    kanji::apply(part, 53, &mut ts, 0, &AsciiPlan::new(full, 53), &[], |_| {
        false
    });
    assert_eq!(format!("{ts:?}"), before);
    let mut ts = vec![
        selected("一〇万", 53, "名詞,数,*,*", "イチレイマン", "イチレイマン"),
        selected("円", 62, "名詞,接尾,助数詞,*", "エン", "エン"),
    ];
    let before = format!("{ts:?}");
    kanji::apply(
        "一〇万円",
        53,
        &mut ts,
        0,
        &AsciiPlan::new("一〇万円", 53),
        &[],
        |_| false,
    );
    assert_eq!(format!("{ts:?}"), before);
    let mut ts = chars(part, 53);
    let before = format!("{ts:?}");
    let protection = 53..56;
    kanji::apply(
        part,
        53,
        &mut ts,
        0,
        &AsciiPlan::new(part, 53),
        std::slice::from_ref(&protection),
        |_| false,
    );
    assert_eq!(format!("{ts:?}"), before);
}

#[test]
fn minute_span_reuses_minute_euphony_without_touching_fraction_or_other_fields() {
    for (raw, expected) in [
        ("1", "イップンカン"),
        ("2", "ニフンカン"),
        ("3", "サンプンカン"),
        ("4", "ヨンプンカン"),
        ("6", "ロップンカン"),
        ("8", "ハップンカン"),
        ("10", "ジュップンカン"),
        ("20", "ニジュップンカン"),
        ("30", "サンジュップンカン"),
        ("1000", "センプンカン"),
    ] {
        let (r, p) = ascii::integer_readings(raw.parse().unwrap());
        let mut ts = vec![
            selected(raw, 41, "名詞,数,*,*", &r, &p),
            selected(
                "分間",
                41 + raw.len(),
                "名詞,接尾,助数詞,*",
                "フンカン",
                "フンカン",
            ),
        ];
        let before = ts.clone();
        apply_counter_readings(&mut ts);
        assert_eq!(
            ts.iter()
                .map(|t| t.pronunciation.as_ref())
                .collect::<String>(),
            expected,
            "{raw}"
        );
        for (after, mut baseline) in ts.iter().zip(before) {
            baseline.reading = Arc::clone(&after.reading);
            baseline.pronunciation = Arc::clone(&after.pronunciation);
            assert_eq!(format!("{after:?}"), format!("{baseline:?}"));
        }
    }
    let mut ts = vec![
        selected("10", 0, "名詞,数,*,*", "ジュウ", "ジュー"),
        selected("分の", 2, "名詞,接尾,助数詞,*", "ブンノ", "ブンノ"),
        selected("1", 8, "名詞,数,*,*", "イチ", "イチ"),
    ];
    let before = format!("{ts:?}");
    apply_counter_readings(&mut ts);
    assert_eq!(format!("{ts:?}"), before);
    let mut ts = vec![
        selected("10", 0, "名詞,数,*,*", "ジュウ", "ジュー"),
        selected("分間", 2, "名詞,接尾,助数詞,*", "フンカン", "フンカン"),
    ];
    let before = format!("{ts:?}");
    let protection = 0..8;
    apply_counter_readings_with_protection(&mut ts, std::slice::from_ref(&protection));
    assert_eq!(format!("{ts:?}"), before);
}

#[cfg(feature = "build")]
#[test]
fn calendar_zero_month_requires_a_selected_standard_year_and_unquoted_global_frame() {
    use crate::{
        Analyzer,
        dict::{DictBuilder, DictEntry},
    };
    let mut b = DictBuilder::new();
    for c in "〇零一二三四五".chars() {
        let s = c.to_string();
        let r = numeral_glyph_readings(None, c, None)[0];
        b.add_entry(DictEntry {
            surface: s.clone().into(),
            base_form: s.into(),
            pos: "名詞,数,*,*".into(),
            reading: r.into(),
            pronunciation: r.into(),
            ..Default::default()
        });
    }
    for (s, pos, r) in [
        ("年", "名詞,接尾,助数詞,*", "ネン"),
        ("月", "名詞,一般,*,*", "ツキ"),
    ] {
        b.add_entry(DictEntry {
            surface: s.into(),
            base_form: s.into(),
            pos: pos.into(),
            reading: r.into(),
            pronunciation: r.into(),
            ..Default::default()
        });
    }
    let mut a = Analyzer::from_dict(b.build().unwrap());
    for (text, expected) in [
        ("二〇三五年一〇月", "ニセンサンジューゴネンジューガツ"),
        ("二〇三四年一零月", "ニセンサンジューヨネンジューガツ"),
    ] {
        let ts = a.tokenize(text);
        assert_eq!(
            ts.iter()
                .map(|t| t.pronunciation.as_ref())
                .collect::<String>(),
            expected
        );
        assert_eq!(
            ts.iter().map(|t| t.surface.as_ref()).collect::<String>(),
            text
        );
    }
    for text in [
        "一〇月",
        "二〇三五年一〇月間",
        "二〇三五年一〇月A",
        "ID 二〇三五年一〇月",
        "「二〇三五年一〇月」",
        "二〇三五年一〇月さん",
    ] {
        let ts = a.tokenize(text);
        assert!(
            !ts.iter()
                .any(|t| t.surface.as_ref() == "月" && t.pronunciation.as_ref() == "ガツ"),
            "{text}"
        );
    }
}

#[test]
fn hand_counter_reuses_soft_only_for_a_whole_standard_number_and_known_te_suffix() {
    for (raw, expected) in [
        ("1", "イッテ"),
        ("3", "サンテ"),
        ("8", "ハッテ"),
        ("10", "ジュッテ"),
        ("18", "ジュウハッテ"),
        ("21", "ニジュウイッテ"),
    ] {
        let (r, p) = ascii::integer_readings(raw.parse().unwrap());
        let mut ts = vec![
            selected(raw, 41, "名詞,数,*,*", &r, &p),
            selected("手", 41 + raw.len(), "名詞,接尾,助数詞,*", "テ", "テ"),
        ];
        let before = ts.clone();
        apply_counter_readings(&mut ts);
        assert_eq!(
            ts.iter().map(|t| t.reading.as_ref()).collect::<String>(),
            expected
        );
        for (actual, mut old) in ts.iter().zip(before) {
            old.reading = actual.reading.clone();
            old.pronunciation = actual.pronunciation.clone();
            assert_eq!(format!("{actual:?}"), format!("{old:?}"));
        }
    }
    let mut ts = vec![
        selected("十", 0, "名詞,数,*,*", "ジュウ", "ジュー"),
        selected("八", 3, "名詞,数,*,*", "ハチ", "ハチ"),
        selected("手", 6, "名詞,接尾,助数詞,*", "テ", "テ"),
    ];
    apply_counter_readings(&mut ts);
    assert_eq!(
        ts.iter()
            .map(|t| t.pronunciation.as_ref())
            .collect::<String>(),
        "ジューハッテ"
    );
    for raw in ["１８", "1８", "一八", "七八"] {
        let mut ts = chars(raw, 41);
        ts.push(selected(
            "手",
            41 + raw.len(),
            "名詞,接尾,助数詞,*",
            "テ",
            "テ",
        ));
        let before = format!("{ts:?}");
        apply_counter_readings(&mut ts);
        assert_eq!(format!("{ts:?}"), before, "{raw}");
    }
}

#[test]
fn hand_counter_preserves_nonstandard_metadata_known_words_decimals_and_ids() {
    for change in 0..11 {
        let mut ts = vec![
            selected("1", 0, "名詞,数,*,*", "イチ", "イチ"),
            selected("手", 1, "名詞,接尾,助数詞,*", "テ", "テ"),
        ];
        match change {
            0 => ts[1].is_known = false,
            1 => ts[1].reading = "シュ".into(),
            2 => ts[1].pronunciation = "シュ".into(),
            3 => ts[1].base_form = "custom".into(),
            4 => ts[1].pos = "名詞,一般,*,*".into(),
            5 => ts[1].conj_type = "custom".into(),
            6 => ts[1].conj_form = "custom".into(),
            7 => ts[0].base_form = "氏名".into(),
            8 => ts[0].reading = "ヒト".into(),
            9 => ts[0].conj_type = "custom".into(),
            _ => ts[0].conj_form = "custom".into(),
        }
        let before = format!("{ts:?}");
        apply_counter_readings(&mut ts);
        assert_eq!(format!("{ts:?}"), before, "guard {change}");
    }
    for raw in ["1.5", "0.8", "0"] {
        let (r, p) = ascii::readings(raw, ascii::parse(raw).unwrap());
        let mut ts = vec![
            selected(raw, 0, "名詞,数,*,*", &r, &p),
            selected("手", raw.len(), "名詞,接尾,助数詞,*", "テ", "テ"),
        ];
        let before = format!("{ts:?}");
        apply_counter_readings(&mut ts);
        assert_eq!(format!("{ts:?}"), before, "{raw}");
    }
    let mut ts = vec![selected(
        "三手",
        0,
        "名詞,固有名詞,人名,姓",
        "ミツデ",
        "ミツデ",
    )];
    let before = format!("{ts:?}");
    apply_counter_readings(&mut ts);
    assert_eq!(format!("{ts:?}"), before);
    let mut ts = vec![
        selected("1", 0, "名詞,数,*,*", "イチ", "イチ"),
        selected("手", 1, "名詞,接尾,助数詞,*", "テ", "テ"),
    ];
    let protection = 0..4;
    let before = format!("{ts:?}");
    apply_counter_readings_with_protection(&mut ts, std::slice::from_ref(&protection));
    assert_eq!(format!("{ts:?}"), before);
}
