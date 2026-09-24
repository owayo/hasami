//! 小さな文字列表（品詞・活用型・活用形・文字カテゴリ名）
//!
//! `count: u32`, `offsets: u32 × (count + 1)`（先頭 0、単調非減少、末尾 = 本体の長さ）、本体（UTF-8）。
//! ロード時に全体を検証して `Vec<Arc<str>>` に 1 回だけ実体化する。

use super::DictError;
#[cfg(feature = "build")]
use std::collections::HashMap;
use std::sync::Arc;

/// 表の文字列を出現順に重複なく集める
#[cfg(feature = "build")]
#[derive(Default)]
pub struct StringTableBuilder {
    ids: HashMap<Arc<str>, u32>,
    strings: Vec<Arc<str>>,
}

#[cfg(feature = "build")]
impl StringTableBuilder {
    /// 文字列の番号を返す（初めて見る文字列なら末尾に足す）
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.strings.len() as u32;
        let arc: Arc<str> = Arc::from(s);
        self.strings.push(Arc::clone(&arc));
        self.ids.insert(arc, id);
        id
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let body_len: usize = self.strings.iter().map(|s| s.len()).sum();
        let mut out = Vec::with_capacity(4 + 4 * (self.strings.len() + 1) + body_len);
        out.extend_from_slice(&(self.strings.len() as u32).to_le_bytes());
        let mut offset = 0u32;
        out.extend_from_slice(&offset.to_le_bytes());
        for s in &self.strings {
            offset += s.len() as u32;
            out.extend_from_slice(&offset.to_le_bytes());
        }
        for s in &self.strings {
            out.extend_from_slice(s.as_bytes());
        }
        out
    }
}

/// 文字列表を検証して実体化する。`max_count` は番号の型で表せる個数の上限
pub fn parse(bytes: &[u8], what: &str, max_count: usize) -> Result<Vec<Arc<str>>, DictError> {
    let corrupt = |m: String| DictError::corrupt(format!("{what}: {m}"));
    if bytes.len() < 4 {
        return Err(corrupt("string table is shorter than its count".into()));
    }
    let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    if count > max_count {
        return Err(corrupt(format!(
            "{count} strings exceed the limit of {max_count}"
        )));
    }
    let body_start = count
        .checked_add(1)
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(4))
        .filter(|&n| n <= bytes.len())
        .ok_or_else(|| corrupt("offset array runs past the end of the section".into()))?;
    let body = &bytes[body_start..];
    let offset_at = |i: usize| {
        let at = 4 + 4 * i;
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    };
    if offset_at(0) != 0 {
        return Err(corrupt("first offset is not zero".into()));
    }
    if offset_at(count) != body.len() {
        return Err(corrupt(format!(
            "last offset {} does not match the body length {}",
            offset_at(count),
            body.len()
        )));
    }
    let mut strings = Vec::with_capacity(count);
    for i in 0..count {
        let (start, end) = (offset_at(i), offset_at(i + 1));
        if start > end || end > body.len() {
            return Err(corrupt(format!(
                "offsets of string {i} are out of order or range"
            )));
        }
        let s = std::str::from_utf8(&body[start..end])
            .map_err(|_| corrupt(format!("string {i} is not valid UTF-8")))?;
        strings.push(Arc::from(s));
    }
    Ok(strings)
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_first_appearance_order() {
        let mut b = StringTableBuilder::default();
        assert_eq!(b.intern("名詞,一般,*,*"), 0);
        assert_eq!(b.intern("助詞,格助詞,一般,*"), 1);
        assert_eq!(b.intern("名詞,一般,*,*"), 0);
        assert_eq!(b.intern(""), 2);
        let strings = parse(&b.to_bytes(), "POS_STRINGS", 10).unwrap();
        let strings: Vec<&str> = strings.iter().map(|s| &**s).collect();
        assert_eq!(strings, ["名詞,一般,*,*", "助詞,格助詞,一般,*", ""]);
    }

    #[test]
    fn empty_table_roundtrips() {
        let b = StringTableBuilder::default();
        assert!(parse(&b.to_bytes(), "t", 0).unwrap().is_empty());
    }

    #[test]
    fn rejects_broken_tables() {
        let mut b = StringTableBuilder::default();
        b.intern("あ");
        b.intern("いう");
        let good = b.to_bytes();
        assert!(parse(&good, "t", 2).is_ok());
        // 個数の上限
        assert!(parse(&good, "t", 1).is_err());
        // 短すぎる
        assert!(parse(&good[..3], "t", 2).is_err());
        assert!(parse(&good[..10], "t", 2).is_err());
        // 本体の途中で切れる
        assert!(parse(&good[..good.len() - 1], "t", 2).is_err());
        // オフセットが減る
        let mut bad = good.clone();
        bad[8..12].copy_from_slice(&7u32.to_le_bytes());
        assert!(parse(&bad, "t", 2).is_err());
        // 先頭のオフセットが 0 でない
        let mut bad = good.clone();
        bad[4..8].copy_from_slice(&1u32.to_le_bytes());
        assert!(parse(&bad, "t", 2).is_err());
        // UTF-8 でない（「あ」の 2 バイト目までで区切る）
        let mut bad = good.clone();
        bad[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert!(parse(&bad, "t", 2).is_err());
        // 個数がとても大きい
        let mut bad = good.clone();
        bad[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&bad, "t", usize::MAX).is_err());
    }
}
