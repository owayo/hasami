//! アナライザー - 形態素解析の高レベルAPI

use crate::char_class::CharClassifier;
use crate::dict::Dictionary;
use crate::lattice::{LatticeWorkspace, Token};
use crate::mmap_dict::MmapDictionary;
use std::io;
use std::path::Path;
use std::sync::Arc;

/// 文境界文字かどうか判定
#[inline]
fn is_sentence_boundary(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '!' | '?' | '\n')
}

/// 辞書バックエンド
enum DictBackend {
    /// mmap辞書（本番用、ゼロコピー高速）
    Mmap {
        dict: Box<MmapDictionary>,
        classifier: CharClassifier,
    },
    /// インメモリ辞書（テスト用）
    InMemory { dict: Arc<Dictionary> },
}

/// 形態素解析器
pub struct Analyzer {
    backend: DictBackend,
    workspace: LatticeWorkspace,
}

impl Analyzer {
    /// .hsd 辞書ファイルからアナライザーを生成
    pub fn load<P: AsRef<Path>>(dict_path: P) -> io::Result<Self> {
        let dict = MmapDictionary::load(dict_path)?;
        let classifier = CharClassifier::default_japanese();
        Ok(Analyzer {
            backend: DictBackend::Mmap {
                dict: Box::new(dict),
                classifier,
            },
            workspace: LatticeWorkspace::new(),
        })
    }

    /// 辞書オブジェクトから直接生成（テスト用）
    pub fn from_dict(dict: Dictionary) -> Self {
        Analyzer {
            backend: DictBackend::InMemory {
                dict: Arc::new(dict),
            },
            workspace: LatticeWorkspace::new(),
        }
    }

    /// テキストを形態素解析（文分割で高速化）
    pub fn tokenize(&mut self, input: &str) -> Vec<Token> {
        if input.is_empty() {
            return vec![];
        }
        self.tokenize_sentences(input)
    }

    /// テキストを文境界で分割して各文を独立に解析
    fn tokenize_sentences(&mut self, input: &str) -> Vec<Token> {
        let mut all_tokens = Vec::new();
        let mut seg_start = 0;

        for (i, c) in input.char_indices() {
            if is_sentence_boundary(c) {
                let seg_end = i + c.len_utf8();
                let segment = &input[seg_start..seg_end];
                if !segment.is_empty() {
                    let tokens = self.tokenize_segment(segment);
                    for mut t in tokens {
                        t.start += seg_start;
                        t.end += seg_start;
                        all_tokens.push(t);
                    }
                }
                seg_start = seg_end;
            }
        }

        // 最後のセグメント
        if seg_start < input.len() {
            let segment = &input[seg_start..];
            let tokens = self.tokenize_segment(segment);
            for mut t in tokens {
                t.start += seg_start;
                t.end += seg_start;
                all_tokens.push(t);
            }
        }

        all_tokens
    }

    #[inline]
    fn tokenize_segment(&mut self, segment: &str) -> Vec<Token> {
        match &self.backend {
            DictBackend::Mmap { dict, classifier } => {
                self.workspace.tokenize_v2(segment, dict, classifier)
            }
            DictBackend::InMemory { dict } => self.workspace.tokenize(segment, dict),
        }
    }

    /// 複数テキストをバッチ処理
    pub fn tokenize_batch(&mut self, inputs: &[&str]) -> Vec<Vec<Token>> {
        inputs.iter().map(|input| self.tokenize(input)).collect()
    }
}

/// MeCab互換の出力フォーマット
pub fn format_mecab(tokens: &[Token]) -> String {
    let mut output = String::new();
    for token in tokens {
        output.push_str(&token.surface);
        output.push('\t');
        output.push_str(&token.pos);
        if !token.base_form.is_empty() {
            output.push(',');
            output.push_str(&token.base_form);
        }
        if !token.reading.is_empty() {
            output.push(',');
            output.push_str(&token.reading);
        }
        if !token.pronunciation.is_empty() {
            output.push(',');
            output.push_str(&token.pronunciation);
        }
        output.push('\n');
    }
    output.push_str("EOS\n");
    output
}

/// Wakachi（分かち書き）出力
pub fn format_wakachi(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| &*t.surface)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};

    fn make_analyzer() -> Analyzer {
        let mut builder = DictBuilder::new();
        let words = vec![
            ("私", 1, 1, 3000, "名詞,代名詞,一般,*", "ワタシ"),
            ("は", 2, 2, 4000, "助詞,係助詞,*,*", "ハ"),
            ("猫", 3, 3, 3500, "名詞,一般,*,*", "ネコ"),
            ("です", 4, 4, 4000, "助動詞,*,*,*", "デス"),
        ];

        for (surface, lid, rid, cost, pos, reading) in words {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: lid,
                right_id: rid,
                cost,
                pos: pos.into(),
                base_form: surface.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
            });
        }

        Analyzer::from_dict(builder.build())
    }

    #[test]
    fn test_basic_tokenize() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私は猫です");

        assert!(!tokens.is_empty());
        let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(surfaces, vec!["私", "は", "猫", "です"]);
    }

    #[test]
    fn test_wakachi() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私は猫です");
        let result = format_wakachi(&tokens);
        assert_eq!(result, "私 は 猫 です");
    }

    #[test]
    fn test_empty_input() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_workspace_reuse_perf() {
        let mut analyzer = make_analyzer();
        for _ in 0..100 {
            let tokens = analyzer.tokenize("私は猫です");
            assert_eq!(tokens.len(), 4);
        }
    }
}
