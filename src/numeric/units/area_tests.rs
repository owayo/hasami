use super::*;

#[test]
fn area_frames_cover_only_complete_supported_units_and_keep_byte_offsets() {
    for (unit, r, p) in [
        ("m2", "ヘイホウメートル", "ヘイホーメートル"),
        ("m²", "ヘイホウメートル", "ヘイホーメートル"),
        ("cm2", "ヘイホウセンチメートル", "ヘイホーセンチメートル"),
        ("cm²", "ヘイホウセンチメートル", "ヘイホーセンチメートル"),
        ("m3", "リッポウメートル", "リッポーメートル"),
        ("m³", "リッポウメートル", "リッポーメートル"),
    ] {
        assert_eq!(area_readings(unit), Some((r, p)));
        assert!(requires_frame(unit));
        let text = format!("面積1.25{unit}を測る。");
        let plan = super::super::AsciiPlan::new(&text, 31);
        let frames = plan.unit_frames_in(31, 31 + text.len());
        assert_eq!(frames.len(), 1, "{text}");
        assert_eq!(frames[0].number, 37..41);
        assert_eq!(frames[0].end, 41 + unit.len());
        assert_eq!(frames[0].reading, r);
        assert!(plan.unit_frames_in(38, 31 + text.len()).is_empty());
    }
    for text in [
        "A70m2",
        "A70m²",
        "70m2X",
        "70m²X",
        "70m²2",
        "70m²²",
        "ID 70m2",
        "型番70m2",
        "070m2",
        "00.75m2",
        "7,0m2",
        "7.0.1m2",
        "70m2+1",
        "70m²+1",
        "70m²/2",
        "70m2=1",
        "70m²=1",
        "70m²Ａ",
        "70m²１",
        "70m²×2",
        "70m²／2",
        "70M2",
        "70M²",
        "70ｍ2",
        "70ｍ²",
        "７０m2",
        "m2",
        "m²",
        "70mm2",
        "70cm3",
    ] {
        let plan = super::super::AsciiPlan::new(text, 0);
        assert!(plan.unit_frames_in(0, text.len()).is_empty(), "{text}");
    }
    assert_eq!(reading("km2"), Some("ヘイホウキロメートル"));
    assert_eq!(reading("ｍ2"), None);
    assert_eq!(reading("ｍ²"), None);
}

fn token(s: &str, start: usize, pos: &str, r: &str, p: &str, cost: i16) -> Token {
    Token {
        surface: s.into(),
        base_form: s.into(),
        start,
        end: start + s.len(),
        pos: pos.into(),
        conj_type: "".into(),
        conj_form: "".into(),
        reading: r.into(),
        pronunciation: p.into(),
        word_cost: cost,
        is_known: true,
    }
}

#[test]
fn empty_standard_unit_is_completed_without_changing_dictionary_metadata() {
    let mut parts = vec![
        token("7", 31, "名詞,数,*,*", "ナナ", "ナナ", 17),
        token("0", 32, "名詞,数,*,*", "ゼロ", "ゼロ", 10),
        token("m2", 33, "名詞,一般,*,*", "", "", 4557),
    ];
    let before = parts.clone();
    let plan = super::super::AsciiPlan::new("70m2", 31);
    coalesce_units("70m2", 31, &mut parts, 0, &plan);
    assert_eq!(format!("{:?}", &parts[..2]), format!("{:?}", &before[..2]));
    let mut expected = before[2].clone();
    expected.reading = "ヘイホウメートル".into();
    expected.pronunciation = "ヘイホーメートル".into();
    assert_eq!(format!("{:?}", parts[2]), format!("{expected:?}"));
    for defect in 0..7 {
        let mut bad = before.clone();
        match defect {
            0 => bad[2].base_form = "custom".into(),
            1 => bad[2].pos = "名詞,固有名詞,人名,一般".into(),
            2 => bad[2].conj_type = "custom".into(),
            3 => bad[2].conj_form = "基本形".into(),
            4 => bad[2].reading = "カスタム".into(),
            5 => bad[2].pronunciation = "カスタム".into(),
            _ => bad[0].base_form = "custom-number".into(),
        }
        let old = format!("{bad:?}");
        coalesce_units("70m2", 31, &mut bad, 0, &plan);
        assert_eq!(format!("{bad:?}"), old, "{defect}");
    }
}

#[cfg(feature = "build")]
mod analyzer_tests {
    use crate::Analyzer;
    use crate::dict::{DictBuilder, DictEntry};

