use super::*;
use crate::{Analyzer, dict::DictBuilder};

fn entry(surface: &str, pos: &str, reading: &str, pronunciation: &str, cost: i16) -> DictEntry {
    DictEntry {
        surface: surface.into(),
        pos: pos.into(),
        base_form: surface.into(),
        reading: reading.into(),
        pronunciation: pronunciation.into(),
        cost,
        conj_type: "*".into(),
        conj_form: "*".into(),
        ..Default::default()
    }
}

fn dictionary(include_rank: bool, ambiguous: bool) -> Dictionary {
    let mut builder = DictBuilder::new();
    let mut family = entry(
        "八段",
        "名詞,固有名詞,人名,姓",
        "ハッタン",
        "ハッタン",
        3582,
    );
    family.left_id = 1290;
    family.right_id = 1290;
    builder.add_entry(family);
    if include_rank {
        let mut rank = entry("八段", "名詞,一般,*,*", "ハチダン", "ハチダン", 5606);
        rank.left_id = 1285;
        rank.right_id = 1285;
        builder.add_entry(rank.clone());
        if ambiguous {
            // 複数の独自読みを持つ辞書では、低コストを選んで読みを確定しない。
            rank.reading = "ハチダーン".into();
            rank.pronunciation = "ハチダーン".into();
            builder.add_entry(rank);
        }
    }
    builder.add_entry(entry(
        "九段",
        "名詞,固有名詞,地域,一般",
        "クダン",
        "クダン",
        7928,
    ));
    // 地域guardを欠いた実装なら、同表層の一般候補があってもメタデータが変わる。
    builder.add_entry(entry("九段", "名詞,一般,*,*", "クダン", "クダン", 5606));
    builder.build().unwrap()
}

