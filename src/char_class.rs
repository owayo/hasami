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
pub(crate) struct ClassProps {
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

impl ClassProps {
    /// 同じ文字種の文字が `run` 文字（1 以上）続く位置で作る未知語の長さ（文字数）を、作る順に渡す
    ///
    /// [`CharClassifier::group_at_cb`] と同じ長さを、並びを走査せずに出す。
    /// - group: 並び全体を 1 つ。length が 0 でなければ `max(length, 2)` 文字で打ち切る
    ///   （2 文字目を足してから上限と比べるので、length が 1 でも 2 文字になる）
    /// - group でない: 1 文字から `min(run, length)` 文字まで（length が 0 なら 1 文字だけ）
    #[inline]
    pub(crate) fn for_each_unk_len(&self, run: u32, mut cb: impl FnMut(u32)) {
        if self.group {
            let len = if self.max_length == 0 {
                run
            } else {
                run.min(self.max_length.max(2))
            };
            cb(len);
        } else {
            let max = if self.max_length == 0 {
                1
            } else {
                self.max_length
            };
            for len in 1..=run.min(max) {
                cb(len);
            }
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

/// 全 CharType の一覧（type_index の順序と一致）
pub const ALL_CHAR_TYPES: [CharType; 9] = [
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
    let mut cache = [ClassProps::default(); NUM_CHAR_TYPES];
    for &ct in &ALL_CHAR_TYPES {
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
                        return char_type_of_class(class_name);
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

        fallback_char_type(c)
    }

    /// U+0000〜U+FFFF の文字種の表（[`type_index`] の値）。[`CharClassifier::classify_char`] と同じ結果を返す
    ///
    /// 解析の最内側で文字ごとに引く（二分探索とカテゴリ名の照合を毎回しない）。範囲は開始位置の昇順に
    /// 並んでいる前提（[`CharClassifier::from_definitions`] が並べる）。このとき `classify_char` が見るのは
    /// 「開始位置が cp 以下の最後の範囲」1 つだけで、それが cp を含まなければ Unicode のブロックによる
    /// 分類になる。そこで、範囲ごとに「次の範囲の開始位置の手前まで」のうち範囲に含まれる部分を塗る
    /// （開始位置が同じ範囲は後のものだけが当たる）。
    pub(crate) fn bmp_type_table(&self) -> Box<[u8]> {
        let mut table = vec![type_index(CharType::Default) as u8; 0x10000].into_boxed_slice();
        // Unicode のブロックによる分類。優先度の低い範囲から塗る
        for &(start, end, t) in FALLBACK_RANGES.iter().rev() {
            if start <= 0xFFFF {
                table[start as usize..=end.min(0xFFFF) as usize].fill(type_index(t) as u8);
            }
        }
        for (k, (start, end, name)) in self.ranges.iter().enumerate() {
            let next = self.ranges.get(k + 1).map_or(u32::MAX, |r| r.0);
            if next <= *start || *start > 0xFFFF {
                continue;
            }
            let last = (*end).min(next - 1).min(0xFFFF);
            if last < *start {
                continue;
            }
            table[*start as usize..=last as usize].fill(type_index(char_type_of_class(name)) as u8);
        }
        table
    }

    /// 文字クラスの定義を取得
    pub fn get_class(&self, class_name: &str) -> Option<&CharClass> {
        self.classes.get(class_name)
    }

    /// CharType に対応する ClassProps を取得（O(1)）
    #[inline]
    pub(crate) fn props_for(&self, ct: CharType) -> ClassProps {
        self.props_cache[type_index(ct)]
    }
}

/// char.def のカテゴリ名を文字種に写す（KANJINUMERIC は漢字、GREEK・CYRILLIC など 9 種に無いものは DEFAULT）
fn char_type_of_class(name: &str) -> CharType {
    match name {
        "HIRAGANA" => CharType::Hiragana,
        "KATAKANA" => CharType::Katakana,
        "KANJI" | "KANJINUMERIC" => CharType::Kanji,
        "ALPHA" => CharType::Alpha,
        "NUMERIC" => CharType::Numeric,
        "SYMBOL" => CharType::Symbol,
        "SPACE" => CharType::Space,
        _ => CharType::Default,
    }
}

/// char.def の範囲に当たらない文字の、Unicode のブロックによる簡易分類（先に書いたものが優先）
///
/// 「〇」以外の漢数字（一・二・…・兆）は CJK 統合漢字の範囲に入っている。
const FALLBACK_RANGES: &[(u32, u32, CharType)] = &[
    // 空白
    (0x0020, 0x0020, CharType::Space),
    (0x3000, 0x3000, CharType::Space),
    (0x0009, 0x000D, CharType::Space),
    // ASCII数字
    (0x0030, 0x0039, CharType::Numeric),
    // ASCII英字・全角英字
    (0x0041, 0x005A, CharType::Alpha),
    (0x0061, 0x007A, CharType::Alpha),
    (0xFF21, 0xFF3A, CharType::Alpha),
    (0xFF41, 0xFF5A, CharType::Alpha),
    // 全角数字
    (0xFF10, 0xFF19, CharType::NumericWide),
    // ひらがな
    (0x3040, 0x309F, CharType::Hiragana),
    // カタカナ
    (0x30A0, 0x30FF, CharType::Katakana),
    (0x31F0, 0x31FF, CharType::Katakana),
    (0xFF65, 0xFF9F, CharType::Katakana),
    // CJK統合漢字
    (0x4E00, 0x9FFF, CharType::Kanji),
    (0x3400, 0x4DBF, CharType::Kanji),
    (0xF900, 0xFAFF, CharType::Kanji),
    (0x20000, 0x2A6DF, CharType::Kanji),
    // 漢数字の「〇」
    (0x3007, 0x3007, CharType::Kanji),
    // ASCII記号
    (0x0021, 0x002F, CharType::Symbol),
    (0x003A, 0x0040, CharType::Symbol),
    (0x005B, 0x0060, CharType::Symbol),
    (0x007B, 0x007E, CharType::Symbol),
    // 全角記号・句読点
    (0x3000, 0x303F, CharType::Symbol),
    (0xFF01, 0xFF0F, CharType::Symbol),
    (0xFF1A, 0xFF20, CharType::Symbol),
];

/// char.def の範囲に当たらない文字の文字種（Unicode のブロックによる簡易分類）
fn fallback_char_type(c: char) -> CharType {
    let cp = c as u32;
    FALLBACK_RANGES
        .iter()
        .find(|&&(start, end, _)| (start..=end).contains(&cp))
        .map_or(CharType::Default, |&(_, _, t)| t)
}

impl CharClassifier {
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
        for ct in ALL_CHAR_TYPES {
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

    // --- 追加テスト: 文字分類の網羅的カバレッジ ---

    #[test]
    fn test_classify_fullwidth_alpha() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char('Ａ'), CharType::Alpha);
        assert_eq!(cc.classify_char('ｚ'), CharType::Alpha);
    }

    #[test]
    fn test_classify_fullwidth_numeric() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char('０'), CharType::NumericWide);
        assert_eq!(cc.classify_char('９'), CharType::NumericWide);
    }

