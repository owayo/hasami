//! 動詞・固有名詞への機能語由来の直接借用を防ぎ、既存の長音修復を保つ。

#![cfg(feature = "build")]

use hasami::dict::{DictBuilder, DictEntry};

fn entry(surface: &str, pos: &str, reading: &str, pronunciation: &str) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        pos: pos.into(),
        base_form: surface.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
        ..Default::default()
    }
}

#[test]
fn particle_and_verb_pronunciations_survive_both_input_orders() {
    let mut verb_he = entry("へ", "動詞,自立,*,*", "ヘ", "ヘ");
    verb_he.base_form = "へる".into();
    verb_he.conj_type = "一段".into();
    verb_he.conj_form = "連用形".into();
    let mut verb_ha = entry("は", "動詞,自立,*,*", "ハ", "ハ");
    verb_ha.base_form = "はる".into();
    verb_ha.conj_type = "五段・ラ行".into();
    verb_ha.conj_form = "体言接続特殊２".into();
    let entries = [
        entry("へ", "助詞,格助詞,一般,*", "ヘ", "エ"),
        verb_he,
        entry("は", "助詞,係助詞,*,*", "ハ", "ワ"),
        verb_ha,
    ];
    for reverse in [false, true] {
        let mut builder = DictBuilder::new();
        let mut ordered = entries.to_vec();
        if reverse {
            ordered.reverse();
        }
        for value in ordered {
            builder.add_entry(value);
        }
        let before = format!("{:?}", builder.entries());
        assert_eq!(builder.repair_pronunciation(), 0, "reverse={reverse}");
        // 発音に加え、原形・活用・文脈ID・コストなどの素性もそのまま残す。
        assert_eq!(format!("{:?}", builder.entries()), before);
        assert_eq!(builder.repair_pronunciation(), 0);
    }
}

#[test]
fn only_verb_and_proper_noun_targets_veto_function_word_donors() {
    let roles = [
        ("助詞,格助詞,一般,*", false, false),
        ("助動詞,*,*,*", false, false),
        ("名詞,一般,*,*", true, false),
        ("動詞,自立,*,*", true, true),
        ("名詞,固有名詞,一般,*", true, true),
        ("副詞,一般,*,*", true, false),
        ("形容詞,自立,*,*", true, false),
        // 品詞要素の一部だけが一致する文字列は、助詞・動詞・固有名詞とみなさない。
        ("助詞類,*,*,*", true, false),
        ("動詞的,*,*,*", true, false),
        ("名詞,固有名詞的,*,*", true, false),
    ];
    // 人工entryで借用元・借用先の品詞だけを切り分ける。
    for (donor_pos, lexical_donor, _) in roles {
        for (target_pos, _, restricted_target) in roles {
            let permitted = !restricted_target || lexical_donor;
            let mut builder = DictBuilder::new();
            builder.add_entry(entry("同形", donor_pos, "コウ", "コー"));
            builder.add_entry(entry("同形", target_pos, "コウ", "コウ"));
            assert_eq!(
                builder.repair_pronunciation(),
                usize::from(permitted),
                "donor={donor_pos}, target={target_pos}"
            );
            assert_eq!(
                &*builder.entries()[1].pronunciation,
                if permitted { "コー" } else { "コウ" }
            );
            assert_eq!(builder.repair_pronunciation(), 0);
        }
    }
}

#[test]
fn replacement_drops_old_function_evidence_and_ignores_nonselected_pronunciations() {
    let mut builder = DictBuilder::new();
    // 最初の読み通りの候補にある機能語の証拠は、異なる発音へ交替した時に持ち越さない。
    builder.add_entry(entry("同形", "助詞,格助詞,一般,*", "コウ", "コウ"));
    builder.add_entry(entry("同形", "名詞,一般,*,*", "コウ", "コー"));
    // 後の別PNは従来どおり採用せず、その機能語の証拠でも選択PNを拒まない。
    builder.add_entry(entry("同形", "助動詞,*,*,*", "コウ", "ゴー"));
    builder.add_entry(entry("同形", "動詞,自立,*,*", "コウ", "コウ"));
    assert_eq!(builder.repair_pronunciation(), 2);
    assert_eq!(&*builder.entries()[0].pronunciation, "コー");
    assert_eq!(&*builder.entries()[1].pronunciation, "コー");
    assert_eq!(&*builder.entries()[2].pronunciation, "ゴー");
    assert_eq!(&*builder.entries()[3].pronunciation, "コー");
    assert_eq!(builder.repair_pronunciation(), 0);
}