fn tokens(words: &[&str], offset: usize) -> Vec<Token> {
    let mut start = offset;
    words
        .iter()
        .filter_map(|&surface| {
            if matches!(surface, " " | "\t" | "　") {
                start += surface.len();
                return None;
            }
            let (pos, base, kind, form, reading, pronunciation, known, cost) = match surface {
                "八" => ("名詞,数,*,*", surface, "", "", "ハチ", "ハチ", true, 100),
                "段" => (
                    "名詞,接尾,助数詞,*",
                    surface,
                    "",
                    "",
                    "ダン",
                    "ダン",
                    true,
                    100,
                ),
                "苗字" => (
                    "名詞,一般,*,*",
                    surface,
                    "",
                    "",
                    "ミョウジ",
                    "ミョージ",
                    true,
                    3000,
                ),
                "八段" => (
                    "名詞,固有名詞,人名,姓",
                    surface,
                    "",
                    "",
                    "ハッタン",
                    "ハッタン",
                    true,
                    3582,
                ),
                "九段" => (
                    "名詞,固有名詞,地域,一般",
                    surface,
                    "",
                    "",
                    "クダン",
                    "クダン",
                    true,
                    7928,
                ),
                "初段" => (
                    "名詞,一般,*,*",
                    surface,
                    "",
                    "",
                    "ショダン",
                    "ショダン",
                    true,
                    5606,
                ),
                "段位" => (
                    "名詞,一般,*,*",
                    surface,
                    "",
                    "",
                    "ダンイ",
                    "ダンイ",
                    true,
                    3000,
                ),
                "棋士" => ("名詞,一般,*,*", surface, "", "", "キシ", "キシ", true, 3000),
                "姓" => ("名詞,一般,*,*", surface, "", "", "セイ", "セイ", true, 3000),
                "名字" => (
                    "名詞,一般,*,*",
                    surface,
                    "",
                    "",
                    "ミョウジ",
                    "ミョージ",
                    true,
                    3000,
                ),
                "田中" => (
                    "名詞,固有名詞,人名,姓",
                    surface,
                    "",
                    "",
                    "タナカ",
                    "タナカ",
                    true,
                    3000,
                ),
                "太郎" => (
                    "名詞,固有名詞,人名,名",
                    surface,
                    "",
                    "",
                    "タロウ",
                    "タロー",
                    true,
                    3000,
                ),
                "新町太郎" => (
                    "名詞,固有名詞,人名,一般",
                    surface,
                    "",
                    "",
                    "シンマチタロウ",
                    "シンマチタロー",
                    true,
                    3000,
                ),
                "を" => (
                    "助詞,格助詞,一般,*",
                    surface,
                    "",
                    "",
                    "ヲ",
                    "ヲ",
                    true,
                    3000,
                ),
                "に" => (
                    "助詞,格助詞,一般,*",
                    surface,
                    "",
                    "",
                    "ニ",
                    "ニ",
                    true,
                    3000,
                ),
                "は" => ("助詞,係助詞,*,*", surface, "", "", "ハ", "ワ", true, 3000),
                "が" => (
                    "助詞,格助詞,一般,*",
                    surface,
                    "",
                    "",
                    "ガ",
                    "ガ",
                    true,
                    3000,
                ),
                "の" => ("助詞,連体化,*,*", surface, "", "", "ノ", "ノ", true, 3000),
                "という" => (
                    "助詞,格助詞,連語,*",
                    surface,
                    "",
                    "",
                    "トイウ",
                    "トイウ",
                    true,
                    3000,
                ),
                "目指す" => (
                    "動詞,自立,*,*",
                    surface,
                    "五段・サ行",
                    "基本形",
                    "メザス",
                    "メザス",
                    true,
                    3000,
                ),
                "目指さ" => (
                    "動詞,自立,*,*",
                    "目指す",
                    "五段・サ行",
                    "未然形",
                    "メザサ",
                    "メザサ",
                    true,
                    3000,
                ),
                "昇段" => (
                    "名詞,サ変接続,*,*",
                    surface,
                    "",
                    "",
                    "ショウダン",
                    "ショーダン",
                    true,
                    3000,
                ),
                "し" => (
                    "動詞,自立,*,*",
                    "する",
                    "サ変・スル",
                    "連用形",
                    "シ",
                    "シ",
                    true,
                    3000,
                ),
                "た" => (
                    "助動詞,*,*,*",
                    surface,
                    "特殊・タ",
                    "基本形",
                    "タ",
                    "タ",
                    true,
                    3000,
                ),
                "ない" => (
                    "助動詞,*,*,*",
                    surface,
                    "特殊・ナイ",
                    "基本形",
                    "ナイ",
                    "ナイ",
                    true,
                    3000,
                ),
                "なく" => (
                    "助動詞,*,*,*",
                    "ない",
                    "特殊・ナイ",
                    "連用テ接続",
                    "ナク",
                    "ナク",
                    true,
                    3000,
                ),
                "で" => (
                    "助動詞,*,*,*",
                    "だ",
                    "特殊・ダ",
                    "連用形",
                    "デ",
                    "デ",
                    true,
                    3000,
                ),
                "です" => (
                    "助動詞,*,*,*",
                    surface,
                    "特殊・デス",
                    "基本形",
                    "デス",
                    "デス",
                    true,
                    3000,
                ),
                "だ" => (
                    "助動詞,*,*,*",
                    surface,
                    "特殊・ダ",
                    "基本形",
                    "ダ",
                    "ダ",
                    true,
                    3000,
                ),
                "さん" => (
                    "名詞,接尾,人名,*",
                    surface,
                    "",
                    "",
                    "サン",
                    "サン",
                    true,
                    3000,
                ),
                "氏" => ("名詞,接尾,人名,*", surface, "", "", "シ", "シ", true, 3000),
                "「" | "『" => (
                    "記号,括弧開,*,*",
                    surface,
                    "",
                    "",
                    surface,
                    surface,
                    true,
                    3000,
                ),
                "」" | "』" => (
                    "記号,括弧閉,*,*",
                    surface,
                    "",
                    "",
                    surface,
                    surface,
                    true,
                    3000,
                ),
                "。" => (
                    "記号,句点,*,*",
                    surface,
                    "",
                    "",
                    surface,
                    surface,
                    true,
                    3000,
                ),
                _ => ("名詞,一般,*,*", surface, "", "", "", "", false, 3000),
            };
            let token = Token {
                surface: surface.into(),
                start,
                end: start + surface.len(),
                pos: pos.into(),
                base_form: base.into(),
                conj_type: kind.into(),
                conj_form: form.into(),
                reading: reading.into(),
                pronunciation: pronunciation.into(),
                word_cost: cost,
                is_known: known,
            };
            start = token.end;
            Some(token)
        })
        .collect()
}

