//! 空白を挟む暦月の読みと、原入力のバイト範囲。
#![cfg(feature = "build")]

use hasami::dict::{DictBuilder, DictEntry};
use hasami::{Analyzer, Dictionary, Token};

fn dictionary() -> Dictionary {
    let mut builder = DictBuilder::new();
    for (glyphs, reading, pronunciation) in [
        ("0０〇", "ゼロ", "ゼロ"),
        ("1１一", "イチ", "イチ"),
        ("2２二", "ニ", "ニ"),
        ("3３三", "サン", "サン"),
        ("4４四", "ヨン", "ヨン"),
        ("5５五", "ゴ", "ゴ"),
        ("6６六", "ロク", "ロク"),
        ("7７七", "ナナ", "ナナ"),
        ("8８八", "ハチ", "ハチ"),
        ("9９九", "キュウ", "キュー"),
        ("十", "ジュウ", "ジュー"),
    ] {
        for glyph in glyphs.chars() {
            builder.add_entry(DictEntry {
                surface: glyph.to_string().into(),
                pos: "名詞,数,*,*".into(),
                reading: reading.into(),
                pronunciation: pronunciation.into(),
                cost: -100,
                ..Default::default()
            });
        }
    }
    for (surface, pos, reading) in [
        ("月", "名詞,一般,*,*", "ツキ"),
        ("か月", "名詞,接尾,助数詞,*", "カゲツ"),
        ("月曜日", "名詞,副詞可能,*,*", "ゲツヨウビ"),
        ("曜", "名詞,一般,*,*", "ヨウ"),
        ("間", "名詞,接尾,一般,*", "カン"),
        ("本", "名詞,接尾,助数詞,*", "ホン"),
        ("前", "名詞,副詞可能,*,*", "マエ"),
    ] {
        builder.add_entry(DictEntry {
            surface: surface.into(),
            pos: pos.into(),
            reading: reading.into(),
            pronunciation: reading.into(),
            cost: -100,
            ..Default::default()
        });
    }
    builder.build().unwrap()
}

fn readings(tokens: &[Token]) -> (String, String) {
    (
        tokens.iter().map(|t| t.reading.as_ref()).collect(),
        tokens.iter().map(|t| t.pronunciation.as_ref()).collect(),
    )
}

const MONTHS: [(&str, &str, &str, &str, &str); 12] = [
    ("1", "１", "一", "イチ", "イチ"),
    ("2", "２", "二", "ニ", "ニ"),
    ("3", "３", "三", "サン", "サン"),
    ("4", "４", "四", "シ", "シ"),
    ("5", "５", "五", "ゴ", "ゴ"),
    ("6", "６", "六", "ロク", "ロク"),
    ("7", "７", "七", "シチ", "シチ"),
    ("8", "８", "八", "ハチ", "ハチ"),
    ("9", "９", "九", "ク", "ク"),
    ("10", "１０", "十", "ジュウ", "ジュー"),
    ("11", "１１", "十一", "ジュウイチ", "ジューイチ"),
    ("12", "１２", "十二", "ジュウニ", "ジューニ"),
];

#[test]
fn spaced_months_cover_all_months_and_notations() {
    let mut analyzer = Analyzer::from_dict(dictionary());
    for (ascii, wide, kanji, reading, pronunciation) in MONTHS {
        for number in [ascii, wide, kanji] {
            for gap in [" ", "\t", " \t "] {
                let text = format!("{number}{gap}月");
                let out = analyzer.try_tokenize(&text).unwrap();
                assert_eq!(
                    readings(&out),
                    (format!("{reading}ガツ"), format!("{pronunciation}ガツ")),
                    "{text:?}: {out:?}"
                );
                for token in &out {
                    assert_eq!(&text[token.start..token.end], token.surface.as_ref());
                }
                assert_eq!(out.last().unwrap().start, number.len() + gap.len());
            }
        }
    }
}

#[test]
fn non_calendar_contexts_keep_the_moon_reading() {
    let mut analyzer = Analyzer::from_dict(dictionary());
    for text in [
        "月がきれい",
        "3か月",
        "3 か月",
        "9 月曜",
        "9 月曜日",
        "9 月間",
        "四 月間",
        "9\n月",
        "9\r\n月",
        "9　月",
        "0 月",
        "13 月",
        "109 月",
        "十九 月",
        "09 月",
        "０９ 月",
        "一二 月",
        "十 二 月",
        "1９ 月",
        "v9 月",
        "v 9 月",
        "v (9 月",
        "1.9 月",
        "1/9 月",
        "１．９ 月",
        "九．九 月",
        "1e9 月",
        "9x 月",
        "99九 月",
        "3 本",
    ] {
        let out = analyzer.try_tokenize(text).unwrap();
        assert!(
            out.iter().filter(|t| t.surface.as_ref() == "月").all(|t| {
                t.reading.as_ref() == "ツキ" && t.pronunciation.as_ref() == "ツキ"
            }),
            "{text:?}: {out:?}"
        );
    }
}

#[test]
fn month_readings_preserve_offsets_previous_output_and_the_dictionary_cache() {
    let dict = dictionary();
    let mut analyzer = Analyzer::from_shared(std::sync::Arc::new(dict));
    for _ in 0..2 {
        for text in [
            "9 月の利用実績",
            "（9 月）",
            "2026年9 月1日",
            "前。9 月",
            "9 月、7 月",
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert!(
                out.iter()
                    .any(|t| t.surface.as_ref() == "月" && t.reading.as_ref() == "ガツ")
            );
            for token in &out {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
        assert_eq!(
            readings(&analyzer.tokenize("月")),
            ("ツキ".into(), "ツキ".into())
        );
        assert_eq!(
            readings(&analyzer.tokenize("九本")),
            ("キュウホン".into(), "キューホン".into())
        );
    }
    let mut workspace = hasami::lattice::LatticeWorkspace::new();
    let mut out = workspace.tokenize("九", analyzer.dictionary()).unwrap();
    let previous = format!("{:?}", out[0]);
    workspace
        .tokenize_into("12 月", analyzer.dictionary(), 91, &mut out)
        .unwrap();
    assert_eq!(format!("{:?}", out[0]), previous);
    assert_eq!(
        readings(&out[1..]),
        ("ジュウニガツ".into(), "ジューニガツ".into())
    );
    for t in &out[1..] {
        assert_eq!(&"12 月"[t.start - 91..t.end - 91], t.surface.as_ref());
    }
    let mut out = workspace.tokenize("九", analyzer.dictionary()).unwrap();
    workspace
        .tokenize_into(" 月", analyzer.dictionary(), 3, &mut out)
        .unwrap();
    assert_eq!(readings(&out), ("キュウツキ".into(), "キューツキ".into()));
}

#[test]
#[ignore = "配布辞書が必要"]
fn distributed_spaced_month_readings() {
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("dict/{name}.hsd")).unwrap();
        for (ascii, wide, kanji, reading, pronunciation) in MONTHS {
            for number in [ascii, wide, kanji] {
                let text = format!("{number} 月");
                let out = analyzer.try_tokenize(&text).unwrap();
                assert_eq!(
                    readings(&out),
                    (format!("{reading}ガツ"), format!("{pronunciation}ガツ")),
                    "{name}: {text}: {out:?}"
                );
                for token in &out {
                    assert_eq!(&text[token.start..token.end], token.surface.as_ref());
                }
            }
        }
    }
}