#[test]
fn equal_selected_pronunciations_veto_even_with_a_separate_lexical_arc() {
    let particle = entry("同形", "助詞,格助詞,一般,*", "コウ", "コー");
    let lexical = entry("同形", "名詞,一般,*,*", "コウ", "コー");
    assert!(!std::sync::Arc::ptr_eq(
        &particle.pronunciation,
        &lexical.pronunciation
    ));
    for reverse in [false, true] {
        let mut builder = DictBuilder::new();
        let sources = if reverse {
            [lexical.clone(), particle.clone()]
        } else {
            [particle.clone(), lexical.clone()]
        };
        for source in sources {
            builder.add_entry(source);
        }
        builder.add_entry(entry("同形", "動詞,自立,*,*", "コウ", "コウ"));
        builder.add_entry(entry("同形", "名詞,固有名詞,一般,*", "コウ", "コウ"));
        assert_eq!(builder.repair_pronunciation(), 0, "reverse={reverse}");
        assert_eq!(&*builder.entries()[2].pronunciation, "コウ");
        assert_eq!(&*builder.entries()[3].pronunciation, "コウ");
        assert_eq!(builder.repair_pronunciation(), 0);
    }
}

#[test]
fn common_noun_in_direction_context_keeps_particle_pronunciation() {
    let mut builder = DictBuilder::new();
    let mut particle = entry("へ", "助詞,格助詞,一般,*", "ヘ", "エ");
    particle.cost = 1000;
    let mut noun = entry("へ", "名詞,一般,*,*", "ヘ", "ヘ");
    noun.cost = -1000;
    for value in [
        entry("町", "名詞,一般,*,*", "マチ", "マチ"),
        particle,
        noun,
        entry("と", "助詞,格助詞,引用,*", "ト", "ト"),
    ] {
        builder.add_entry(value);
    }
    let fixed = builder.repair_pronunciation();
    assert_eq!(builder.repair_pronunciation(), 0);
    let mut analyzer = hasami::Analyzer::from_dict(builder.build().unwrap());
    let tokens = analyzer.tokenize("町へと");
    assert_eq!(tokens.len(), 3);
    assert_eq!(&*tokens[1].surface, "へ");
    assert_eq!(&*tokens[1].pos, "名詞,一般,*,*");
    assert_eq!(&*tokens[1].pronunciation, "エ");
    assert_eq!(fixed, 1);
}

#[test]
fn common_noun_borrowing_does_not_override_verb_on_a_second_repair() {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("へ", "助詞,格助詞,一般,*", "ヘ", "エ"));
    builder.add_entry(entry("へ", "名詞,一般,*,*", "ヘ", "ヘ"));
    builder.add_entry(entry("へ", "動詞,自立,*,*", "ヘ", "ヘ"));
    assert_eq!(builder.repair_pronunciation(), 1);
    assert_eq!(&*builder.entries()[1].pronunciation, "エ");
    assert_eq!(&*builder.entries()[2].pronunciation, "ヘ");
    assert_eq!(builder.repair_pronunciation(), 0);
    assert_eq!(&*builder.entries()[2].pronunciation, "ヘ");
}

#[test]
fn adverb_keeps_auxiliary_long_vowel_while_verb_is_protected() {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("とう", "助動詞,*,*,*", "トウ", "トー"));
    builder.add_entry(entry("とう", "副詞,助詞類接続,*,*", "トウ", "トウ"));
    builder.add_entry(entry("とう", "動詞,自立,*,*", "トウ", "トウ"));
    let fixed = builder.repair_pronunciation();
    assert_eq!(&*builder.entries()[1].pronunciation, "トー");
    assert_eq!(&*builder.entries()[2].pronunciation, "トウ");
    assert_eq!(fixed, 1);
    assert_eq!(builder.repair_pronunciation(), 0);
}