#[test]
fn horizontal_space_preserves_explicit_names_without_joining_positive_frames() {
    let dict = dictionary(true, false);
    for words in [
        &["ハッタン", "と", "読む", " ", "八段", "を", "目指す", "。"][..],
        &[
            "ハッタン",
            "と",
            "読む",
            " ",
            "「",
            "八段",
            "」",
            "を",
            "目指す",
            "。",
        ][..],
        &["ハッタン", "と", "読む", "\t", "八段", "を", "目指す", "。"][..],
        &["ハッタン", "と", "読む", "　", "八段", "を", "目指す", "。"][..],
        &["段位", "は", " ", "八段", "です", "。"][..],
    ] {
        let input = words.concat();
        let mut values = tokens(words, 0);
        let before = snapshot(&values);
        let mut spans = Vec::new();
        super::apply(
            &mut values,
            &dict,
            &[],
            &mut ContextualReadingContext::new(&input, 0, Some(&mut spans)),
        )
        .unwrap();
        assert_eq!(snapshot(&values), before, "{input}");
        assert!(spans.is_empty(), "{input}");
    }
    // 改行を越えた読み指定は、次の独立した段位目標へ漏らさない。
    let input = "ハッタンと読む\n八段を目指す。";
    let mut values = tokens(&["八段", "を", "目指す", "。"], "ハッタンと読む\n".len());
    super::apply(
        &mut values,
        &dict,
        &[],
        &mut ContextualReadingContext::new(input, 0, None),
    )
    .unwrap();
    assert_eq!(values[0].pronunciation.as_ref(), "ハチダン");
}

fn snapshot(tokens: &[Token]) -> Vec<String> {
    // DebugにはTokenの全11fieldが含まれる。非対象語もコストまで比較する。
    tokens.iter().map(|t| format!("{t:?}")).collect()
}

#[test]
fn rank_shapes_are_canonical_and_do_not_generate_readings() {
    for text in [
        "初段",
        "一段",
        "四段",
        "七段",
        "八段",
        "九段",
        "十段",
        "十一段",
        "二十段",
        "九十九段",
    ] {
        assert!(rank_surface(text), "{text}");
    }
    for text in [
        "段",
        "人名段",
        "一十段",
        "〇段",
        "零段",
        "九八段",
        "8段",
        "８段",
        "八段位",
        "八段A1",
    ] {
        assert!(!rank_surface(text), "{text}");
    }
}

