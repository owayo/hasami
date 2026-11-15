//! 利用者が確定した IME 語彙を、形態素の並びから慎重に分類する。
//!
//! 品詞判定は IPAdic 形式を前提とする。語彙の自動登録を許可する API ではなく、
//! 呼び出し側が未知語・数量・句を区別するための材料を返す。
use crate::{Analyzer, DictError, Token};

/// 解析された語の並び。空白などを含み入力を覆えない場合は `Other`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermShape {
    KnownWord,
    KnownCompound,
    Quantity,
    PhraseLike,
    ContainsUnknown,
    Other,
}
/// 語形の分類と、その根拠になった解析結果。
#[derive(Debug)]
pub struct TermReport {
    pub shape: TermShape,
    pub tokens: Vec<Token>,
    pub covers_input: bool,
}

impl Token {
    /// IPAdic の助詞・助動詞・非自立語・活用語の接尾辞か。
    pub fn is_dependent(&self) -> bool {
        let fields: Vec<_> = self.pos.split(',').collect();
        matches!(fields.first(), Some(&"助詞" | &"助動詞"))
            || fields.get(1) == Some(&"非自立")
            || (matches!(fields.first(), Some(&"動詞" | &"形容詞"))
                && fields.get(1) == Some(&"接尾"))
    }
    /// 依存語を除いた名詞・動詞・形容詞・副詞か。既知語かは別途確認する。
    pub fn is_content_word(&self) -> bool {
        !self.is_dependent()
            && self
                .pos
                .split(',')
                .next()
                .is_some_and(|p| matches!(p, "名詞" | "動詞" | "形容詞" | "副詞"))
    }
}
impl Analyzer {
    /// 入力を解析し、全文の被覆と IPAdic の品詞に基づいて語形を分類する。
    ///
    /// 数量と依存語は既知語判定より先に調べる。解析器が数詞を合成した場合も
    /// `Quantity` となり、未知語を含む句は `PhraseLike` となる。
    pub fn try_classify_term(&mut self, text: &str) -> Result<TermReport, DictError> {
        let tokens = self.try_tokenize(text)?;
        let covers_input = tokens
            .iter()
            .map(|t| t.surface.as_ref())
            .collect::<String>()
            == text;
        let shape = if !covers_input || tokens.is_empty() {
            TermShape::Other
        } else if tokens
            .iter()
            .all(|t| t.pos.starts_with("名詞,数") || t.pos.contains("助数詞"))
        {
            TermShape::Quantity
        } else if tokens.iter().any(Token::is_dependent) {
            TermShape::PhraseLike
        } else if tokens.iter().any(|t| !t.is_known) {
            TermShape::ContainsUnknown
        } else if tokens.len() == 1 && tokens.iter().all(Token::is_content_word) {
            TermShape::KnownWord
        } else if tokens.iter().all(|t| t.pos.starts_with("名詞,")) {
            TermShape::KnownCompound
        } else {
            TermShape::PhraseLike
        };
        Ok(TermReport {
            shape,
            tokens,
            covers_input,
        })
    }
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;
    use crate::{DictEntry, dict::DictBuilder};

    fn analyzer() -> Analyzer {
        let mut builder = DictBuilder::new();
        for (surface, pos, reading) in [
            ("猫", "名詞,一般,*,*", "ネコ"),
            ("図鑑", "名詞,一般,*,*", "ズカン"),
            ("は", "助詞,係助詞,*,*", "ハ"),
            ("です", "助動詞,*,*,*", "デス"),
            ("こと", "名詞,非自立,一般,*", "コト"),
            ("二", "名詞,数,*,*", "ニ"),
            ("冊", "名詞,接尾,助数詞,*", "サツ"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: 1,
                right_id: 1,
                cost: -1000,
                pos: pos.into(),
                base_form: surface.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
                ..Default::default()
            });
        }
        Analyzer::from_dict(builder.build().unwrap())
    }

    #[test]
    fn distinguishes_words_compounds_quantities_and_phrases() {
        let mut analyzer = analyzer();
        for (text, expected) in [
            ("猫", TermShape::KnownWord),
            ("猫図鑑", TermShape::KnownCompound),
            ("二冊", TermShape::Quantity),
            ("猫は猫です", TermShape::PhraseLike),
            ("こと", TermShape::PhraseLike),
            ("XYZ", TermShape::ContainsUnknown),
            ("XYZです", TermShape::PhraseLike),
        ] {
            let report = analyzer.try_classify_term(text).unwrap();
            assert_eq!(report.shape, expected, "{text}");
            assert!(report.covers_input);
            assert_eq!(
                report
                    .tokens
                    .iter()
                    .map(|t| &*t.surface)
                    .collect::<String>(),
                text
            );
        }
    }

    #[test]
    fn empty_and_uncovered_input_cannot_be_registered_as_words() {
        let mut analyzer = analyzer();
        let empty = analyzer.try_classify_term("").unwrap();
        assert_eq!(empty.shape, TermShape::Other);
        assert!(empty.tokens.is_empty());
        for text in [" 猫", "猫 図鑑", "猫\t"] {
            let report = analyzer.try_classify_term(text).unwrap();
            assert_eq!(report.shape, TermShape::Other);
            assert!(!report.covers_input);
        }
    }

    #[test]
    fn content_word_does_not_imply_a_dictionary_match() {
        let mut token = analyzer().tokenize("猫").remove(0);
        token.is_known = false;
        assert!(token.is_content_word());
        for pos in [
            "助詞,係助詞,*,*",
            "助動詞,*,*,*",
            "動詞,接尾,*,*",
            "形容詞,非自立,*,*",
        ] {
            token.pos = pos.into();
            assert!(token.is_dependent());
            assert!(!token.is_content_word());
        }
        token.pos = "記号,一般,*,*".into();
        assert!(!token.is_dependent());
        assert!(!token.is_content_word());
    }
}
