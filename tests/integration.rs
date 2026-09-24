//! インテグレーションテスト: 辞書構築→解析→出力の一貫性検証
//! （辞書を組み立てるので `build` feature が要る）
#![cfg(feature = "build")]

use hasami::Dictionary;
use hasami::analyzer::{Analyzer, format_mecab, format_wakachi};
use hasami::dict::{DictBuilder, DictEntry};

/// ビルダーを .hsd に書き出す
fn write_hsd(builder: &DictBuilder, path: &std::path::Path) {
    builder
        .write_hsd(path, &builder.write_options(), |_, _| {})
        .unwrap();
}

/// テスト用の辞書を構築するヘルパー
fn build_test_dictionary() -> Dictionary {
    test_builder().build().unwrap()
}

/// テスト用の辞書のビルダー
fn test_builder() -> DictBuilder {
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
            ..Default::default()
        });
    }

    builder
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
    let tmp = std::env::temp_dir().join("hasami_integration_test.hsd");
    write_hsd(&test_builder(), &tmp);

    // Load and tokenize via mmap path
    let mut analyzer = Analyzer::load(&tmp).unwrap();
    let tokens = analyzer.tokenize("私は猫です");

    let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
    assert_eq!(surfaces, vec!["私", "は", "猫", "です"]);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_mmap_roundtrip_preserves_metadata() {
    let tmp = std::env::temp_dir().join("hasami_integration_meta.hsd");
    write_hsd(&test_builder(), &tmp);
    let loaded = Dictionary::load(&tmp).unwrap();

    // Verify all entries survive roundtrip
    assert_eq!(loaded.entry_count(), test_builder().entry_count());

    // Verify specific entry data
    loaded
        .for_each_entry(|e| {
            assert!(!e.surface.is_empty(), "Entry has empty surface");
            assert!(!e.pos.is_empty(), "Entry {} has empty POS", e.surface);
            Ok(())
        })
        .unwrap();

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
        ..Default::default()
    });
    let tmp1 = std::env::temp_dir().join("hasami_merge_base.hsd");
    write_hsd(&builder1, &tmp1);

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
        ..Default::default()
    });
    assert_eq!(builder2.entry_count(), 2);

    let tmp2 = std::env::temp_dir().join("hasami_merge_result.hsd");
    write_hsd(&builder2, &tmp2);

    let loaded = Dictionary::load(&tmp2).unwrap();
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
    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());

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
    let tmp = std::env::temp_dir().join("hasami_clone_share.hsd");
    write_hsd(&test_builder(), &tmp);

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
    let tmp = std::env::temp_dir().join("hasami_concurrent.hsd");
    write_hsd(&test_builder(), &tmp);

    let analyzer = Analyzer::load(&tmp).unwrap();
    analyzer.prewarm(); // 並列前に辞書のページを読み込んでおく

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
    let tmp = std::env::temp_dir().join("hasami_prewarm.hsd");
    write_hsd(&test_builder(), &tmp);

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
fn test_old_format_versions_are_rejected_with_rebuild_hint() {
    // v4 のファイルの version (offset=8..12) を旧形式の番号に書き換えると、
    // 作り直しを案内するエラーで拒否する
    let tmp = std::env::temp_dir().join(format!("hasami_{}_old_version.hsd", std::process::id()));
    write_hsd(&test_builder(), &tmp);
    let original = std::fs::read(&tmp).unwrap();

    for version in [2u32, 3] {
        let mut bytes = original.clone();
        bytes[8..12].copy_from_slice(&version.to_le_bytes());
        std::fs::write(&tmp, &bytes).unwrap();
        let err = Analyzer::load(&tmp).err().unwrap().to_string();
        assert!(err.contains("build-dict.sh"), "{err}");
    }

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_char_def_roundtrip() {
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
        ..Default::default()
    });
    builder.set_char_classifier(custom_classifier);

    let tmp = std::env::temp_dir().join("hasami_char_def_v4.hsd");
    write_hsd(&builder, &tmp);

    let loaded = Dictionary::load(&tmp).unwrap();
    let restored = loaded.char_classifier();

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
        ..Default::default()
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

    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());

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

    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
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

// ==========================================================================
// 削除リスト CSV（品詞で限定する 3 列目）
// ==========================================================================

