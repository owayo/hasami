//! 一般語の読みの誤記だけを直し、有効な四つ仮名と別の素性を保つ。
#![cfg(feature = "build")]

use hasami::{Analyzer, DictEntry, Dictionary, dict::DictBuilder};

fn entry(
    surface: &str,
    base: &str,
    kind: &str,
    form: &str,
    reading: &str,
    modern: &str,
) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        base_form: base.into(),
        pos: if kind == "*" {
            "名詞,一般,*,*"
        } else {
            "動詞,自立,*,*"
        }
        .into(),
        conj_type: kind.into(),
        conj_form: form.into(),
        reading: reading.into(),
        pronunciation: modern.into(),
        left_id: 1,
        right_id: 1,
        cost: 1234,
    }
}

fn misspellings() -> Vec<DictEntry> {
    [
        ("図形", "図形", "*", "*", "ヅケイ", "ズケイ"),
        ("預け", "預け", "*", "*", "アヅケ", "アズケ"),
        ("預ける", "預ける", "一段", "基本形", "アヅケル", "アズケル"),
        ("預け", "預ける", "一段", "未然形", "アヅケ", "アズケ"),
        ("預け", "預ける", "一段", "連用形", "アヅケ", "アズケ"),
        ("預けれ", "預ける", "一段", "仮定形", "アヅケレ", "アズケレ"),
        (
            "預けよ",
            "預ける",
            "一段",
            "未然ウ接続",
            "アヅケヨ",
            "アズケヨ",
        ),
        (
            "預けよ",
            "預ける",
            "一段",
            "命令ｙｏ",
            "アヅケヨ",
            "アズケヨ",
        ),
        (
            "預けろ",
            "預ける",
            "一段",
            "命令ｒｏ",
            "アヅケロ",
            "アズケロ",
        ),
    ]
    .into_iter()
    .map(|(surface, base, kind, form, reading, modern)| {
        entry(surface, base, kind, form, reading, modern)
    })
    .collect()
}

#[test]
fn lexical_repair_changes_only_readings_once_and_survives_hsd_roundtrip() {
    let mut builder = DictBuilder::new();
    let before = misspellings();
    for e in &before {
        builder.add_entry(e.clone());
    }
    assert_eq!(builder.repair_lexical_readings(), before.len());
    for (old, new) in before.iter().zip(builder.entries()) {
        let mut restored = new.clone();
        restored.reading = old.reading.clone();
        assert_eq!(format!("{restored:?}"), format!("{old:?}"));
        assert_eq!(new.reading, old.pronunciation);
    }
    assert_eq!(builder.repair_lexical_readings(), 0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("readings.hsd");
    builder
        .write_hsd(&path, &builder.write_options(), |_, _| {})
        .unwrap();
    let dictionary = Dictionary::load(&path).unwrap();
    dictionary.verify().unwrap();
    let mut analyzer = Analyzer::from_dict(dictionary);
    for (surface, reading) in [
        ("図形", "ズケイ"),
        ("預ける", "アズケル"),
        ("預けれ", "アズケレ"),
    ] {
        let tokens = analyzer.try_tokenize(surface).unwrap();
        assert_eq!(tokens.len(), 1);
        let t = &tokens[0];
        assert_eq!(t.reading.as_ref(), reading);
        assert_eq!(t.pronunciation.as_ref(), reading);
        assert_eq!((t.start, t.end), (0, surface.len()));
        assert_eq!(t.word_cost, 1234);
        assert!(t.is_known);
    }
}

#[test]
fn lexical_repair_preserves_valid_yotsugana_names_custom_readings_and_grammar() {
    let mut builder = DictBuilder::new();
    for e in [
        entry(
            "続く",
            "続く",
            "五段・カ行イ音便",
            "基本形",
            "ツヅク",
            "ツズク",
        ),
        entry("縮む", "縮む", "五段・マ行", "基本形", "チヂム", "チジム"),
        entry("図形", "図形", "*", "*", "ズケイ", "ズケイ"),
        entry("預ける", "預ける", "一段", "基本形", "アズケル", "アズケル"),
        entry("図形的", "図形的", "*", "*", "ヅケイテキ", "ズケイテキ"),
    ] {
        builder.add_entry(e);
    }
    for mut e in misspellings() {
        e.pos = "名詞,固有名詞,人名,姓".into();
        builder.add_entry(e);
    }
    for e in misspellings() {
        let mut custom = e.clone();
        custom.base_form = "独自の原形".into();
        builder.add_entry(custom);
        let mut custom = e.clone();
        custom.reading = "ベツヨミ".into();
        builder.add_entry(custom);
        let mut custom = e.clone();
        custom.pronunciation = "ベツハツオン".into();
        builder.add_entry(custom);
        let mut custom = e.clone();
        custom.conj_type = "独自の活用型".into();
        builder.add_entry(custom);
        let mut custom = e;
        custom.conj_form = "独自の活用形".into();
        builder.add_entry(custom);
    }
    let before = format!("{:?}", builder.entries());
    assert_eq!(builder.repair_lexical_readings(), 0);
    assert_eq!(format!("{:?}", builder.entries()), before);
}

#[test]
fn repair_command_corrects_reading_even_when_pronunciation_repair_is_disabled() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.hsd");
    let output = directory.path().join("output.hsd");
    let mut builder = DictBuilder::new();
    builder.add_entry(misspellings().remove(0));
    builder
        .write_hsd(&input, &builder.write_options(), |_, _| {})
        .unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_hasami"))
        .args(["repair", "--no-pronunciation-repair", "--dict"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut analyzer = Analyzer::load(&output).unwrap();
    let tokens = analyzer.try_tokenize("図形").unwrap();
    assert_eq!(tokens[0].reading.as_ref(), "ズケイ");
    assert_eq!(tokens[0].pronunciation.as_ref(), "ズケイ");
    drop(analyzer);
    let before = std::fs::read(&output).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_hasami"))
        .args(["repair", "--no-pronunciation-repair", "--dict"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(std::fs::read(&output).unwrap(), before);
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_lexical_reading_repairs_keep_valid_yotsugana() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        for (surface, reading, pronunciation) in [
            ("図形", "ズケイ", "ズケイ"),
            ("預ける", "アズケル", "アズケル"),
            ("預けれ", "アズケレ", "アズケレ"),
            ("預けろ", "アズケロ", "アズケロ"),
            ("続く", "ツヅク", "ツズク"),
            ("縮む", "チヂム", "チジム"),
        ] {
            let tokens = analyzer.try_tokenize(surface).unwrap();
            let actual_reading: String = tokens.iter().map(|t| t.reading.as_ref()).collect();
            let actual_pronunciation: String =
                tokens.iter().map(|t| t.pronunciation.as_ref()).collect();
            assert_eq!(actual_reading, reading, "{name}: {surface}: {tokens:?}");
            assert_eq!(
                actual_pronunciation, pronunciation,
                "{name}: {surface}: {tokens:?}"
            );
        }
    }
}
