//! 文字分類 - Unicode文字種に基づく未知語処理

use std::collections::HashMap;

/// 文字クラス定義
#[derive(Clone, Debug)]
pub struct CharClass {
    /// クラス名
    pub name: String,
    /// invoke: 常に未知語処理を起動するか
    pub invoke: bool,
    /// group: 同一クラスの文字をグルーピングするか
    pub group: bool,
    /// length: グルーピング時の最大長（0=無制限）
    pub length: u32,
}

/// CharType ごとの属性キャッシュ（HashMap 参照を排除するための固定長配列用）
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct ClassProps {
    invoke: bool,
    group: bool,
    max_length: u32,
}

impl Default for ClassProps {
    fn default() -> Self {
        // デフォルト: group=true（classes に定義がない場合のフォールバック動作を維持）
        ClassProps {
            invoke: false,
            group: true,
            max_length: 0,
        }
    }
}

/// CharType の総数
const NUM_CHAR_TYPES: usize = 9;

/// 文字分類器
#[derive(Clone, Debug)]
pub struct CharClassifier {
    /// 文字クラス定義
    pub classes: HashMap<String, CharClass>,
    /// Unicode範囲マッピング (start, end, class_name) - ソート済み
    pub ranges: Vec<(u32, u32, String)>,
    /// CharType ごとの属性キャッシュ（ホットパス用）
    props_cache: [ClassProps; NUM_CHAR_TYPES],
}

/// 文字種（簡易分類）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharType {
    /// ひらがな
    Hiragana,
    /// カタカナ
    Katakana,
    /// 漢字
    Kanji,
    /// ASCII英字
    Alpha,
    /// ASCII数字
    Numeric,
    /// 全角数字
    NumericWide,
    /// 記号・その他
    Symbol,
    /// 空白
    Space,
    /// その他
    Default,
}

/// CharType → 配列インデックス変換
#[inline]
pub fn type_index(ct: CharType) -> usize {
    match ct {
        CharType::Hiragana => 0,
        CharType::Katakana => 1,
        CharType::Kanji => 2,
        CharType::Alpha => 3,
        CharType::Numeric => 4,
        CharType::NumericWide => 5,
        CharType::Symbol => 6,
        CharType::Space => 7,
        CharType::Default => 8,
    }
}

impl CharType {
    /// 文字クラス名を返す
    pub fn class_name(&self) -> &'static str {
        match self {
            CharType::Hiragana => "HIRAGANA",
            CharType::Katakana => "KATAKANA",
            CharType::Kanji => "KANJI",
            CharType::Alpha => "ALPHA",
            CharType::Numeric => "NUMERIC",
            CharType::NumericWide => "NUMERIC",
            CharType::Symbol => "SYMBOL",
            CharType::Space => "SPACE",
            CharType::Default => "DEFAULT",
        }
    }
}

/// classes HashMap から props_cache を構築する
fn build_props_cache(classes: &HashMap<String, CharClass>) -> [ClassProps; NUM_CHAR_TYPES] {
    let all_types = [
        CharType::Hiragana,
        CharType::Katakana,
        CharType::Kanji,
        CharType::Alpha,
        CharType::Numeric,
        CharType::NumericWide,
        CharType::Symbol,
        CharType::Space,
        CharType::Default,
    ];
    let mut cache = [ClassProps::default(); NUM_CHAR_TYPES];
    for &ct in &all_types {
        let idx = type_index(ct);
        if let Some(class) = classes.get(ct.class_name()) {
            cache[idx] = ClassProps {
                invoke: class.invoke,
                group: class.group,
                max_length: class.length,
            };
        }
    }
    cache
}

impl CharClassifier {
    /// デフォルトの日本語文字分類器
    pub fn default_japanese() -> Self {
        let mut classes = HashMap::new();

        // MeCab互換の文字クラス定義
        let defs = vec![
            ("DEFAULT", false, true, 0),
            ("SPACE", false, true, 0),
            ("KANJI", false, false, 2),
            ("HIRAGANA", false, true, 2),
            ("KATAKANA", true, true, 2),
            ("ALPHA", true, true, 0),
            ("NUMERIC", true, true, 0),
            ("SYMBOL", true, true, 0),
            ("KANJINUMERIC", true, true, 0),
        ];

        for (name, invoke, group, length) in defs {
            classes.insert(
                name.to_string(),
                CharClass {
                    name: name.to_string(),
                    invoke,
                    group,
                    length,
                },
            );
        }

        let props_cache = build_props_cache(&classes);

        CharClassifier {
            classes,
            ranges: Vec::new(),
            props_cache,
        }
    }

