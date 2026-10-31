//! 明示されたASCII英数字識別子を保護し、限定した未知IDだけを字母・単桁読みに写す。

use super::Analyzer;
use crate::{DictError, Token};

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
    (!result.is_empty()).then_some(result)
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
    if !identifier_envelope(input) {
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
    Ok(literal(input))
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
}
