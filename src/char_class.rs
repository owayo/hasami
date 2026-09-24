//! 文字分類 - char.def による文字種の判定と、未知語の候補の作り方

use std::collections::HashMap;

/// 文字クラス定義（char.def のカテゴリ）
#[derive(Clone, Debug)]
pub struct CharClass {
    /// クラス名
    pub name: String,
    /// invoke: 既知語があっても未知語処理を起動するか
    pub invoke: bool,
    /// group: 同じ文字種の並び全体を 1 つの未知語の候補にするか
    pub group: bool,
    /// length: 1〜length 文字の接頭辞を未知語の候補にする（0 なら作らない）
    pub length: u32,
}

/// 同じ文字種の並びから未知語の候補を作る規則（char.def の group・length。MeCab と同じ意味）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnkGrouping {
    group: bool,
    length: u32,
}

impl UnkGrouping {
    /// char.def に定義の無い文字種（並び全体だけを候補にする）
    const UNDEFINED: UnkGrouping = UnkGrouping {
        group: true,
        length: 0,
    };

    /// 同じ文字種の文字が `run` 文字（1 以上）続く位置で作る未知語の候補の長さ（文字数）を、作る順に渡す
    ///
    /// MeCab の tokenizer と同じ: group なら並び全体（並びが [`MAX_GROUPING_SIZE`] + 1 字まで）を 1 つ、
    /// 続けて 1〜length 字の接頭辞（並び全体と同じ長さは除く）。候補が 1 つも無く、その位置から始まる
    /// 既知語も無いときに 1 文字の未知語を足すのは呼び出し側。
    #[inline]
    pub(crate) fn for_each_len(&self, run: u32, mut cb: impl FnMut(u32)) {
        if self.group && run - 1 <= MAX_GROUPING_SIZE {
            cb(run);
        }
        for len in 1..=run.min(self.length) {
            if !(self.group && len == run) {
                cb(len);
            }
        }
    }
}

/// MeCab の `max-grouping-size` の既定値。group の文字種の並びは、先頭の後ろがこの文字数以下のときだけ
/// 1 つの候補にする
const MAX_GROUPING_SIZE: u32 = 24;

/// 文字分類器
#[derive(Clone, Debug)]
pub struct CharClassifier {
    /// 文字クラス定義
    pub classes: HashMap<String, CharClass>,
    /// Unicode範囲マッピング (start, end, class_name)。開始位置の昇順（同じ開始位置は char.def の順）
    pub ranges: Vec<(u32, u32, String)>,
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

impl CharClassifier {
    /// デフォルトの日本語文字分類器（char.def を読まないときに使う）
    ///
    /// カテゴリは配布辞書の IPAdic（`scripts/prepare_ipadic.py` で整えた char.def）と同じ。
    /// 文字種は Unicode のブロックで決める（範囲の定義は持たない）。
    pub fn default_japanese() -> Self {
        let defs = [
            ("DEFAULT", false, true, 0),
            ("SPACE", false, true, 0),
            ("KANJI", false, false, 2),
            ("SYMBOL", false, false, 0),
            ("NUMERIC", true, true, 1),
            ("ALPHA", true, true, 1),
            ("HIRAGANA", false, false, 2),
            ("KATAKANA", true, true, 2),
            ("KANJINUMERIC", true, true, 0),
        ];
        let classes = defs
            .into_iter()
            .map(|(name, invoke, group, length)| {
                let class = CharClass {
                    name: name.to_string(),
                    invoke,
                    group,
                    length,
                };
                (name.to_string(), class)
            })
            .collect();
        CharClassifier {
            classes,
            ranges: Vec::new(),
        }
    }

    /// char.def の定義から構築（範囲は char.def の順に渡す）
    pub fn from_definitions(
        classes: HashMap<String, CharClass>,
        mut ranges: Vec<(u32, u32, String)>,
    ) -> Self {
        // 開始位置の順に並べる。同じ開始位置の範囲は char.def の順のまま（後の行が勝つ）
        ranges.sort_by_key(|&(start, _, _)| start);
        CharClassifier { classes, ranges }
    }

    /// 文字のCharTypeを判定
    ///
    /// 文字を含む char.def の範囲のうち、開始位置が最も大きいもの（同じなら char.def の後の行）の文字種。
    /// どの範囲にも含まれなければ Unicode のブロックによる簡易分類。範囲が重なるとき MeCab は後の行が勝つが、
    /// hasami は開始位置で決める（IPAdic の「々」は記号の範囲 0x3000..0x303F の中で漢字と定義されている）。
    pub fn classify_char(&self, c: char) -> CharType {
        let cp = c as u32;
        let before = self.ranges.partition_point(|&(start, _, _)| start <= cp);
        match self.ranges[..before]
            .iter()
            .rev()
            .find(|&&(_, end, _)| cp <= end)
        {
            Some((_, _, name)) => char_type_of_class(name),
            None => fallback_char_type(c),
        }
    }