#[test]
fn real_general_candidate_supplies_all_features_in_strong_frames() {
    let dict = dictionary(true, false);
    for words in [
        &["段位", "は", "八段", "です", "。"][..],
        &["八段", "を", "目指す", "。"][..],
        &["段位", "の", "八段", "を", "目指す", "。"][..],
        &["八段", "を", "目指さ", "ない", "。"][..],
        &["八段", "に", "昇段", "し", "た", "。"][..],
        &["段位", "は", "「", "八段", "」", "です", "。"][..],
        &["「", "八段", "」", "という", "段位", "だ", "。"][..],
        &["棋士", "の", "田中", "八段", "が"][..],
        &["田中", "太郎", "八段", "が"][..],
        &["新町太郎", "八段", "が"][..],
    ] {
        let input = words.concat();
        let mut values = tokens(words, 0);
        let before = snapshot(&values);
        let target = values
            .iter()
            .position(|t| t.surface.as_ref() == "八段")
            .unwrap();
        let mut spans = Vec::new();
        super::apply(
            &mut values,
            &dict,
            &[],
            &mut ContextualReadingContext::new(&input, 0, Some(&mut spans)),
        )
        .unwrap();
        let rank = &values[target];
        assert_eq!(rank.pos.as_ref(), "名詞,一般,*,*");
        assert_eq!(rank.base_form.as_ref(), "八段");
        assert_eq!(rank.reading.as_ref(), "ハチダン");
        assert_eq!(rank.pronunciation.as_ref(), "ハチダン");
        assert_eq!(rank.word_cost, 5606);
        assert!(rank.is_known && rank.conj_type.is_empty() && rank.conj_form.is_empty());
        assert_eq!(
            spans,
            [crate::ContextualReadingSpan {
                start: rank.start,
                end: rank.end,
                pronunciation: "ハチダン".into()
            }]
        );
        for (i, t) in values.iter().enumerate() {
            assert_eq!(&input[t.start..t.end], t.surface.as_ref());
            if i != target {
                assert_eq!(format!("{t:?}"), before[i]);
            }
        }
    }
}

#[test]
fn name_location_bare_id_and_sentence_contexts_keep_all_eleven_fields() {
    let dict = dictionary(true, false);
    for words in [
        &["八段"][..],
        &["姓", "は", "八段", "です", "。"][..],
        &["姓", "は", "「", "八段", "」", "です", "。"][..],
        &["「", "八段", "」", "という", "名字", "だ", "。"][..],
        &["段位", "で", "は", "なく", "姓", "は", "八段", "です", "。"][..],
        &["同じ", "姓", "の", "八段", "を", "目指す", "。"][..],
        &["姓", "が", "同じ", "八段", "を", "目指す", "。"][..],
        &["ハッタン", "と", "読む", "八段", "を", "目指す", "。"][..],
        &[
            "ハッタン",
            "と",
            "読む",
            "「",
            "八段",
            "」",
            "を",
            "目指す",
            "。",
        ][..],
        &[
            "ハッタン",
            "と",
            "読む",
            "姓",
            "の",
            "「",
            "八段",
            "」",
            "を",
            "目指す",
            "。",
        ][..],
        &["名字", "の", "八段", "を", "目指す", "。"][..],
        &["苗字", "の", "八段", "を", "目指す", "。"][..],
        &["人名", "の", "八段", "を", "目指す", "。"][..],
        &[
            "「",
            "八段",
            "」",
            "という",
            "段位",
            "で",
            "は",
            "なく",
            "姓",
            "です",
            "。",
        ][..],
        &["八段", "さん", "が"][..],
        &["八段", "氏", "が"][..],
        &["八段", "太郎"][..],
        &["田中", "八段"][..],
        &["太郎", "八段"][..],
        &["id", "八段", "を", "目指す", "。"][..],
        &["id", "段位", "は", "八段", "です", "。"][..],
        &["段位", "は", "八段", "A1"][..],
        &["段位", "は", "。", "八段", "です", "。"][..],
        &["棋士", "の", "田中", "。", "八段", "です", "。"][..],
        &["段位", "は", "「", "文", "。", "八段", "」", "です"][..],
        &["九段", "を", "目指す", "。"][..],
        &["初段", "を", "目指す", "。"][..],
    ] {
        let input = words.concat();
        let mut values = tokens(words, 0);
        let before = snapshot(&values);
        let mut spans = Vec::new();
        super::apply(
            &mut values,
            &dict,
            &[],
            &mut ContextualReadingContext::new(&input, 0, Some(&mut spans)),
        )
        .unwrap();
        assert_eq!(snapshot(&values), before, "{input}");
        assert!(spans.is_empty(), "{input}");
    }
}

