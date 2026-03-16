//! インテグレーションテスト: 辞書構築→解析→出力の一貫性検証

use hasami::analyzer::{Analyzer, format_mecab, format_wakachi};
use hasami::dict::{DictBuilder, DictEntry};
use hasami::mmap_dict::{MmapDictBuilder, MmapDictionary};

/// テスト用の辞書を構築するヘルパー
fn build_test_dictionary() -> hasami::dict::Dictionary {
    let mut builder = DictBuilder::new();
    let words = vec![
        ("私", 1, 1, 3000, "名詞,代名詞,一般,*", "ワタシ", "ワタシ"),
        ("は", 2, 2, 4000, "助詞,係助詞,*,*", "ハ", "ワ"),
        ("猫", 3, 3, 3500, "名詞,一般,*,*", "ネコ", "ネコ"),
        ("です", 4, 4, 4000, "助動詞,*,*,*", "デス", "デス"),
        (
            "東京",
            5,
            5,
            2500,
            "名詞,固有名詞,地域,一般",
            "トウキョウ",
            "トーキョー",
        ),
        ("都", 6, 6, 5000, "名詞,接尾,地域,*", "ト", "ト"),
        (
            "東京都",
            7,
            7,
            2000,
            "名詞,固有名詞,地域,一般",
            "トウキョウト",
            "トーキョート",
        ),
        ("に", 8, 8, 4000, "助詞,格助詞,一般,*", "ニ", "ニ"),
        ("住む", 9, 9, 4500, "動詞,自立,*,*", "スム", "スム"),
        ("住ん", 9, 9, 4500, "動詞,自立,*,*", "スン", "スン"),
        ("で", 10, 10, 4000, "助詞,接続助詞,*,*", "デ", "デ"),
        ("いる", 11, 11, 4500, "動詞,非自立,*,*", "イル", "イル"),
        ("人", 12, 12, 3000, "名詞,一般,*,*", "ヒト", "ヒト"),
        ("が", 13, 13, 4000, "助詞,格助詞,一般,*", "ガ", "ガ"),
        ("多い", 14, 14, 4000, "形容詞,自立,*,*", "オオイ", "オーイ"),
    ];

    for (surface, lid, rid, cost, pos, reading, pronunciation) in words {
        builder.add_entry(DictEntry {
            surface: surface.into(),
            left_id: lid,
            right_id: rid,
            cost,
            pos: pos.into(),
            base_form: surface.into(),
            reading: reading.into(),
            pronunciation: pronunciation.into(),
        });
    }

    builder.build()
}

// ==========================================================================
// 基本的なインテグレーションテスト
// ==========================================================================

#[test]
fn test_end_to_end_tokenize() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("私は猫です");

    let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(surfaces, vec!["私", "は", "猫", "です"]);
}

#[test]
fn test_end_to_end_mecab_format() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("私は猫です");
    let output = format_mecab(&tokens);

    assert!(output.starts_with("私\t"));
    assert!(output.contains("名詞,代名詞"));
    assert!(output.ends_with("EOS\n"));
    // Should contain 4 token lines + EOS
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 5); // 4 tokens + EOS
}

#[test]
fn test_end_to_end_wakachi_format() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("私は猫です");
    let output = format_wakachi(&tokens);
    assert_eq!(output, "私 は 猫 です");
}

// ==========================================================================
// mmap辞書ラウンドトリップ
// ==========================================================================

