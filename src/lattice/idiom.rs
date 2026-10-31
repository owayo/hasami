//! 品詞・活用と連続した語を限定し、成句の候補を補正する。語義一般・人名には広げない。
//!
//! 読みの根拠（小学館の辞典を掲載するコトバンク）:
//! - https://kotobank.jp/word/真に受ける-632659
//! - https://kotobank.jp/word/身を粉にする-636948
//! - https://kotobank.jp/word/えも言われぬ-446752
//! - https://kotobank.jp/word/体良-2064820
//! - https://kotobank.jp/word/間も無く-635819
//! - https://kotobank.jp/word/分-122190

use super::Token;
use std::ops::Range;
use std::sync::Arc;

struct Correction {
    target: usize,
    len: usize,
    incorrect: &'static [&'static str],
    corrected: &'static str,
}

fn word(token: &Token, surface: &str, pos: &str, base: &str) -> bool {
    token.is_known
        && &*token.surface == surface
        && &*token.base_form == base
        && token.pos.starts_with(pos)
}

fn fixed(token: &Token, surface: &str, pos: &str) -> bool {
    word(token, surface, pos, surface) && token.conj_type.is_empty() && token.conj_form.is_empty()
}

fn noun(token: &Token, surface: &str) -> bool {
    fixed(token, surface, "名詞,一般,")
}

fn particle(token: &Token, surface: &str, pos: &str) -> bool {
    fixed(token, surface, pos)
}

fn inflected(token: &Token, base: &str, pos: &str, kind: &str, forms: &[(&str, &str)]) -> bool {
    token.is_known
        && &*token.base_form == base
        && token.pos.starts_with(pos)
        && &*token.conj_type == kind
        && forms
            .iter()
            .any(|&(surface, form)| &*token.surface == surface && &*token.conj_form == form)
}

fn receive(token: &Token) -> bool {
    inflected(
        token,
        "受ける",
        "動詞,自立,",
        "一段",
        &[
            ("受ける", "基本形"),
            ("受け", "連用形"),
            ("受け", "未然形"),
            ("受けれ", "仮定形"),
            ("受けろ", "命令ｒｏ"),
            ("受けよ", "命令ｙｏ"),
            ("受けよ", "未然ウ接続"),
        ],
    )
}

fn do_verb(token: &Token) -> bool {
    inflected(
        token,
        "する",
        "動詞,自立,",
        "サ変・スル",
        &[
            ("する", "基本形"),
            ("し", "連用形"),
            ("し", "未然形"),
            ("しよ", "未然ウ接続"),
            ("せ", "未然形"),
            ("せ", "未然ヌ接続"),
            ("すれ", "仮定形"),
            ("しろ", "命令ｒｏ"),
            ("せよ", "命令ｙｏ"),
        ],
    )
}

fn refuse(token: &Token) -> bool {
    inflected(
        token,
        "断る",
        "動詞,自立,",
        "五段・ラ行",
        &[
            ("断る", "基本形"),
            ("断り", "連用形"),
            ("断っ", "連用タ接続"),
            ("断ら", "未然形"),
            ("断ろ", "未然ウ接続"),
            ("断れ", "仮定形"),
            ("断れ", "命令ｅ"),
        ],
    )
}

fn bad(token: &Token) -> bool {
    inflected(
        token,
        "悪い",
        "形容詞,自立,",
        "形容詞・アウオ段",
        &[
            ("悪い", "基本形"),
            ("悪く", "連用テ接続"),
            ("悪かっ", "連用タ接続"),
            ("悪けれ", "仮定形"),
            ("悪かろ", "未然ウ接続"),
            ("悪から", "未然ヌ接続"),
        ],
    )
}

fn none(token: &Token) -> bool {
    ["ない", "無い"].iter().any(|base| {
        inflected(
            token,
            base,
            "形容詞,自立,",
            "形容詞・アウオ段",
            &[
                ("ない", "基本形"),
                ("なく", "連用テ接続"),
                ("無い", "基本形"),
                ("無く", "連用テ接続"),
            ],
        )
    })
}