    /// char.def の定義から構築
    pub fn from_definitions(
        classes: HashMap<String, CharClass>,
        mut ranges: Vec<(u32, u32, String)>,
    ) -> Self {
        // ranges をソートして二分探索を可能にする
        ranges.sort_unstable_by_key(|&(start, _, _)| start);
        let props_cache = build_props_cache(&classes);
        CharClassifier {
            classes,
            ranges,
            props_cache,
        }
    }

    /// props_cache を classes HashMap から再構築する
    ///
    /// export_char_classifier 等で classes を直接変更した後に呼ぶこと
    pub fn rebuild_props_cache(&mut self) {
        self.props_cache = build_props_cache(&self.classes);
    }

    /// 文字のCharTypeを判定
    #[inline]
    pub fn classify_char(&self, c: char) -> CharType {
        let cp = c as u32;

        // char.def の範囲マッピングがあればそちらを優先（二分探索）
        if !self.ranges.is_empty() {
            // cp を含む可能性のある範囲を二分探索で探す
            // start <= cp となる最後のエントリを見つける
            let idx = self.ranges.partition_point(|&(start, _, _)| start <= cp);
            if idx > 0 {
                // idx-1 が start <= cp を満たす最後のエントリ
                // そこから逆方向に、cp がまだ範囲内にある限り探索
                for i in (0..idx).rev() {
                    let (start, end, ref class_name) = self.ranges[i];
                    if start > cp {
                        continue;
                    }
                    if cp <= end {
                        return match class_name.as_str() {
                            "HIRAGANA" => CharType::Hiragana,
                            "KATAKANA" => CharType::Katakana,
                            "KANJI" | "KANJINUMERIC" => CharType::Kanji,
                            "ALPHA" => CharType::Alpha,
                            "NUMERIC" => CharType::Numeric,
                            "SYMBOL" => CharType::Symbol,
                            "SPACE" => CharType::Space,
                            _ => CharType::Default,
                        };
                    }
                    // 範囲が重ならない場合は早期終了可能
                    // ただし char.def は重複範囲を持つ可能性があるので、
                    // start が cp より大幅に小さければ打ち切る
                    if end < cp {
                        break;
                    }
                }
            }
        }

        // Unicodeプロパティベースのフォールバック
        match cp {
            // 空白
            0x0020 | 0x3000 | 0x0009..=0x000D => CharType::Space,
            // ASCII数字
            0x0030..=0x0039 => CharType::Numeric,
            // ASCII英字
            0x0041..=0x005A | 0x0061..=0x007A => CharType::Alpha,
            // 全角英字
            0xFF21..=0xFF3A | 0xFF41..=0xFF5A => CharType::Alpha,
            // 全角数字
            0xFF10..=0xFF19 => CharType::NumericWide,
            // ひらがな
            0x3040..=0x309F => CharType::Hiragana,
            // カタカナ
            0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF65..=0xFF9F => CharType::Katakana,
            // CJK統合漢字
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x20000..=0x2A6DF => {
                CharType::Kanji
            }
            // 漢数字（match で判定、文字列探索を排除）
            _ if matches!(
                c,
                '〇' | '一'
                    | '二'
                    | '三'
                    | '四'
                    | '五'
                    | '六'
                    | '七'
                    | '八'
                    | '九'
                    | '十'
                    | '百'
                    | '千'
                    | '万'
                    | '億'
                    | '兆'
            ) =>
            {
                CharType::Kanji
            }
            // ASCII記号
            0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E => {
                CharType::Symbol
            }
            // 全角記号・句読点
            0x3000..=0x303F | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 => CharType::Symbol,
            _ => CharType::Default,
        }
    }

    /// 文字クラスの定義を取得
    pub fn get_class(&self, class_name: &str) -> Option<&CharClass> {
        self.classes.get(class_name)
    }

    /// CharType に対応する ClassProps を取得（O(1)）
    #[inline]
    fn props_for(&self, ct: CharType) -> ClassProps {
        self.props_cache[type_index(ct)]
    }

