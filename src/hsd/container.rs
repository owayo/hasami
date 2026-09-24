//! v4 のコンテナ: 64 バイトのヘッダとセクション表
//!
//! | オフセット | 型 | 内容 |
//! | --- | --- | --- |
//! | 0 | [u8; 8] | magic `HSMDICT\0` |
//! | 8 | u32 | version = 4 |
//! | 12 | u32 | flags。bit0 = 支配エントリを除いた最終辞書。それ以外のビットはエラー |
//! | 16 | u32 | セクション数 (1 以上 64 以下) |
//! | 20 | u32 | 予約 (0) |
//! | 24 | u64 | ファイル長 (実際の長さと一致しなければエラー) |
//! | 32 | [u8; 32] | 予約 (すべて 0) |
//!
//! セクション表はオフセット 64 から `セクション数 × 24` バイト: `id: u32`, `予約: u32 (0)`,
//! `offset: u64`, `len: u64`。各セクションは 64 の倍数のオフセットに置き、ヘッダ・表・
//! 他のセクションと重ならない。同じ id が 2 回あればエラー、未知の id は範囲と整列を
//! 検査してから無視する。

use super::DictError;
#[cfg(feature = "build")]
use std::io::{self, Write};

pub const MAGIC: [u8; 8] = *b"HSMDICT\0";
pub const VERSION: u32 = 4;
pub const HEADER_LEN: usize = 64;
pub const SECTION_ENTRY_LEN: usize = 24;
pub const MAX_SECTIONS: usize = 64;
/// セクションの配置境界（キャッシュライン）
pub const SECTION_ALIGN: usize = 64;
/// 既知のセクション id の最大値 + 1（id で引く配列の長さ）
const SECTION_SLOTS: usize = SectionId::CategoryNames as usize + 1;

/// flags の bit0: 支配エントリを除いた最終辞書（repair・merge の入力にできない）
pub const FLAG_PRUNED_DOMINATED: u32 = 1;
const KNOWN_FLAGS: u32 = FLAG_PRUNED_DOMINATED;

/// セクションの種類
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum SectionId {
    /// メタデータ（`key=value` 行）
    Meta = 1,
    /// 文字符号の 2 段表の上段: u16 × 0x1100
    CharBlocks = 2,
    /// 文字符号の 2 段表の下段: u16 × 256 × 表の数
    CharTables = 3,
    /// trie のノード: `{base: u32, check: u32}`
    TrieNodes = 4,
    /// trie の末尾レコード
    TrieTails = 5,
    /// エントリ: `{left_and_last: u16, right: u16, cost: i16}`
    Entries = 6,
    /// エントリごとの素性レコードのバイトオフセット: u32
    FeatureOffsets = 7,
    /// 素性レコードの列
    Features = 8,
    /// 品詞の文字列表
    PosStrings = 9,
    /// 活用型の文字列表
    ConjTypeStrings = 10,
    /// 活用形の文字列表
    ConjFormStrings = 11,
    /// 接続行列（転置）: `num_left: u32`, `num_right: u32`, i16 × num_left × num_right
    Matrix = 12,
    /// char.def のカテゴリ
    CharCategories = 13,
    /// char.def の文字範囲
    CharRanges = 14,
    /// 文字種ごとの未知語テンプレートの範囲
    UnkBuckets = 15,
    /// 未知語テンプレート
    UnkTemplates = 16,
    /// char.def のカテゴリ名の文字列表
    CategoryNames = 17,
}

impl SectionId {
    pub const ALL: [SectionId; 17] = [
        SectionId::Meta,
        SectionId::CharBlocks,
        SectionId::CharTables,
        SectionId::TrieNodes,
        SectionId::TrieTails,
        SectionId::Entries,
        SectionId::FeatureOffsets,
        SectionId::Features,
        SectionId::PosStrings,
        SectionId::ConjTypeStrings,
        SectionId::ConjFormStrings,
        SectionId::Matrix,
        SectionId::CharCategories,
        SectionId::CharRanges,
        SectionId::UnkBuckets,
        SectionId::UnkTemplates,
        SectionId::CategoryNames,
    ];

    pub fn from_u32(id: u32) -> Option<SectionId> {
        SectionId::ALL.iter().copied().find(|s| *s as u32 == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            SectionId::Meta => "META",
            SectionId::CharBlocks => "CHAR_BLOCKS",
            SectionId::CharTables => "CHAR_TABLES",
            SectionId::TrieNodes => "TRIE_NODES",
            SectionId::TrieTails => "TRIE_TAILS",
            SectionId::Entries => "ENTRIES",
            SectionId::FeatureOffsets => "FEATURE_OFFSETS",
            SectionId::Features => "FEATURES",
            SectionId::PosStrings => "POS_STRINGS",
            SectionId::ConjTypeStrings => "CONJ_TYPE_STRINGS",
            SectionId::ConjFormStrings => "CONJ_FORM_STRINGS",
            SectionId::Matrix => "MATRIX",
            SectionId::CharCategories => "CHAR_CATEGORIES",
            SectionId::CharRanges => "CHAR_RANGES",
            SectionId::UnkBuckets => "UNK_BUCKETS",
            SectionId::UnkTemplates => "UNK_TEMPLATES",
            SectionId::CategoryNames => "CATEGORY_NAMES",
        }
    }