    #[test]
    fn test_classify_halfwidth_katakana() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char('ｱ'), CharType::Katakana);
        assert_eq!(cc.classify_char('ﾝ'), CharType::Katakana);
    }

    #[test]
    fn test_classify_cjk_extension_b() {
        let cc = CharClassifier::default_japanese();
        // CJK Unified Ideographs Extension B (U+20000)
        assert_eq!(cc.classify_char('\u{20000}'), CharType::Kanji);
    }

    #[test]
    fn test_classify_ascii_symbols() {
        let cc = CharClassifier::default_japanese();
        for c in "!@#$%^&*(){}[]|\\:;\"'<>,./~`".chars() {
            let ct = cc.classify_char(c);
            assert!(
                ct == CharType::Symbol || ct == CharType::Default,
                "Expected Symbol or Default for '{}', got {:?}",
                c,
                ct
            );
        }
    }

    #[test]
    fn test_classify_fullwidth_symbols() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char('。'), CharType::Symbol);
        assert_eq!(cc.classify_char('、'), CharType::Symbol);
        assert_eq!(cc.classify_char('！'), CharType::Symbol);
    }

    #[test]
    fn test_classify_space_variants() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char(' '), CharType::Space); // ASCII space
        assert_eq!(cc.classify_char('\u{3000}'), CharType::Space); // ideographic space
        assert_eq!(cc.classify_char('\t'), CharType::Space); // tab
        assert_eq!(cc.classify_char('\n'), CharType::Space); // newline
    }

    #[test]
    fn test_classify_hiragana_range() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char('ぁ'), CharType::Hiragana);
        assert_eq!(cc.classify_char('ん'), CharType::Hiragana);
        assert_eq!(cc.classify_char('ゔ'), CharType::Hiragana);
    }

    #[test]
    fn test_classify_katakana_range() {
        let cc = CharClassifier::default_japanese();
        assert_eq!(cc.classify_char('ァ'), CharType::Katakana);
        assert_eq!(cc.classify_char('ン'), CharType::Katakana);
        assert_eq!(cc.classify_char('ヴ'), CharType::Katakana);
    }

    #[test]
    fn test_type_index_roundtrip() {
        // Each type maps to a unique index 0..8
        let mut indices: Vec<usize> = ALL_CHAR_TYPES.iter().map(|&ct| type_index(ct)).collect();
        indices.sort();
        assert_eq!(indices, (0..9).collect::<Vec<_>>());
    }

    #[test]
    fn test_class_name_mapping() {
        assert_eq!(CharType::Hiragana.class_name(), "HIRAGANA");
        assert_eq!(CharType::Katakana.class_name(), "KATAKANA");
        assert_eq!(CharType::Kanji.class_name(), "KANJI");
        assert_eq!(CharType::Alpha.class_name(), "ALPHA");
        assert_eq!(CharType::Numeric.class_name(), "NUMERIC");
        assert_eq!(CharType::NumericWide.class_name(), "NUMERIC");
        assert_eq!(CharType::Symbol.class_name(), "SYMBOL");
        assert_eq!(CharType::Space.class_name(), "SPACE");
        assert_eq!(CharType::Default.class_name(), "DEFAULT");
    }

    #[test]
    fn test_group_at_empty_string() {
        let cc = CharClassifier::default_japanese();
        let groups = cc.group_at("", 0);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_group_at_single_char() {
        let cc = CharClassifier::default_japanese();
        let groups = cc.group_at("A", 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, 1);
        assert_eq!(groups[0].1, CharType::Alpha);
    }

    #[test]
    fn test_group_at_mixed_script_boundary() {
        let cc = CharClassifier::default_japanese();
        // Alpha group followed by hiragana
        let groups = cc.group_at("ABCあいう", 0);
        assert_eq!(groups[0].1, CharType::Alpha);
        assert_eq!(groups[0].0, 3); // "ABC" = 3 bytes
    }

    #[test]
    fn test_group_at_kanji_max_length() {
        let cc = CharClassifier::default_japanese();
        // KANJI has group=false, length=2 → should get individual chars up to 2
        let groups = cc.group_at("漢字列", 0);
        assert!(groups.len() <= 2);
        assert_eq!(groups[0].1, CharType::Kanji);
    }

    #[test]
    fn test_group_at_cb_consistency() {
        let cc = CharClassifier::default_japanese();
        let text = "カタカナabc漢字";
        let groups = cc.group_at(text, 0);

        let mut cb_results = Vec::new();
        cc.group_at_cb(text, 0, |len, ct| {
            cb_results.push((len, ct));
        });
        assert_eq!(groups, cb_results);
    }

    #[test]
    fn test_from_definitions() {
        let mut classes = HashMap::new();
        classes.insert(
            "HIRAGANA".to_string(),
            CharClass {
                name: "HIRAGANA".to_string(),
                invoke: true,
                group: true,
                length: 5,
            },
        );
        let ranges = vec![(0x3040, 0x309F, "HIRAGANA".to_string())];
        let cc = CharClassifier::from_definitions(classes, ranges);

        assert_eq!(cc.classify_char('あ'), CharType::Hiragana);
        let props = cc.props_for(CharType::Hiragana);
        assert!(props.invoke);
        assert_eq!(props.max_length, 5);
    }

    #[test]
    fn test_get_class_existing() {
        let cc = CharClassifier::default_japanese();
        let cls = cc.get_class("KATAKANA");
        assert!(cls.is_some());
        assert!(cls.unwrap().group);
    }

    #[test]
    fn test_get_class_nonexistent() {
        let cc = CharClassifier::default_japanese();
        assert!(cc.get_class("NONEXISTENT").is_none());
    }

    /// IPAdic の char.def に近い、重なりと開始位置の重複を含む範囲
    fn overlapping_classifier() -> CharClassifier {
        let mut classes = HashMap::new();
        for (name, invoke, group, length) in [
            ("DEFAULT", false, true, 0),
            ("SPACE", false, true, 0),
            ("KANJI", false, false, 2),
            ("SYMBOL", true, true, 0),
            ("NUMERIC", true, true, 0),
            ("ALPHA", true, true, 0),
            ("HIRAGANA", false, true, 2),
            ("KATAKANA", true, true, 2),
            ("KANJINUMERIC", true, true, 0),
            ("GREEK", true, true, 0),
            ("CYRILLIC", true, true, 0),
        ] {
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
        let ranges = [
            (0x0020, 0x0020, "SPACE"),
            (0x00D0, 0x00D0, "SPACE"),
            (0x0009, 0x000D, "SPACE"),
            (0x0030, 0x0039, "NUMERIC"),
            (0x0041, 0x005A, "ALPHA"),
            (0x0061, 0x007A, "ALPHA"),
            (0x0021, 0x002F, "SYMBOL"),
            (0x0391, 0x03C9, "GREEK"),
            (0x0400, 0x04F9, "CYRILLIC"),
            (0x3000, 0x303F, "SYMBOL"),
            (0x3005, 0x3005, "KANJI"),
            (0x3007, 0x3007, "KANJINUMERIC"),
            (0x3041, 0x309F, "HIRAGANA"),
            (0x30A1, 0x30FF, "KATAKANA"),
            (0x30FC, 0x30FC, "HIRAGANA"),
            (0x4E00, 0x9FA5, "KANJI"),
            (0x4E00, 0x4E00, "KANJINUMERIC"),
            (0x4E8C, 0x4E8C, "KANJINUMERIC"),
            (0xFF10, 0xFF19, "NUMERIC"),
            (0xFF21, 0xFF3A, "ALPHA"),
            (0xFF66, 0xFF9D, "KATAKANA"),
            (0xFF00, 0xFFEF, "SYMBOL"),
            (0x20000, 0x2A6DF, "KANJI"),
            (0x1F300, 0x1F5FF, "SYMBOL"),
        ];
        CharClassifier::from_definitions(
            classes,
            ranges
                .iter()
                .map(|&(s, e, n)| (s, e, n.to_string()))
                .collect(),
        )
    }

    /// 範囲の表にする前の判定（`FALLBACK_RANGES` と同じ結果になることを確かめる）
    fn reference_fallback(c: char) -> CharType {
        match c as u32 {
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

    #[test]
    fn test_fallback_ranges_match_the_reference() {
        for c in (0..=0x10FFFFu32).filter_map(char::from_u32) {
            assert_eq!(
                fallback_char_type(c),
                reference_fallback(c),
                "U+{:04X}",
                c as u32
            );
        }
    }

    fn assert_table_matches(cc: &CharClassifier) {
        let table = cc.bmp_type_table();
        assert_eq!(table.len(), 0x10000);
        for c in (0..=0xFFFFu32).filter_map(char::from_u32) {
            assert_eq!(
                table[c as usize] as usize,
                type_index(cc.classify_char(c)),
                "U+{:04X}",
                c as u32
            );
        }
    }

    #[test]
    fn test_bmp_type_table_matches_classify_char() {
        assert_table_matches(&CharClassifier::default_japanese());
        assert_table_matches(&overlapping_classifier());

        // 開始位置と長さをばらばらにした範囲（重なり・開始位置の重複・BMP をまたぐもの）
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let names = ["HIRAGANA", "KANJI", "ALPHA", "SYMBOL", "GREEK", "SPACE"];
        for _ in 0..20 {
            let mut ranges = Vec::new();
            for _ in 0..(next() % 60) {
                let start = (next() % 0x11000) as u32;
                let len = (next() % 0x800) as u32;
                let name = names[(next() % names.len() as u64) as usize];
                ranges.push((start, start + len, name.to_string()));
                if next() % 4 == 0 {
                    // 同じ開始位置の範囲
                    ranges.push((start, start + len / 2, names[0].to_string()));
                }
            }
            let cc = CharClassifier::from_definitions(HashMap::new(), ranges);
            assert_table_matches(&cc);
        }
    }

    #[test]
    fn test_unk_lengths_match_group_at_cb() {
        // 文字種の並びの長さと char.def の group・length の組み合わせごとに、group_at_cb と同じ長さを出す
        let text = "アイウエオカキクケコ漢字漢字漢字ABCDEFGHIJ123456789あいうえおかき。、！";
        for group in [false, true] {
            for length in [0, 1, 2, 3, 5] {
                let mut cc = overlapping_classifier();
                for class in cc.classes.values_mut() {
                    class.group = group;
                    class.length = length;
                }
                cc.rebuild_props_cache();
                let chars: Vec<(usize, char)> = text.char_indices().collect();
                for (k, &(pos, c)) in chars.iter().enumerate() {
                    let ct = cc.classify_char(c);
                    let run = chars[k..]
                        .iter()
                        .take_while(|&&(_, c2)| cc.classify_char(c2) == ct)
                        .count() as u32;
                    let mut expected = Vec::new();
                    cc.group_at_cb(text, pos, |len, _| expected.push(len));
                    let mut actual = Vec::new();
                    cc.props_for(ct).for_each_unk_len(run, |len| {
                        // 文字数をバイト数にする
                        let end = chars.get(k + len as usize).map_or(text.len(), |&(p, _)| p);
                        actual.push(end - pos);
                    });
                    assert_eq!(actual, expected, "group={group} length={length} at {pos}");
                }
            }
        }
    }

    #[test]
    fn test_default_japanese_has_all_classes() {
        let cc = CharClassifier::default_japanese();
        let expected = [
            "DEFAULT", "SPACE", "KANJI", "HIRAGANA", "KATAKANA", "ALPHA", "NUMERIC", "SYMBOL",
        ];
        for name in expected {
            assert!(cc.get_class(name).is_some(), "Missing class: {}", name);
        }
    }
}
