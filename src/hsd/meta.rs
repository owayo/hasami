//! 辞書のメタデータ（META セクション）
//!
//! UTF-8 の `key=value` 行（`\n` 区切り）。各行は最初の `=` で分け、key は `[a-z0-9_]+`、
//! value は改行を含まない。必須キーは `name` と `pos_scheme`（`ipadic` / `unidic`）。
//! 重複キーはエラー、未知のキーは保持して `hasami info` で表示する。総長は 64KiB 以下。
//! `pos_scheme` は自己申告で、辞書の中身の検証にはならない（品詞の正規化表を選ぶためのヒント）。

use super::DictError;
use std::fmt;

pub const MAX_META_LEN: usize = 64 * 1024;

pub const KEY_NAME: &str = "name";
pub const KEY_POS_SCHEME: &str = "pos_scheme";
pub const KEY_HASAMI_VERSION: &str = "hasami_version";
pub const KEY_SOURCES: &str = "sources";
pub const KEY_REPAIRS: &str = "repairs";
pub const KEY_PRUNED_DOMINATED: &str = "pruned_dominated";
/// matrix.def なしで作った辞書が置く、使われている文脈 ID を覆うゼロ行列の印（`true`）
pub const KEY_ZERO_MATRIX: &str = "zero_matrix";

/// 品詞体系
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PosScheme {
    Ipadic,
    Unidic,
}

impl PosScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            PosScheme::Ipadic => "ipadic",
            PosScheme::Unidic => "unidic",
        }
    }

    pub fn parse(s: &str) -> Option<PosScheme> {
        match s {
            "ipadic" => Some(PosScheme::Ipadic),
            "unidic" => Some(PosScheme::Unidic),
            _ => None,
        }
    }
}

impl fmt::Display for PosScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// メタデータ（書かれた順を保つ）
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    entries: Vec<(String, String)>,
}

fn is_valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl Meta {
    /// 必須キーだけを持つメタデータ
    pub fn new(name: &str, pos_scheme: PosScheme) -> Self {
        let mut meta = Meta {
            entries: Vec::new(),
        };
        meta.entries.push((KEY_NAME.into(), name.into()));
        meta.entries
            .push((KEY_POS_SCHEME.into(), pos_scheme.as_str().into()));
        meta
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// 値を設定する（既にあれば置き換え、なければ末尾に足す）
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), DictError> {
        if !is_valid_key(key) {
            return Err(DictError::invalid(format!(
                "invalid metadata key `{key}` (use [a-z0-9_]+)"
            )));
        }
        if value.contains(['\n', '\r']) {
            return Err(DictError::invalid(format!(
                "metadata value for `{key}` must not contain line breaks"
            )));
        }
        if key == KEY_POS_SCHEME && PosScheme::parse(value).is_none() {
            return Err(DictError::invalid(format!(
                "pos_scheme must be `ipadic` or `unidic`, not `{value}`"
            )));
        }
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_owned(),
            None => self.entries.push((key.to_owned(), value.to_owned())),
        }
        Ok(())
    }

    /// キーを取り除く（必須キーは取り除けない）
    pub fn remove(&mut self, key: &str) {
        if key != KEY_NAME && key != KEY_POS_SCHEME {
            self.entries.retain(|(k, _)| k != key);
        }
    }

    pub fn name(&self) -> &str {
        self.get(KEY_NAME).unwrap_or_default()
    }

    pub fn pos_scheme(&self) -> PosScheme {
        self.get(KEY_POS_SCHEME)
            .and_then(PosScheme::parse)
            .unwrap_or(PosScheme::Ipadic)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, DictError> {
        let mut out = String::new();
        for (k, v) in &self.entries {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
        if out.len() > MAX_META_LEN {
            return Err(DictError::invalid(format!(
                "metadata is {} bytes (limit {MAX_META_LEN})",
                out.len()
            )));
        }
        Ok(out.into_bytes())
    }

    pub fn parse(bytes: &[u8]) -> Result<Meta, DictError> {
        let corrupt = |m: String| DictError::corrupt(format!("META: {m}"));
        if bytes.len() > MAX_META_LEN {
            return Err(corrupt(format!("{} bytes exceed the limit", bytes.len())));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| corrupt("not valid UTF-8".into()))?;
        let mut entries: Vec<(String, String)> = Vec::new();
        // 末尾の改行の後ろは行として数えない
        for (i, line) in text
            .strip_suffix('\n')
            .unwrap_or(text)
            .split('\n')
            .enumerate()
        {
            if text.is_empty() {
                break;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| corrupt(format!("line {} has no `=`", i + 1)))?;
            if !is_valid_key(key) {
                return Err(corrupt(format!(
                    "line {} has an invalid key `{key}`",
                    i + 1
                )));
            }
            if value.contains('\r') {
                return Err(corrupt(format!(
                    "line {} contains a carriage return",
                    i + 1
                )));
            }
            if entries.iter().any(|(k, _)| k == key) {
                return Err(corrupt(format!("key `{key}` appears twice")));
            }
            entries.push((key.to_owned(), value.to_owned()));
        }
        let meta = Meta { entries };
        if meta.get(KEY_NAME).is_none() {
            return Err(corrupt("required key `name` is missing".into()));
        }
        match meta.get(KEY_POS_SCHEME) {
            None => return Err(corrupt("required key `pos_scheme` is missing".into())),
            Some(s) if PosScheme::parse(s).is_none() => {
                return Err(corrupt(format!("unknown pos_scheme `{s}`")));
            }
            Some(_) => {}
        }
        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_order_and_unknown_keys() {
        let mut meta = Meta::new("ipadic", PosScheme::Ipadic);
        meta.set(KEY_SOURCES, "ipadic@61b90ba").unwrap();
        meta.set("custom_key", "値 = そのまま").unwrap();
        let parsed = Meta::parse(&meta.to_bytes().unwrap()).unwrap();
        assert_eq!(parsed, meta);
        assert_eq!(parsed.get("custom_key"), Some("値 = そのまま"));
        assert_eq!(parsed.name(), "ipadic");
        assert_eq!(parsed.pos_scheme(), PosScheme::Ipadic);
    }

    #[test]
    fn set_replaces_and_validates() {
        let mut meta = Meta::new("x", PosScheme::Unidic);
        meta.set(KEY_NAME, "y").unwrap();
        assert_eq!(meta.name(), "y");
        assert!(meta.set("Bad-Key", "v").is_err());
        assert!(meta.set("k", "a\nb").is_err());
        assert!(meta.set(KEY_POS_SCHEME, "sudachi").is_err());
        meta.remove(KEY_NAME);
        assert_eq!(meta.name(), "y");
    }

    #[test]
    fn rejects_broken_metadata() {
        for bad in [
            &b"pos_scheme=ipadic\n"[..],              // name がない
            b"name=x\n",                              // pos_scheme がない
            b"name=x\npos_scheme=mecab\n",            // 未知の pos_scheme
            b"name=x\npos_scheme=ipadic\nname=y\n",   // 重複キー
            b"name=x\npos_scheme=ipadic\nnoequals\n", // = がない
            b"name=x\npos_scheme=ipadic\nKey=v\n",    // key の文字
            b"name=x\r\npos_scheme=ipadic\n",         // CR
            b"name=\xFF\npos_scheme=ipadic\n",        // UTF-8 でない
            b"",
        ] {
            assert!(
                Meta::parse(bad).is_err(),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
        let long = format!("name={}\npos_scheme=ipadic\n", "x".repeat(MAX_META_LEN));
        assert!(Meta::parse(long.as_bytes()).is_err());
    }
}
