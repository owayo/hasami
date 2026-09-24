//! v4 形式の往復・再現性・検証のテスト

use super::container::{self, SectionId};
use super::meta::{self, Meta, PosScheme};
use super::{DictError, Dictionary, WriteOptions};
use crate::analyzer::Analyzer;
use crate::dict::{ConnectionMatrix, DictBuilder, DictEntry, UnkEntry};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// テストごとに別の一時ディレクトリ
fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "hasami-v4-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[allow(clippy::too_many_arguments)]
fn entry(
    surface: &str,
    left_id: u16,
    right_id: u16,
    cost: i16,
    pos: &str,
    conj: (&str, &str),
    base: &str,
    reading: &str,
    pron: &str,
) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        left_id,
        right_id,
        cost,
        pos: pos.into(),
        conj_type: conj.0.into(),
        conj_form: conj.1.into(),
        base_form: base.into(),
        reading: reading.into(),
        pronunciation: pron.into(),
    }
}

/// 活用・原形・発音の違い、同じ表層形の複数候補、接頭辞で終わるキー、長いキーを含む辞書
fn sample_entries() -> Vec<DictEntry> {
    let none = ("*", "*");
    vec![
        entry(
            "東京",
            1,
            1,
            3000,
            "名詞,固有名詞,地域,一般",
            none,
            "東京",
            "トウキョウ",
            "トーキョー",
        ),
        entry(
            "東",
            1,
            1,
            4000,
            "名詞,一般,*,*",
            none,
            "東",
            "ヒガシ",
            "ヒガシ",
        ),
        entry(
            "東京都",
            1,
            1,
            2000,
            "名詞,固有名詞,地域,一般",
            none,
            "東京都",
            "トウキョウト",
            "トーキョート",
        ),
        entry(
            "示し",
            2,
            2,
            3500,
            "動詞,自立,*,*",
            ("五段・サ行", "連用形"),
            "示す",
            "シメシ",
            "シメシ",
        ),
        entry(
            "示す",
            2,
            2,
            3500,
            "動詞,自立,*,*",
            ("五段・サ行", "基本形"),
            "示す",
            "シメス",
            "シメス",
        ),
        entry(
            "金",
            1,
            1,
            3000,
            "名詞,一般,*,*",
            none,
            "金",
            "キン",
            "キン",
        ),
        entry(
            "金",
            3,
            3,
            3500,
            "名詞,固有名詞,人名,姓",
            none,
            "金",
            "カネ",
            "カネ",
        ),
        entry(
            "金",
            1,
            1,
            2500,
            "名詞,一般,*,*",
            none,
            "金",
            "カネ",
            "カネ",
        ),
        entry(
            "Siemens",
            1,
            1,
            3000,
            "名詞,固有名詞,組織,*",
            none,
            "Siemens",
            "シーメンス",
            "シーメンス",
        ),
        entry(
            "ひらがな",
            1,
            1,
            3000,
            "名詞,一般,*,*",
            none,
            "ひらがな",
            "",
            "",
        ),
        entry(
            "𠮷野家",
            1,
            1,
            3000,
            "名詞,固有名詞,組織,*",
            none,
            "𠮷野家",
            "ヨシノヤ",
            "ヨシノヤ",
        ),
        entry(
            "とても長い表層形を持つ語がここにあって末尾圧縮の長さが百二十八バイトを超えるようにしてある",
            1,
            1,
            3000,
            "名詞,一般,*,*",
            none,
            "とても長い表層形を持つ語がここにあって末尾圧縮の長さが百二十八バイトを超えるようにしてある",
            "トテモナガイ",
            "トテモナガイ",
        ),
    ]
}

fn sample_builder() -> DictBuilder {
    let mut builder = DictBuilder::new();
    builder.set_matrix(ConnectionMatrix::zeros(4, 4));
    for e in sample_entries() {
        builder.add_entry(e);
    }
    builder
}

/// エントリの比較用のキー（表層形のバイト順、同じ表層形の中は元の順）
fn describe(e: &DictEntry) -> String {
    format!(
        "{},{},{},{},{},{},{},{},{},{}",
        e.surface,
        e.left_id,
        e.right_id,
        e.cost,
        e.pos,
        e.conj_type,
        e.conj_form,
        e.base_form,
        e.reading,
        e.pronunciation
    )
}