    /// テキストの指定位置から、同じ文字種の連続文字列を取得（コールバック方式）
    #[inline]
    pub fn group_at_cb(&self, text: &str, byte_pos: usize, mut cb: impl FnMut(usize, CharType)) {
        let remaining = &text[byte_pos..];
        let mut chars = remaining.chars();

        let first_char = match chars.next() {
            Some(c) => c,
            None => return,
        };
        let char_type = self.classify_char(first_char);
        let props = self.props_for(char_type);

        if props.group {
            let mut byte_len = first_char.len_utf8();
            let mut char_count = 1u32;

            for c in chars {
                if self.classify_char(c) != char_type {
                    break;
                }
                byte_len += c.len_utf8();
                char_count += 1;
                if props.max_length > 0 && char_count >= props.max_length {
                    break;
                }
            }
            cb(byte_len, char_type);
        } else {
            let max = if props.max_length == 0 {
                1
            } else {
                props.max_length as usize
            };
            let mut byte_offset = 0;
            let mut count = 0;

            for c in remaining.chars() {
                if self.classify_char(c) != char_type {
                    break;
                }
                byte_offset += c.len_utf8();
                count += 1;
                cb(byte_offset, char_type);
                if count >= max {
                    break;
                }
            }
        }
    }

    /// テキストの指定位置から、同じ文字種の連続文字列を取得
    /// 戻り値: (バイト長, 文字クラス名)
    pub fn group_at(&self, text: &str, byte_pos: usize) -> Vec<(usize, CharType)> {
        let remaining = &text[byte_pos..];
        let mut chars = remaining.chars();

        let first_char = match chars.next() {
            Some(c) => c,
            None => return vec![],
        };
        let char_type = self.classify_char(first_char);
        let props = self.props_for(char_type);

        let mut results = Vec::new();

        if props.group {
            // 同一文字種をグルーピング
            let mut byte_len = first_char.len_utf8();
            let mut char_count = 1u32;

            for c in chars {
                if self.classify_char(c) != char_type {
                    break;
                }
                byte_len += c.len_utf8();
                char_count += 1;
                if props.max_length > 0 && char_count >= props.max_length {
                    break;
                }
            }
            results.push((byte_len, char_type));
        } else {
            // 1文字ずつ
            let max = if props.max_length == 0 {
                1
            } else {
                props.max_length as usize
            };
            let mut byte_offset = 0;
            let mut count = 0;

            for c in remaining.chars() {
                if self.classify_char(c) != char_type {
                    break;
                }
                byte_offset += c.len_utf8();
                count += 1;
                results.push((byte_offset, char_type));
                if count >= max {
                    break;
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char('あ'), CharType::Hiragana);
        assert_eq!(cc.classify_char('ア'), CharType::Katakana);
        assert_eq!(cc.classify_char('漢'), CharType::Kanji);
        assert_eq!(cc.classify_char('A'), CharType::Alpha);
        assert_eq!(cc.classify_char('1'), CharType::Numeric);
        assert_eq!(cc.classify_char(' '), CharType::Space);
        assert_eq!(cc.classify_char('。'), CharType::Symbol);
    }

    #[test]
    fn test_kanji_numerals() {
        let cc = CharClassifier::default_japanese();
        for c in "〇一二三四五六七八九十百千万億兆".chars() {
            assert_eq!(cc.classify_char(c), CharType::Kanji, "Failed for '{}'", c);
        }
    }

    #[test]
    fn test_group() {
        let cc = CharClassifier::default_japanese();
        let text = "カタカナhello漢字";
        let groups = cc.group_at(text, 0);
        assert!(!groups.is_empty());
        // KATAKANA length=2 なので最大2文字 = 6バイト
        assert_eq!(groups[0].0, 6);
        assert_eq!(groups[0].1, CharType::Katakana);
    }

    #[test]
    fn test_props_cache_consistency() {
        let cc = CharClassifier::default_japanese();
        // props_cache と HashMap の結果が一致することを確認
        let all_types = [
            CharType::Hiragana,
            CharType::Katakana,
            CharType::Kanji,
            CharType::Alpha,
            CharType::Numeric,
            CharType::NumericWide,
            CharType::Symbol,
            CharType::Space,
            CharType::Default,
        ];
        for ct in all_types {
            let props = cc.props_for(ct);
            let class = cc.classes.get(ct.class_name());
            let expected_group = class.is_none_or(|c| c.group);
            let expected_max = class.map_or(0, |c| c.length);
            assert_eq!(props.group, expected_group, "group mismatch for {:?}", ct);
            assert_eq!(
                props.max_length, expected_max,
                "max_length mismatch for {:?}",
                ct
            );
        }
    }

    #[test]
    fn test_rebuild_props_cache() {
        let mut cc = CharClassifier::default_japanese();
        // Simulate what export_char_classifier does
        cc.classes.get_mut("KATAKANA").unwrap().invoke = false;
        cc.rebuild_props_cache();
        let props = cc.props_for(CharType::Katakana);
        assert!(!props.invoke);
    }
}