#[test]
fn test_mmap_roundtrip_full_pipeline() {
    let dict = build_test_dictionary();
    let builder = MmapDictBuilder::from_dictionary(&dict);

    let tmp = std::env::temp_dir().join("hasami_integration_test.hsd");
    builder.write(&tmp).unwrap();

    // Load and tokenize via mmap path
    let mut analyzer = Analyzer::load(&tmp).unwrap();
    let tokens = analyzer.tokenize("私は猫です");

    let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(surfaces, vec!["私", "は", "猫", "です"]);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_mmap_roundtrip_preserves_metadata() {
    let dict = build_test_dictionary();
    let builder = MmapDictBuilder::from_dictionary(&dict);

    let tmp = std::env::temp_dir().join("hasami_integration_meta.hsd");
    builder.write(&tmp).unwrap();
    let loaded = MmapDictionary::load(&tmp).unwrap();

    // Verify all entries survive roundtrip
    assert_eq!(loaded.entry_count() as usize, dict.entries.len());

    // Verify specific entry data
    for i in 0..loaded.entry_count() {
        let surface = loaded.entry_surface(i);
        let pos = loaded.entry_pos(i);
        assert!(!surface.is_empty(), "Entry {} has empty surface", i);
        assert!(!pos.is_empty(), "Entry {} has empty POS", i);
    }

    let _ = std::fs::remove_file(&tmp);
}

// ==========================================================================
// 文分割テスト
// ==========================================================================

#[test]
fn test_sentence_splitting() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let tokens = analyzer.tokenize("私は猫です。私は猫です。");
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, "私は猫です。私は猫です。");
}

#[test]
fn test_multiple_sentence_boundaries() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let tokens = analyzer.tokenize("猫！猫？猫。");
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, "猫！猫？猫。");
}

#[test]
fn test_newline_as_boundary() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let tokens = analyzer.tokenize("猫\n猫");
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, "猫\n猫");
}

// ==========================================================================
// トークン位置の正確性
// ==========================================================================

#[test]
fn test_token_byte_positions_correct() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let input = "私は猫です";
    let tokens = analyzer.tokenize(input);

    // Check continuity - no gaps or overlaps
    let mut expected_start = 0;
    for t in &tokens {
        assert_eq!(t.start, expected_start, "Gap at byte {}", expected_start);
        assert!(t.end > t.start, "Zero-length token");
        assert_eq!(&*t.surface, &input[t.start..t.end], "Surface mismatch");
        expected_start = t.end;
    }
    assert_eq!(expected_start, input.len(), "Tokens don't cover full input");
}

#[test]
fn test_token_positions_with_sentence_boundary() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let input = "猫。猫";
    let tokens = analyzer.tokenize(input);

    let mut expected_start = 0;
    for t in &tokens {
        assert_eq!(t.start, expected_start);
        assert_eq!(&*t.surface, &input[t.start..t.end]);
        expected_start = t.end;
    }
    assert_eq!(expected_start, input.len());
}

// ==========================================================================
// 未知語処理
// ==========================================================================

#[test]
fn test_all_unknown_words() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let tokens = analyzer.tokenize("ABCDEFG");
    assert!(!tokens.is_empty());
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, "ABCDEFG");

    // All should be unknown
    for t in &tokens {
        assert!(!t.is_known, "Expected unknown for '{}'", &*t.surface);
    }
}

#[test]
fn test_mixed_known_unknown() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let tokens = analyzer.tokenize("私はDOGです");
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, "私はDOGです");

    // 私, は, です should be known
    assert!(tokens.first().unwrap().is_known);
}

#[test]
fn test_emoji_as_unknown() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let tokens = analyzer.tokenize("猫🐱");
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, "猫🐱");
}

// ==========================================================================
// エッジケース
// ==========================================================================

#[test]
fn test_empty_string() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    assert!(analyzer.tokenize("").is_empty());
}

#[test]
fn test_single_known_char() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("猫");
    assert_eq!(tokens.len(), 1);
    assert_eq!(&*tokens[0].surface, "猫");
}

#[test]
fn test_single_unknown_char() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("X");
    assert_eq!(tokens.len(), 1);
    assert!(!tokens[0].is_known);
}

#[test]
fn test_very_long_input() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let input = "私は猫です。".repeat(100);
    let tokens = analyzer.tokenize(&input);
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, input);
}

#[test]
fn test_numbers_only() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("12345");
    assert!(!tokens.is_empty());
    let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(reconstructed, "12345");
}

#[test]
fn test_whitespace_only() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("   ");
    assert!(!tokens.is_empty());
}

#[test]
fn test_symbols_only() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("!!!");
    assert!(!tokens.is_empty());
}