fn yoku(token: &Token) -> bool {
    fixed(token, "よく", "副詞,一般,")
        || inflected(
            token,
            "よい",
            "形容詞,自立,",
            "形容詞・アウオ段",
            &[("よく", "連用テ接続")],
        )
}

fn bun(token: &Token, surface: &str) -> bool {
    noun(token, surface)
        || fixed(token, surface, "名詞,接尾,一般,")
        || fixed(token, surface, "名詞,接尾,助数詞,")
}

/// 「一年の間」「この間」等の時間範囲や、複合語の末尾を成句と解釈しない。
fn blocked_left(tokens: &[Token], i: usize, bun: bool) -> bool {
    let Some(prev) = i.checked_sub(1).and_then(|p| tokens.get(p)) else {
        return false;
    };
    &*prev.surface == "の"
        || prev.pos.starts_with("名詞,")
        || prev.pos.starts_with("連体詞,")
        || prev.pos.starts_with("接頭詞,")
        || (bun && prev.pos.starts_with("動詞,"))
        || prev.surface.chars().last().is_some_and(|c| {
            matches!(c, '0'..='9' | '０'..='９' | '〇' | '零' | '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百' | '千' | '万' | '億' | '兆')
        })
}

fn correction(tokens: &[Token], i: usize) -> Option<Correction> {
    let rest = &tokens[i..];
    let first = &rest[0];
    let (target, len, incorrect, corrected): (_, _, &'static [&'static str], _) = match &*first
        .surface
    {
        "真に" if rest.len() >= 2 && fixed(first, "真に", "副詞,一般,") && receive(&rest[1]) => {
            (i, 2, &["シンニ"], "マニ")
        }
        "真" if rest.len() >= 3
            && noun(first, "真")
            && particle(&rest[1], "に", "助詞,格助詞,")
            && receive(&rest[2]) =>
        {
            (i, 3, &["シン"], "マ")
        }
        "身" if rest.len() >= 5
            && noun(first, "身")
            && &*first.reading == "ミ"
            && !i.checked_sub(1).is_some_and(|p| {
                tokens[p].surface.as_ref() == "の"
                    || tokens[p].pos.starts_with("形容詞,")
                    || tokens[p].pos.starts_with("連体詞,")
                    || tokens[p].pos.starts_with("動詞,")
                    || tokens[p].pos.starts_with("助動詞,")
            })
            && particle(&rest[1], "を", "助詞,格助詞,")
            && noun(&rest[2], "粉")
            && particle(&rest[3], "に", "助詞,格助詞,")
            && do_verb(&rest[4]) =>
        {
            (i + 2, 5, &["コナ", "フン"], "コ")
        }
        "得" if rest.len() >= 5
            && noun(first, "得")
            && particle(&rest[1], "も", "助詞,係助詞,")
            && inflected(
                &rest[2],
                "言う",
                "動詞,自立,",
                "五段・ワ行促音便",
                &[("言わ", "未然形")],
            )
            && inflected(&rest[3], "れる", "動詞,接尾,", "一段", &[("れ", "未然形")])
            && inflected(&rest[4], "ぬ", "助動詞,", "特殊・ヌ", &[("ぬ", "基本形")]) =>
        {
            (i, 5, &["トク"], "エ")
        }
        "体" if rest.len() >= 3 && noun(first, "体") && yoku(&rest[1]) && refuse(&rest[2]) => {
            (i, 3, &["カラダ", "タイ"], "テイ")
        }
        "体よく" if rest.len() >= 2 && fixed(first, "体よく", "副詞,") && refuse(&rest[1]) => {
            (i, 2, &["カラダヨク", "タイヨク"], "テイヨク")
        }
        "間" if rest.len() >= 3
            && noun(first, "間")
            && !blocked_left(tokens, i, false)
            && particle(&rest[1], "も", "助詞,係助詞,")
            && none(&rest[2]) =>
        {
            (i, 3, &["アイダ", "カン"], "マ")
        }
        "分" if rest.len() >= 3
            && bun(first, "分")
            && !blocked_left(tokens, i, true)
            && particle(&rest[1], "の", "助詞,")
            && bad(&rest[2]) =>
        {
            (i, 3, &["ブン", "フン"], "ブ")
        }
        "分の"
            if rest.len() >= 2
                && bun(first, "分の")
                && !blocked_left(tokens, i, true)
                && bad(&rest[1]) =>
        {
            (i, 2, &["ブンノ", "フンノ"], "ブノ")
        }
        _ => return None,
    };
    Some(Correction {
        target,
        len,
        incorrect,
        corrected,
    })
}