/// テスト用の一時ファイルに内容を書き、パスを返す（並列実行で衝突しないよう PID を付ける）
fn write_temp(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("hasami_{}_{}", std::process::id(), name));
    std::fs::write(&path, content).unwrap();
    path
}

/// 同じ表層形・読みで品詞だけ違うエントリを持つビルダー
fn builder_with_name_collisions() -> DictBuilder {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("林", "名詞,固有名詞,人名,姓", "林", "リン", "リン"));
    builder.add_entry(entry("林", "名詞,接尾,一般,*", "林", "リン", "リン"));
    builder.add_entry(entry(
        "林",
        "名詞,固有名詞,人名,姓",
        "林",
        "ハヤシ",
        "ハヤシ",
    ));
    builder
}

fn readings_with_pos(builder: &DictBuilder) -> Vec<(String, String)> {
    builder
        .entries()
        .iter()
        .map(|e| (e.reading.to_string(), e.pos.to_string()))
        .collect()
}

#[test]
fn test_remove_csv_two_columns_matches_every_pos() {
    let mut builder = builder_with_name_collisions();
    let csv = write_temp("remove_two_columns.csv", "# 既存の 2 列形式\n\n林,リン\n");

    let stats = builder.drop_entries_from_csv(&csv).unwrap();

    assert_eq!(stats.rows, 1);
    assert_eq!(stats.dropped, 2);
    assert_eq!(stats.unmatched_rows, 0);
    let rest = readings_with_pos(&builder);
    assert_eq!(
        rest,
        vec![("ハヤシ".to_string(), "名詞,固有名詞,人名,姓".to_string())]
    );
    let _ = std::fs::remove_file(&csv);
}

#[test]
fn test_remove_csv_pos_prefix_limits_removal() {
    let mut builder = builder_with_name_collisions();
    let csv = write_temp("remove_pos_prefix.csv", "林,リン,\"名詞,固有名詞,人名\"\n");

    let stats = builder.drop_entries_from_csv(&csv).unwrap();

    // 人名の「林(リン)」だけが落ち、同じ表層形・読みの接尾語と「林(ハヤシ)」は残る
    assert_eq!(stats.dropped, 1);
    let rest = readings_with_pos(&builder);
    assert!(rest.contains(&("リン".to_string(), "名詞,接尾,一般,*".to_string())));
    assert!(rest.contains(&("ハヤシ".to_string(), "名詞,固有名詞,人名,姓".to_string())));
    assert_eq!(rest.len(), 2);
    let _ = std::fs::remove_file(&csv);
}

#[test]
fn test_remove_csv_pos_prefix_is_element_wise() {
    let mut builder = builder_with_name_collisions();
    // 「人」は「人名」の途中までなので、要素単位の前方一致では一致しない
    let csv = write_temp(
        "remove_pos_element_wise.csv",
        "林,リン,\"名詞,固有名詞,人\"\n",
    );

    let stats = builder.drop_entries_from_csv(&csv).unwrap();

    assert_eq!(stats.dropped, 0);
    assert_eq!(stats.unmatched_rows, 1);
    assert_eq!(builder.entry_count(), 3);
    let _ = std::fs::remove_file(&csv);
}

#[test]
fn test_remove_csv_reports_unmatched_rows_with_line_numbers() {
    let mut builder = builder_with_name_collisions();
    let csv = write_temp(
        "remove_unmatched.csv",
        "# 外国人名\n\n林,リン,\"名詞,固有名詞,人名\"\n  # 字下げしたコメント\n王,ワン,\"名詞,固有名詞,人名\"\n李,リ\n",
    );

    let stats = builder.drop_entries_from_csv(&csv).unwrap();

    assert_eq!(stats.rows, 3);
    assert_eq!(stats.dropped, 1);
    assert_eq!(stats.unmatched_rows, 2);
    assert_eq!(
        stats.unmatched_samples,
        vec![
            "5: 王,ワン,\"名詞,固有名詞,人名\"".to_string(),
            "6: 李,リ".to_string()
        ]
    );
    let _ = std::fs::remove_file(&csv);
}