    fn analyzer(whole: Option<&str>, custom: bool) -> Analyzer {
        let mut b = DictBuilder::new();
        for (i, r) in [
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
        ]
        .iter()
        .enumerate()
        {
            let s = i.to_string();
            b.add_entry(DictEntry {
                surface: s.clone().into(),
                base_form: s.into(),
                pos: "名詞,数,*,*".into(),
                reading: (*r).into(),
                pronunciation: (*r).into(),
                cost: 10 + i as i16,
                ..Default::default()
            });
        }
        for (s, pos, r, p, cost) in [
            ("m", "記号,アルファベット,*,*", "エム", "エム", 30),
            (
                "cm",
                "名詞,一般,*,*",
                "センチメートル",
                "センチメートル",
                40,
            ),
            (
                "m2",
                "名詞,一般,*,*",
                if custom { "カスタム" } else { "" },
                if custom { "カスタム" } else { "" },
                if custom { -100 } else { 300 },
            ),
            (
                "cm2",
                "名詞,一般,*,*",
                "ヘイホウセンチメートル",
                "ヘイホウセンチメートル",
                300,
            ),
            (
                "m3",
                "名詞,一般,*,*",
                "リッポウメートル",
                "リッポウメートル",
                300,
            ),
            (
                "70m",
                "名詞,固有名詞,一般,*",
                "ナナジュウメートル",
                "ナナジュウメートル",
                -20000,
            ),
            (".", "記号,一般,*,*", "．", "．", 50),
            (",", "記号,読点,*,*", "，", "，", 60),
        ] {
            b.add_entry(DictEntry {
                surface: s.into(),
                base_form: s.into(),
                pos: pos.into(),
                reading: r.into(),
                pronunciation: p.into(),
                cost,
                ..Default::default()
            });
        }
        if let Some(s) = whole {
            b.add_entry(DictEntry {
                surface: s.into(),
                base_form: "custom-whole".into(),
                pos: "名詞,固有名詞,一般,*".into(),
                reading: "カスタム".into(),
                pronunciation: "カスタム".into(),
                cost: -30000,
                ..Default::default()
            });
        }
        Analyzer::from_dict(b.build().unwrap())
    }

    #[test]
    fn actual_analyzer_recovers_area_and_volume_without_crossing_known_wholes() {
        let mut a = analyzer(None, false);
        for (text, nr, np, ur, up) in [
            (
                "70m2",
                "ナナジュウ",
                "ナナジュー",
                "ヘイホウメートル",
                "ヘイホーメートル",
            ),
            (
                "70m²",
                "ナナジュウ",
                "ナナジュー",
                "ヘイホウメートル",
                "ヘイホーメートル",
            ),
            (
                "70cm2",
                "ナナジュウ",
                "ナナジュー",
                "ヘイホウセンチメートル",
                "ヘイホーセンチメートル",
            ),
            (
                "70cm²",
                "ナナジュウ",
                "ナナジュー",
                "ヘイホウセンチメートル",
                "ヘイホーセンチメートル",
            ),
            (
                "70m3",
                "ナナジュウ",
                "ナナジュー",
                "リッポウメートル",
                "リッポーメートル",
            ),
            (
                "70m³",
                "ナナジュウ",
                "ナナジュー",
                "リッポウメートル",
                "リッポーメートル",
            ),
            (
                "1.25m2",
                "イチテンニゴ",
                "イチテンニゴ",
                "ヘイホウメートル",
                "ヘイホーメートル",
            ),
            (
                "1,200m3",
                "センニヒャク",
                "センニヒャク",
                "リッポウメートル",
                "リッポーメートル",
            ),
        ] {
            let out = a.try_tokenize(text).unwrap();
            assert_eq!(out.len(), 2, "{text}: {out:?}");
            assert_eq!(
                (out[0].reading.as_ref(), out[0].pronunciation.as_ref()),
                (nr, np)
            );
            assert_eq!(
                (out[1].reading.as_ref(), out[1].pronunciation.as_ref()),
                (ur, up)
            );
            assert_eq!(out[0].base_form, out[0].surface);
            assert_eq!(out[0].pos.as_ref(), "名詞,数,*,*");
            assert!(!out[0].is_known);
            assert_eq!(out[1].start, out[0].end);
            assert_eq!(out[1].end, text.len());
            assert_eq!(
                a.quantity_pronunciation(text).unwrap(),
                Some(format!("{np}{up}"))
            );
            let span = a.quantity_spans(text).unwrap();
            assert_eq!(span.len(), 1);
            assert_eq!((span[0].start, span[0].end), (0, text.len()));
            assert_eq!(span[0].pronunciation, format!("{np}{up}"));
        }
        assert_eq!(a.tokenize("70m2")[0].word_cost, 27);
        // 低コストのm+2が選ばれる辞書では、その二断片のコストだけを合計する。
        assert_eq!(a.tokenize("70m2")[1].word_cost, 42);
        assert!(!a.tokenize("70m2")[1].is_known);
        for text in ["70m2", "70m2装置"] {
            let mut a = analyzer(Some(text), false);
            let out = a.tokenize(text);
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].base_form.as_ref(), "custom-whole");
            assert_eq!(out[0].pronunciation.as_ref(), "カスタム");
            assert_eq!(out[0].word_cost, -30000);
            assert_eq!(a.quantity_pronunciation(text).unwrap(), None);
        }
        let mut a = analyzer(None, true);
        assert!(
            a.tokenize("70m2")
                .iter()
                .any(|t| t.pronunciation.as_ref() == "カスタム")
        );
        assert_eq!(a.quantity_pronunciation("70m2").unwrap(), None);
    }
}
