//! 素性レコード（最良パスのトークンを作るときだけ読む冷たい情報）
//!
//! レコード: `pos_id: u16`, `conj_type_id: u16`, `conj_form_id: u16`, `flags: u8`, 読み, [発音], [原形]
//!
//! - flags: bit0 = 原形が表層形と同じ（原形を持たない）、bit1 = 発音が読みと同じ（発音を持たない）、
//!   bit2 = 読みがカタカナ詰め、bit3 = 発音がカタカナ詰め。bit4〜7 は 0。
//!   bit1 が立つときは bit3 も 0
//! - 文字列 = `len: varint`（LEB128、最大 5 バイト、残りの長さ以下）+ 本体。カタカナ詰めは
//!   U+30A0〜U+30FF を 1 文字 1 バイト（`cp - 0x30A0`、0x5F 以下）にしたもの。それ以外は UTF-8
//! - レコードは重複を除いて 1 つの領域に並べ、エントリからはバイトオフセットで指す

use super::DictError;
use std::collections::HashMap;

pub const BASE_IS_SURFACE: u8 = 1 << 0;
pub const PRON_IS_READING: u8 = 1 << 1;
pub const READING_KANA: u8 = 1 << 2;
pub const PRON_KANA: u8 = 1 << 3;
const KNOWN_FLAGS: u8 = BASE_IS_SURFACE | PRON_IS_READING | READING_KANA | PRON_KANA;

const KANA_BASE: u32 = 0x30A0;
const KANA_MAX_BYTE: u8 = 0x5F;
/// レコード先頭の固定部（3 つの u16 と flags）
const FIXED_LEN: usize = 7;
const MAX_VARINT_LEN: usize = 5;

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
pub struct FeatureInput<'a> {
    pub pos_id: u16,
    pub conj_type_id: u16,
    pub conj_form_id: u16,
    pub surface: &'a str,
    pub base_form: &'a str,
    pub reading: &'a str,
    pub pronunciation: &'a str,
}

/// レコードを正準形（同じ値なら同じバイト列）で符号化する
pub fn encode(input: &FeatureInput<'_>, out: &mut Vec<u8>) {
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
    let mut push_str = |packed: Option<Vec<u8>>, s: &str| {
        let bytes = packed.as_deref().unwrap_or(s.as_bytes());
        push_varint(out, bytes.len() as u32);
        out.extend_from_slice(bytes);
    };
    push_str(reading_kana, input.reading);
    if !pron_is_reading {
        push_str(pron_kana, input.pronunciation);
    }
    if !base_is_surface {
        push_str(None, input.base_form);
    }
}

/// 素性レコードを重複を除いて並べる
#[derive(Default)]
pub struct FeatureTableBuilder {
    blob: Vec<u8>,
    offsets: HashMap<Box<[u8]>, u32>,
    scratch: Vec<u8>,
}

impl FeatureTableBuilder {
    /// レコードを足し、そのバイトオフセットを返す。領域が 4GiB を超えるならエラー
    pub fn intern(&mut self, input: &FeatureInput<'_>) -> Result<u32, DictError> {
        self.scratch.clear();
        encode(input, &mut self.scratch);
        if let Some(&offset) = self.offsets.get(self.scratch.as_slice()) {
            return Ok(offset);
        }
        let offset = u32::try_from(self.blob.len())
            .ok()
            .filter(|&o| {
                (o as usize)
                    .checked_add(self.scratch.len())
                    .is_some_and(|e| e <= u32::MAX as usize)
            })
            .ok_or_else(|| DictError::invalid("feature records exceed 4 GiB"))?;
        self.blob.extend_from_slice(&self.scratch);
        self.offsets
            .insert(self.scratch.clone().into_boxed_slice(), offset);
        Ok(offset)
    }

    /// 重複を除いたレコードの数
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    pub fn into_blob(self) -> Vec<u8> {
        self.blob
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
    /// レコードの終わりの位置
    #[cfg(test)]
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

/// レコードを復号して検証する（オフセット・長さ・カタカナ・UTF-8・flags）。文字列表の範囲は呼び出し側で確かめる
pub fn decode(blob: &[u8], offset: usize) -> Result<FeatureRef<'_>, DictError> {
    let fixed = blob
        .get(offset..offset.saturating_add(FIXED_LEN))
        .ok_or_else(|| corrupt(offset, "offset out of range"))?;
    let u16_at = |i: usize| u16::from_le_bytes([fixed[i], fixed[i + 1]]);
    let flags = fixed[6];
    if flags & !KNOWN_FLAGS != 0 {
        return Err(corrupt(offset, "unknown flag bits"));
    }
    if flags & PRON_IS_READING != 0 && flags & PRON_KANA != 0 {
        return Err(corrupt(
            offset,
            "PRON_KANA set although the pronunciation is omitted",
        ));
    }
    let pos = offset + FIXED_LEN;
    let (reading, pos) = read_str(blob, pos, flags & READING_KANA != 0, offset)?;
    let (pronunciation, pos) = if flags & PRON_IS_READING != 0 {
        (None, pos)
    } else {
        let (s, pos) = read_str(blob, pos, flags & PRON_KANA != 0, offset)?;
        (Some(s), pos)
    };
    #[cfg_attr(not(test), allow(unused_variables))]
    let (base_form, pos) = if flags & BASE_IS_SURFACE != 0 {
        (None, pos)
    } else {
        let (s, pos) = read_str(blob, pos, false, offset)?;
        (Some(s), pos)
    };
    Ok(FeatureRef {
        pos_id: u16_at(0),
        conj_type_id: u16_at(2),
        conj_form_id: u16_at(4),
        reading,
        pronunciation,
        base_form,
        #[cfg(test)]
        end: pos,
    })
}

#[cfg(test)]
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
        let mut buf = vec![0xAA]; // 先頭をずらしてオフセットの扱いも確かめる
        encode(i, &mut buf);
        let f = decode(&buf, 1).unwrap();
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
        let mut buf = Vec::new();
        encode(&input("東京", "東京", "トウキョウ", "トウキョウ"), &mut buf);
        // 固定部 7 + 長さ 1 + カタカナ 5 文字
        assert_eq!(buf.len(), 7 + 1 + 5);
        assert_eq!(buf[6], BASE_IS_SURFACE | PRON_IS_READING | READING_KANA);
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
        let mut good = Vec::new();
        encode(&input("方法", "方", "ホウホウ", "ホーホー"), &mut good);
        assert!(decode(&good, 0).is_ok());
        // 範囲外のオフセット
        assert!(decode(&good, good.len()).is_err());
        assert!(decode(&good, usize::MAX).is_err());
        // 途中で切れる
        assert!(decode(&good[..good.len() - 1], 0).is_err());
        // 未知の flags
        let mut bad = good.clone();
        bad[6] |= 0x10;
        assert!(decode(&bad, 0).is_err());
        // 発音を省略しつつ PRON_KANA
        let mut bad = good.clone();
        bad[6] = PRON_IS_READING | PRON_KANA;
        assert!(decode(&bad, 0).is_err());
        // カタカナ詰めの範囲外
        let mut bad = good.clone();
        bad[8] = 0x60;
        assert!(decode(&bad, 0).is_err());
        // UTF-8 でない原形
        let mut bad = good.clone();
        let last = bad.len() - 1;
        bad[last] = 0xFF;
        assert!(decode(&bad, 0).is_err());
    }
}
