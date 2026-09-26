//! セクションに並べる固定長レコード（bytemuck で mmap からそのまま参照する）

use bytemuck::{Pod, Zeroable};

/// GRAMMAR の 1 要素（6 バイト）。各番号は対応する文字列表を指す。
/// 重複を除いた素性での頻度順、同頻度なら 3 番号の辞書順で並ぶ。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Pod, Zeroable)]
pub struct GrammarRecord {
    pub pos_id: u16,
    pub conj_type_id: u16,
    pub conj_form_id: u16,
}

/// ENTRIES の 1 要素（6 バイト）
///
/// エントリは表層形のバイト順に並ぶ（同じ表層形の中は元の順）。同じ表層形のエントリの並びを
/// 「群」と呼び、trie の値は群の先頭の番号を指す。`left_and_last` の最上位ビットが群の最後の印。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct EntryRecord {
    pub left_and_last: u16,
    pub right_id: u16,
    pub cost: i16,
}

/// `left_and_last` の最上位ビット: 群の最後のエントリ
pub const LAST_IN_GROUP: u16 = 0x8000;
/// 文脈 ID（左）の上限（この値未満）。最上位ビットを群の印に使うので 15 ビット
pub const LEFT_ID_LIMIT: usize = 0x8000;

impl EntryRecord {
    #[inline]
    pub fn left_id(&self) -> u16 {
        self.left_and_last & !LAST_IN_GROUP
    }

    #[inline]
    pub fn is_last(&self) -> bool {
        self.left_and_last & LAST_IN_GROUP != 0
    }
}

/// CHAR_CATEGORIES の 1 要素（char.def のカテゴリ定義、16 バイト）
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CharCategoryRecord {
    /// CATEGORY_NAMES の番号
    pub name_id: u32,
    /// 1 なら既知語があっても未知語処理を起動する
    pub invoke: u8,
    /// 1 なら同じカテゴリの文字をまとめる
    pub group: u8,
    pub _pad0: u16,
    /// まとめるときの最大文字数（0 = 無制限）
    pub length: u32,
    pub _pad1: u32,
}

/// CHAR_RANGES の 1 要素（char.def の文字範囲、16 バイト）
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CharRangeRecord {
    /// 範囲の先頭のコードポイント
    pub start: u32,
    /// 範囲の最後のコードポイント（含む）
    pub end: u32,
    /// CATEGORY_NAMES の番号
    pub category_name_id: u32,
    pub _pad: u32,
}

/// UNK_BUCKETS の 1 要素（文字種ごとの未知語テンプレートの範囲、8 バイト）。
/// 文字種の並びは `char_class::ALL_CHAR_TYPES` と同じ
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UnkBucket {
    pub template_start: u32,
    pub template_count: u16,
    pub invoke: u8,
    pub _pad: u8,
}

/// UNK_TEMPLATES の 1 要素（unk.def の 1 行、12 バイト）
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UnkTemplate {
    /// POS_STRINGS の番号
    pub pos_id: u32,
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
    pub _pad: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_sizes_match_the_format() {
        assert_eq!(size_of::<GrammarRecord>(), 6);
        assert_eq!(size_of::<EntryRecord>(), 6);
        assert_eq!(size_of::<CharCategoryRecord>(), 16);
        assert_eq!(size_of::<CharRangeRecord>(), 16);
        assert_eq!(size_of::<UnkBucket>(), 8);
        assert_eq!(size_of::<UnkTemplate>(), 12);
    }

    #[test]
    fn entry_record_splits_left_id_and_last_flag() {
        let e = EntryRecord {
            left_and_last: 1234 | LAST_IN_GROUP,
            right_id: 5,
            cost: -3,
        };
        assert_eq!(e.left_id(), 1234);
        assert!(e.is_last());
    }
}