#[test]
fn test_remove_csv_rejects_row_without_reading() {
    let mut builder = builder_with_name_collisions();
    let csv = write_temp("remove_short_row.csv", "林,リン\n林\n");

    let err = builder.drop_entries_from_csv(&csv).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains(&format!("{}:2:", csv.display())), "{msg}");
    // エラーのときは 1 件も消さない
    assert_eq!(builder.entry_count(), 3);
    let _ = std::fs::remove_file(&csv);
}

// ==========================================================================
// 接続行列の範囲外の文脈 ID
// ==========================================================================

fn entry_with_ids(surface: &str, left_id: u16, right_id: u16) -> DictEntry {
    DictEntry {
        left_id,
        right_id,
        ..entry(surface, "名詞,一般,*,*", surface, "テスト", "テスト")
    }
}

/// left_id が 3 種類、right_id が 2 種類の接続行列
fn small_matrix() -> hasami::dict::ConnectionMatrix {
    hasami::dict::ConnectionMatrix::zeros(3, 2)
}

#[test]
fn test_drop_invalid_context_ids() {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry_with_ids("有効", 2, 1));
    builder.add_entry(entry_with_ids("左が範囲外", 3, 1));
    builder.add_entry(entry_with_ids("右が範囲外", 0, 2));

    // 接続行列が無ければ判定できないので何もしない
    assert_eq!(builder.drop_invalid_context_ids(), 0);

    builder.set_matrix(small_matrix());
    assert_eq!(builder.drop_invalid_context_ids(), 2);
    let surfaces: Vec<&str> = builder.entries().iter().map(|e| &*e.surface).collect();
    assert_eq!(surfaces, vec!["有効"]);
    assert!(builder.check_context_ids().is_ok());
}

#[test]
fn test_check_context_ids_reports_out_of_range_entries() {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry_with_ids("有効", 2, 1));
    builder.add_entry(entry_with_ids("範囲外", 5, 1));
    // 接続行列が無ければ検査しない
    assert!(builder.check_context_ids().is_ok());

    builder.set_matrix(small_matrix());
    let msg = builder.check_context_ids().unwrap_err().to_string();
    assert!(msg.contains("1 entries have context IDs"), "{msg}");
    assert!(msg.contains("範囲外"), "{msg}");
    assert!(msg.contains("--drop-invalid-context-ids"), "{msg}");
}

#[test]
fn test_check_context_ids_covers_unknown_word_templates() {
    let mut builder = DictBuilder::new();
    builder.set_matrix(small_matrix());
    let unk = write_temp("unk_out_of_range.def", "DEFAULT,5,1,100,名詞,一般,*,*\n");
    builder.load_unk(&unk).unwrap();

    let msg = builder.check_context_ids().unwrap_err().to_string();
    assert!(msg.contains("unknown-word template DEFAULT"), "{msg}");
    let _ = std::fs::remove_file(&unk);
}

#[test]
fn test_add_csv_rejects_out_of_range_ids_with_line_number() {
    let csv = write_temp(
        "lex_out_of_range.csv",
        "猫,1,1,100,名詞,一般,*,*,*,*,猫,ネコ,ネコ\n犬,1,7,100,名詞,一般,*,*,*,*,犬,イヌ,イヌ\n",
    );

    // 接続行列が無ければ範囲を判定できないので読み込める
    let mut without_matrix = DictBuilder::new();
    without_matrix.add_csv(&csv).unwrap();
    assert_eq!(without_matrix.entry_count(), 2);

    let mut builder = DictBuilder::new();
    builder.set_matrix(small_matrix());
    let msg = builder.add_csv(&csv).unwrap_err().to_string();
    assert!(msg.contains(&format!("{}:2:", csv.display())), "{msg}");
    assert!(msg.contains("right_id=7"), "{msg}");
    let _ = std::fs::remove_file(&csv);
}

// ==========================================================================
// export（.hsd → MeCab 形式 CSV）
// ==========================================================================

