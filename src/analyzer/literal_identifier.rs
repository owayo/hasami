//! 明示されたASCII英数字識別子を保護し、限定した未知IDだけを字母・単桁読みに写す。

use super::Analyzer;
use crate::{DictError, Token};

/// 呼出側が識別子と指定した、ASCII郵便形または限定した電話形だけを認可する。
/// 形が一致しても、入力の意味が電話番号や郵便番号だとは保証しない。
fn numeric_literal_shape(input: &str) -> bool {
    if !matches!(input.len(), 8 | 12 | 13)
        || !input.as_bytes().first().is_some_and(u8::is_ascii_digit)
    {
        return false;
    }
    let groups: Vec<_> = input.split('-').collect();
    if !groups
        .iter()
        .all(|group| !group.is_empty() && group.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    match groups.as_slice() {
        [first, last] => first.len() == 3 && last.len() == 4,
        [first, middle, last] => {
            input.starts_with('0')
                && (2..=5).contains(&first.len())
                && (1..=4).contains(&middle.len())
                && last.len() == 4
                && matches!(first.len() + middle.len() + last.len(), 10 | 11)
        }
        _ => false,
    }
}

fn identifier_envelope(input: &str) -> bool {
    let bytes = input.as_bytes();
    if !bytes
        .first()
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        || !bytes
            .last()
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return false;
    }
    let (mut letter, mut digit, mut separator) = (false, false, false);
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b == b'_' {
            letter |= b.is_ascii_alphabetic();
            digit |= b.is_ascii_digit();
            separator = false;
        } else if matches!(b, b'-' | b'.') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    letter && digit
}

