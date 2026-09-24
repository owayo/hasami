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
fn test_unknown_alpha_token_has_kana_reading() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    let tokens = analyzer.tokenize("XYZ");

    for t in &tokens {
        if !t.is_known && t.surface.chars().all(|c| c.is_ascii_alphabetic()) {
            assert!(
                !t.reading.is_empty(),
                "Unknown alpha word '{}' should have kana reading",
                &*t.surface
            );
            assert!(
                !t.pronunciation.is_empty(),
                "Unknown alpha word '{}' should have kana pronunciation",
                &*t.surface
            );
        }
    }
}

#[test]
fn test_unknown_kana_token_has_kana_reading() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    // 辞書に無い仮名語でも、表層形から読みを補完できる
    let tokens = analyzer.tokenize("ングゖー");

    for t in &tokens {
        assert!(
            !t.reading.is_empty(),
            "Kana word '{}' should have kana reading",
            &*t.surface
        );
        assert!(
            !t.pronunciation.is_empty(),
            "Kana word '{}' should have kana pronunciation",
            &*t.surface
        );
    }
    let joined: String = tokens.iter().map(|t| t.reading.to_string()).collect();
    assert_eq!(joined, "ングヶー", "ひらがなはカタカナに写像される");
}

#[test]
fn test_unit_reading_after_number() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    // 数字 + 単位文字の場合、単位読みになること
    let tokens = analyzer.tokenize("100W");
    let w_token = tokens.iter().find(|t| &*t.surface == "W");
    assert!(w_token.is_some(), "Should have a W token");
    assert_eq!(&*w_token.unwrap().reading, "ワット");

    let tokens = analyzer.tokenize("5A");
    let a_token = tokens.iter().find(|t| &*t.surface == "A");
    assert!(a_token.is_some(), "Should have an A token");
    assert_eq!(&*a_token.unwrap().reading, "アンペア");

    let tokens = analyzer.tokenize("12V");
    let v_token = tokens.iter().find(|t| &*t.surface == "V");
    assert!(v_token.is_some(), "Should have a V token");
    assert_eq!(&*v_token.unwrap().reading, "ボルト");
}

#[test]
fn test_unit_reading_not_after_punctuation() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    // 読点・句点は桁区切りの「1,000」と同じ文字だが数字ではない。
    // 数字扱いすると直後の英字に単位読みが付き、「,Aさん」が「アンペアさん」になる
    for text in [",A", ".A"] {
        let tokens = analyzer.tokenize(text);
        let a_token = tokens.iter().find(|t| &*t.surface == "A");
        assert!(a_token.is_some(), "Should have an A token in {text}");
        assert_eq!(&*a_token.unwrap().reading, "エー", "in {text}");
    }

    // 桁区切りを含む数字の直後は従来どおり単位読みになる
    let tokens = analyzer.tokenize("1,000W");
    let w_token = tokens.iter().find(|t| &*t.surface == "W");
    assert!(w_token.is_some(), "Should have a W token");
    assert_eq!(&*w_token.unwrap().reading, "ワット");
}

#[test]
fn test_iteration_mark_repeats_previous_reading() {
    let mut builder = DictBuilder::new();
    // 「々」は記号として辞書に入っていて読みを持たない。辞書に「前々項」のような
    // 語が無いと 1 文字ずつに割れ、補完しないと「々」だけ無音になる
    builder.add_entry(entry("前", "名詞,一般,*,*", "前", "ゼン", "ゼン"));
    builder.add_entry(entry("々", "記号,一般,*,*", "々", "", ""));
    builder.add_entry(entry("項", "名詞,一般,*,*", "項", "コウ", "コー"));
    let mut analyzer = Analyzer::from_dict(builder.build());

    let tokens = analyzer.tokenize("前々項");
    let mark = tokens
        .iter()
        .find(|t| &*t.surface == "々")
        .expect("々 token");
    assert_eq!(&*mark.pronunciation, "ゼン");
}

#[test]
fn test_alpha_reading_without_number() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);

    // 数字が前にない場合はアルファベット読み
    let tokens = analyzer.tokenize("W");
    let w_token = tokens.iter().find(|t| &*t.surface == "W");
    assert!(w_token.is_some(), "Should have a W token");
    assert_eq!(&*w_token.unwrap().reading, "ダブリュー");
}