#[test]
fn test_export_roundtrip_through_add_csv() {
    let mut builder = DictBuilder::new();
    let originals = vec![
        DictEntry {
            left_id: 3,
            right_id: 4,
            cost: -120,
            ..entry(
                "東京",
                "名詞,固有名詞,地域,一般",
                "東京",
                "トウキョウ",
                "トーキョー",
            )
        },
        // 表層形・品詞に区切り文字を含む語（半角カンマの読点）
        entry(",", "記号,読点,*,*", ",", "、", "、"),
        // 原形が表層形と違い、活用型・活用形を持つ語
        DictEntry {
            conj_type: "一段".into(),
            conj_form: "連用形".into(),
            ..entry("食べ", "動詞,自立,*,*", "食べる", "タベ", "タベ")
        },
        // 読み・発音が空の語
        entry("backend", "名詞,一般,*,*", "backend", "", ""),
        // 引用符を含む語
        entry(
            "\"引用\"",
            "記号,一般,*,*",
            "\"引用\"",
            "インヨウ",
            "インヨー",
        ),
    ];
    for e in &originals {
        builder.add_entry(e.clone());
    }
    let hsd = std::env::temp_dir().join(format!("hasami_{}_export.hsd", std::process::id()));
    write_hsd(&builder, &hsd);

    let loaded = Dictionary::load(&hsd).unwrap();
    let mut buf = Vec::new();
    assert_eq!(
        hasami::dict::write_lexicon_csv(&loaded, &mut buf).unwrap(),
        5
    );
    let csv = write_temp("export_roundtrip.csv", std::str::from_utf8(&buf).unwrap());

    let mut restored = DictBuilder::new();
    restored.add_csv(&csv).unwrap();

    let key = |e: &DictEntry| {
        (
            e.surface.to_string(),
            e.left_id,
            e.right_id,
            e.cost,
            e.pos.to_string(),
            e.conj_type.to_string(),
            e.conj_form.to_string(),
            e.base_form.to_string(),
            e.reading.to_string(),
            e.pronunciation.to_string(),
        )
    };
    // 活用型・活用形が無い語は `*` で書き出され、`*` のまま読み戻る
    let originals: Vec<DictEntry> = originals
        .into_iter()
        .map(|e| DictEntry {
            conj_type: if e.conj_type.is_empty() {
                "*".into()
            } else {
                e.conj_type
            },
            conj_form: if e.conj_form.is_empty() {
                "*".into()
            } else {
                e.conj_form
            },
            ..e
        })
        .collect();
    let mut expected: Vec<_> = originals.iter().map(key).collect();
    let mut actual: Vec<_> = restored.entries().iter().map(key).collect();
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);

    let _ = std::fs::remove_file(&hsd);
    let _ = std::fs::remove_file(&csv);
}

// ==========================================================================
// hasami repair の CLI
// ==========================================================================

/// `hasami repair` を実行し、書き出された辞書の (表層形, 読み) を返す
fn run_cli_repair(name: &str, extra_args: &[&str]) -> Vec<(String, String)> {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("、", "記号,読点,*,*", "、", "、", "、"));
    builder.add_entry(entry("林", "名詞,固有名詞,人名,姓", "林", "リン", "リン"));
    builder.add_entry(entry(
        "林",
        "名詞,固有名詞,人名,姓",
        "林",
        "ハヤシ",
        "ハヤシ",
    ));
    let dir = std::env::temp_dir();
    let input = dir.join(format!("hasami_{}_{name}_in.hsd", std::process::id()));
    let output = dir.join(format!("hasami_{}_{name}_out.hsd", std::process::id()));
    write_hsd(&builder, &input);
    let list = write_temp(&format!("{name}.csv"), "林,リン,\"名詞,固有名詞,人名\"\n");

    let result = std::process::Command::new(env!("CARGO_BIN_EXE_hasami"))
        .arg("repair")
        .arg("--dict")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--remove")
        .arg(&list)
        .args(extra_args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "hasami repair failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let repaired = Dictionary::load(&output).unwrap();
    let mut entries = Vec::new();
    repaired
        .for_each_entry(|e| {
            entries.push((e.surface.to_string(), e.reading.to_string()));
            Ok(())
        })
        .unwrap();
    for path in [&input, &output, &list] {
        let _ = std::fs::remove_file(path);
    }
    entries
}