    /// U+0000〜U+FFFF の文字種の表（[`type_index`] の値）。[`CharClassifier::classify_char`] と同じ結果を返す
    ///
    /// 解析の最内側で文字ごとに引く（範囲を探さない）。Unicode のブロックによる分類の上に、char.def の範囲を
    /// 開始位置の順に塗るので、重なる文字には開始位置が最も大きい範囲（同じなら後の行）が残る。
    pub(crate) fn bmp_type_table(&self) -> Box<[u8]> {
        let mut table = vec![type_index(CharType::Default) as u8; 0x10000].into_boxed_slice();
        let mut paint = |start: u32, end: u32, t: CharType| {
            if start <= 0xFFFF {
                table[start as usize..=end.min(0xFFFF) as usize].fill(type_index(t) as u8);
            }
        };
        // Unicode のブロックによる分類。優先度の低い範囲から塗る
        for &(start, end, t) in FALLBACK_RANGES.iter().rev() {
            paint(start, end, t);
        }
        for (start, end, name) in &self.ranges {
            paint(*start, *end, char_type_of_class(name));
        }
        table
    }

    /// 文字クラスの定義を取得
    pub fn get_class(&self, class_name: &str) -> Option<&CharClass> {
        self.classes.get(class_name)
    }

    /// 文字種の未知語の候補の作り方（char.def に定義が無ければ並び全体だけ）
    pub(crate) fn unk_grouping(&self, ct: CharType) -> UnkGrouping {
        self.classes
            .get(ct.class_name())
            .map_or(UnkGrouping::UNDEFINED, |c| UnkGrouping {
                group: c.group,
                length: c.length,
            })
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
        assert!(cc.get_class("HIRAGANA").unwrap().invoke);
        assert_eq!(
            cc.unk_grouping(CharType::Hiragana),
            UnkGrouping {
                group: true,
                length: 5
            }
        );
        // 定義の無い文字種は並び全体だけ
        assert_eq!(cc.unk_grouping(CharType::Kanji), UnkGrouping::UNDEFINED);
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

    fn unk_lens(group: bool, length: u32, run: u32) -> Vec<u32> {
        let mut lens = Vec::new();
        UnkGrouping { group, length }.for_each_len(run, |len| lens.push(len));
        lens
    }

    #[test]
    fn test_unk_candidates_follow_mecab() {
        // group: 並び全体、続けて 1〜length 字の接頭辞（並び全体と同じ長さは除く）
        assert_eq!(unk_lens(true, 2, 1), [1]);
        assert_eq!(unk_lens(true, 2, 2), [2, 1]);
        assert_eq!(unk_lens(true, 2, 7), [7, 1, 2]);
        assert_eq!(unk_lens(true, 1, 3), [3, 1]);
        assert_eq!(unk_lens(true, 0, 3), [3]);
        // 並び全体は 25 字まで（先頭の後ろが max-grouping-size = 24 字以下）
        assert_eq!(unk_lens(true, 0, 25), [25]);
        assert!(unk_lens(true, 0, 26).is_empty());
        assert_eq!(unk_lens(true, 2, 26), [1, 2]);
        // group でない: 1〜length 字
        assert_eq!(unk_lens(false, 2, 1), [1]);
        assert_eq!(unk_lens(false, 2, 5), [1, 2]);
        assert_eq!(unk_lens(false, 3, 2), [1, 2]);
        assert!(unk_lens(false, 0, 4).is_empty());
    }

    #[test]
    fn test_overlapping_ranges_prefer_the_largest_start() {
        let cc = overlapping_classifier();
        // 英字の範囲 0x00C0..0x00FF の中に 0x00D0 だけ空白。その後ろの文字も英字（包む範囲に戻る）
        let latin = CharClassifier::from_definitions(
            HashMap::new(),
            vec![
                (0x00D0, 0x00D0, "SPACE".to_string()),
                (0x00C0, 0x00FF, "ALPHA".to_string()),
                (0x00D7, 0x00D7, "SYMBOL".to_string()),
            ],
        );
        assert_eq!(latin.classify_char('\u{00C9}'), CharType::Alpha);
        assert_eq!(latin.classify_char('\u{00D0}'), CharType::Space);
        assert_eq!(latin.classify_char('é'), CharType::Alpha);
        assert_eq!(latin.classify_char('×'), CharType::Symbol);
        // 記号の範囲の中で漢字と定義した「々」は漢字、その後ろは記号
        assert_eq!(cc.classify_char('々'), CharType::Kanji);
        assert_eq!(cc.classify_char('〆'), CharType::Symbol);
        assert_eq!(cc.classify_char('「'), CharType::Symbol);
        // 開始位置が同じ範囲は後の行が勝つ
        let same_start = CharClassifier::from_definitions(
            HashMap::new(),
            vec![
                (0x3007, 0x3007, "KANJI".to_string()),
                (0x3000, 0x303F, "SYMBOL".to_string()),
                (0x3007, 0x3007, "HIRAGANA".to_string()),
            ],
        );
        assert_eq!(same_start.classify_char('〇'), CharType::Hiragana);
        assert_table_matches(&latin);
        assert_table_matches(&same_start);
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