#[test]
fn missing_ambiguous_changed_metadata_unknown_and_protected_candidates_are_held() {
    let words = ["段位", "は", "八段", "です", "。"];
    let input = words.concat();
    for dict in [dictionary(false, false), dictionary(true, true)] {
        let mut values = tokens(&words, 0);
        let before = snapshot(&values);
        super::apply(
            &mut values,
            &dict,
            &[],
            &mut ContextualReadingContext::new(&input, 0, None),
        )
        .unwrap();
        assert_eq!(snapshot(&values), before);
    }
    let dict = dictionary(true, false);
    for protected in [0.."段位".len(), "段位は".len().."段位は八段".len()] {
        let mut values = tokens(&words, 0);
        let before = snapshot(&values);
        super::apply(
            &mut values,
            &dict,
            std::slice::from_ref(&protected),
            &mut ContextualReadingContext::new(&input, 0, None),
        )
        .unwrap();
        assert_eq!(snapshot(&values), before);
    }
    for field in 0..3 {
        let mut values = tokens(&words, 0);
        match field {
            0 => values[0].is_known = false,
            1 => values[2].word_cost += 1,
            _ => values[2].is_known = false,
        }
        let before = snapshot(&values);
        super::apply(
            &mut values,
            &dict,
            &[],
            &mut ContextualReadingContext::new(&input, 0, None),
        )
        .unwrap();
        assert_eq!(snapshot(&values), before);
    }
}

#[test]
fn independent_workspace_origins_keep_exact_ranges() {
    let words = ["段位", "は", "八段", "です", "。"];
    let input = words.concat();
    let mut values = tokens(&words, 101);
    let mut spans = Vec::new();
    super::apply(
        &mut values,
        &dictionary(true, false),
        &[],
        &mut ContextualReadingContext::new(&input, 101, Some(&mut spans)),
    )
    .unwrap();
    assert_eq!(spans[0].start, 101 + "段位は".len());
    assert_eq!(spans[0].end, 101 + "段位は八段".len());
    assert_eq!(spans.len(), 1);
}

#[test]
fn analyzer_uses_the_same_selection_for_normal_and_query_routes() {
    let mut builder = DictBuilder::new();
    for (surface, pos, r, p, cost) in [
        (
            "八段",
            "名詞,固有名詞,人名,姓",
            "ハッタン",
            "ハッタン",
            3582,
        ),
        ("八段", "名詞,一般,*,*", "ハチダン", "ハチダン", 5606),
        ("段位", "名詞,一般,*,*", "ダンイ", "ダンイ", 1000),
        ("姓", "名詞,一般,*,*", "セイ", "セイ", 1000),
        ("は", "助詞,係助詞,*,*", "ハ", "ワ", 1000),
        ("です", "助動詞,*,*,*", "デス", "デス", 1000),
        ("。", "記号,句点,*,*", "。", "。", 1000),
        ("関数", "名詞,一般,*,*", "カンスウ", "カンスー", 1000),
        ("の", "助詞,連体化,*,*", "ノ", "ノ", 1000),
        ("値", "名詞,一般,*,*", "ネ", "ネ", 1000),
        ("と", "助詞,格助詞,一般,*", "ト", "ト", 1000),
        ("、", "記号,読点,*,*", "、", "、", 1000),
    ] {
        let mut row = entry(surface, pos, r, p, cost);
        if surface == "です" {
            row.conj_type = "特殊・デス".into();
            row.conj_form = "基本形".into();
        }
        builder.add_entry(row);
    }
    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
    let input = "関数の値と段位は八段です。姓は八段です。";
    let actual = analyzer.try_tokenize(input).unwrap();
    let ranks: Vec<_> = actual
        .iter()
        .filter(|t| t.surface.as_ref() == "八段")
        .collect();
    assert_eq!(ranks.len(), 2);
    assert_eq!(ranks[0].pronunciation.as_ref(), "ハチダン");
    assert_eq!(ranks[0].pos.as_ref(), "名詞,一般,*,*");
    assert_eq!(ranks[1].pronunciation.as_ref(), "ハッタン");
    let spans = analyzer.contextual_reading_spans(input).unwrap();
    assert_eq!(
        spans
            .iter()
            .map(|s| s.pronunciation.as_str())
            .collect::<Vec<_>>(),
        ["アタイ", "ハチダン"]
    );
    assert!(spans.windows(2).all(|s| s[0].end <= s[1].start));
    assert_eq!(
        snapshot(&analyzer.try_tokenize(input).unwrap()),
        snapshot(&actual)
    );
    assert!(
        analyzer
            .contextual_reading_spans("姓は八段です。")
            .unwrap()
            .is_empty()
    );
    // 同じchunkにrank/valueを交互に並べる。recordでの中間insertを要しない。
    for count in [1, 8, 32] {
        let input = format!("{}。", "段位は八段です、関数の値と".repeat(count));
        let actual = analyzer.try_tokenize(&input).unwrap();
        let spans = analyzer.contextual_reading_spans(&input).unwrap();
        assert_eq!(spans.len(), count * 2);
        assert!(spans.windows(2).all(|pair| pair[0].end <= pair[1].start));
        for span in &spans {
            let token = actual
                .iter()
                .find(|t| t.start == span.start && t.end == span.end)
                .unwrap();
            assert_eq!(span.pronunciation, token.pronunciation.as_ref());
            assert!(matches!(token.surface.as_ref(), "八段" | "値"));
        }
    }
}