#[test]
fn test_unknown_non_alpha_token_empty_reading() {
    let dict = build_test_dictionary();
    let mut analyzer = Analyzer::from_dict(dict);
    // 絵文字は辞書に存在しないので未知語になる
    let tokens = analyzer.tokenize("🍣");

    for t in &tokens {
        if !t.is_known {
            assert!(
                t.reading.is_empty(),
                "Unknown non-alpha word should have empty reading"
            );
        }
    }
}

// ==========================================================================
// 並行解析テスト（Clone impl で辞書共有）
// ==========================================================================

#[test]
fn test_analyzer_clone_shares_dict_and_isolates_workspace() {
    let dict = build_test_dictionary();
    let builder = MmapDictBuilder::from_dictionary(&dict);
    let tmp = std::env::temp_dir().join("hasami_clone_share.hsd");
    builder.write(&tmp).unwrap();

    let mut a = Analyzer::load(&tmp).unwrap();
    let mut b = a.clone();

    // 両方が同じ結果を返す（辞書を共有）
    let ta: Vec<String> = a
        .tokenize("私は猫です")
        .into_iter()
        .map(|t| t.surface.to_string())
        .collect();
    let tb: Vec<String> = b
        .tokenize("私は猫です")
        .into_iter()
        .map(|t| t.surface.to_string())
        .collect();
    assert_eq!(ta, tb);
    assert_eq!(ta, vec!["私", "は", "猫", "です"]);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_concurrent_tokenize_across_threads() {
    let dict = build_test_dictionary();
    let builder = MmapDictBuilder::from_dictionary(&dict);
    let tmp = std::env::temp_dir().join("hasami_concurrent.hsd");
    builder.write(&tmp).unwrap();

    let analyzer = Analyzer::load(&tmp).unwrap();
    analyzer.prewarm(); // 並列前にArcキャッシュ温める

    let inputs = [
        "私は猫です",
        "東京都に住んでいる",
        "人が多い",
        "私は猫です。東京都に住んでいる。",
    ];

    let results = std::thread::scope(|s| {
        let handles: Vec<_> = inputs
            .iter()
            .map(|input| {
                let mut worker = analyzer.clone();
                let input = *input;
                s.spawn(move || {
                    let tokens = worker.tokenize(input);
                    tokens
                        .into_iter()
                        .map(|t| t.surface.to_string())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });

    // 各スレッドが入力を完全再構築できているはず
    for (input, surfaces) in inputs.iter().zip(results.iter()) {
        let reconstructed: String = surfaces.concat();
        assert_eq!(&reconstructed, input, "Reconstruction failed for {input}");
    }

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_analyzer_send_and_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<Analyzer>();
    assert_sync::<Analyzer>();
}

#[test]
fn test_prewarm_idempotent() {
    let dict = build_test_dictionary();
    let builder = MmapDictBuilder::from_dictionary(&dict);
    let tmp = std::env::temp_dir().join("hasami_prewarm.hsd");
    builder.write(&tmp).unwrap();

    let analyzer = Analyzer::load(&tmp).unwrap();
    // 複数回呼んでも問題ない
    analyzer.prewarm();
    analyzer.prewarm();
    analyzer.prewarm();

    let _ = std::fs::remove_file(&tmp);
}

// ==========================================================================
// char.def 復元テスト（v3 フォーマット）
// ==========================================================================

#[test]
fn test_v2_backward_compat_load() {
    // v3 ファイルを書き出した後、version バイト (offset=8..12) を 2 に書き換えて
    // v2 後方互換パスで読めるかを検証する。
    let dict = build_test_dictionary();
    let builder = MmapDictBuilder::from_dictionary(&dict);
    let tmp = std::env::temp_dir().join("hasami_v2_compat.hsd");
    builder.write(&tmp).unwrap();

    let mut bytes = std::fs::read(&tmp).unwrap();
    // version バイト書き換え（little-endian u32）
    bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
    std::fs::write(&tmp, &bytes).unwrap();

    // v2 として読めることを確認
    let mut analyzer = Analyzer::load(&tmp).unwrap();
    let tokens = analyzer.tokenize("私は猫です");
    let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(surfaces, vec!["私", "は", "猫", "です"]);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_char_def_roundtrip_v3() {
    use hasami::char_class::{CharClass, CharClassifier, CharType};
    use std::collections::HashMap;

    // カスタム char.def を持つ辞書を構築
    let mut classes = HashMap::new();
    classes.insert(
        "HIRAGANA".to_string(),
        CharClass {
            name: "HIRAGANA".to_string(),
            invoke: true,
            group: false,
            length: 7, // 通常 default_japanese は length=2
        },
    );
    classes.insert(
        "KATAKANA".to_string(),
        CharClass {
            name: "KATAKANA".to_string(),
            invoke: false,
            group: true,
            length: 5,
        },
    );
    let ranges = vec![
        (0x3040, 0x309F, "HIRAGANA".to_string()),
        (0x30A0, 0x30FF, "KATAKANA".to_string()),
    ];
    let custom_classifier = CharClassifier::from_definitions(classes, ranges);

    let mut builder = DictBuilder::new();
    builder.add_entry(DictEntry {
        surface: "猫".into(),
        left_id: 1,
        right_id: 1,
        cost: 100,
        pos: "名詞,一般,*,*".into(),
        base_form: "猫".into(),
        reading: "ネコ".into(),
        pronunciation: "ネコ".into(),
    });
    builder.set_char_classifier(custom_classifier);
    let dict = builder.build();

    let mmap_builder = MmapDictBuilder::from_dictionary(&dict);
    let tmp = std::env::temp_dir().join("hasami_char_def_v3.hsd");
    mmap_builder.write(&tmp).unwrap();

    let loaded = MmapDictionary::load(&tmp).unwrap();
    let restored = loaded.build_classifier();

    // カスタム length が復元されているか
    let h = restored.get_class("HIRAGANA").expect("HIRAGANA missing");
    assert!(h.invoke);
    assert!(!h.group);
    assert_eq!(h.length, 7);

    let k = restored.get_class("KATAKANA").expect("KATAKANA missing");
    assert!(!k.invoke);
    assert!(k.group);
    assert_eq!(k.length, 5);

    // ranges が復元され、classify_char が正しく動作するか
    assert_eq!(restored.classify_char('あ'), CharType::Hiragana);
    assert_eq!(restored.classify_char('ア'), CharType::Katakana);

    let _ = std::fs::remove_file(&tmp);
}

// ==========================================================================
// 辞書修復（壊れた発音・誤読エントリの除去）
// ==========================================================================

fn entry(
    surface: &str,
    pos: &str,
    base_form: &str,
    reading: &str,
    pronunciation: &str,
) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        left_id: 1,
        right_id: 1,
        cost: 3000,
        pos: pos.into(),
        base_form: base_form.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
    }
}

#[test]
fn test_repair_pronunciation_borrows_long_vowel_form() {
    let mut builder = DictBuilder::new();
    // IPAdic 由来の健全なエントリと、SudachiDict 由来の壊れたエントリが共存する状況
    builder.add_entry(entry(
        "方法",
        "名詞,一般,*,*",
        "方法",
        "ホウホウ",
        "ホーホー",
    ));
    builder.add_entry(entry("方法", "名詞,一般,*,*", "方法", "ホウホウ", "方法"));

    assert_eq!(builder.repair_pronunciation(), 1);

    let entries = builder.entries();
    // 壊れていた方は、同じ読みを持つ健全なエントリの長音表記を借用する
    assert_eq!(&*entries[1].pronunciation, "ホーホー");
    assert_eq!(&*entries[0].pronunciation, "ホーホー");
}

#[test]
fn test_repair_pronunciation_falls_back_to_reading() {
    let mut builder = DictBuilder::new();
    // 借用元がない場合は読みをそのまま発音にする
    builder.add_entry(entry("案件", "名詞,一般,*,*", "案件", "アンケン", "案件"));

    assert_eq!(builder.repair_pronunciation(), 1);
    assert_eq!(&*builder.entries()[0].pronunciation, "アンケン");
}

#[test]
fn test_repair_pronunciation_composes_from_parts() {
    let mut builder = DictBuilder::new();
    // 借用元が無い複合語でも、部品の発音が辞書にあれば組み立てられる
    builder.add_entry(entry("商", "名詞,一般,*,*", "商", "ショウ", "ショー"));
    builder.add_entry(entry("材", "名詞,一般,*,*", "材", "ザイ", "ザイ"));
    builder.add_entry(entry(
        "商材",
        "名詞,一般,*,*",
        "商材",
        "ショウザイ",
        "ショウザイ",
    ));

    builder.repair_pronunciation();

    let composed = builder
        .entries()
        .iter()
        .find(|e| &*e.surface == "商材")
        .expect("商材 entry");
    assert_eq!(&*composed.pronunciation, "ショーザイ");
}

#[test]
fn test_repair_pronunciation_keeps_morpheme_boundary() {
    let mut builder = DictBuilder::new();
    // 「小売」の「コウ」は「小(コ)」と「売(ウリ)」の境界なので長音にしてはいけない。
    // かな列だけを見て「オ段 + ウ」を長音化すると「コーリ」になってしまう
    builder.add_entry(entry("小", "名詞,一般,*,*", "小", "コ", "コ"));
    builder.add_entry(entry("小", "名詞,一般,*,*", "小", "ショウ", "ショー"));
    builder.add_entry(entry("売", "名詞,一般,*,*", "売", "ウリ", "ウリ"));
    builder.add_entry(entry("小売", "名詞,一般,*,*", "小売", "コウリ", "コウリ"));

    builder.repair_pronunciation();

    let kept = builder
        .entries()
        .iter()
        .find(|e| &*e.surface == "小売")
        .expect("小売 entry");
    assert_eq!(&*kept.pronunciation, "コウリ");
}

#[test]
fn test_repair_pronunciation_ignores_hiragana_parts() {
    let mut builder = DictBuilder::new();
    // 助詞を含む「の上(ノーエ)」を部品に使うと「雲の上」が「クモノーエ」になる。
    // ひらがなを含む語は部品にしない
    builder.add_entry(entry("雲", "名詞,一般,*,*", "雲", "クモ", "クモ"));
    builder.add_entry(entry("の上", "名詞,一般,*,*", "の上", "ノウエ", "ノーエ"));
    builder.add_entry(entry(
        "雲の上",
        "名詞,一般,*,*",
        "雲の上",
        "クモノウエ",
        "クモノウエ",
    ));

    builder.repair_pronunciation();

    let kept = builder
        .entries()
        .iter()
        .find(|e| &*e.surface == "雲の上")
        .expect("雲の上 entry");
    assert_eq!(&*kept.pronunciation, "クモノウエ");
}

#[test]
fn test_repair_pronunciation_composes_general_proper_noun() {
    let mut builder = DictBuilder::new();
    // NEologd は「高品質」のような普通名詞も「固有名詞,一般」で登録している
    builder.add_entry(entry("高", "名詞,一般,*,*", "高", "コウ", "コー"));
    builder.add_entry(entry(
        "品質",
        "名詞,一般,*,*",
        "品質",
        "ヒンシツ",
        "ヒンシツ",
    ));
    builder.add_entry(entry(
        "高品質",
        "名詞,固有名詞,一般,*",
        "高品質",
        "コウヒンシツ",
        "コウヒンシツ",
    ));

    builder.repair_pronunciation();

    let composed = builder
        .entries()
        .iter()
        .find(|e| &*e.surface == "高品質")
        .expect("高品質 entry");
    assert_eq!(&*composed.pronunciation, "コーヒンシツ");
}

#[test]
fn test_repair_pronunciation_recomposes_partially_repaired() {
    let mut builder = DictBuilder::new();
    // 借用元の発音が末尾しか直っていないことがある（「機密情報」が
    // 「キミツジョウホオ」で止まる）。長音化されていない部分が残っていれば
    // 分割して組み立て直す
    builder.add_entry(entry("機密", "名詞,一般,*,*", "機密", "キミツ", "キミツ"));
    builder.add_entry(entry(
        "情報",
        "名詞,一般,*,*",
        "情報",
        "ジョウホウ",
        "ジョーホー",
    ));
    builder.add_entry(entry(
        "機密情報",
        "名詞,一般,*,*",
        "機密情報",
        "キミツジョウホウ",
        "キミツジョウホオ",
    ));

    builder.repair_pronunciation();

    let composed = builder
        .entries()
        .iter()
        .find(|e| &*e.surface == "機密情報")
        .expect("機密情報 entry");
    assert_eq!(&*composed.pronunciation, "キミツジョーホー");
}

#[test]
fn test_repair_pronunciation_skips_composition_for_proper_nouns() {
    let mut builder = DictBuilder::new();
    // 人名・地名の読みは部品から組み立てられないので合成しない
    builder.add_entry(entry("幸", "名詞,一般,*,*", "幸", "コウ", "コー"));
    builder.add_entry(entry("太", "名詞,一般,*,*", "太", "タ", "タ"));
    builder.add_entry(entry(
        "幸太",
        "名詞,固有名詞,人名,名",
        "幸太",
        "コウタ",
        "コウタ",
    ));

    builder.repair_pronunciation();

    let kept = builder
        .entries()
        .iter()
        .find(|e| &*e.surface == "幸太")
        .expect("幸太 entry");
    assert_eq!(&*kept.pronunciation, "コウタ");
}

#[test]
fn test_repair_pronunciation_clears_non_kana_reading() {
    let mut builder = DictBuilder::new();
    // 読みもラテン文字のままなら空にして、解析時の読み補完に委ねる
    builder.add_entry(entry(
        "Siemens",
        "名詞,固有名詞,人名,一般",
        "Siemens",
        "siemens",
        "Siemens",
    ));

    assert_eq!(builder.repair_pronunciation(), 1);
    assert_eq!(&*builder.entries()[0].reading, "");
    assert_eq!(&*builder.entries()[0].pronunciation, "");
}

#[test]
fn test_drop_conflicting_ortho_variants() {
    let mut builder = DictBuilder::new();
    // 形容詞「高い(タカイ)」と、表記ゆれ由来の名詞「高い→高位(コウイ)」
    builder.add_entry(entry("高い", "形容詞,自立,*,*", "高い", "タカイ", "タカイ"));
    builder.add_entry(entry("高い", "名詞,一般,*,*", "高位", "コウイ", "コウイ"));
    // 読みが一致する異表記は誤読にならないので残す
    builder.add_entry(entry(
        "くらい",
        "助詞,副助詞,*,*",
        "くらい",
        "クライ",
        "クライ",
    ));
    builder.add_entry(entry("くらい", "名詞,一般,*,*", "位", "クライ", "クライ"));
    // 衝突する活用語がない名詞はそのまま
    builder.add_entry(entry("高位", "名詞,一般,*,*", "高位", "コウイ", "コウイ"));

    assert_eq!(builder.drop_conflicting_ortho_variants(), 1);

    let surfaces: Vec<(&str, &str)> = builder
        .entries()
        .iter()
        .map(|e| (&*e.surface, &*e.reading))
        .collect();
    assert!(!surfaces.contains(&("高い", "コウイ")));
    assert!(surfaces.contains(&("高い", "タカイ")));
    assert!(surfaces.contains(&("くらい", "クライ")));
    assert!(surfaces.contains(&("高位", "コウイ")));
}

#[test]
fn test_short_alpha_ignores_dict_reading() {
    let mut builder = DictBuilder::new();
    // 単独英字に単位読み・略称読みが登録されていても、綴り読みを優先する
    builder.add_entry(entry("A", "名詞,一般,*,*", "A", "アンペア", "アンペア"));
    builder.add_entry(entry(
        "cs",
        "名詞,固有名詞,一般,*",
        "CS",
        "クレディスイス",
        "クレディスイス",
    ));
    // 3 文字以上の語は辞書の読みを尊重する
    builder.add_entry(entry(
        "NASA",
        "名詞,固有名詞,組織,*",
        "NASA",
        "ナサ",
        "ナサ",
    ));

    let mut analyzer = Analyzer::from_dict(builder.build());

    let tokens = analyzer.tokenize("A");
    assert_eq!(&*tokens[0].reading, "エー");

    let tokens = analyzer.tokenize("cs");
    assert_eq!(&*tokens[0].reading, "シーエス");

    let tokens = analyzer.tokenize("NASA");
    assert_eq!(&*tokens[0].reading, "ナサ");
}

#[test]
fn test_broken_dict_reading_falls_back_to_spellout() {
    let mut builder = DictBuilder::new();
    // 読みがラテン文字のままのエントリは信用せず綴り読みにする
    builder.add_entry(entry(
        "backend",
        "名詞,一般,*,*",
        "backend",
        "backend",
        "backend",
    ));

    let mut analyzer = Analyzer::from_dict(builder.build());
    let tokens = analyzer.tokenize("backend");
    assert_eq!(&*tokens[0].reading, "ビーエーシーケーイーエヌディー");
}

#[test]
fn test_drop_numeral_misreadings() {
    let mut builder = DictBuilder::new();
    // 漢数字だけで綴られた人名エントリは数詞に勝ってしまうので落とす
    builder.add_entry(entry("十五", "名詞,数,*,*", "十五", "ジュウゴ", "ジューゴ"));
    builder.add_entry(entry(
        "十五",
        "名詞,固有名詞,人名,名",
        "十五",
        "トウゴ",
        "トーゴ",
    ));
    // 1 文字の漢数字は対象外（人名「一(はじめ)」等との共存が必要）
    builder.add_entry(entry(
        "一",
        "名詞,固有名詞,人名,名",
        "一",
        "ハジメ",
        "ハジメ",
    ));
    // 漢数字以外を含む語は対象外
    builder.add_entry(entry(
        "十五夜",
        "名詞,固有名詞,一般,*",
        "十五夜",
        "ジュウゴヤ",
        "ジューゴヤ",
    ));

    assert_eq!(builder.drop_numeral_misreadings(), 1);

    let readings: Vec<&str> = builder.entries().iter().map(|e| &*e.reading).collect();
    assert!(!readings.contains(&"トウゴ"));
    assert!(readings.contains(&"ジュウゴ"));
    assert!(readings.contains(&"ハジメ"));
    assert!(readings.contains(&"ジュウゴヤ"));
}

#[test]
fn test_keep_numeral_words_that_are_not_proper_nouns() {
    let mut builder = DictBuilder::new();
    // 漢数字で綴る一般語・副詞は数詞でなくても残す
    builder.add_entry(entry(
        "万一",
        "副詞,助詞類接続,*,*",
        "万一",
        "マンイチ",
        "マンイチ",
    ));
    builder.add_entry(entry(
        "二三",
        "名詞,副詞可能,*,*",
        "二三",
        "ニサン",
        "ニサン",
    ));
    builder.add_entry(entry(
        "八百万",
        "名詞,一般,*,*",
        "八百万",
        "ヤオヨロズ",
        "ヤオヨロズ",
    ));

    assert_eq!(builder.drop_numeral_misreadings(), 0);
    assert_eq!(builder.entries().len(), 3);
}

#[test]
fn test_drop_person_name_conflicting_with_pronoun() {
    let mut builder = DictBuilder::new();
    // 代名詞と衝突する 1 文字の人名は落とす（「何なのか」が「ガナノカ」になるのを防ぐ）
    builder.add_entry(entry("何", "名詞,代名詞,一般,*", "何", "ナニ", "ナニ"));
    builder.add_entry(entry("何", "名詞,固有名詞,人名,姓", "何", "ガ", "ガ"));
    // 代名詞と衝突しない人名はそのまま
    builder.add_entry(entry(
        "湊",
        "名詞,固有名詞,人名,名",
        "湊",
        "ミナト",
        "ミナト",
    ));
    // 2 文字以上の人名は代名詞と衝突しても対象外。実辞書にある衝突をそのまま使う
    // （代名詞「貴郎(アナタ)」と人名「貴郎(タカオ)」）
    builder.add_entry(entry(
        "貴郎",
        "名詞,代名詞,一般,*",
        "貴郎",
        "アナタ",
        "アナタ",
    ));
    builder.add_entry(entry(
        "貴郎",
        "名詞,固有名詞,人名,名",
        "貴郎",
        "タカオ",
        "タカオ",
    ));

    assert_eq!(builder.drop_conflicting_ortho_variants(), 1);

    let readings: Vec<&str> = builder.entries().iter().map(|e| &*e.reading).collect();
    assert!(!readings.contains(&"ガ"));
    assert!(readings.contains(&"ナニ"));
    assert!(readings.contains(&"ミナト"));
    assert!(readings.contains(&"アナタ"));
    assert!(readings.contains(&"タカオ"));
}

#[test]
fn test_repair_prefers_long_vowel_form_over_reading() {
    let mut builder = DictBuilder::new();
    // SudachiDict 由来のエントリは発音が読みと同じで長音表記を持たない。
    // IPAdic 由来の「リョー」を借りて長音を復元する
    builder.add_entry(entry("量", "名詞,一般,*,*", "量", "リョウ", "リョー"));
    builder.add_entry(entry("量", "名詞,接尾,一般,*", "量", "リョウ", "リョウ"));
    // 発音と読みが同じでも、他に候補がなければそのまま（「思う」を「オモー」にしない）
    builder.add_entry(entry("思う", "動詞,自立,*,*", "思う", "オモウ", "オモウ"));

    assert_eq!(builder.repair_pronunciation(), 1);

    let entries = builder.entries();
    assert_eq!(&*entries[0].pronunciation, "リョー");
    assert_eq!(&*entries[1].pronunciation, "リョー");
    assert_eq!(&*entries[2].pronunciation, "オモウ");
}