fn expected_order(entries: &[DictEntry]) -> Vec<String> {
    let mut sorted: Vec<&DictEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.surface.as_bytes().cmp(b.surface.as_bytes()));
    sorted.into_iter().map(describe).collect()
}

fn all_entries(dict: &Dictionary) -> Vec<String> {
    let mut out = Vec::new();
    dict.for_each_entry(|e| {
        out.push(describe(&e));
        Ok(())
    })
    .unwrap();
    out
}

#[test]
fn entries_roundtrip_in_memory() {
    let dict = sample_builder().build().unwrap();
    assert_eq!(all_entries(&dict), expected_order(&sample_entries()));
    assert_eq!(dict.entry_count(), sample_entries().len());
    dict.verify().unwrap();
}

#[test]
fn lookup_returns_every_prefix_and_group() {
    let dict = sample_builder().build().unwrap();
    let hits = dict.lookup("東京都庁").unwrap();
    let ends: Vec<usize> = hits.iter().map(|(end, _)| *end).collect();
    assert_eq!(ends, ["東".len(), "東京".len(), "東京都".len()]);
    let kin = dict.lookup("金曜").unwrap();
    assert_eq!(kin.len(), 1);
    // 同じ表層形の候補は元の順に並ぶ
    let readings: Vec<&str> = kin[0].1.iter().map(|e| &*e.reading).collect();
    assert_eq!(readings, ["キン", "カネ", "カネ"]);
    assert!(dict.lookup("大阪").unwrap().is_empty());
}