#[test]
fn explicit_surname_labels_join_real_candidates_with_offsets_and_append_prefix() {
    let dict = dictionary(true, false);
    for words in [
        &["姓", "は", "八", "段", "です", "。"][..],
        &["名字", "が", "『", "八", "段", "』", "だ", "。"],
        &["苗字", "は", "八", "段", "です", "。"],
        &[
            "段位", "は", "未定", "。", "姓", "は", "八", "段", "です", "。",
        ],
    ] {
        let input = words.concat();
        let original = tokens(words, 101);
        let start = 101 + input.find("八段").unwrap();
        let end = start + "八段".len();
        let outside = snapshot(
            &original
                .iter()
                .filter(|t| t.end <= start || t.start >= end)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let mut out = tokens(&["初段", "。"], 0);
        let first = out.len();
        let prefix = snapshot(&out);
        out.extend(original);
        let mut spans = Vec::new();
        let mut context = ContextualReadingContext::new(&input, 101, Some(&mut spans));
        apply_explicit_names(&mut out, first, &dict, &[], &mut context).unwrap();
        apply(&mut out[first..], &dict, &[], &mut context).unwrap();
        assert_eq!(snapshot(&out[..first]), prefix);
        let surname = out.iter().find(|t| t.start == start).unwrap();
        assert_eq!(surname.surface.as_ref(), "八段");
        assert_eq!(surname.pos.as_ref(), "名詞,固有名詞,人名,姓");
        assert_eq!(surname.base_form.as_ref(), "八段");
        assert_eq!(surname.reading.as_ref(), "ハッタン");
        assert_eq!(surname.pronunciation.as_ref(), "ハッタン");
        assert_eq!(surname.word_cost, 3582);
        assert!(surname.is_known && surname.conj_type.is_empty() && surname.conj_form.is_empty());
        assert_eq!((surname.start, surname.end), (start, end));
        let rest: Vec<_> = out[first..]
            .iter()
            .filter(|t| t.end <= start || t.start >= end)
            .cloned()
            .collect();
        assert_eq!(snapshot(&rest), outside);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end), (start, end));
        assert_eq!(spans[0].pronunciation, "ハッタン");
    }
}