fn shape(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.len() < 5 || !bytes[0].is_ascii_uppercase() {
        return false;
    }
    let (mut separator, mut digit) = (false, false);
    for &b in bytes {
        if b.is_ascii_uppercase() || b.is_ascii_digit() {
            digit |= b.is_ascii_digit();
            separator = false;
        } else if matches!(b, b'-' | b'_') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    digit && !separator
}

fn literal(input: &str) -> Option<String> {
    let mut result = String::new();
    for c in input.chars() {
        if c.is_ascii_uppercase() {
            result.push_str(&crate::lattice::alphabet_reading(&c.to_string())?);
        } else if c.is_ascii_digit() {
            result.push_str(&crate::numeric::literal_digit_pronunciation(c)?);
        } else if !matches!(c, '-' | '_') {
            return None;
        }
    }
    Some(result)
}

/// 選択された既知の断片が字母と各桁を読むだけなら、その英語の数字音も保つ。
/// 語の展開形や数の位取りは認可しない。
fn literal_speech(surface: &str, speech: &str) -> bool {
    if speech.is_empty() {
        return false;
    }
    const ENGLISH_DIGITS: [&str; 10] = [
        "ゼロ",
        "ワン",
        "ツー",
        "スリー",
        "フォー",
        "ファイブ",
        "シックス",
        "セブン",
        "エイト",
        "ナイン",
    ];
    let mut remaining = vec![speech];
    for c in surface.chars() {
        let Some(standard) = literal(&c.to_string()) else {
            return false;
        };
        if standard.is_empty() {
            return false;
        }
        let english = c
            .is_ascii_digit()
            .then(|| ENGLISH_DIGITS[c as usize - '0' as usize]);
        let mut next = Vec::new();
        for rest in remaining {
            if let Some(tail) = rest.strip_prefix(&standard) {
                next.push(tail);
            }
            if let Some(tail) = english.and_then(|word| rest.strip_prefix(word))
                && !next.contains(&tail)
            {
                next.push(tail);
            }
        }
        if next.is_empty() {
            return false;
        }
        remaining = next;
    }
    remaining.contains(&"")
}

fn covered(input: &str, tokens: &[Token]) -> bool {
    tokens.first().is_some_and(|t| t.start == 0)
        && tokens.last().is_some_and(|t| t.end == input.len())
        && tokens.windows(2).all(|p| p[0].end == p[1].start)
        && tokens
            .iter()
            .all(|t| input.get(t.start..t.end) == Some(t.surface.as_ref()))
}

pub(super) fn normalize(analyzer: &mut Analyzer, input: &str) -> Result<Option<String>, DictError> {
    let numeric_literal = numeric_literal_shape(input);
    if !numeric_literal && !identifier_envelope(input) {
        return Ok(None);
    }
    // 既知の一語は長さ・大小文字・字母読みの適用条件より先に原文で保護する。
    // 辞書の照合は接頭辞検索なので、全体長と一致するエントリだけを使う。
    if analyzer
        .dictionary()
        .lookup(input)?
        .iter()
        .any(|(end, rows)| *end == input.len() && !rows.is_empty())
    {
        return Ok(Some(input.to_owned()));
    }
    if numeric_literal {
        return Ok(literal(input));
    }
    if !shape(input) {
        return Ok(None);
    }
    let plan = crate::numeric::AsciiPlan::new(input, 0);
    if !plan.identifier_envelope_exact(0, input.len()) {
        return Ok(None);
    }
    // ID内では既知の英字語幹が未知語経路に負けることもある。連続する英字語幹の
    // 全体に明示読みがあれば、選択経路にかかわらず既存の数値変換へ戻す。
    for stem in input
        .split(|c: char| !c.is_ascii_uppercase())
        .filter(|s| s.len() > 1)
    {
        let spelling = literal(stem).expect("ASCII英字の語幹");
        if analyzer
            .dictionary()
            .lookup(stem)?
            .iter()
            .any(|(end, rows)| {
                *end == stem.len()
                    && rows.iter().any(|row| {
                        (!row.reading.is_empty() && row.reading.as_ref() != spelling)
                            || (!row.pronunciation.is_empty()
                                && row.pronunciation.as_ref() != spelling)
                    })
            })
        {
            return Ok(None);
        }
    }
    let tokens = analyzer.try_tokenize(input)?;
    if crate::numeric::units::quantity_pronunciation(input, &tokens).is_some()
        || !covered(input, &tokens)
    {
        return Ok(None);
    }
    for token in &tokens {
        if token.is_known && token.surface.bytes().all(|b| b.is_ascii_uppercase()) {
            let spelling = literal(&token.surface).expect("ASCII英字の語幹");
            if (!token.reading.is_empty() && token.reading.as_ref() != spelling)
                || (!token.pronunciation.is_empty() && token.pronunciation.as_ref() != spelling)
            {
                return Ok(None);
            }
            // 識別子の保護が既知語の読みを字母読みに戻す場合も、選択された元登録の
            // POS・原形・コスト・活用と一致する明示読みを保護する。同じ素性の異読は
            // 区別できないので、いずれかが字母読みでなければ保守的にNoneを返す。
            for (end, rows) in analyzer.dictionary().lookup(&token.surface)? {
                if end != token.surface.len() {
                    continue;
                }
                if rows.iter().any(|row| {
                    let conjugation_matches = |source: &str, selected: &str| {
                        source == selected || source == "*" && selected.is_empty()
                    };
                    row.cost == token.word_cost
                        && row.pos == token.pos
                        && row.base_form == token.base_form
                        && conjugation_matches(&row.conj_type, &token.conj_type)
                        && conjugation_matches(&row.conj_form, &token.conj_form)
                        && ((!row.reading.is_empty() && row.reading.as_ref() != spelling)
                            || (!row.pronunciation.is_empty()
                                && row.pronunciation.as_ref() != spelling))
                }) {
                    return Ok(None);
                }
            }
        }
    }
    let mut result = String::new();
    for token in &tokens {
        let mixed_literal = token.is_known_base_form()
            && token.surface.bytes().any(|b| b.is_ascii_uppercase())
            && token.surface.bytes().any(|b| b.is_ascii_digit())
            && token
                .surface
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            && literal_speech(&token.surface, &token.reading)
            && literal_speech(&token.surface, &token.pronunciation);
        if mixed_literal {
            result.push_str(&token.pronunciation);
        } else {
            // shape と covered で確認済みの部分範囲。区切りだけの断片は空文字になる。
            result.push_str(&literal(&token.surface).expect("確認済みASCII識別子の断片"));
        }
    }
    Ok(Some(result))
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};

    fn analyzer(whole: Option<&str>) -> Analyzer {
        let mut b = DictBuilder::new();
        for (s, base, pos, r) in [
            ("M4", "NGC6121", "名詞,固有名詞,一般,*", "エヌジーシー"),
            ("1B", "1B", "名詞,一般,*,*", "ファーストベースマン"),
            ("-4", "−4", "名詞,固有名詞,一般,*", "マイナスヨン"),
            ("4-1", "4-1", "名詞,固有名詞,人名,一般", "シッピン"),
            ("NASA", "NASA", "名詞,固有名詞,組織,*", "ナサ"),
            ("COP", "COP", "名詞,固有名詞,一般,*", "コップ"),
            ("AI", "AI", "名詞,固有名詞,一般,*", "アイ"),
            ("V", "明示した原形", "名詞,固有名詞,一般,*", "カスタム"),
            ("G2", "G2", "名詞,固有名詞,一般,*", "ジェネレーションツー"),
            ("M98", "M98", "名詞,固有名詞,一般,*", "エヌジーシー"),
            ("A3", "A3", "名詞,固有名詞,一般,*", "エースリー"),
            ("U2", "U2", "名詞,固有名詞,一般,*", "ャ"),
            ("C3PO", "C3PO", "名詞,固有名詞,一般,*", "シースリーピーオー"),
            ("3G", "3G", "名詞,固有名詞,一般,*", "スリージー"),
            ("Q4F", "Q4F", "名詞,固有名詞,一般,*", "キューフォーエフ"),
            ("Q2F", "Q2F", "名詞,固有名詞,一般,*", "キューツーエフ"),
            (
                "Q24F",
                "Q24F",
                "名詞,固有名詞,一般,*",
                "キューニジュウヨンエフ",
            ),
            (
                "Python3",
                "Python3",
                "名詞,固有名詞,一般,*",
                "パイソンスリー",
            ),
            (
                "GPT-5.2",
                "GPT-5.2",
                "名詞,固有名詞,一般,*",
                "メイジシタヨミ",
            ),
        ] {
            b.add_entry(DictEntry {
                surface: s.into(),
                base_form: base.into(),
                pos: pos.into(),
                reading: r.into(),
                pronunciation: r.into(),
                cost: if s == "COP" { 30000 } else { -10000 },
                ..Default::default()
            });
        }
        if let Some(r) = whole {
            b.add_entry(DictEntry {
                surface: "ZX1B-90Q".into(),
                base_form: "明示原形".into(),
                pos: "名詞,固有名詞,人名,姓".into(),
                reading: r.into(),
                pronunciation: r.into(),
                cost: -20000,
                ..Default::default()
            });
        }
        Analyzer::from_dict(b.build().unwrap())
    }

    #[test]
    fn explicit_literal_result_keeps_default_all_fields_and_uses_existing_speech() {
        let mut a = analyzer(None);
        for (input, expected) in [
            ("ZX1B-90Q", "ゼットエックスイチビーキューゼロキュー"),
            ("ZX-M4-190Q", "ゼットエックスエムヨンイチキューゼロキュー"),
            (
                "ABCD-4-1-Z99",
                "エービーシーディーヨンイチゼットキューキュー",
            ),
            (
                "ISBN978-4-1234-5678-0",
                "アイエスビーエヌキューナナハチヨンイチニサンヨンゴロクナナハチゼロ",
            ),
        ] {
            let before = a.try_tokenize(input).unwrap();
            assert_eq!(
                a.normalize_identifier(input).unwrap().as_deref(),
                Some(expected)
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", a.try_tokenize(input).unwrap())
            );
        }
    }

    #[test]
    fn known_custom_mixed_quantity_formula_url_and_non_ascii_inputs_are_none() {
        let mut a = analyzer(None);
        for input in [
            "NASA3X9",
            "COP28",
            "AI64Z",
            "AB12V",
            "GitHub3",
            "Large70",
            "32GB",
            "20mA",
            "32bit",
            "41回",
            "3分の1",
            "2025-01-09",
            "978-4-1234-5678-0",
            "AB12+CD34",
            "AB12/CD34",
            "AB12.CD34",
            "http://AB12",
            "AB12=CD34",
            "AB--12",
            "AB12_",
            " AB12Z",
            "AB12Z ",
            "ＡＢ12Z",
            "AB１2Z",
            "AB\n12Z",
            "AB12Z😀",
        ] {
            assert_eq!(
                a.normalize_identifier(input).unwrap(),
                None,
                "{input}: {:?}",
                a.try_tokenize(input).unwrap()
            );
        }
        for r in ["カスタム", ""] {
            let mut a = analyzer(Some(r));
            assert_eq!(
                a.normalize_identifier("ZX1B-90Q").unwrap().as_deref(),
                Some("ZX1B-90Q")
            );
        }
    }

    #[test]
    fn known_whole_ids_are_preserved_before_short_mixed_or_dotted_guards() {
        let mut a = analyzer(None);
        for input in ["G2", "M98", "A3", "U2", "C3PO", "3G", "Python3", "GPT-5.2"] {
            let before = a.try_tokenize(input).unwrap();
            assert_eq!(
                a.normalize_identifier(input).unwrap().as_deref(),
                Some(input)
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", a.try_tokenize(input).unwrap())
            );
        }
        for input in [
            "R-4",
            "IL-4",
            "GPT-5.3",
            "AB12+CD34",
            "2025-01-09",
            "https://AB12",
        ] {
            assert_eq!(a.normalize_identifier(input).unwrap(), None);
        }
    }

    #[test]
    fn selected_known_literal_fragments_keep_english_digits_without_word_expansions() {
        let mut a = analyzer(None);
        for (input, fragment, expected) in [
            ("ZXQ4F-1B", "Q4F", "ゼットエックスキューフォーエフイチビー"),
            ("ZXQ2F-M4", "Q2F", "ゼットエックスキューツーエフエムヨン"),
            (
                "ZXQ24F-Z9",
                "Q24F",
                "ゼットエックスキューニヨンエフゼットキュー",
            ),
        ] {
            let before = a.try_tokenize(input).unwrap();
            assert!(
                before
                    .iter()
                    .any(|t| t.is_known && t.surface.as_ref() == fragment)
            );
            assert_eq!(
                a.normalize_identifier(input).unwrap().as_deref(),
                Some(expected)
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", a.try_tokenize(input).unwrap())
            );
        }
    }

    #[test]
    fn explicit_numeric_literal_shapes_ignore_partial_minus_but_keep_all_fields() {
        let mut a = analyzer(None);
        for (input, expected) in [
            ("123-4567", "イチニサンヨンゴロクナナ"),
            ("03-1234-5678", "ゼロサンイチニサンヨンゴロクナナハチ"),
            (
                "090-1234-5678",
                "ゼロキューゼロイチニサンヨンゴロクナナハチ",
            ),
        ] {
            let before = a.try_tokenize(input).unwrap();
            if input == "123-4567" {
                assert!(
                    before
                        .iter()
                        .any(|token| token.is_known && token.surface.as_ref() == "-4")
                );
            }
            assert_eq!(
                a.normalize_identifier(input).unwrap().as_deref(),
                Some(expected)
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{:?}", a.try_tokenize(input).unwrap())
            );
        }
        for input in [
            "2025-01-09",
            "978-4-1234-5678-0",
            "12-34",
            "123",
            "1234567",
            "1+2",
            "123-4567+8",
            "１２３-４５６７",
            "123−4567",
            " 123-4567",
            "0-12345-6789",
            "03-1234-56789",
            "12-3456-7890",
        ] {
            assert_eq!(a.normalize_identifier(input).unwrap(), None, "{input}");
        }
    }

    #[test]
    fn numeric_known_whole_keeps_explicit_reading_before_literal_shape() {
        let mut builder = DictBuilder::new();
        builder.add_entry(DictEntry {
            surface: "123-4567".into(),
            base_form: "明示した原形".into(),
            pos: "名詞,固有名詞,一般,*".into(),
            reading: "メイジヨミ".into(),
            pronunciation: "メイジヨミ".into(),
            cost: -20000,
            ..Default::default()
        });
        let mut a = Analyzer::from_dict(builder.build().unwrap());
        let before = a.try_tokenize("123-4567").unwrap();
        assert_eq!(
            a.normalize_identifier("123-4567").unwrap().as_deref(),
            Some("123-4567")
        );
        assert_eq!(
            format!("{before:?}"),
            format!("{:?}", a.try_tokenize("123-4567").unwrap())
        );
    }
}