    /// 無くてもよいセクション（無ければ空として扱う）
    fn is_optional(self) -> bool {
        matches!(self, SectionId::TrieTails)
    }
}

/// ファイル内のセクションの位置
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionRange {
    pub offset: usize,
    pub len: usize,
}

impl SectionRange {
    pub fn end(&self) -> usize {
        self.offset + self.len
    }
}

/// 検証済みのヘッダ
#[derive(Debug)]
pub struct Layout {
    pub flags: u32,
    /// `SectionId as usize` で引く（解析のたびに引くのでハッシュ表にしない）。無いセクションは None
    sections: [Option<SectionRange>; SECTION_SLOTS],
    /// 表にあった未知の id の数（前方互換のため無視したもの）
    pub unknown_sections: usize,
}

impl Layout {
    /// セクションの位置。無くてもよいセクションが無ければ長さ 0 の範囲を返す
    #[inline]
    pub fn get(&self, id: SectionId) -> SectionRange {
        self.sections[id as usize].unwrap_or(SectionRange { offset: 0, len: 0 })
    }

    pub fn sections(&self) -> impl Iterator<Item = (SectionId, SectionRange)> + '_ {
        SectionId::ALL
            .iter()
            .filter_map(|&id| self.sections[id as usize].map(|r| (id, r)))
    }
}

fn read_u32(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

fn read_u64(data: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(data[at..at + 8].try_into().unwrap())
}

/// ヘッダとセクション表を読み、規範どおりか検査する（O(セクション数)）
pub fn parse(data: &[u8]) -> Result<Layout, DictError> {
    if cfg!(target_endian = "big") {
        return Err(DictError::UnsupportedPlatform);
    }
    if data.len() < 12 || data[..8] != MAGIC {
        // magic だけ合っていて短いファイルは壊れたファイルとして扱う
        if data.len() >= 8 && data[..8] == MAGIC {
            return Err(DictError::corrupt("file is shorter than the header"));
        }
        return Err(DictError::NotHsd);
    }
    let version = read_u32(data, 8);
    if version != VERSION {
        return Err(DictError::UnsupportedVersion(version));
    }
    if data.len() < HEADER_LEN {
        return Err(DictError::corrupt(
            "file is shorter than the 64-byte header",
        ));
    }
    let flags = read_u32(data, 12);
    if flags & !KNOWN_FLAGS != 0 {
        return Err(DictError::corrupt(format!(
            "unknown header flags: {flags:#x}"
        )));
    }
    let count = read_u32(data, 16) as usize;
    if count == 0 || count > MAX_SECTIONS {
        return Err(DictError::corrupt(format!(
            "section count {count} is out of range (1..={MAX_SECTIONS})"
        )));
    }
    if read_u32(data, 20) != 0 || data[32..HEADER_LEN].iter().any(|&b| b != 0) {
        return Err(DictError::corrupt("reserved header bytes are not zero"));
    }
    let file_len = read_u64(data, 24);
    if file_len != data.len() as u64 {
        return Err(DictError::corrupt(format!(
            "header says the file is {file_len} bytes but it is {} bytes (truncated or appended?)",
            data.len()
        )));
    }
    let table_end = HEADER_LEN + count * SECTION_ENTRY_LEN;
    if table_end > data.len() {
        return Err(DictError::corrupt(
            "section table runs past the end of the file",
        ));
    }

    let mut sections = [None; SECTION_SLOTS];
    let mut ranges: Vec<(usize, usize, u32)> = Vec::with_capacity(count);
    let mut unknown_sections = 0;
    for i in 0..count {
        let at = HEADER_LEN + i * SECTION_ENTRY_LEN;
        let id = read_u32(data, at);
        if read_u32(data, at + 4) != 0 {
            return Err(DictError::corrupt(format!(
                "reserved field of section entry {i} is not zero"
            )));
        }
        let offset = read_u64(data, at + 8);
        let len = read_u64(data, at + 16);
        let end = offset
            .checked_add(len)
            .filter(|&end| end <= data.len() as u64)
            .ok_or_else(|| {
                DictError::corrupt(format!(
                    "section {id} (offset {offset}, {len} bytes) runs past the end of the file"
                ))
            })?;
        let (offset, end) = (offset as usize, end as usize);
        if offset % SECTION_ALIGN != 0 {
            return Err(DictError::corrupt(format!(
                "section {id} is not aligned to {SECTION_ALIGN} bytes (offset {offset})"
            )));
        }
        if offset < table_end {
            return Err(DictError::corrupt(format!(
                "section {id} overlaps the header or the section table"
            )));
        }
        ranges.push((offset, end, id));
        match SectionId::from_u32(id) {
            Some(sid) => {
                let range = SectionRange {
                    offset,
                    len: end - offset,
                };
                if sections[sid as usize].replace(range).is_some() {
                    return Err(DictError::corrupt(format!(
                        "section {} appears twice",
                        sid.name()
                    )));
                }
            }
            None => unknown_sections += 1,
        }
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(DictError::corrupt(format!(
                "sections {} and {} overlap",
                pair[0].2, pair[1].2
            )));
        }
    }
    for id in SectionId::ALL {
        if !id.is_optional() && sections[id as usize].is_none() {
            return Err(DictError::corrupt(format!(
                "required section {} is missing",
                id.name()
            )));
        }
    }
    Ok(Layout {
        flags,
        sections,
        unknown_sections,
    })
}

