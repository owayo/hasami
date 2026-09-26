//! 素性レコード（最良パスのトークンを作るときだけ読む冷たい情報）
//!
//! レコード: `grammar_id: varint(u32)`, `flags: u8`, 読み, [発音], [原形]
//! 文法番号は GRAMMAR の 6 バイト要素（品詞・活用型・活用形の u16 番号）を指す。
//!
//! - flags: bit0 = 原形が表層形と同じ（原形を持たない）、bit1 = 発音が読みと同じ（発音を持たない）、
//!   bit2 = 読みがカタカナ詰め、bit3 = 発音がカタカナ詰め。bit4〜7 は 0。
//!   bit1 が立つときは bit3 も 0
//! - 文字列 = `len: varint`（LEB128、最大 5 バイト、残りの長さ以下）+ 本体。カタカナ詰めは
//!   U+30A0〜U+30FF を 1 文字 1 バイト（`cp - 0x30A0`、0x5F 以下）にしたもの。それ以外は UTF-8
//! - レコードは重複を除いて 1 つの領域に並べ、エントリからはバイトオフセットで指す

use super::DictError;
use super::records::GrammarRecord;
#[cfg(feature = "build")]
use std::collections::HashMap;
use std::sync::Arc;

pub const BASE_IS_SURFACE: u8 = 1 << 0;
pub const PRON_IS_READING: u8 = 1 << 1;
pub const READING_KANA: u8 = 1 << 2;
pub const PRON_KANA: u8 = 1 << 3;
const KNOWN_FLAGS: u8 = BASE_IS_SURFACE | PRON_IS_READING | READING_KANA | PRON_KANA;

const KANA_BASE: u32 = 0x30A0;
const KANA_MAX_BYTE: u8 = 0x5F;
const MAX_VARINT_LEN: usize = 5;

#[cfg(feature = "build")]
pub fn push_varint(buf: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

/// varint を読む。Returns: (値, 読んだ後の位置)。途中で切れる・5 バイトを超える・u32 に収まらないなら None
pub fn read_varint(buf: &[u8], mut pos: usize) -> Option<(u32, usize)> {
    let mut value = 0u32;
    for i in 0..MAX_VARINT_LEN {
        let b = *buf.get(pos)?;
        pos += 1;
        let payload = (b & 0x7F) as u32;
        // 5 バイト目は下位 4 ビットしか使えない（32 ビットを超える）
        if i == MAX_VARINT_LEN - 1 && payload > 0x0F {
            return None;
        }
        value |= payload << (7 * i);
        if b & 0x80 == 0 {
            return Some((value, pos));
        }
    }
    None
}

/// 文字列がカタカナ詰めにできれば詰めたバイト列を返す（空文字列は詰めない）
#[cfg(feature = "build")]
fn pack_kana(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() {
        return None;
    }
    s.chars()
        .map(|c| {
            let cp = c as u32;
            (KANA_BASE..=KANA_BASE + KANA_MAX_BYTE as u32)
                .contains(&cp)
                .then(|| (cp - KANA_BASE) as u8)
        })
        .collect()
}

/// 素性レコードに入れる値
#[cfg(feature = "build")]
pub struct FeatureInput<'a> {
    pub pos_id: u16,
    pub conj_type_id: u16,
    pub conj_form_id: u16,
    pub surface: &'a str,
    pub base_form: &'a str,
    pub reading: &'a str,
    pub pronunciation: &'a str,
}