/// `--no-pronunciation-repair` は削除リストだけを適用し、記号の読みを変えない
#[test]
fn test_cli_repair_without_pronunciation_repair_keeps_symbol_readings() {
    let entries = run_cli_repair("cli_repair_keep", &["--no-pronunciation-repair"]);
    assert!(entries.contains(&("、".into(), "、".into())), "{entries:?}");
    assert!(
        entries.contains(&("林".into(), "ハヤシ".into())),
        "{entries:?}"
    );
    assert!(
        !entries.contains(&("林".into(), "リン".into())),
        "{entries:?}"
    );
}

/// 既定では発音の修復が走るが、記号の読み (記号そのもの) は空にしない
#[test]
fn test_cli_repair_keeps_symbol_readings_by_default() {
    let entries = run_cli_repair("cli_repair_default", &[]);
    assert!(entries.contains(&("、".into(), "、".into())), "{entries:?}");
    assert!(
        !entries.contains(&("林".into(), "リン".into())),
        "{entries:?}"
    );
}

/// 読みも発音もカタカナでない記号以外の語 (ラテン文字の読み) は、発音の修復で読みを空にして
/// 解析時の綴り読みに任せる
#[test]
fn test_repair_pronunciation_clears_latin_readings_but_keeps_symbols() {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("、", "記号,読点,*,*", "、", "、", "、"));
    builder.add_entry(entry(
        "Siemens",
        "名詞,固有名詞,組織,*",
        "Siemens",
        "siemens",
        "siemens",
    ));
    assert_eq!(builder.repair_pronunciation(), 1);
    let readings: Vec<(&str, &str)> = builder
        .entries()
        .iter()
        .map(|e| (&*e.surface, &*e.reading))
        .collect();
    assert_eq!(readings, vec![("、", "、"), ("Siemens", "")]);
}

/// matrix.def なしで作った辞書（使われている ID を覆うゼロ行列）を読み込んでも、文脈 ID の
/// 検査で全エントリが範囲外にならず、`drop_invalid_context_ids` も何も消さない
#[test]
fn test_load_hsd_without_matrix_keeps_all_entries() {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("猫", "名詞,一般,*,*", "猫", "ネコ", "ネコ"));
    builder.add_entry(entry("犬", "名詞,一般,*,*", "犬", "イヌ", "イヌ"));
    let hsd = std::env::temp_dir().join(format!("hasami_{}_no_matrix.hsd", std::process::id()));
    write_hsd(&builder, &hsd);

    let mut loaded = DictBuilder::new();
    loaded.load_hsd(&hsd).unwrap();
    assert!(loaded.check_context_ids().is_ok());
    assert_eq!(loaded.drop_invalid_context_ids(), 0);
    assert_eq!(loaded.entry_count(), 2);

    let _ = std::fs::remove_file(&hsd);
}

/// MeCab 形式 CSV の `#` で始まる行は、エントリの列数に満たなければコメントとして読み飛ばす。
/// `#` で始まるハッシュタグの語 (13 列そろった行) は読み込む
#[test]
fn test_add_csv_skips_comment_lines_but_keeps_hashtag_entries() {
    let csv = write_temp(
        "add_csv_comments.csv",
        "# 「MySQL」の読みを足す\n\
         MySQL,1288,1288,-9000,名詞,固有名詞,一般,*,*,*,MySQL,マイエスキューエル,マイエスキューエル\n\
         \n\
         #タグ,1288,1288,3942,名詞,固有名詞,一般,*,*,*,#タグ,タグ,タグ\n",
    );
    let mut builder = DictBuilder::new();
    builder.add_csv(&csv).unwrap();
    let surfaces: Vec<&str> = builder.entries().iter().map(|e| &*e.surface).collect();
    assert_eq!(surfaces, vec!["MySQL", "#タグ"]);
    let _ = std::fs::remove_file(&csv);
}

/// CSV のエラーは空行を含めた実際の行番号で報告する
#[test]
fn test_add_csv_reports_physical_line_number() {
    let csv = write_temp(
        "add_csv_line_no.csv",
        "猫,1,1,3000,名詞,一般,*,*,*,*,猫,ネコ,ネコ\n\n\n犬,1,1\n",
    );
    let mut builder = DictBuilder::new();
    let err = builder.add_csv(&csv).unwrap_err().to_string();
    assert!(err.contains(":4: too few columns"), "{err}");
    let _ = std::fs::remove_file(&csv);
}
