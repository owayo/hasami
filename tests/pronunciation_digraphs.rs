//! 辞書修復で小書き母音の前のウを保ち、長音や別読みを変えない。
#![cfg(feature = "build")]

use hasami::Analyzer;
use hasami::dict::{DictBuilder, DictEntry};

fn entry(surface: &str, reading: &str, pronunciation: &str, pos: &str) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        left_id: 1,
        right_id: 1,
        cost: 1234,
        pos: pos.into(),
        conj_type: "*".into(),
        conj_form: "*".into(),
        base_form: surface.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
    }
}

#[test]
fn repair_restores_only_aligned_digraphs_once_per_entry() {
    let mut builder = DictBuilder::new();
    for (surface, reading, broken) in [
        ("試験ウァ", "シケンウァ", "シケンーァ"),
        ("試験ウィ", "シケンウィ", "シケンーィ"),
        ("試験ウェ", "シケンウェ", "シケンーェ"),
        ("試験ウォ", "シケンウォ", "シケンーォ"),
        ("ウェウォ", "ウェウォ", "ーェーォ"),
        ("学術ウェイ", "ガクジュツウェイ", "ガクジュツーェイ"),
        ("東京都ウェア", "トウキョウトウェア", "トーキョートーェア"),
    ] {
        builder.add_entry(entry(surface, reading, broken, "名詞,一般,*,*"));
    }
    let before = builder.entries().to_vec();
    assert_eq!(builder.repair_pronunciation(), 7);
    for (original, repaired) in before.iter().zip(builder.entries()) {
        let mut restored = repaired.clone();
        restored.pronunciation = original.pronunciation.clone();
        assert_eq!(format!("{original:?}"), format!("{restored:?}"));
        let expected = if original.surface.as_ref() == "東京都ウェア" {
            "トーキョートウェア"
        } else {
            &original.reading
        };
        assert_eq!(repaired.pronunciation.as_ref(), expected);
    }
    let once = format!("{:?}", builder.entries());
    assert_eq!(builder.repair_pronunciation(), 0);
    assert_eq!(format!("{:?}", builder.entries()), once);
    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
    let tokens = analyzer.try_tokenize("学術ウェイ").unwrap();
    assert_eq!(tokens.len(), 1);
    assert!(tokens[0].is_known);
    assert_eq!(tokens[0].pronunciation.as_ref(), "ガクジュツウェイ");
    assert_eq!(tokens[0].word_cost, 1234);
    assert_eq!(tokens[0].start, 0);
    assert_eq!(tokens[0].end, "学術ウェイ".len());
}

#[test]
fn repair_preserves_sound_pronunciations_misalignment_and_person_names() {
    let mut builder = DictBuilder::new();
    for (surface, reading, pronunciation, pos) in [
        (
            "ソフトウェア",
            "ソフトウェア",
            "ソフトウェア",
            "名詞,一般,*,*",
        ),
        ("方法", "ホウホウ", "ホーホー", "名詞,一般,*,*"),
        ("小売", "コウリ", "コウリ", "名詞,一般,*,*"),
        ("試験一", "ウエア", "ーェア", "名詞,一般,*,*"),
        ("試験二", "ウェア", "ーエア", "名詞,一般,*,*"),
        ("試験三", "ウェイ", "ーェェイ", "名詞,一般,*,*"),
        ("試験四", "ウィ", "オィ", "名詞,一般,*,*"),
        ("表記差", "ヂャ", "ジャ", "名詞,一般,*,*"),
        ("記号", "*", "*", "記号,一般,*,*"),
        ("名前用例", "ウィン", "ーィン", "名詞,固有名詞,人名,一般"),
        (
            "省略素性の名前用例",
            "ウィン",
            "ーィン",
            "名詞,固有名詞,人名",
        ),
    ] {
        builder.add_entry(entry(surface, reading, pronunciation, pos));
    }
    let before = format!("{:?}", builder.entries());
    assert_eq!(builder.repair_pronunciation(), 0);
    assert_eq!(format!("{:?}", builder.entries()), before);
}