/// 重複排除用の正準キー（文法の 3 番号 + flags + 文字列）を作る。
/// ファイルに書く前に finish で先頭 6 バイトを共有文法番号に置き換える。
#[cfg(feature = "build")]
fn encode(input: &FeatureInput<'_>, out: &mut Vec<u8>) -> Result<(), DictError> {
    let mut flags = 0u8;
    let base_is_surface = input.base_form == input.surface;
    let pron_is_reading = input.pronunciation == input.reading;
    if base_is_surface {
        flags |= BASE_IS_SURFACE;
    }
    if pron_is_reading {
        flags |= PRON_IS_READING;
    }
    let reading_kana = pack_kana(input.reading);
    if reading_kana.is_some() {
        flags |= READING_KANA;
    }
    let pron_kana = if pron_is_reading {
        None
    } else {
        pack_kana(input.pronunciation)
    };
    if pron_kana.is_some() {
        flags |= PRON_KANA;
    }

    out.extend_from_slice(&input.pos_id.to_le_bytes());
    out.extend_from_slice(&input.conj_type_id.to_le_bytes());
    out.extend_from_slice(&input.conj_form_id.to_le_bytes());
    out.push(flags);
    let mut push_str = |packed: Option<Vec<u8>>, s: &str| -> Result<(), DictError> {
        let bytes = packed.as_deref().unwrap_or(s.as_bytes());
        let len = u32::try_from(bytes.len())
            .map_err(|_| DictError::invalid("feature string exceeds u32 length"))?;
        push_varint(out, len);
        out.extend_from_slice(bytes);
        Ok(())
    };
    push_str(reading_kana, input.reading)?;
    if !pron_is_reading {
        push_str(pron_kana, input.pronunciation)?;
    }
    if !base_is_surface {
        push_str(None, input.base_form)?;
    }
    Ok(())
}

/// 素性レコードを重複を除いて並べる
#[cfg(feature = "build")]
#[derive(Default)]
pub struct FeatureTableBuilder {
    blob: Vec<u8>,
    records: HashMap<Box<[u8]>, u32>,
    starts: Vec<usize>,
    scratch: Vec<u8>,
}

/// 共有文法表と、素性番号から最終レコードのバイトオフセットへの対応。
#[cfg(feature = "build")]
pub struct FeatureTable {
    pub grammar: Vec<GrammarRecord>,
    pub offsets: Vec<u32>,
    pub blob: Vec<u8>,
}

#[cfg(feature = "build")]
impl FeatureTableBuilder {
    /// レコードを足し、重複を除いた素性番号を返す。バイトオフセットは finish で確定する。
    pub fn intern(&mut self, input: &FeatureInput<'_>) -> Result<u32, DictError> {
        self.scratch.clear();
        encode(input, &mut self.scratch)?;
        if let Some(&id) = self.records.get(self.scratch.as_slice()) {
            return Ok(id);
        }
        let id = u32::try_from(self.starts.len())
            .map_err(|_| DictError::invalid("too many distinct feature records"))?;
        self.blob
            .len()
            .checked_add(self.scratch.len())
            .ok_or_else(|| DictError::invalid("canonical feature records exceed address space"))?;
        self.starts.push(self.blob.len());
        self.blob.extend_from_slice(&self.scratch);
        self.records
            .insert(self.scratch.clone().into_boxed_slice(), id);
        Ok(id)
    }