// ==========================================================================
// バッチ処理
// ==========================================================================

#[test]
fn test_batch_tokenize() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let inputs = vec!["私は猫です", "東京都に", ""];
    let results = analyzer.tokenize_batch(&inputs);

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].len(), 4); // 私 は 猫 です
    assert!(!results[1].is_empty());
    assert!(results[2].is_empty());
}

// ==========================================================================
// ワークスペース再利用の一貫性
// ==========================================================================

#[test]
fn test_workspace_reuse_produces_consistent_results() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    let inputs = [
        "私は猫です",
        "東京都に住んでいる",
        "猫が多い",
        "",
        "ABCDEF",
        "私は猫です",
    ];

    let first_results: Vec<Vec<String>> = inputs
        .iter()
        .map(|input| {
            analyzer
                .tokenize(input)
                .iter()
                .map(|t| t.surface.to_string())
                .collect()
        })
        .collect();

    // Run again - should produce identical results
    let second_results: Vec<Vec<String>> = inputs
        .iter()
        .map(|input| {
            analyzer
                .tokenize(input)
                .iter()
                .map(|t| t.surface.to_string())
                .collect()
        })
        .collect();

    assert_eq!(first_results, second_results);
}

// ==========================================================================
// 辞書マージ（ラウンドトリップ）
// ==========================================================================

#[test]
fn test_dict_merge_roundtrip() {
    // Build base dictionary
    let mut builder1 = DictBuilder::new();
    builder1.add_entry(DictEntry {
        surface: "猫".into(),
        left_id: 1,
        right_id: 1,
        cost: 3000,
        pos: "名詞,一般,*,*".into(),
        base_form: "猫".into(),
        reading: "ネコ".into(),
        pronunciation: "ネコ".into(),
    });
    let dict1 = builder1.build();
    let mmap_builder = MmapDictBuilder::from_dictionary(&dict1);
    let tmp1 = std::env::temp_dir().join("hasami_merge_base.hsd");
    mmap_builder.write(&tmp1).unwrap();

    // Load and merge with new entry
    let mut builder2 = DictBuilder::new();
    builder2.load_hsd(&tmp1).unwrap();
    builder2.add_entry(DictEntry {
        surface: "犬".into(),
        left_id: 2,
        right_id: 2,
        cost: 3000,
        pos: "名詞,一般,*,*".into(),
        base_form: "犬".into(),
        reading: "イヌ".into(),
        pronunciation: "イヌ".into(),
    });
    assert_eq!(builder2.entry_count(), 2);

    let merged_dict = builder2.build();
    let merged_builder = MmapDictBuilder::from_dictionary(&merged_dict);
    let tmp2 = std::env::temp_dir().join("hasami_merge_result.hsd");
    merged_builder.write(&tmp2).unwrap();

    let loaded = MmapDictionary::load(&tmp2).unwrap();
    assert_eq!(loaded.entry_count(), 2);

    let _ = std::fs::remove_file(&tmp1);
    let _ = std::fs::remove_file(&tmp2);
}

// ==========================================================================
// Token フィールドの完全性
// ==========================================================================

#[test]
fn test_token_all_fields_populated() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("私は猫です");

    for t in &tokens {
        assert!(!t.surface.is_empty(), "surface is empty");
        assert!(!t.pos.is_empty(), "pos is empty for '{}'", &*t.surface);
        assert!(t.end > t.start, "invalid range for '{}'", &*t.surface);
        // Known words should have base_form
        if t.is_known {
            assert!(
                !t.base_form.is_empty(),
                "base_form empty for known word '{}'",
                &*t.surface
            );
        }
    }
}

#[test]
fn test_known_token_reading() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("私は猫です");

    let watashi = &tokens[0];
    assert_eq!(&*watashi.reading, "ワタシ");
    assert_eq!(&*watashi.pronunciation, "ワタシ");
}

#[test]
fn test_unknown_token_empty_reading() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("XYZ");

    for t in &tokens {
        if !t.is_known {
            assert!(t.reading.is_empty(), "Unknown word should have empty reading");
        }
    }
}