pub(super) fn apply(tokens: &mut [Token], protected: &[Range<usize>]) {
    for i in 0..tokens.len() {
        let Some(rule) = correction(tokens, i) else {
            continue;
        };
        let phrase = &tokens[i..i + rule.len];
        // 空白・改行も含めて飛び越さず、句読点や引用はパターン自体に一致しない。
        if !phrase.windows(2).all(|p| p[0].end == p[1].start)
            || protected
                .iter()
                .any(|p| p.start < phrase.last().unwrap().end && phrase[0].start < p.end)
        {
            continue;
        }
        let token = &mut tokens[rule.target];
        // 確認済みの通常読みに限定し、正しい読みや明示された特殊読みを上書きしない。
        if token.reading == token.pronunciation && rule.incorrect.contains(&token.reading.as_ref())
        {
            let reading: Arc<str> = Arc::from(rule.corrected);
            token.reading = Arc::clone(&reading);
            token.pronunciation = reading;
        }
    }
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;

    fn t(surface: &str, pos: &str, base: &str, kind: &str, form: &str, reading: &str) -> Token {
        Token {
            surface: Arc::from(surface),
            start: 0,
            end: 0,
            pos: Arc::from(pos),
            base_form: Arc::from(base),
            conj_type: Arc::from(kind),
            conj_form: Arc::from(form),
            reading: Arc::from(reading),
            pronunciation: Arc::from(reading),
            word_cost: 1234,
            is_known: true,
        }
    }

    fn n(surface: &str, reading: &str) -> Token {
        t(surface, "名詞,一般,*,*", surface, "", "", reading)
    }

    fn p(surface: &str) -> Token {
        let (pos, reading) = match surface {
            "も" => ("助詞,係助詞,*,*", "モ"),
            "を" => ("助詞,格助詞,一般,*", "ヲ"),
            "に" => ("助詞,格助詞,一般,*", "ニ"),
            _ => ("助詞,連体化,*,*", "ノ"),
        };
        t(surface, pos, surface, "", "", reading)
    }

    fn place(tokens: &mut [Token]) {
        let mut offset = 17;
        for (i, token) in tokens.iter_mut().enumerate() {
            token.start = offset;
            offset += token.surface.len();
            token.end = offset;
            token.word_cost = 1000 + i as i16;
        }
    }

    // 表層・品詞・原形・活用は実辞書で確認した分割。文脈は辞典の最小成句だけ。
    fn fixture(id: usize) -> (Vec<Token>, usize, &'static str) {
        let (mut tokens, target, reading) = match id {
            0 => (
                vec![
                    t("真に", "副詞,一般,*,*", "真に", "", "", "シンニ"),
                    t(
                        "受ける",
                        "動詞,自立,*,*",
                        "受ける",
                        "一段",
                        "基本形",
                        "ウケル",
                    ),
                ],
                0,
                "マニ",
            ),
            1 => (
                vec![
                    n("身", "ミ"),
                    p("を"),
                    n("粉", "コナ"),
                    p("に"),
                    t(
                        "する",
                        "動詞,自立,*,*",
                        "する",
                        "サ変・スル",
                        "基本形",
                        "スル",
                    ),
                ],
                2,
                "コ",
            ),
            2 => (
                vec![
                    n("得", "トク"),
                    p("も"),
                    t(
                        "言わ",
                        "動詞,自立,*,*",
                        "言う",
                        "五段・ワ行促音便",
                        "未然形",
                        "イワ",
                    ),
                    t("れ", "動詞,接尾,*,*", "れる", "一段", "未然形", "レ"),
                    t("ぬ", "助動詞,*,*,*", "ぬ", "特殊・ヌ", "基本形", "ヌ"),
                ],
                0,
                "エ",
            ),
            3 => (
                vec![
                    n("体", "カラダ"),
                    t("よく", "副詞,一般,*,*", "よく", "", "", "ヨク"),
                    t(
                        "断る",
                        "動詞,自立,*,*",
                        "断る",
                        "五段・ラ行",
                        "基本形",
                        "コトワル",
                    ),
                ],
                0,
                "テイ",
            ),
            4 => (
                vec![
                    n("間", "アイダ"),
                    p("も"),
                    t(
                        "ない",
                        "形容詞,自立,*,*",
                        "ない",
                        "形容詞・アウオ段",
                        "基本形",
                        "ナイ",
                    ),
                ],
                0,
                "マ",
            ),
            5 => (
                vec![
                    t("分", "名詞,接尾,一般,*", "分", "", "", "ブン"),
                    p("の"),
                    t(
                        "悪い",
                        "形容詞,自立,*,*",
                        "悪い",
                        "形容詞・アウオ段",
                        "基本形",
                        "ワルイ",
                    ),
                ],
                0,
                "ブ",
            ),
            _ => unreachable!(),
        };
        place(&mut tokens);
        (tokens, target, reading)
    }

    fn unchanged(before: &[Token], after: &[Token]) {
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
    }

    fn only_readings_changed(before: &[Token], after: &[Token]) {
        assert_eq!(before.len(), after.len());
        let mut restored = after.to_vec();
        for (token, original) in restored.iter_mut().zip(before) {
            token.reading = Arc::clone(&original.reading);
            token.pronunciation = Arc::clone(&original.pronunciation);
        }
        unchanged(before, &restored);
    }

    #[test]
    fn minimal_idioms_change_both_readings_and_preserve_every_other_field() {
        for id in 0..6 {
            let (mut tokens, target, expected) = fixture(id);
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            assert_eq!(&*tokens[target].reading, expected, "{id}");
            assert_eq!(&*tokens[target].pronunciation, expected, "{id}");
            only_readings_changed(&before, &tokens);
            let once = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&once, &tokens);
        }
    }

    #[test]
    fn names_special_readings_and_unknown_words_are_preserved() {
        for id in 0..6 {
            for variant in 0..4 {
                let (mut tokens, target, expected) = fixture(id);
                match variant {
                    0 => tokens[target].pos = Arc::from("名詞,固有名詞,人名,名"),
                    1 => {
                        tokens[target].reading = Arc::from("マコトニ");
                        tokens[target].pronunciation = Arc::from("マコトニ");
                    }
                    2 => tokens[target].pronunciation = Arc::from("トクシュ"),
                    _ => {
                        tokens[target].reading = Arc::from(expected);
                        tokens[target].pronunciation = Arc::from(expected);
                    }
                }
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&before, &tokens);
            }
            let (mut tokens, target, _) = fixture(id);
            tokens[target].is_known = false;
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
        }
    }

    #[test]
    fn punctuation_quotes_gaps_and_other_particles_do_not_match() {
        for id in 0..6 {
            for mark in ["、", "。", "「", "」"] {
                let (mut tokens, _, _) = fixture(id);
                tokens.insert(1, t(mark, "記号,一般,*,*", mark, "", "", ""));
                place(&mut tokens);
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&before, &tokens);
            }
            let (mut tokens, _, _) = fixture(id);
            for token in &mut tokens[1..] {
                token.start += 1;
                token.end += 1;
            }
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
        }
        for id in [1, 2, 4, 5] {
            let (mut tokens, _, _) = fixture(id);
            tokens[1] = p(if id == 1 { "に" } else { "を" });
            place(&mut tokens);
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
        }
    }

    #[test]
    fn independent_words_and_other_lemmas_keep_their_readings() {
        for id in 0..6 {
            let (mut tokens, _, _) = fixture(id);
            tokens.last_mut().unwrap().base_form = Arc::from("別語");
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
        }
        let (mut tokens, _, _) = fixture(1);
        tokens.drain(..2); // 粉にするだけでは、通常のコナを保持する。
        let before = tokens.clone();
        apply(&mut tokens, &[]);
        unchanged(&before, &tokens);
        for (surface, reading) in [
            ("真に", "シンニ"),
            ("得", "トク"),
            ("体", "カラダ"),
            ("間", "アイダ"),
            ("分", "ブン"),
        ] {
            let mut tokens = vec![n(surface, reading)];
            place(&mut tokens);
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
        }
    }

    #[test]
    fn modified_body_parts_keep_the_literal_powder_reading() {
        for previous in [
            p("の"),
            t("その", "連体詞,*,*,*", "その", "", "", "ソノ"),
            t(
                "白い",
                "形容詞,自立,*,*",
                "白い",
                "形容詞・アウオ段",
                "基本形",
                "シロイ",
            ),
            t(
                "乾く",
                "動詞,自立,*,*",
                "乾く",
                "五段・カ行イ音便",
                "基本形",
                "カワク",
            ),
            t("た", "助動詞,*,*,*", "た", "特殊・タ", "基本形", "タ"),
        ] {
            let (mut tokens, _, _) = fixture(1);
            tokens.insert(0, previous);
            place(&mut tokens);
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
        }
    }

    #[test]
    fn time_ranges_and_compound_endings_are_not_idioms() {
        for id in [4, 5] {
            for prev in [
                p("の"),
                n("年", "ネン"),
                t("三", "名詞,数,*,*", "三", "", "", "サン"),
                t("その", "連体詞,*,*,*", "その", "", "", "ソノ"),
                t("数", "接頭詞,数接続,*,*", "数", "", "", "スウ"),
            ] {
                let (mut tokens, _, _) = fixture(id);
                tokens.insert(0, prev);
                place(&mut tokens);
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&before, &tokens);
            }
        }
        let (mut tokens, _, _) = fixture(5);
        tokens.insert(
            0,
            t(
                "取り",
                "動詞,自立,*,*",
                "取る",
                "五段・ラ行",
                "連用形",
                "トリ",
            ),
        );
        place(&mut tokens);
        let before = tokens.clone();
        apply(&mut tokens, &[]);
        unchanged(&before, &tokens);
    }

    #[test]
    fn split_and_fused_forms_and_recorded_inflections_are_supported() {
        let (mut tokens, _, _) = fixture(0);
        tokens.splice(..1, [n("真", "シン"), p("に")]);
        tokens.last_mut().unwrap().surface = Arc::from("受け");
        tokens.last_mut().unwrap().conj_form = Arc::from("連用形");
        place(&mut tokens);
        apply(&mut tokens, &[]);
        assert_eq!(&*tokens[0].reading, "マ");

        let (mut tokens, _, _) = fixture(1);
        tokens[4].surface = Arc::from("し");
        tokens[4].conj_form = Arc::from("連用形");
        place(&mut tokens);
        apply(&mut tokens, &[]);
        assert_eq!(&*tokens[2].reading, "コ");

        let (mut tokens, _, _) = fixture(3);
        tokens[1] = t(
            "よく",
            "形容詞,自立,*,*",
            "よい",
            "形容詞・アウオ段",
            "連用テ接続",
            "ヨク",
        );
        tokens[2].surface = Arc::from("断っ");
        tokens[2].conj_form = Arc::from("連用タ接続");
        place(&mut tokens);
        apply(&mut tokens, &[]);
        assert_eq!(&*tokens[0].reading, "テイ");
        tokens.splice(
            ..2,
            [t("体よく", "副詞,一般,*,*", "体よく", "", "", "カラダヨク")],
        );
        place(&mut tokens);
        apply(&mut tokens, &[]);
        assert_eq!(&*tokens[0].reading, "テイヨク");

        let (mut tokens, _, _) = fixture(4);
        tokens[2].surface = Arc::from("なく");
        tokens[2].conj_form = Arc::from("連用テ接続");
        place(&mut tokens);
        apply(&mut tokens, &[]);
        assert_eq!(&*tokens[0].reading, "マ");

        let (mut tokens, _, _) = fixture(5);
        tokens.splice(
            ..2,
            [t("分の", "名詞,接尾,助数詞,*", "分の", "", "", "ブンノ")],
        );
        tokens[1].surface = Arc::from("悪かっ");
        tokens[1].conj_form = Arc::from("連用タ接続");
        place(&mut tokens);
        apply(&mut tokens, &[]);
        assert_eq!(&*tokens[0].reading, "ブノ");
    }

    #[test]
    fn protected_ranges_cover_the_whole_phrase() {
        for id in 0..6 {
            let (mut tokens, _, _) = fixture(id);
            let span = tokens.last().unwrap().start..tokens.last().unwrap().end;
            let before = tokens.clone();
            apply(&mut tokens, &[span]);
            unchanged(&before, &tokens);
        }
    }

    #[test]
    #[ignore = "配布辞書が必要。HASAMI_IDIOM_TEST_DICT で検証する辞書を指定可能"]
    fn distributed_idiom_readings() {
        let path = std::env::var_os("HASAMI_IDIOM_TEST_DICT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "dict/ipadic-neologd-sudachi.hsd".into());
        let dict = crate::Dictionary::load(path).unwrap();
        let examples = [
            ("真に受ける", Some("マニウケル")),
            ("真に受けた", Some("マニウケタ")),
            ("真に受けない", Some("マニウケナイ")),
            ("身を粉にする", Some("ミヲコニスル")),
            ("身を粉にした", Some("ミヲコニシタ")),
            ("魚の身を粉にする", Some("サカナノミヲコナニスル")),
            ("乾いた身を粉にする", Some("カワイタミヲコナニスル")),
            (
                "乾燥させた身を粉にする",
                Some("カンソウサセタミヲコナニスル"),
            ),
            ("得も言われぬ", Some("エモイワレヌ")),
            ("体よく断る", Some("テイヨクコトワル")),
            ("体よく断った", Some("テイヨクコトワッタ")),
            ("間もない", Some("マモナイ")),
            ("間もなく", Some("マモナク")),
            ("分の悪い", Some("ブノワルイ")),
            ("分の悪かった", Some("ブノワルカッタ")),
            ("一年の間もない", None),
            ("三分の悪い", None),
            ("取り分の悪い", None),
            ("真に必要だ", None),
            ("粉にする", None),
            ("得をする", None),
            ("体で覚える", None),
            ("体を成す", None),
            ("身を「粉」にする", None),
        ];
        for (text, expected) in examples {
            let mut tokens = super::super::LatticeWorkspace::new()
                .tokenize(text, &dict)
                .unwrap();
            if let Some(expected) = expected {
                assert_eq!(
                    tokens.iter().map(|t| &*t.reading).collect::<String>(),
                    expected,
                    "{text}"
                );
            }
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
            println!("IDIOM_REAL_TOKEN\t{text}\t{tokens:?}");
        }
        // 任意の非公開回帰入力は環境変数で渡し、公開テストには本文を埋め込まない。
        if let Some(path) = std::env::var_os("HASAMI_IDIOM_CONTEXTS") {
            let contexts = std::fs::read_to_string(path).unwrap();
            let mut workspace = super::super::LatticeWorkspace::new();
            for (i, text) in contexts.lines().enumerate() {
                let tokens = workspace.tokenize(text, &dict).unwrap();
                println!("IDIOM_CONTEXT_TOKEN\t{i}\t{tokens:?}");
            }
        }
    }
}