    /// 重複を除いたレコードの数
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    /// 正準キーの先頭 6 バイトを共有文法番号に置き換える。
    /// 中間領域には 4GiB 制限を課さず、最終 FEATURES が u32 の範囲に収まることを検査する。
    pub fn finish(self) -> Result<FeatureTable, DictError> {
        let Self {
            blob,
            records,
            mut starts,
            scratch,
        } = self;
        drop(records);
        drop(scratch);
        starts.push(blob.len());
        let grammar_at = |start: usize| GrammarRecord {
            pos_id: u16::from_le_bytes([blob[start], blob[start + 1]]),
            conj_type_id: u16::from_le_bytes([blob[start + 2], blob[start + 3]]),
            conj_form_id: u16::from_le_bytes([blob[start + 4], blob[start + 5]]),
        };
        let mut counts: HashMap<GrammarRecord, usize> = HashMap::new();
        for &start in &starts[..starts.len() - 1] {
            *counts.entry(grammar_at(start)).or_default() += 1;
        }
        let mut ranked: Vec<_> = counts.into_iter().collect();
        ranked.sort_unstable_by(|(a, ac), (b, bc)| bc.cmp(ac).then_with(|| a.cmp(b)));
        u32::try_from(ranked.len())
            .map_err(|_| DictError::invalid("too many distinct grammar records"))?;
        // 正準キーの 6 バイトを除き、各文法番号の varint 長を足す。
        // 最終サイズだけに上限を課し、再確保によるピークメモリも避ける。
        let suffix_len = (starts.len() - 1)
            .checked_mul(6)
            .and_then(|headers| blob.len().checked_sub(headers))
            .ok_or_else(|| DictError::invalid("feature record size overflow"))?;
        let final_len = ranked
            .iter()
            .enumerate()
            .try_fold(suffix_len, |len, (id, (_, count))| {
                let id_len =
                    ((u32::BITS - (id as u32).leading_zeros()).max(1) as usize).div_ceil(7);
                count.checked_mul(id_len).and_then(|n| len.checked_add(n))
            })
            .filter(|&len| len <= u32::MAX as usize)
            .ok_or_else(|| DictError::invalid("feature records exceed 4 GiB"))?;
        let grammar: Vec<_> = ranked.into_iter().map(|(g, _)| g).collect();
        let ids: HashMap<_, _> = grammar
            .iter()
            .enumerate()
            .map(|(i, &g)| (g, i as u32))
            .collect();
        let mut out = Vec::with_capacity(final_len);
        let mut offsets = Vec::with_capacity(starts.len() - 1);
        for pair in starts.windows(2) {
            let start = pair[0];
            let offset = u32::try_from(out.len())
                .map_err(|_| DictError::invalid("feature records exceed 4 GiB"))?;
            let id = ids[&grammar_at(start)];
            let id_len = ((u32::BITS - id.leading_zeros()).max(1) as usize).div_ceil(7);
            let suffix = &blob[start + 6..pair[1]];
            out.len()
                .checked_add(id_len)
                .and_then(|n| n.checked_add(suffix.len()))
                .filter(|&len| len <= u32::MAX as usize)
                .ok_or_else(|| DictError::invalid("feature records exceed 4 GiB"))?;
            offsets.push(offset);
            push_varint(&mut out, id);
            out.extend_from_slice(suffix);
        }
        Ok(FeatureTable {
            grammar,
            offsets,
            blob: out,
        })
    }
}

/// 素性レコードの文字列（カタカナ詰めか UTF-8）
#[derive(Clone, Copy, Debug)]
pub enum PackedStr<'a> {
    Kana(&'a [u8]),
    Utf8(&'a str),
}

impl PackedStr<'_> {
    pub fn into_string(self) -> String {
        match self {
            PackedStr::Utf8(s) => s.to_owned(),
            PackedStr::Kana(bytes) => {
                let mut s = String::with_capacity(bytes.len() * 3);
                for &b in bytes {
                    // 復号時に 0x5F 以下を確かめてあるので必ず文字になる
                    s.push(char::from_u32(KANA_BASE + b as u32).unwrap_or('\u{FFFD}'));
                }
                s
            }
        }
    }

    /// `Arc<str>` にする。カタカナ詰めは `scratch` に復号してから 1 回だけ確保する
    /// （String を作ってから Arc に移すと確保が 2 回・解放が 1 回になる）
    pub fn to_arc(self, scratch: &mut String) -> Arc<str> {
        match self {
            PackedStr::Utf8(s) => Arc::from(s),
            PackedStr::Kana(bytes) => {
                scratch.clear();
                for &b in bytes {
                    scratch.push(char::from_u32(KANA_BASE + b as u32).unwrap_or('\u{FFFD}'));
                }
                Arc::from(scratch.as_str())
            }
        }
    }

    pub fn is_empty(self) -> bool {
        match self {
            PackedStr::Kana(b) => b.is_empty(),
            PackedStr::Utf8(s) => s.is_empty(),
        }
    }
}