#[test]
fn explicit_surname_selection_holds_nonframes_custom_readings_and_protection() {
    let dict = dictionary(true, false);
    for words in [
        &["段位", "は", "八", "段", "です", "。"][..],
        &["八", "段"],
        &["姓", "は", "八", " ", "段", "です", "。"],
        &["姓", "は", "「", "八", "」", "段", "です", "。"],
        &["姓", "は", "八", "段", "（ハチダン）", "です", "。"],
        &["姓", "は", "八", "段", "で", "は", "ない", "。"],
        &["百姓", "は", "八", "段", "です", "。"],
        &["X", "姓", "は", "八", "段", "です", "。"],
        &["姓", "は", "八", "段", "です", "X"],
        &["姓", "は", "八段", "です", "。"],
    ] {
        let input = words.concat();
        let mut actual = tokens(words, 0);
        let before = snapshot(&actual);
        let mut spans = Vec::new();
        let mut context = ContextualReadingContext::new(&input, 0, Some(&mut spans));
        apply_explicit_names(&mut actual, 0, &dict, &[], &mut context).unwrap();
        assert_eq!(snapshot(&actual), before, "{input}");
        assert!(spans.is_empty());
    }
    let input = "姓は八段です。";
    let words = ["姓", "は", "八", "段", "です", "。"];
    for field in ["reading", "unknown_label", "base"] {
        let mut actual = tokens(&words, 0);
        match field {
            "reading" => actual[2].reading = "ハツ".into(),
            "unknown_label" => actual[0].is_known = false,
            _ => actual[2].base_form = "別".into(),
        }
        let before = snapshot(&actual);
        let mut context = ContextualReadingContext::new(input, 0, None);
        apply_explicit_names(&mut actual, 0, &dict, &[], &mut context).unwrap();
        assert_eq!(snapshot(&actual), before, "{field}");
    }
    // 数字だけでなくラベルが保護された場合も、frame全体を変更しない。
    for protected in [0.."姓".len(), "姓は".len().."姓は八段".len()] {
        let mut actual = tokens(&words, 0);
        let before = snapshot(&actual);
        let mut context = ContextualReadingContext::new(input, 0, None);
        apply_explicit_names(
            &mut actual,
            0,
            &dict,
            std::slice::from_ref(&protected),
            &mut context,
        )
        .unwrap();
        assert_eq!(snapshot(&actual), before);
    }
    for dict in [dictionary(false, false), dictionary(true, true)] {
        let mut actual = tokens(&words, 0);
        let before = snapshot(&actual);
        let mut context = ContextualReadingContext::new(input, 0, None);
        apply_explicit_names(&mut actual, 0, &dict, &[], &mut context).unwrap();
        assert_eq!(snapshot(&actual), before);
    }
    let general = entry("八段", "名詞,一般,*,*", "ハチダン", "ハチダン", 5606);
    let family = entry(
        "八段",
        "名詞,固有名詞,人名,姓",
        "ハッタン",
        "ハッタン",
        3582,
    );
    let other_family = entry("八段", "名詞,固有名詞,人名,姓", "ハタノ", "ハタノ", 3582);
    for entries in [vec![general.clone()], vec![general, family, other_family]] {
        let mut builder = DictBuilder::new();
        for entry in entries {
            builder.add_entry(entry);
        }
        let dict = builder.build().unwrap();
        let mut actual = tokens(&words, 0);
        let before = snapshot(&actual);
        let mut context = ContextualReadingContext::new(input, 0, None);
        apply_explicit_names(&mut actual, 0, &dict, &[], &mut context).unwrap();
        assert_eq!(snapshot(&actual), before);
    }
    // 素性の選び直しだけで発音が同じなら、actual-pronunciation照会へ記録しない。
    let mut builder = DictBuilder::new();
    builder.add_entry(entry("八段", "名詞,一般,*,*", "ハチダン", "ハチダン", 5606));
    builder.add_entry(entry(
        "八段",
        "名詞,固有名詞,人名,姓",
        "ハチダン",
        "ハチダン",
        3582,
    ));
    let dict = builder.build().unwrap();
    let mut actual = tokens(&words, 0);
    let mut spans = Vec::new();
    let mut context = ContextualReadingContext::new(input, 0, Some(&mut spans));
    apply_explicit_names(&mut actual, 0, &dict, &[], &mut context).unwrap();
    assert!(
        actual
            .iter()
            .any(|t| t.surface.as_ref() == "八段" && t.pos.as_ref() == "名詞,固有名詞,人名,姓")
    );
    assert!(spans.is_empty());
}