#[test]
fn file_roundtrip_keeps_metadata() {
    let dir = temp_dir();
    let path = dir.join("sample.hsd");
    let builder = sample_builder();
    let mut meta = Meta::new("sample", PosScheme::Unidic);
    meta.set(meta::KEY_SOURCES, "test@1").unwrap();
    let stats = builder
        .write_hsd(
            &path,
            &WriteOptions {
                meta,
                prune_dominated: false,
            },
            |_, _| {},
        )
        .unwrap();
    assert_eq!(stats.entries, sample_entries().len());
    assert_eq!(stats.bytes, std::fs::metadata(&path).unwrap().len());

    let dict = Dictionary::load(&path).unwrap();
    assert_eq!(dict.meta().name(), "sample");
    assert_eq!(dict.meta().pos_scheme(), PosScheme::Unidic);
    assert_eq!(dict.meta().get(meta::KEY_SOURCES), Some("test@1"));
    assert_eq!(dict.meta().get(meta::KEY_PRUNED_DOMINATED), Some("false"));
    assert_eq!(
        dict.meta().get(meta::KEY_HASAMI_VERSION),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert!(!dict.is_pruned());
    assert_eq!(all_entries(&dict), expected_order(&sample_entries()));
    // 一時ファイルは残らない
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn same_input_gives_identical_bytes() {
    let dir = temp_dir();
    let (a, b) = (dir.join("a.hsd"), dir.join("b.hsd"));
    let builder = sample_builder();
    let opts = builder.write_options();
    builder.write_hsd(&a, &opts, |_, _| {}).unwrap();
    sample_builder().write_hsd(&b, &opts, |_, _| {}).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tokens_expose_conjugation_and_base_form() {
    let mut analyzer = Analyzer::from_dict(sample_builder().build().unwrap());
    let tokens = analyzer.tokenize("示し");
    assert_eq!(tokens.len(), 1);
    let t = &tokens[0];
    assert_eq!(&*t.conj_type, "五段・サ行");
    assert_eq!(&*t.conj_form, "連用形");
    assert_eq!(&*t.base_form, "示す");
    assert_eq!(&*t.reading, "シメシ");

    // 活用しない語は空文字列、発音は読みと別に持てる
    let tokens = analyzer.tokenize("東京");
    assert_eq!(&*tokens[0].conj_type, "");
    assert_eq!(&*tokens[0].conj_form, "");
    assert_eq!(&*tokens[0].pronunciation, "トーキョー");

    // 未知語
    let tokens = analyzer.tokenize("ABC");
    assert!(!tokens[0].is_known);
    assert_eq!(&*tokens[0].conj_type, "");
}

#[test]
fn empty_and_missing_conjugation_become_asterisk() {
    let mut builder = DictBuilder::new();
    builder.add_entry(DictEntry {
        surface: "猫".into(),
        pos: "名詞,一般,*,*".into(),
        base_form: "猫".into(),
        ..Default::default()
    });
    let dict = builder.build().unwrap();
    let hits = dict.lookup("猫").unwrap();
    assert_eq!(&*hits[0].1[0].conj_type, "*");
    assert_eq!(&*hits[0].1[0].conj_form, "*");
}

#[test]
fn prune_dominated_keeps_the_best_path() {
    let mut builder = DictBuilder::new();
    let mut matrix = ConnectionMatrix::zeros(3, 3);
    // 右文脈 1 → 左文脈 2 を安くして、候補の選び方に接続コストが効くようにする
    matrix.costs[2 * 3 + 1] = -500;
    builder.set_matrix(matrix);
    let none = ("*", "*");
    for e in [
        entry(
            "金",
            1,
            1,
            3000,
            "名詞,一般,*,*",
            none,
            "金",
            "キン",
            "キン",
        ),
        // 同じ文脈 ID でコストが高い → 除かれる
        entry(
            "金",
            1,
            1,
            3500,
            "名詞,一般,*,*",
            none,
            "金",
            "カネ",
            "カネ",
        ),
        // 同じ文脈 ID で同じコスト → 先にある方が残る
        entry(
            "金",
            1,
            1,
            3000,
            "名詞,一般,*,*",
            none,
            "金",
            "コン",
            "コン",
        ),
        // 文脈 ID が違う → 残る
        entry(
            "金",
            2,
            2,
            3100,
            "名詞,固有名詞,人名,姓",
            none,
            "金",
            "キム",
            "キム",
        ),
        entry(
            "曜日",
            2,
            2,
            2000,
            "名詞,一般,*,*",
            none,
            "曜日",
            "ヨウビ",
            "ヨウビ",
        ),
        entry(
            "金曜",
            1,
            1,
            9000,
            "名詞,一般,*,*",
            none,
            "金曜",
            "キンヨウ",
            "キンヨウ",
        ),
    ] {
        builder.add_entry(e);
    }
    let plain = builder.build_with(&builder.write_options()).unwrap();
    let mut opts = builder.write_options();
    opts.prune_dominated = true;
    let pruned = builder.build_with(&opts).unwrap();
    assert!(pruned.is_pruned());
    assert_eq!(pruned.entry_count(), plain.entry_count() - 2);
    let kin: Vec<String> = pruned.lookup("金").unwrap()[0]
        .1
        .iter()
        .map(|e| e.reading.to_string())
        .collect();
    assert_eq!(kin, ["キン", "キム"]);

    let mut a = Analyzer::from_dict(plain);
    let mut b = Analyzer::from_dict(pruned);
    for text in ["金", "金曜日", "金金", "曜日金", "金曜日の金"] {
        let ta: Vec<String> = a
            .tokenize(text)
            .iter()
            .map(|t| format!("{}/{}/{}", t.surface, t.pos, t.reading))
            .collect();
        let tb: Vec<String> = b
            .tokenize(text)
            .iter()
            .map(|t| format!("{}/{}/{}", t.surface, t.pos, t.reading))
            .collect();
        assert_eq!(ta, tb, "{text}");
    }
}

#[test]
fn pruned_dictionary_cannot_be_edited() {
    let dir = temp_dir();
    let path = dir.join("final.hsd");
    let builder = sample_builder();
    let mut opts = builder.write_options();
    opts.prune_dominated = true;
    builder.write_hsd(&path, &opts, |_, _| {}).unwrap();
    let err = DictBuilder::new().load_hsd(&path).unwrap_err();
    assert!(matches!(err, DictError::Invalid(_)));
    assert!(err.to_string().contains("--prune-dominated"), "{err}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn matrix_is_read_in_the_transposed_orientation() {
    // 正方でない行列: left_id は 3 種類、right_id は 2 種類
    let mut matrix = ConnectionMatrix::zeros(3, 2);
    // 前の語の right_id 1 → 次の語の left_id 2 のコスト
    matrix.costs[2 * 2 + 1] = 777;
    let mut builder = DictBuilder::new();
    builder.set_matrix(matrix.clone());
    builder.add_entry(entry(
        "a",
        2,
        1,
        0,
        "名詞,一般,*,*",
        ("*", "*"),
        "a",
        "エー",
        "エー",
    ));
    let dict = builder.build().unwrap();
    assert_eq!(dict.matrix_dims(), (3, 2));
    let back = dict.connection_matrix().unwrap();
    assert_eq!(back.costs, matrix.costs);
    assert_eq!(back.cost(1, 2), Some(777));
}

#[test]
fn connection_costs_decide_the_segmentation() {
    // 「東京都」を 1 語で持つが、接続コストで「東京」+「都」を選ばせる
    let mut matrix = ConnectionMatrix::zeros(4, 4);
    // right 1 (東京) → left 2 (都) を大きく下げる
    matrix.costs[2 * 4 + 1] = -5000;
    let mut builder = DictBuilder::new();
    builder.set_matrix(matrix);
    let none = ("*", "*");
    builder.add_entry(entry(
        "東京",
        1,
        1,
        3000,
        "名詞,固有名詞,地域,一般",
        none,
        "東京",
        "トウキョウ",
        "トウキョウ",
    ));
    builder.add_entry(entry(
        "都",
        2,
        2,
        3000,
        "名詞,接尾,地域,*",
        none,
        "都",
        "ト",
        "ト",
    ));
    builder.add_entry(entry(
        "東京都",
        3,
        3,
        2000,
        "名詞,固有名詞,地域,一般",
        none,
        "東京都",
        "トウキョウト",
        "トウキョウト",
    ));
    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
    let surfaces: Vec<String> = analyzer
        .tokenize("東京都")
        .iter()
        .map(|t| t.surface.to_string())
        .collect();
    assert_eq!(surfaces, ["東京", "都"]);
}

#[test]
fn dictionary_without_matrix_uses_a_zero_matrix() {
    let dir = temp_dir();
    let path = dir.join("nomatrix.hsd");
    let mut builder = DictBuilder::new();
    builder.add_entry(entry(
        "猫",
        5,
        7,
        100,
        "名詞,一般,*,*",
        ("*", "*"),
        "猫",
        "ネコ",
        "ネコ",
    ));
    builder
        .write_hsd(&path, &builder.write_options(), |_, _| {})
        .unwrap();
    let dict = Dictionary::load(&path).unwrap();
    assert_eq!(dict.matrix_dims(), (6, 8));
    assert_eq!(dict.meta().get(meta::KEY_ZERO_MATRIX), Some("true"));
    assert!(dict.connection_matrix().is_none());

    // 取り込んだ後に、もっと大きな ID のエントリを足しても書き出せる
    let mut merged = DictBuilder::new();
    merged.load_hsd(&path).unwrap();
    merged.add_entry(entry(
        "犬",
        20,
        30,
        100,
        "名詞,一般,*,*",
        ("*", "*"),
        "犬",
        "イヌ",
        "イヌ",
    ));
    let dict = merged.build().unwrap();
    assert_eq!(dict.matrix_dims(), (21, 31));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unknown_words_use_templates_or_defaults() {
    // unk.def が無い文字種は既定の品詞・コスト
    let mut analyzer = Analyzer::from_dict(sample_builder().build().unwrap());
    let tokens = analyzer.tokenize("XYZ");
    assert_eq!(&*tokens[0].pos, "名詞,固有名詞,組織,*");

    // unk.def のテンプレートがあればそれを使う
    let mut builder = sample_builder();
    let mut unk = std::collections::HashMap::new();
    unk.insert(
        "ALPHA".to_string(),
        vec![UnkEntry {
            char_class: "ALPHA".into(),
            left_id: 1,
            right_id: 1,
            cost: 1000,
            pos: "名詞,一般,*,*".into(),
        }],
    );
    builder.set_unk_entries(unk);
    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
    let tokens = analyzer.tokenize("XYZ");
    assert_eq!(&*tokens[0].pos, "名詞,一般,*,*");
    assert_eq!(tokens[0].word_cost, 1000);
}

#[test]
fn import_keeps_char_definitions_and_unknown_templates() {
    let dir = temp_dir();
    let char_def = dir.join("char.def");
    std::fs::write(
        &char_def,
        "DEFAULT 0 1 0\nALPHA 1 1 0\nKANJI 0 0 2\nKATAKANA 1 1 2\n0x0041..0x005A ALPHA\n0x4E00..0x9FFF KANJI\n0x30A1..0x30FF KATAKANA\n",
    )
    .unwrap();
    let unk_def = dir.join("unk.def");
    std::fs::write(
        &unk_def,
        "ALPHA,1,1,4000,名詞,固有名詞,組織,*\nKANJI,1,1,7000,名詞,一般,*,*\n",
    )
    .unwrap();
    let mut builder = sample_builder();
    builder.load_char_def(&char_def).unwrap();
    builder.load_unk(&unk_def).unwrap();
    let path = dir.join("with-defs.hsd");
    builder
        .write_hsd(&path, &builder.write_options(), |_, _| {})
        .unwrap();

    let dict = Dictionary::load(&path).unwrap();
    let classifier = dict.char_classifier();
    assert_eq!(classifier.ranges.len(), 3);
    assert!(classifier.get_class("KATAKANA").unwrap().invoke);
    let unk = dict.unk_entries();
    assert_eq!(unk["ALPHA"][0].cost, 4000);
    assert_eq!(unk["KANJI"][0].pos, "名詞,一般,*,*");

    // repair・merge で取り込んでも失われない（v3 は文字範囲を落としていた）
    let mut again = DictBuilder::new();
    again.load_hsd(&path).unwrap();
    let dict2 = again.build().unwrap();
    assert_eq!(dict2.char_classifier().ranges, classifier.ranges);
    assert_eq!(dict2.unk_entries()["ALPHA"].len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn build_rejects_invalid_input() {
    // エントリなし
    assert!(matches!(
        DictBuilder::new().build(),
        Err(DictError::Invalid(_))
    ));
    // 空の表層形
    let mut b = DictBuilder::new();
    b.add_entry(DictEntry::default());
    assert!(matches!(b.build(), Err(DictError::Invalid(_))));
    // 行列の範囲外の文脈 ID
    let mut b = DictBuilder::new();
    b.set_matrix(ConnectionMatrix::zeros(2, 2));
    b.add_entry(entry(
        "猫",
        2,
        0,
        0,
        "名詞,一般,*,*",
        ("*", "*"),
        "猫",
        "ネコ",
        "ネコ",
    ));
    let err = b.build().unwrap_err();
    assert!(
        err.to_string().contains("outside the connection matrix"),
        "{err}"
    );
}

// --- 壊れたファイル ---

fn sample_bytes() -> Vec<u8> {
    let dir = temp_dir();
    let path = dir.join("s.hsd");
    let builder = sample_builder();
    builder
        .write_hsd(&path, &builder.write_options(), |_, _| {})
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    bytes
}

fn section_range(bytes: &[u8], id: SectionId) -> std::ops::Range<usize> {
    let layout = container::parse(bytes).unwrap();
    let r = layout.get(id);
    r.offset..r.end()
}

#[test]
fn rejects_other_versions_and_files() {
    let bytes = sample_bytes();
    let mut v3 = bytes.clone();
    v3[8..12].copy_from_slice(&3u32.to_le_bytes());
    let err = Dictionary::from_bytes(&v3).err().unwrap();
    assert!(matches!(err, DictError::UnsupportedVersion(3)));
    assert!(err.to_string().contains("build-dict.sh"));

    assert!(matches!(
        Dictionary::from_bytes(b"hello"),
        Err(DictError::NotHsd)
    ));
    assert!(matches!(
        Dictionary::from_bytes(&bytes[..bytes.len() - 1]),
        Err(DictError::Corrupt(_))
    ));
}

#[test]
fn detects_broken_references() {
    let bytes = sample_bytes();

    // 素性レコードのオフセットを範囲外にする → 解析時にエラー（panic しない）
    let mut bad = bytes.clone();
    let offsets = section_range(&bad, SectionId::FeatureOffsets);
    for chunk in bad[offsets].chunks_mut(4) {
        chunk.copy_from_slice(&u32::MAX.to_le_bytes());
    }
    let dict = Dictionary::from_bytes(&bad).unwrap();
    let mut analyzer = Analyzer::from_dict(dict);
    assert!(matches!(
        analyzer.try_tokenize("東京"),
        Err(DictError::Corrupt(_))
    ));
    assert!(analyzer.dictionary().verify().is_err());

    // 群の最後の印を消す → 群が隣と混ざるので verify で見つかる
    let mut bad = bytes.clone();
    let entries = section_range(&bad, SectionId::Entries);
    for rec in bad[entries].chunks_mut(6) {
        rec[1] &= 0x7F;
    }
    let dict = Dictionary::from_bytes(&bad).unwrap();
    assert!(dict.verify().is_err());
    // 最後の群が配列の外まで続く → 解析時にエラー
    let mut analyzer = Analyzer::from_dict(dict);
    assert!(analyzer.try_tokenize("𠮷野家").is_err());

    // 文脈 ID を行列の外にする → 解析時にエラー
    let mut bad = bytes.clone();
    let entries = section_range(&bad, SectionId::Entries);
    for rec in bad[entries].chunks_mut(6) {
        rec[2..4].copy_from_slice(&999u16.to_le_bytes());
    }
    let mut analyzer = Analyzer::from_dict(Dictionary::from_bytes(&bad).unwrap());
    assert!(matches!(
        analyzer.try_tokenize("東京"),
        Err(DictError::Corrupt(_))
    ));

    // FEATURE_OFFSETS の件数がエントリと合わない → ロード時にエラー
    let layout = container::parse(&bytes).unwrap();
    let r = layout.get(SectionId::FeatureOffsets);
    let entry_at = (0..17)
        .map(|i| container::HEADER_LEN + i * container::SECTION_ENTRY_LEN)
        .find(|&at| {
            u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
                == SectionId::FeatureOffsets as u32
        })
        .unwrap();
    let mut bad = bytes.clone();
    bad[entry_at + 16..entry_at + 24].copy_from_slice(&((r.len - 4) as u64).to_le_bytes());
    assert!(matches!(
        Dictionary::from_bytes(&bad),
        Err(DictError::Corrupt(_))
    ));
}

#[test]
fn random_corruption_never_panics() {
    let bytes = sample_bytes();
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let body_start = section_range(&bytes, SectionId::Meta).start;
    for _ in 0..2000 {
        let mut bad = bytes.clone();
        for _ in 0..1 + next() % 4 {
            let at = body_start + (next() as usize) % (bad.len() - body_start);
            bad[at] ^= 1 << (next() % 8);
        }
        let Ok(dict) = Dictionary::from_bytes(&bad) else {
            continue;
        };
        let _ = dict.verify();
        let _ = dict.lookup("東京都");
        let _ = dict.for_each_entry(|_| Ok(()));
        let mut analyzer = Analyzer::from_dict(dict);
        for text in ["東京都に住む", "金曜日", "𠮷野家とSiemens", "ひらがな示し"]
        {
            let _ = analyzer.try_tokenize(text);
        }
    }
}

#[test]
fn token_cache_does_not_change_results_or_leak_between_dictionaries() {
    use crate::lattice::LatticeWorkspace;
    let a = sample_builder().build().unwrap();
    // 同じ表層形・同じエントリ番号で読みだけ違う辞書
    let mut other = DictBuilder::new();
    other.set_matrix(ConnectionMatrix::zeros(4, 4));
    for e in sample_entries() {
        other.add_entry(DictEntry {
            reading: format!("{}ベツ", e.reading).into(),
            pronunciation: format!("{}ベツ", e.pronunciation).into(),
            ..e
        });
    }
    let b = other.build().unwrap();
    let describe = |tokens: &[crate::lattice::Token]| -> Vec<String> {
        tokens
            .iter()
            .map(|t| {
                format!(
                    "{}/{}/{}/{}/{}",
                    t.surface, t.start, t.pos, t.reading, t.word_cost
                )
            })
            .collect()
    };
    let text = "東京都の示し金曜日𠮷野家";
    let mut fresh_a = LatticeWorkspace::new();
    let expected_a = describe(&fresh_a.tokenize(text, &a).unwrap());
    let mut fresh_b = LatticeWorkspace::new();
    let expected_b = describe(&fresh_b.tokenize(text, &b).unwrap());
    assert_ne!(expected_a, expected_b);

    // 1 つのワークスペースで辞書を交互に使っても、キャッシュが混ざらない
    let mut ws = LatticeWorkspace::new();
    for _ in 0..3 {
        assert_eq!(describe(&ws.tokenize(text, &a).unwrap()), expected_a);
        assert_eq!(describe(&ws.tokenize(text, &b).unwrap()), expected_b);
    }
    // 位置が違っても、キャッシュから作ったトークンの位置は入力どおり
    let tokens = ws.tokenize("金金", &a).unwrap();
    assert_eq!((tokens[0].start, tokens[1].start), (0, "金".len()));
}