/// 復号した素性レコード（原形・発音は省略されていれば None）
#[derive(Clone, Copy, Debug)]
pub struct FeatureRef<'a> {
    pub pos_id: u16,
    pub conj_type_id: u16,
    pub conj_form_id: u16,
    pub reading: PackedStr<'a>,
    /// None なら読みと同じ
    pub pronunciation: Option<PackedStr<'a>>,
    /// None なら表層形と同じ
    pub base_form: Option<PackedStr<'a>>,
    /// レコードの終わりの位置（`build` feature のテストだけが読む）
    #[cfg(all(test, feature = "build"))]
    pub end: usize,
}

fn corrupt(offset: usize, what: &str) -> DictError {
    DictError::corrupt(format!("feature record at offset {offset}: {what}"))
}

fn read_str<'a>(
    blob: &'a [u8],
    pos: usize,
    kana: bool,
    start: usize,
) -> Result<(PackedStr<'a>, usize), DictError> {
    let (len, pos) = read_varint(blob, pos).ok_or_else(|| corrupt(start, "broken length"))?;
    let end = pos
        .checked_add(len as usize)
        .filter(|&e| e <= blob.len())
        .ok_or_else(|| corrupt(start, "string runs past the end of FEATURES"))?;
    let bytes = &blob[pos..end];
    let s = if kana {
        if bytes.iter().any(|&b| b > KANA_MAX_BYTE) {
            return Err(corrupt(start, "packed katakana byte out of range"));
        }
        PackedStr::Kana(bytes)
    } else {
        PackedStr::Utf8(std::str::from_utf8(bytes).map_err(|_| corrupt(start, "invalid UTF-8"))?)
    };
    Ok((s, end))
}