#[test]
fn analyzer_coalesces_explicit_surnames_but_preserves_correct_rank_splits_and_math() {
    let mut builder = DictBuilder::new();
    for (surface, pos, r, p, cost) in [
        (
            "八段",
            "名詞,固有名詞,人名,姓",
            "ハッタン",
            "ハッタン",
            3582,
        ),
        ("八段", "名詞,一般,*,*", "ハチダン", "ハチダン", 5606),
        ("八", "名詞,数,*,*", "ハチ", "ハチ", 100),
        ("段", "名詞,接尾,助数詞,*", "ダン", "ダン", 100),
        ("段位", "名詞,一般,*,*", "ダンイ", "ダンイ", 1000),
        ("姓", "名詞,一般,*,*", "セイ", "セイ", 1000),
        ("は", "助詞,係助詞,*,*", "ハ", "ワ", 1000),
        ("です", "助動詞,*,*,*", "デス", "デス", 1000),
        ("、", "記号,読点,*,*", "、", "、", 1000),
        ("。", "記号,句点,*,*", "。", "。", 1000),
        ("関数", "名詞,一般,*,*", "カンスウ", "カンスー", 1000),
        ("の", "助詞,連体化,*,*", "ノ", "ノ", 1000),
        ("値", "名詞,一般,*,*", "ネ", "ネ", 1000),
        ("と", "助詞,格助詞,一般,*", "ト", "ト", 1000),
    ] {
        let mut row = entry(surface, pos, r, p, cost);
        if pos == "名詞,固有名詞,人名,姓" {
            row.left_id = 1290;
            row.right_id = 1290;
        } else if surface == "八段" {
            row.left_id = 1285;
            row.right_id = 1285;
        }
        if surface == "です" {
            row.conj_type = "特殊・デス".into();
            row.conj_form = "基本形".into();
        }
        builder.add_entry(row);
    }
    let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
    for count in [1, 8, 32] {
        let input = format!(
            "{}。",
            "段位は八段です、姓は八段です、関数の値と".repeat(count)
        );
        let actual = analyzer.try_tokenize(&input).unwrap();
        assert_eq!(
            actual.iter().filter(|t| t.surface.as_ref() == "八").count(),
            count
        );
        let surnames: Vec<_> = actual
            .iter()
            .filter(|t| t.surface.as_ref() == "八段")
            .collect();
        assert_eq!(surnames.len(), count);
        for surname in surnames {
            assert_eq!(surname.pos.as_ref(), "名詞,固有名詞,人名,姓");
            assert_eq!(surname.reading.as_ref(), "ハッタン");
            assert_eq!(surname.pronunciation.as_ref(), "ハッタン");
            assert_eq!(surname.word_cost, 3582);
        }
        let spans = analyzer.contextual_reading_spans(&input).unwrap();
        assert_eq!(spans.len(), count * 2);
        assert!(spans.windows(2).all(|pair| pair[0].end <= pair[1].start));
        for span in spans {
            let token = actual
                .iter()
                .find(|t| t.start == span.start && t.end == span.end)
                .unwrap();
            assert_eq!(span.pronunciation, token.pronunciation.as_ref());
            assert!(matches!(span.pronunciation.as_str(), "ハッタン" | "アタイ"));
        }
        assert_eq!(
            snapshot(&analyzer.try_tokenize(&input).unwrap()),
            snapshot(&actual)
        );
    }
}