/// セクションを並べてファイルの内容を書き出す
///
/// `sections` は (id, 中身) の列。書き出す順はこの順で、各セクションを 64 バイト境界に置く。
/// Returns: 書き出したバイト数
#[cfg(feature = "build")]
pub fn write<W: Write>(
    out: &mut W,
    flags: u32,
    sections: &[(SectionId, &[u8])],
) -> io::Result<u64> {
    assert!(!sections.is_empty() && sections.len() <= MAX_SECTIONS);
    assert_eq!(flags & !KNOWN_FLAGS, 0);
    let table_end = HEADER_LEN + sections.len() * SECTION_ENTRY_LEN;
    let mut offset = table_end.next_multiple_of(SECTION_ALIGN);
    let mut table = Vec::with_capacity(sections.len() * SECTION_ENTRY_LEN);
    let mut placed = Vec::with_capacity(sections.len());
    for (id, body) in sections {
        table.extend_from_slice(&(*id as u32).to_le_bytes());
        table.extend_from_slice(&0u32.to_le_bytes());
        table.extend_from_slice(&(offset as u64).to_le_bytes());
        table.extend_from_slice(&(body.len() as u64).to_le_bytes());
        placed.push(offset);
        offset = (offset + body.len()).next_multiple_of(SECTION_ALIGN);
    }
    // 最後のセクションの後ろは詰め物をしない
    let file_len = sections
        .last()
        .map_or(table_end, |(_, body)| placed[placed.len() - 1] + body.len())
        as u64;

    let mut header = [0u8; HEADER_LEN];
    header[..8].copy_from_slice(&MAGIC);
    header[8..12].copy_from_slice(&VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&flags.to_le_bytes());
    header[16..20].copy_from_slice(&(sections.len() as u32).to_le_bytes());
    header[24..32].copy_from_slice(&file_len.to_le_bytes());
    out.write_all(&header)?;
    out.write_all(&table)?;

    let mut written = table_end;
    let zeros = [0u8; SECTION_ALIGN];
    for ((_, body), &at) in sections.iter().zip(&placed) {
        out.write_all(&zeros[..at - written])?;
        out.write_all(body)?;
        written = at + body.len();
    }
    debug_assert_eq!(written as u64, file_len);
    Ok(file_len)
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;

    fn all_sections() -> Vec<(SectionId, Vec<u8>)> {
        SectionId::ALL
            .iter()
            .map(|&id| (id, vec![id as u8; id as usize * 3]))
            .collect()
    }

    fn write_to_vec(flags: u32, sections: &[(SectionId, Vec<u8>)]) -> Vec<u8> {
        let refs: Vec<(SectionId, &[u8])> =
            sections.iter().map(|(id, b)| (*id, b.as_slice())).collect();
        let mut buf = Vec::new();
        let len = write(&mut buf, flags, &refs).unwrap();
        assert_eq!(len as usize, buf.len());
        buf
    }

    #[test]
    fn roundtrip_places_sections_on_64_byte_boundaries() {
        let sections = all_sections();
        let buf = write_to_vec(FLAG_PRUNED_DOMINATED, &sections);
        let layout = parse(&buf).unwrap();
        assert_eq!(layout.flags, FLAG_PRUNED_DOMINATED);
        for (id, body) in &sections {
            let r = layout.get(*id);
            assert_eq!(r.offset % SECTION_ALIGN, 0);
            assert_eq!(&buf[r.offset..r.end()], body.as_slice());
        }
    }

    #[test]
    fn missing_optional_section_reads_as_empty() {
        let sections: Vec<_> = all_sections()
            .into_iter()
            .filter(|(id, _)| *id != SectionId::TrieTails)
            .collect();
        let buf = write_to_vec(0, &sections);
        let layout = parse(&buf).unwrap();
        assert_eq!(layout.get(SectionId::TrieTails).len, 0);
    }

    #[test]
    fn rejects_missing_required_section() {
        let sections: Vec<_> = all_sections()
            .into_iter()
            .filter(|(id, _)| *id != SectionId::Matrix)
            .collect();
        let buf = write_to_vec(0, &sections);
        let err = parse(&buf).unwrap_err().to_string();
        assert!(err.contains("MATRIX"), "{err}");
    }

    #[test]
    fn rejects_old_versions_with_rebuild_hint() {
        let mut buf = write_to_vec(0, &all_sections());
        buf[8..12].copy_from_slice(&3u32.to_le_bytes());
        let err = parse(&buf).unwrap_err();
        assert!(matches!(err, DictError::UnsupportedVersion(3)));
        assert!(err.to_string().contains("build-dict.sh"));
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(parse(b"not a dictionary"), Err(DictError::NotHsd)));
        assert!(matches!(parse(b""), Err(DictError::NotHsd)));
        assert!(matches!(parse(b"HSMDICT\0"), Err(DictError::Corrupt(_))));
    }

    #[test]
    fn rejects_truncated_and_appended_files() {
        let buf = write_to_vec(0, &all_sections());
        assert!(matches!(
            parse(&buf[..buf.len() - 1]),
            Err(DictError::Corrupt(_))
        ));
        let mut longer = buf.clone();
        longer.push(0);
        assert!(matches!(parse(&longer), Err(DictError::Corrupt(_))));
    }

    #[test]
    fn rejects_unknown_flags_and_nonzero_reserved_bytes() {
        let buf = write_to_vec(0, &all_sections());
        let mut bad = buf.clone();
        bad[12] = 2;
        assert!(matches!(parse(&bad), Err(DictError::Corrupt(_))));
        let mut bad = buf.clone();
        bad[20] = 1;
        assert!(matches!(parse(&bad), Err(DictError::Corrupt(_))));
        let mut bad = buf.clone();
        bad[63] = 1;
        assert!(matches!(parse(&bad), Err(DictError::Corrupt(_))));
    }

    fn section_entry_at(i: usize) -> usize {
        HEADER_LEN + i * SECTION_ENTRY_LEN
    }

    #[test]
    fn rejects_duplicate_misaligned_and_overlapping_sections() {
        let buf = write_to_vec(0, &all_sections());
        // 2 つ目の id を 1 つ目と同じにする
        let mut dup = buf.clone();
        let first = section_entry_at(0);
        let second = section_entry_at(1);
        let id0 = dup[first..first + 4].to_vec();
        dup[second..second + 4].copy_from_slice(&id0);
        assert!(parse(&dup).unwrap_err().to_string().contains("twice"));

        // オフセットを 64 の倍数からずらす
        let mut misaligned = buf.clone();
        let off = read_u64(&buf, second + 8) + 8;
        misaligned[second + 8..second + 16].copy_from_slice(&off.to_le_bytes());
        assert!(
            parse(&misaligned)
                .unwrap_err()
                .to_string()
                .contains("aligned")
        );

        // 2 つ目のセクションを 1 つ目に重ねる
        let mut overlap = buf.clone();
        let off0 = read_u64(&buf, first + 8);
        overlap[second + 8..second + 16].copy_from_slice(&off0.to_le_bytes());
        assert!(parse(&overlap).unwrap_err().to_string().contains("overlap"));

        // 長さをファイルの外まで伸ばす
        let mut past_end = buf.clone();
        past_end[second + 16..second + 24].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(
            parse(&past_end)
                .unwrap_err()
                .to_string()
                .contains("past the end")
        );
    }

    #[test]
    fn ignores_unknown_section_ids() {
        let mut sections = all_sections();
        let refs: Vec<(SectionId, &[u8])> =
            sections.iter().map(|(id, b)| (*id, b.as_slice())).collect();
        let mut buf = Vec::new();
        write(&mut buf, 0, &refs).unwrap();
        // 最後のセクションの id を未知の値に書き換える
        let last = section_entry_at(sections.len() - 1);
        buf[last..last + 4].copy_from_slice(&999u32.to_le_bytes());
        let (removed_id, _) = sections.pop().unwrap();
        let err = parse(&buf);
        // 取り除いたのが必須セクションなら欠落エラー、そうでなければ読める
        match err {
            Ok(layout) => assert_eq!(layout.unknown_sections, 1),
            Err(e) => assert!(e.to_string().contains(removed_id.name()), "{e}"),
        }
    }
}
