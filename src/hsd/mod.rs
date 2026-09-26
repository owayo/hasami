//! 辞書形式 v5 (.hsd)
//!
//! ファイルは 64 バイトのヘッダ、セクション表、64 バイト境界に置いたセクションからなる。
//! 数値はすべてリトルエンディアンで、読み込み側は mmap した領域を型付きスライスとして
//! そのまま参照する（ゼロコピー）。形式の規範は `container`（ヘッダ・セクション表）、
//! `trie`（文字単位 double-array）、`features`（素性レコード）、`strtab`（文字列表）、
//! `meta`（メタデータ）の各モジュールに書く。
//!
//! 書き出しは `writer`、読み込み・検証は `reader` が受け持つ。旧形式（v1〜v4）は読まない。

pub(crate) mod container;
pub(crate) mod features;
pub mod meta;
pub(crate) mod reader;
pub(crate) mod records;
pub(crate) mod strtab;
#[cfg(all(test, feature = "build"))]
mod tests;
pub mod trie;
#[cfg(feature = "build")]
pub(crate) mod writer;

use std::fmt;
use std::io;

pub use meta::{Meta, PosScheme};
pub use reader::{Dictionary, VerifyReport};
#[cfg(feature = "build")]
pub use writer::{WriteOptions, WriteStats};

/// この hasami が読み書きする辞書の形式の版（リリースの目録の `format_version`）
pub const FORMAT_VERSION: u32 = container::VERSION;

/// 辞書の読み込み・書き出し・解析で起きるエラー
#[derive(Debug)]
pub enum DictError {
    /// ファイルの読み書きに失敗した
    Io(io::Error),
    /// hasami の辞書ファイルではない（先頭の magic が一致しない）
    NotHsd,
    /// 読めない版の辞書（v1〜v4 は作り直しを案内する）
    UnsupportedVersion(u32),
    /// ビッグエンディアン機では読めない
    UnsupportedPlatform,
    /// 形式に違反している（壊れたファイル）。解析中に見つけた不正な参照もこれで返す
    Corrupt(String),
    /// 辞書を作れない入力（空の表層形、範囲外の文脈 ID、除去済み辞書の再編集など）と、
    /// 読めない置き方のバイト列（8 バイト境界にない [`Dictionary::from_static`] の入力）
    Invalid(String),
    /// 既定の場所に辞書が無い（探した場所）。[`crate::Analyzer::load_default`] が返す
    NotFound(Vec<String>),
}

impl DictError {
    pub(crate) fn corrupt(message: impl Into<String>) -> Self {
        DictError::Corrupt(message.into())
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        DictError::Invalid(message.into())
    }
}

impl fmt::Display for DictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DictError::Io(e) => write!(f, "{e}"),
            DictError::NotHsd => write!(f, "not a hasami dictionary (.hsd): magic bytes mismatch"),
            DictError::UnsupportedVersion(v @ 1..=4) => write!(
                f,
                "dictionary format v{v} is no longer supported; rebuild it with scripts/build-dict.sh \
                 (or `hasami build`) to get the v{} format",
                container::VERSION
            ),
            DictError::UnsupportedVersion(v) => write!(
                f,
                "unsupported dictionary format version: {v} (this hasami reads v{})",
                container::VERSION
            ),
            DictError::UnsupportedPlatform => {
                write!(
                    f,
                    "big-endian machines are not supported by the .hsd format"
                )
            }
            DictError::Corrupt(m) => write!(f, "corrupt dictionary: {m}"),
            DictError::Invalid(m) => write!(f, "{m}"),
            DictError::NotFound(searched) => write!(
                f,
                "no dictionary found (searched: {}); run `hasami dict download` to put the recommended \
                 dictionary in the data directory, or set {} to a .hsd file",
                searched.join(", "),
                crate::analyzer::DICT_ENV
            ),
        }
    }
}

impl std::error::Error for DictError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DictError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for DictError {
    fn from(e: io::Error) -> Self {
        DictError::Io(e)
    }
}

impl From<trie::TrieError> for DictError {
    fn from(e: trie::TrieError) -> Self {
        match e {
            trie::TrieError::Build(m) => DictError::Invalid(m),
            trie::TrieError::Corrupt(m) => DictError::Corrupt(format!("trie: {m}")),
        }
    }
}

/// `io::Result` を返す既存の API から `?` で使えるようにする
impl From<DictError> for io::Error {
    fn from(e: DictError) -> Self {
        match e {
            DictError::Io(e) => e,
            DictError::Invalid(m) => io::Error::new(io::ErrorKind::InvalidInput, m),
            e @ DictError::NotFound(_) => io::Error::new(io::ErrorKind::NotFound, e.to_string()),
            other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
        }
    }
}