#[test]
fn particle_target_keeps_its_previous_lexical_borrowing() {
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("ヲ", "名詞,固有名詞,一般,*", "ヲ", "オ"));
    builder.add_entry(entry("ヲ", "助詞,格助詞,一般,*", "ヲ", "ヲ"));
    let fixed = builder.repair_pronunciation();
    assert_eq!(&*builder.entries()[1].pronunciation, "オ");
    assert_eq!(fixed, 1);
    assert_eq!(builder.repair_pronunciation(), 0);
}

#[test]
fn auxiliary_pronunciations_do_not_replace_verb_or_proper_noun_values() {
    let entries = [
        entry("とう", "助動詞,*,*,*", "トウ", "トー"),
        entry("とう", "動詞,自立,*,*", "トウ", "トウ"),
        entry("なかつ", "助動詞,*,*,*", "ナカツ", "ナカッ"),
        entry("なかつ", "名詞,固有名詞,一般,*", "ナカツ", "ナカツ"),
    ];
    for reverse in [false, true] {
        let mut builder = DictBuilder::new();
        let mut ordered = entries.to_vec();
        if reverse {
            ordered.reverse();
        }
        for value in ordered {
            builder.add_entry(value);
        }
        let before = format!("{:?}", builder.entries());
        assert_eq!(builder.repair_pronunciation(), 0, "reverse={reverse}");
        assert_eq!(format!("{:?}", builder.entries()), before);
        assert_eq!(builder.repair_pronunciation(), 0);
    }
}

#[test]
fn same_role_long_vowels_and_composed_parts_keep_their_previous_results() {
    let mut builder = DictBuilder::new();
    for value in [
        entry("方法", "名詞,一般,*,*", "ホウホウ", "ホーホー"),
        entry("方法", "名詞,固有名詞,一般,*", "ホウホウ", "ホウホウ"),
        entry("方法", "名詞,一般,*,*", "ホウホウ", "方法"),
        entry("商", "名詞,一般,*,*", "ショウ", "ショー"),
        entry("材", "名詞,一般,*,*", "ザイ", "ザイ"),
        entry("商材", "名詞,一般,*,*", "ショウザイ", "ショウザイ"),
        entry("小", "名詞,一般,*,*", "コ", "コ"),
        entry("小", "名詞,一般,*,*", "ショウ", "ショー"),
        entry("売", "名詞,一般,*,*", "ウリ", "ウリ"),
        entry("小売", "名詞,一般,*,*", "コウリ", "コウリ"),
    ] {
        builder.add_entry(value);
    }
    assert_eq!(builder.repair_pronunciation(), 3);
    let entries = builder.entries();
    assert!(entries[..3].iter().all(|e| &*e.pronunciation == "ホーホー"));
    assert_eq!(&*entries[5].pronunciation, "ショーザイ");
    assert_eq!(&*entries[9].pronunciation, "コウリ");
    assert_eq!(builder.repair_pronunciation(), 0);
}

#[test]
fn composed_parts_keep_the_selected_pronunciation_even_from_a_function_word() {
    let mut builder = DictBuilder::new();
    // 人工entryで直接借用のvetoと部品候補の選択が独立していることを確かめる。
    for value in [
        entry("商", "助動詞,*,*,*", "ショウ", "ショー"),
        entry("商", "名詞,一般,*,*", "ショウ", "ゴー"),
        entry("材", "名詞,一般,*,*", "ザイ", "ザイ"),
        entry("商材", "名詞,固有名詞,一般,*", "ショウザイ", "ショウザイ"),
    ] {
        builder.add_entry(value);
    }
    assert_eq!(builder.repair_pronunciation(), 1);
    assert_eq!(&*builder.entries()[1].pronunciation, "ゴー");
    assert_eq!(&*builder.entries()[3].pronunciation, "ショーザイ");
    assert_eq!(builder.repair_pronunciation(), 0);
}