/// レコードを復号して検証する（文法番号・オフセット・長さ・カタカナ・UTF-8・flags）。
/// 文法表の各番号が文字列表の範囲に収まることはロード時に検証する。
pub fn decode<'a>(
    blob: &'a [u8],
    offset: usize,
    grammar: &[GrammarRecord],
) -> Result<FeatureRef<'a>, DictError> {
    let (id, pos) =
        read_varint(blob, offset).ok_or_else(|| corrupt(offset, "broken grammar ID"))?;
    let g = grammar
        .get(id as usize)
        .ok_or_else(|| corrupt(offset, "grammar ID out of range"))?;
    let flags = *blob
        .get(pos)
        .ok_or_else(|| corrupt(offset, "missing flags"))?;
    if flags & !KNOWN_FLAGS != 0 {
        return Err(corrupt(offset, "unknown flag bits"));
    }
    if flags & PRON_IS_READING != 0 && flags & PRON_KANA != 0 {
        return Err(corrupt(
            offset,
            "PRON_KANA set although the pronunciation is omitted",
        ));
    }
    let pos = pos + 1;
    let (reading, pos) = read_str(blob, pos, flags & READING_KANA != 0, offset)?;
    let (pronunciation, pos) = if flags & PRON_IS_READING != 0 {
        (None, pos)
    } else {
        let (s, pos) = read_str(blob, pos, flags & PRON_KANA != 0, offset)?;
        (Some(s), pos)
    };
    #[cfg_attr(not(all(test, feature = "build")), allow(unused_variables))]
    let (base_form, pos) = if flags & BASE_IS_SURFACE != 0 {
        (None, pos)
    } else {
        let (s, pos) = read_str(blob, pos, false, offset)?;
        (Some(s), pos)
    };
    Ok(FeatureRef {
        pos_id: g.pos_id,
        conj_type_id: g.conj_type_id,
        conj_form_id: g.conj_form_id,
        reading,
        pronunciation,
        base_form,
        #[cfg(all(test, feature = "build"))]
        end: pos,
    })
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;

    fn input<'a>(
        surface: &'a str,
        base: &'a str,
        reading: &'a str,
        pron: &'a str,
    ) -> FeatureInput<'a> {
        FeatureInput {
            pos_id: 3,
            conj_type_id: 1,
            conj_form_id: 2,
            surface,
            base_form: base,
            reading,
            pronunciation: pron,
        }
    }

    fn roundtrip(i: &FeatureInput<'_>) -> (String, String, String) {
        let table = single(i);
        let mut buf = vec![0xAA]; // 先頭をずらしてオフセットの扱いも確かめる
        buf.extend_from_slice(&table.blob);
        let f = decode(&buf, 1, &table.grammar).unwrap();
        assert_eq!(f.end, buf.len());
        assert_eq!((f.pos_id, f.conj_type_id, f.conj_form_id), (3, 1, 2));
        let reading = f.reading.into_string();
        let pron = f
            .pronunciation
            .map_or_else(|| reading.clone(), |p| p.into_string());
        let base = f
            .base_form
            .map_or_else(|| i.surface.to_owned(), |b| b.into_string());
        (base, reading, pron)
    }

    fn single(i: &FeatureInput<'_>) -> FeatureTable {
        let mut b = FeatureTableBuilder::default();
        assert_eq!(b.intern(i).unwrap(), 0);
        b.finish().unwrap()
    }

    #[test]
    fn varint_roundtrip_and_limits() {
        for v in [0u32, 1, 127, 128, 300, 16_383, 16_384, u32::MAX] {
            let mut buf = Vec::new();
            push_varint(&mut buf, v);
            assert!(buf.len() <= MAX_VARINT_LEN);
            assert_eq!(read_varint(&buf, 0), Some((v, buf.len())));
        }
        // 途中で切れる
        assert_eq!(read_varint(&[0x80], 0), None);
        // 6 バイト以上
        assert_eq!(read_varint(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00], 0), None);
        // 5 バイト目が u32 を超える
        assert_eq!(read_varint(&[0xFF, 0xFF, 0xFF, 0xFF, 0x1F], 0), None);
    }

    #[test]
    fn omits_base_and_pronunciation_when_equal() {
        let table = single(&input("東京", "東京", "トウキョウ", "トウキョウ"));
        // 文法番号 1 + flags 1 + 長さ 1 + カタカナ 5 文字
        assert_eq!(table.blob.len(), 2 + 1 + 5);
        assert_eq!(
            table.blob[1],
            BASE_IS_SURFACE | PRON_IS_READING | READING_KANA
        );
        assert_eq!(
            roundtrip(&input("東京", "東京", "トウキョウ", "トウキョウ")),
            ("東京".into(), "トウキョウ".into(), "トウキョウ".into())
        );
    }

    #[test]
    fn keeps_different_base_and_pronunciation() {
        assert_eq!(
            roundtrip(&input("方法", "方法", "ホウホウ", "ホーホー")),
            ("方法".into(), "ホウホウ".into(), "ホーホー".into())
        );
        assert_eq!(
            roundtrip(&input("示し", "示す", "シメシ", "シメシ")),
            ("示す".into(), "シメシ".into(), "シメシ".into())
        );
    }

    #[test]
    fn non_katakana_strings_are_stored_as_utf8() {
        // 読みが空、ひらがな・英字の発音、長音記号・中黒を含むカタカナ
        for (reading, pron) in [
            ("", ""),
            ("", "ア"),
            ("ひらがな", "ヒラガナ"),
            ("Siemens", "シーメンス"),
            ("ア・ラ・カルト", "アーラーカルト"),
            ("ヴァ゠ヿ", "ヴァ"),
        ] {
            let (_, r, p) = roundtrip(&input("x", "y", reading, pron));
            assert_eq!((r.as_str(), p.as_str()), (reading, pron));
        }
    }

    #[test]
    fn builder_deduplicates_records() {
        let mut b = FeatureTableBuilder::default();
        let a = b
            .intern(&input("東京", "東京", "トウキョウ", "トウキョウ"))
            .unwrap();
        // 表層形が違っても、原形 = 表層形なら同じレコードになる
        let a2 = b
            .intern(&input(
                "とうきょう",
                "とうきょう",
                "トウキョウ",
                "トウキョウ",
            ))
            .unwrap();
        let c = b
            .intern(&input("京都", "京都", "キョウト", "キョウト"))
            .unwrap();
        assert_eq!(a, a2);
        assert_ne!(a, c);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn rejects_broken_records() {
        let table = single(&input("方法", "方", "ホウホウ", "ホーホー"));
        let good = table.blob;
        let decode =
            |bytes: &[u8], offset| super::decode(bytes, offset, &table.grammar).map(|_| ());
        assert!(decode(&good, 0).is_ok());
        // 範囲外のオフセット
        assert!(decode(&good, good.len()).is_err());
        assert!(decode(&good, usize::MAX).is_err());
        // 途中で切れる
        assert!(decode(&good[..good.len() - 1], 0).is_err());
        // 未知の flags
        let mut bad = good.clone();
        bad[1] |= 0x10;
        assert!(decode(&bad, 0).is_err());
        // 発音を省略しつつ PRON_KANA
        let mut bad = good.clone();
        bad[1] = PRON_IS_READING | PRON_KANA;
        assert!(decode(&bad, 0).is_err());
        // カタカナ詰めの範囲外
        let mut bad = good.clone();
        bad[3] = 0x60;
        assert!(decode(&bad, 0).is_err());
        // UTF-8 でない原形
        let mut bad = good.clone();
        let last = bad.len() - 1;
        bad[last] = 0xFF;
        assert!(decode(&bad, 0).is_err());
        // 文法番号の範囲外、途中切れ、u32 のオーバーフロー
        let mut bad = good.clone();
        bad[0] = 1;
        assert!(decode(&bad, 0).is_err());
        assert!(decode(&[0x80], 0).is_err());
        assert!(decode(&[0xff, 0xff, 0xff, 0xff, 0x10], 0).is_err());
        assert!(decode(&[0], 0).is_err());
    }

    #[test]
    fn grammar_frequency_counts_unique_features_and_breaks_ties_by_ids() {
        let build = || {
            let mut b = FeatureTableBuilder::default();
            for (pos, reading) in [(5, "a"), (5, "b"), (1, "c"), (3, "d")] {
                let mut i = input("x", "x", reading, reading);
                i.pos_id = pos;
                b.intern(&i).unwrap();
                if pos == 3 {
                    // 同じ素性を何回参照しても、共有文法の頻度は増えない。
                    for _ in 0..10 {
                        b.intern(&i).unwrap();
                    }
                }
            }
            b.finish().unwrap()
        };
        let a = build();
        let b = build();
        assert_eq!(
            a.grammar.iter().map(|g| g.pos_id).collect::<Vec<_>>(),
            [5, 1, 3]
        );
        assert_eq!(a.grammar, b.grammar);
        assert_eq!(a.offsets, b.offsets);
        assert_eq!(a.blob, b.blob);
        for (index, expected) in [5, 5, 1, 3].into_iter().enumerate() {
            assert_eq!(
                decode(&a.blob, a.offsets[index] as usize, &a.grammar)
                    .unwrap()
                    .pos_id,
                expected
            );
        }
    }

    #[test]
    fn grammar_has_no_u16_tuple_limit_and_ids_cross_varint_boundaries() {
        let mut b = FeatureTableBuilder::default();
        // 個々の文字列表は u16 の範囲でも、その組は 65536 通りを超えられる。
        for id in 0..=65_536u32 {
            let mut i = input("x", "x", "x", "x");
            i.pos_id = (id >> 16) as u16;
            i.conj_type_id = id as u16;
            i.conj_form_id = 0;
            assert_eq!(b.intern(&i).unwrap(), id);
        }
        let table = b.finish().unwrap();
        assert_eq!(table.grammar.len(), 65_537);
        for id in [0, 127, 128, 16_383, 16_384, 65_535, 65_536] {
            let offset = table.offsets[id] as usize;
            let (stored, end) = read_varint(&table.blob, offset).unwrap();
            assert_eq!(stored as usize, id);
            let expected_len = if id < 128 {
                1
            } else if id < 16_384 {
                2
            } else {
                3
            };
            assert_eq!(end - offset, expected_len);
            let f = decode(&table.blob, offset, &table.grammar).unwrap();
            assert_eq!(
                (f.pos_id, f.conj_type_id, f.conj_form_id),
                ((id >> 16) as u16, id as u16, 0)
            );
        }
    }
}
