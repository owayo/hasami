//! 共通の文脈判断で実際に補正した発音を、入力の座標で返す。

/// 共通規則が読み・発音を実際に補正した範囲と発音。
///
/// 位置は入力全体のUTF-8バイト位置。有限な数学・設定構文の「値」の補正と、
/// 明確な段位構文で実辞書の一般名詞候補へ選び直し、または明示姓ラベルで
/// 数詞と「段」を実姓候補へ結合して、発音が変わった箇所を返す。
/// 辞書の全候補、未補正の語、すでに別の発音を持つ語は含めない。
/// 引用による既選択発音の保持や、全トークンのかな化には使わない。モデルは不要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextualReadingSpan {
    /// トークンの先頭（この位置を含む）。
    pub start: usize,
    /// トークンの末尾（この位置を含まない）。
    pub end: usize,
    /// 同じ解析で実際に補正した発音。
    pub pronunciation: String,
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;
    use crate::{Analyzer, DictEntry, dict::DictBuilder};

    fn analyzer(value_pos: &str, value_r: &str, value_p: &str, include_value: bool) -> Analyzer {
        let mut builder = DictBuilder::new();
        for (surface, pos, base, reading, pronunciation, kind, form) in [
            (
                "関数",
                "名詞,一般,*,*",
                "関数",
                "カンスウ",
                "カンスー",
                "",
                "",
            ),
            (
                "変数",
                "名詞,一般,*,*",
                "変数",
                "ヘンスウ",
                "ヘンスー",
                "",
                "",
            ),
            (
                "定数",
                "名詞,一般,*,*",
                "定数",
                "テイスウ",
                "テイスー",
                "",
                "",
            ),
            (
                "係数",
                "名詞,一般,*,*",
                "係数",
                "ケイスウ",
                "ケイスー",
                "",
                "",
            ),
            ("の", "助詞,連体化,*,*", "の", "ノ", "ノ", "", ""),
            ("を", "助詞,格助詞,一般,*", "を", "ヲ", "ヲ", "", ""),
            (
                "積分",
                "名詞,サ変接続,*,*",
                "積分",
                "セキブン",
                "セキブン",
                "",
                "",
            ),
            (
                "極限",
                "名詞,一般,*,*",
                "極限",
                "キョクゲン",
                "キョクゲン",
                "",
                "",
            ),
            (
                "として",
                "助詞,格助詞,連語,*",
                "として",
                "トシテ",
                "トシテ",
                "",
                "",
            ),
            (
                "計算",
                "名詞,サ変接続,*,*",
                "計算",
                "ケイサン",
                "ケイサン",
                "",
                "",
            ),
            (
                "し",
                "動詞,自立,*,*",
                "する",
                "シ",
                "シ",
                "サ変・スル",
                "連用形",
            ),
            ("た", "助動詞,*,*,*", "た", "タ", "タ", "特殊・タ", "基本形"),
            (
                "求める",
                "動詞,自立,*,*",
                "求める",
                "モトメル",
                "モトメル",
                "一段",
                "基本形",
            ),
            ("f", "記号,アルファベット,*,*", "f", "エフ", "エフ", "", ""),
            ("2", "名詞,数,*,*", "2", "ニ", "ニ", "", ""),
            (
                "f2",
                "名詞,固有名詞,一般,*",
                "f2",
                "エフツー",
                "エフツー",
                "",
                "",
            ),
            ("(", "記号,括弧開,*,*", "(", "(", "(", "", ""),
            (")", "記号,括弧閉,*,*", ")", ")", ")", "", ""),
            ("。", "記号,句点,*,*", "。", "。", "。", "", ""),
            (",", "記号,読点,*,*", ",", "", "", "", ""),
            ("、", "記号,読点,*,*", "、", "、", "、", "", ""),
            ("「", "記号,括弧開,*,*", "「", "「", "「", "", ""),
            ("」", "記号,括弧閉,*,*", "」", "」", "」", "", ""),
            ("魚", "名詞,一般,*,*", "魚", "サカナ", "サカナ", "", ""),
            (
                "座標",
                "名詞,一般,*,*",
                "座標",
                "ザヒョウ",
                "ザヒョー",
                "",
                "",
            ),
            ("差分", "名詞,一般,*,*", "差分", "サブン", "サブン", "", ""),
            (
                "算出",
                "名詞,サ変接続,*,*",
                "算出",
                "サンシュツ",
                "サンシュツ",
                "",
                "",
            ),
            ("三", "名詞,数,*,*", "三", "サン", "サン", "", ""),
            (
                "次元",
                "名詞,接尾,助数詞,*",
                "次元",
                "ジゲン",
                "ジゲン",
                "",
                "",
            ),
            (
                "コンフィグ",
                "名詞,固有名詞,一般,*",
                "config",
                "コンフィグ",
                "コンフィグ",
                "",
                "",
            ),
            (
                "設定",
                "名詞,サ変接続,*,*",
                "設定",
                "セッテイ",
                "セッテイ",
                "",
                "",
            ),
            (
                "する",
                "動詞,自立,*,*",
                "する",
                "スル",
                "スル",
                "サ変・スル",
                "基本形",
            ),
            ("式", "名詞,一般,*,*", "式", "シキ", "シキ", "", ""),
            ("自体", "名詞,一般,*,*", "自体", "ジタイ", "ジタイ", "", ""),
            ("が", "助詞,格助詞,一般,*", "が", "ガ", "ガ", "", ""),
            (
                "返す",
                "動詞,自立,*,*",
                "返す",
                "カエス",
                "カエス",
                "五段・サ行",
                "基本形",
            ),
            (
                "とり",
                "動詞,自立,*,*",
                "とる",
                "トリ",
                "トリ",
                "五段・ラ行",
                "連用形",
            ),
            (
                "うる",
                "動詞,非自立,*,*",
                "うる",
                "ウル",
                "ウル",
                "一段・得ル",
                "基本形",
            ),
            (
                "最大",
                "名詞,一般,*,*",
                "最大",
                "サイダイ",
                "サイダイ",
                "",
                "",
            ),
            (
                "最も",
                "副詞,一般,*,*",
                "最も",
                "モットモ",
                "モットモ",
                "",
                "",
            ),
            ("頻度", "名詞,一般,*,*", "頻度", "ヒンド", "ヒンド", "", ""),
            (
                "高い",
                "形容詞,自立,*,*",
                "高い",
                "タカイ",
                "タカイ",
                "形容詞・アウオ段",
                "基本形",
            ),
            (
                "です",
                "助動詞,*,*,*",
                "です",
                "デス",
                "デス",
                "特殊・デス",
                "基本形",
            ),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                base_form: base.into(),
                reading: reading.into(),
                pronunciation: pronunciation.into(),
                conj_type: kind.into(),
                conj_form: form.into(),
                ..Default::default()
            });
        }
        if include_value {
            builder.add_entry(DictEntry {
                surface: "値".into(),
                pos: value_pos.into(),
                base_form: "値".into(),
                reading: value_r.into(),
                pronunciation: value_p.into(),
                ..Default::default()
            });
        }
        Analyzer::from_dict(builder.build().unwrap())
    }

    fn standard() -> Analyzer {
        analyzer("名詞,一般,*,*", "ネ", "ネ", true)
    }

    #[test]
    fn strong_value_frames_use_original_utf8_ranges_and_actual_corrections_only() {
        let mut analyzer = standard();
        let input = "前置き。\n「式自体が値を返す」。\nとりうる最大の値。\n最も頻度が高い値です。";
        let expected: Vec<_> = input
            .match_indices('値')
            .map(|(start, _)| ContextualReadingSpan {
                start,
                end: start + "値".len(),
                pronunciation: "アタイ".into(),
            })
            .collect();
        let tokens = analyzer.try_tokenize(input).unwrap();
        assert_eq!(analyzer.contextual_reading_spans(input).unwrap(), expected);
        let values: Vec<_> = tokens
            .iter()
            .filter(|t| t.surface.as_ref() == "値")
            .collect();
        assert_eq!(values.len(), 3);
        for (token, span) in values.iter().zip(&expected) {
            assert_eq!(token.reading.as_ref(), "アタイ");
            assert_eq!(token.pronunciation.as_ref(), "アタイ");
            assert_eq!((token.start, token.end), (span.start, span.end));
            assert_eq!(input.get(span.start..span.end), Some("値"));
        }
        assert_eq!(
            format!("{tokens:?}"),
            format!("{:?}", analyzer.try_tokenize(input).unwrap())
        );
        assert!(analyzer.contextual_reading_spans("").unwrap().is_empty());
    }

    #[test]
    fn strong_value_frames_keep_controls_and_preselected_readings() {
        let mut a = standard();
        for input in [
            "魚の値を返す。",
            "最も高い値です。",
            "頻度が高い値です。",
            "式自体が値(ね)を返す。",
            "とりうる最大の値(ね)。",
            "最も頻度が高い値(ね)です。",
            "式 自体が値を返す。",
            "とり うる最大の値。",
            "最も 頻度が高い値です。",
            "「式」自体が値を返す。",
            "とりうる「最大」の値。",
            "最も頻度が「高い」値です。",
            "SKU-式自体が値を返す。",
            "SKU：とりうる最大の値。",
            "SKU/最も頻度が高い値です。",
        ] {
            let before = a.try_tokenize(input).unwrap();
            assert!(
                a.contextual_reading_spans(input).unwrap().is_empty(),
                "{input}"
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", a.try_tokenize(input).unwrap())
            );
        }
        for (pos, reading, pronunciation, known) in [
            ("名詞,固有名詞,人名,名", "ネ", "ネ", true),
            ("名詞,一般,*,*", "アタイ", "アタイ", true),
            ("名詞,一般,*,*", "チ", "チ", true),
            ("名詞,一般,*,*", "ネ", "アタイ", true),
            ("名詞,一般,*,*", "ネ", "ネ", false),
        ] {
            let mut a = analyzer(pos, reading, pronunciation, known);
            for input in [
                "式自体が値を返す。",
                "とりうる最大の値。",
                "最も頻度が高い値です。",
            ] {
                let before = a.try_tokenize(input).unwrap();
                assert!(
                    a.contextual_reading_spans(input).unwrap().is_empty(),
                    "{input}"
                );
                assert_eq!(
                    format!("{before:?}"),
                    format!("{:?}", a.try_tokenize(input).unwrap())
                );
            }
        }
    }

    #[test]
    fn original_utf8_ranges_survive_multiple_chunks_and_match_normal_tokens() {
        let mut analyzer = standard();
        let input = "序文。\n関数の値。f(2)の値を求める。\n積分した値。極限として計算した値。";
        let expected: Vec<_> = input
            .match_indices('値')
            .map(|(start, _)| ContextualReadingSpan {
                start,
                end: start + "値".len(),
                pronunciation: "アタイ".into(),
            })
            .collect();
        let before = analyzer.try_tokenize(input).unwrap();
        let actual = analyzer.contextual_reading_spans(input).unwrap();
        assert_eq!(actual, expected);
        assert!(
            actual
                .iter()
                .all(|s| input.get(s.start..s.end) == Some("値"))
        );
        let plan = crate::numeric::AsciiPlan::new(input, 0);
        let mut collected_tokens = Vec::new();
        let mut collected_spans = Vec::new();
        let mut context =
            crate::lattice::ContextualReadingContext::new(input, 0, Some(&mut collected_spans));
        analyzer
            .tokenize_chunks(input, 0, &mut collected_tokens, &plan, &mut 0, &mut context)
            .unwrap();
        assert_eq!(collected_spans, actual);
        assert_eq!(format!("{before:?}"), format!("{collected_tokens:?}"));
        assert_eq!(
            format!("{before:?}"),
            format!("{:?}", analyzer.try_tokenize(input).unwrap())
        );
    }

    #[test]
    fn controls_and_reused_analyzer_do_not_emit_stale_receipts() {
        let mut analyzer = standard();
        let math = "関数の値。";
        let expected = analyzer.contextual_reading_spans(math).unwrap();
        assert_eq!(expected.len(), 1);
        for input in [
            "",
            " \t\n",
            "未知語。",
            "魚の値を求める。",
            "得られた値。",
            "f2の値を求める。",
            "関数 の値。",
        ] {
            let before = analyzer.try_tokenize(input).unwrap();
            assert!(
                analyzer.contextual_reading_spans(input).unwrap().is_empty(),
                "{input}"
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", analyzer.try_tokenize(input).unwrap())
            );
        }
        assert_eq!(analyzer.contextual_reading_spans(math).unwrap(), expected);
        assert_eq!(
            analyzer.clone().contextual_reading_spans(math).unwrap(),
            expected
        );
    }

    #[test]
    fn setting_receipts_require_the_explicit_technical_head() {
        let mut a = standard();
        for input in ["コンフィグの値を設定する。", "「コンフィグの値を設定した」"]
        {
            let spans = a.contextual_reading_spans(input).unwrap();
            assert_eq!(spans.len(), 1, "{input}");
            assert_eq!(input.get(spans[0].start..spans[0].end), Some("値"));
            assert_eq!(spans[0].pronunciation, "アタイ");
            let before = a.try_tokenize(input).unwrap();
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", a.try_tokenize(input).unwrap())
            );
        }
        for input in [
            "魚の値を設定する。",
            "値を設定する。",
            "コンフィグの値を読む。",
            "「コンフィグ」の値を設定する。",
            "コンフィグの値（ね）を設定する。",
            "コンフィグの値,ね,を設定する。",
            "コンフィグ の値を設定する。",
            "コンフィグの値を 設定する。",
        ] {
            assert!(
                a.contextual_reading_spans(input).unwrap().is_empty(),
                "{input}"
            );
        }
    }

    #[test]
    fn mathematical_head_receipts_match_normal_tokens_and_decline_annotations() {
        let mut a = standard();
        for head in ["変数", "定数", "係数"] {
            for input in [format!("{head}の値。"), format!("「{head}の値」を読む。")] {
                let before = a.try_tokenize(&input).unwrap();
                let spans = a.contextual_reading_spans(&input).unwrap();
                assert_eq!(spans.len(), 1, "{input}");
                assert_eq!(input.get(spans[0].start..spans[0].end), Some("値"));
                assert_eq!(spans[0].pronunciation, "アタイ");
                assert_eq!(
                    format!("{before:?}"),
                    format!("{:?}", a.try_tokenize(&input).unwrap())
                );
            }
            for input in [
                format!("{head}の値（ね）を読む。"),
                format!("{head}の値,ね,を読む。"),
                format!("{head} の値。"),
                format!("「{head}」の値。"),
            ] {
                assert!(
                    a.contextual_reading_spans(&input).unwrap().is_empty(),
                    "{input}"
                );
            }
        }
    }

    #[test]
    fn coordinate_dimension_and_calculated_difference_use_original_ranges() {
        let mut a = standard();
        let input = "座標の値。\n「三次元座標の値」。差分の値を算出した。";
        let tokens = a.try_tokenize(input).unwrap();
        let spans = a.contextual_reading_spans(input).unwrap();
        assert_eq!(spans.len(), 3);
        assert_eq!(
            spans.iter().map(|s| s.start).collect::<Vec<_>>(),
            input
                .match_indices('値')
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        );
        assert!(
            spans
                .iter()
                .all(|s| s.pronunciation == "アタイ" && input.get(s.start..s.end) == Some("値"))
        );
        assert_eq!(
            format!("{tokens:?}"),
            format!("{:?}", a.try_tokenize(input).unwrap())
        );
        for input in [
            "座標の値（ね）を読む。",
            "差分の値,ね,を算出する。",
            "三 次元座標の値。",
            "三次元 座標の値。",
            "三次元座標 の値。",
            "差分の値を 算出する。",
            "差分 の値を算出する。",
            "差分の値を設定する。",
            "「座標」の値。",
            "三「次元座標」の値。",
            "魚の値を算出する。",
            "二次元座標の値。",
            "3次元座標の値。",
        ] {
            assert!(
                a.contextual_reading_spans(input).unwrap().is_empty(),
                "{input}"
            );
        }
    }

    #[test]
    fn identifier_boundaries_use_original_utf8_across_chunks_and_sentences() {
        let mut a = standard();
        for body in [
            "SKU-関数の値。",
            "SKU.変数の値。",
            "ＳＫＵ＿係数の値。",
            "SKU/係数の値。",
            "SKU:定数の値。",
            "ＳＫＵ-関数の値。",
            "SKU--関数の値。",
            "SKU::変数の値。",
            "SKU＿-係数の値。",
        ] {
            let input = format!("序文。\n{body}");
            let tokens = a.try_tokenize(&input).unwrap();
            assert!(
                a.contextual_reading_spans(&input).unwrap().is_empty(),
                "{input}"
            );
            assert!(
                tokens
                    .iter()
                    .filter(|t| t.surface.as_ref() == "値")
                    .all(|t| t.pronunciation.as_ref() == "ネ")
            );
            let sentences = a
                .try_tokenize_sentences(&input, &crate::sentence::SplitOptions::default())
                .unwrap();
            let values: Vec<_> = sentences
                .iter()
                .flat_map(|(_, ts)| ts)
                .filter(|t| t.surface.as_ref() == "値")
                .collect();
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].pronunciation.as_ref(), "ネ");
            assert_eq!(input.get(values[0].start..values[0].end), Some("値"));
            assert_eq!(
                format!("{tokens:?}"),
                format!("{:?}", a.try_tokenize(&input).unwrap())
            );
        }
        for input in [
            "SKU。関数の値。",
            "SKU。変数の値。",
            "SKU- 関数の値。",
            "SKU-「関数の値」。",
        ] {
            assert_eq!(
                a.contextual_reading_spans(input).unwrap().len(),
                1,
                "{input}"
            );
        }
        assert!(a.contextual_reading_spans("").unwrap().is_empty());
        assert_eq!(a.contextual_reading_spans("関数の値。").unwrap().len(), 1);
    }

    #[test]
    fn standalone_workspace_context_uses_its_input_origin_without_stale_prefixes() {
        let a = standard();
        let dict = a.dictionary();
        let mut workspace = crate::lattice::LatticeWorkspace::new();
        let mut out = Vec::new();
        workspace
            .tokenize_into("SKU.変数の値。", dict, 17, &mut out)
            .unwrap();
        let value = out.iter().find(|t| t.surface.as_ref() == "値").unwrap();
        assert_eq!(value.pronunciation.as_ref(), "ネ");
        assert_eq!(value.start, 17 + "SKU.変数の".len());
        out.clear();
        workspace.tokenize_into("SKU.", dict, 0, &mut out).unwrap();
        workspace
            .tokenize_into("変数の値。", dict, 4, &mut out)
            .unwrap();
        let value = out.iter().find(|t| t.surface.as_ref() == "値").unwrap();
        assert_eq!(value.pronunciation.as_ref(), "アタイ");
        assert_eq!(value.start, 4 + "変数の".len());
        workspace.tokenize_into("", dict, 999, &mut out).unwrap();
        let mut later = Vec::new();
        workspace
            .tokenize_into("係数の値。", dict, 31, &mut later)
            .unwrap();
        assert!(
            later
                .iter()
                .any(|t| t.surface.as_ref() == "値" && t.pronunciation.as_ref() == "アタイ")
        );
    }

    #[test]
    fn preexisting_pronunciations_names_and_unknown_values_have_no_receipt() {
        for (pos, reading, pronunciation, known) in [
            ("名詞,一般,*,*", "アタイ", "アタイ", true),
            ("名詞,一般,*,*", "チ", "チ", true),
            ("名詞,一般,*,*", "ネ", "アタイ", true),
            ("名詞,固有名詞,人名,名", "ネ", "ネ", true),
            ("名詞,一般,*,*", "ネ", "ネ", false),
        ] {
            let mut analyzer = analyzer(pos, reading, pronunciation, known);
            let input = "関数の値。";
            let before = analyzer.try_tokenize(input).unwrap();
            assert!(analyzer.contextual_reading_spans(input).unwrap().is_empty());
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", analyzer.try_tokenize(input).unwrap())
            );
        }
    }

    #[test]
    fn quoted_math_uses_the_same_finite_frames_across_chunks() {
        let mut analyzer = standard();
        for (input, expected_readings) in [
            ("「案内。\n関数の値を求める」", vec!["アタイ"]),
            ("「案内。\n『関数の値を求める』」", vec!["アタイ"]),
            ("「案内。」\n関数の値を求める。", vec!["アタイ"]),
            ("「案内。\n関数の値を求める", vec!["アタイ"]),
            (
                "「案内。\n関数の値を求める」関数の値を求める",
                vec!["アタイ", "アタイ"],
            ),
            ("\"案内。\n関数の値を求める\"", vec!["アタイ"]),
            ("'案内。\n関数の値を求める'", vec!["アタイ"]),
        ] {
            let tokens = analyzer.try_tokenize(input).unwrap();
            let values: Vec<_> = tokens
                .iter()
                .filter(|t| t.surface.as_ref() == "値")
                .collect();
            assert_eq!(
                values
                    .iter()
                    .map(|t| t.reading.as_ref())
                    .collect::<Vec<_>>(),
                expected_readings,
                "{input}"
            );
            let expected: Vec<_> = values
                .iter()
                .map(|t| ContextualReadingSpan {
                    start: t.start,
                    end: t.end,
                    pronunciation: t.pronunciation.to_string(),
                })
                .collect();
            assert_eq!(
                analyzer.contextual_reading_spans(input).unwrap(),
                expected,
                "{input}"
            );
            assert_eq!(
                format!("{tokens:?}"),
                format!("{:?}", analyzer.try_tokenize(input).unwrap())
            );
        }
    }

    #[test]
    fn receipts_are_query_local_and_quotes_do_not_create_state() {
        let mut analyzer = standard();
        let open = "「案内。\n関数の値を求める";
        let open_spans = analyzer.contextual_reading_spans(open).unwrap();
        assert_eq!(open_spans.len(), 1);
        assert_eq!(open_spans[0].pronunciation, "アタイ");
        assert_eq!(
            analyzer
                .contextual_reading_spans("関数の値。")
                .unwrap()
                .len(),
            1
        );
        assert!(analyzer.contextual_reading_spans("").unwrap().is_empty());
        let mut workspace = crate::lattice::LatticeWorkspace::new();
        let dictionary = analyzer.dictionary();
        workspace.tokenize("「案内。", dictionary).unwrap();
        let tokens = workspace.tokenize("関数の値。", dictionary).unwrap();
        assert!(
            tokens
                .iter()
                .any(|t| t.surface.as_ref() == "値" && t.pronunciation.as_ref() == "アタイ")
        );
        let input = "「案内。\n関数の値を求める」関数の値を求める。";
        let sentences = analyzer
            .try_tokenize_sentences(input, &crate::sentence::SplitOptions::default())
            .unwrap();
        let readings: Vec<_> = sentences
            .iter()
            .flat_map(|(_, tokens)| tokens)
            .filter(|t| t.surface.as_ref() == "値")
            .map(|t| t.pronunciation.as_ref())
            .collect();
        assert_eq!(readings, ["アタイ", "アタイ"]);
    }

    #[test]
    fn price_contexts_are_unchanged_and_do_not_emit_receipts() {
        let mut a = standard();
        let input = "「魚の値」を聞く。";
        let before = a.try_tokenize(input).unwrap();
        assert!(a.contextual_reading_spans(input).unwrap().is_empty());
        assert_eq!(
            format!("{before:?}"),
            format!("{:?}", a.try_tokenize(input).unwrap())
        );
        for (pos, r, p, known) in [
            ("名詞,一般,*,*", "アタイ", "アタイ", true),
            ("名詞,一般,*,*", "チ", "チ", true),
            ("名詞,一般,*,*", "ネ", "アタイ", true),
            ("名詞,固有名詞,人名,名", "ネ", "ネ", true),
            ("名詞,一般,*,*", "ネ", "ネ", false),
        ] {
            let mut other = analyzer(pos, r, p, known);
            assert!(other.contextual_reading_spans(input).unwrap().is_empty());
        }
    }

    #[test]
    fn gaps_and_internal_quotes_are_not_removed_to_create_a_frame() {
        let mut a = standard();
        for input in [
            "「関数 の値を求める」を読む。",
            "「積分 した値を求める」を読む。",
            "「積 分した値を求める」を読む。",
            "「魚の値」を聞く。",
            "「得られた値」を読む。",
            "「製品関数の値」を読む。",
            "「関数」の値を求める。",
        ] {
            let before = a.try_tokenize(input).unwrap();
            assert!(
                a.contextual_reading_spans(input).unwrap().is_empty(),
                "{input}"
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", a.try_tokenize(input).unwrap())
            );
        }
        for input in ["「関数の値」を読む。", "『積分した値』を読む。"] {
            let spans = a.contextual_reading_spans(input).unwrap();
            assert_eq!(spans.len(), 1, "{input}");
            assert_eq!(spans[0].pronunciation, "アタイ");
            assert_eq!(input.get(spans[0].start..spans[0].end), Some("値"));
        }
    }

    #[test]
    fn errors_discard_partial_receipts_before_reuse() {
        use crate::hsd::{
            container::{self, SectionId},
            writer::{self, DictSource, WriteOptions},
        };
        use std::{collections::HashMap, sync::Arc};

        let healthy = standard();
        let mut entries = Vec::new();
        healthy
            .dictionary()
            .for_each_entry(|entry| {
                entries.push(entry);
                Ok(())
            })
            .unwrap();
        entries.push(DictEntry {
            surface: "故障".into(),
            base_form: "故障".into(),
            pos: "名詞,一般,*,*".into(),
            reading: "コショウ".into(),
            pronunciation: "コショー".into(),
            ..Default::default()
        });
        let classifier = crate::char_class::CharClassifier::default_japanese();
        let unknown = HashMap::new();
        let source = DictSource {
            entries: &entries,
            matrix: None,
            classifier: &classifier,
            unk_entries: &unknown,
        };
        let sections =
            writer::build_sections(&source, &WriteOptions::default(), |_, _| {}).unwrap();
        let (words, len) = sections.to_aligned_buffer();
        let mut bytes = bytemuck::cast_slice(&words)[..len].to_vec();
        let good = Arc::new(crate::Dictionary::from_bytes(&bytes).unwrap());
        let mut broken_group = None;
        good.view()
            .trie
            .for_each_key(|index, surface| {
                if surface == "故障" {
                    broken_group = Some(index as usize);
                }
                Ok(())
            })
            .unwrap();
        let entry_at = container::parse(&bytes)
            .unwrap()
            .get(SectionId::Entries)
            .offset
            + broken_group.unwrap() * 6;
        bytes[entry_at + 2..entry_at + 4].copy_from_slice(&999u16.to_le_bytes());
        let bad = crate::Dictionary::from_bytes(&bytes).unwrap();
        let mut a = Analyzer::from_dict(bad);
        // 最初のチャンクで補正を集めた後、後段の壊れた素性で失敗する。
        let input = "関数の値。\n「案内。\n故障";
        assert!(matches!(
            a.try_tokenize(input),
            Err(crate::DictError::Corrupt(_))
        ));
        assert!(matches!(
            a.contextual_reading_spans(input),
            Err(crate::DictError::Corrupt(_))
        ));
        assert!(a.contextual_reading_spans("").unwrap().is_empty());
        // 素性とentry番号が同じ健全版に戻す。workspaceはリセットせず再利用する。
        a.dict = Arc::clone(&good);
        let math = "関数の値。";
        let expected = Analyzer::from_shared(good)
            .contextual_reading_spans(math)
            .unwrap();
        assert_eq!(expected.len(), 1);
        assert_eq!(a.contextual_reading_spans(math).unwrap(), expected);
    }

    #[test]
    fn ascii_comma_declines_annotation_loss_but_original_japanese_comma_is_corrected() {
        let mut a = standard();
        for input in [
            "関数の値（ね）を読む。",
            "関数の値,ね,を読む.",
            "関数の値,次へ進む。",
        ] {
            let tokens = a.try_tokenize(input).unwrap();
            let value = tokens.iter().find(|t| t.surface.as_ref() == "値").unwrap();
            assert_eq!(value.reading.as_ref(), "ネ", "{input}");
            assert_eq!(value.pronunciation.as_ref(), "ネ", "{input}");
            assert!(a.contextual_reading_spans(input).unwrap().is_empty());
        }
        let input = "関数の値、次へ進む。";
        let spans = a.contextual_reading_spans(input).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].pronunciation, "アタイ");
        for (r, p) in [("アタイ", "アタイ"), ("チ", "チ"), ("ネ", "アタイ")] {
            let mut custom = analyzer("名詞,一般,*,*", r, p, true);
            let input = "関数の値,ね,を読む.";
            let before = custom.try_tokenize(input).unwrap();
            assert!(custom.contextual_reading_spans(input).unwrap().is_empty());
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", custom.try_tokenize(input).unwrap())
            );
        }
    }
}
