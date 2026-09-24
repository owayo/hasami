//! hasami - 高速日本語形態素解析エンジン
//!
//! 文字単位 Double-Array Trie + ラティス + Viterbi による高精度・高速な形態素解析
//!
//! 辞書の要らない文分割（[`sentence`]）は feature なしで使える。形態素解析（`Analyzer`・`Dictionary`・
//! `Token`・品詞の正規化・C FFI）は `analyzer` feature、辞書の構築・修復・書き出し（`DictBuilder`）は
//! `build` feature（`analyzer` を含む）で入る。既定の `cli` はどちらも含む。

#[cfg(feature = "analyzer")]
pub mod analyzer;
#[cfg(feature = "analyzer")]
pub mod char_class;
#[cfg(feature = "analyzer")]
pub mod dict;
#[cfg(feature = "analyzer")]
pub mod ffi;
#[cfg(feature = "analyzer")]
pub mod hsd;
#[cfg(feature = "analyzer")]
pub mod lattice;
#[cfg(feature = "analyzer")]
pub mod pos;
pub mod sentence;

#[cfg(feature = "analyzer")]
pub use analyzer::Analyzer;
#[cfg(feature = "analyzer")]
pub use dict::DictEntry;
#[cfg(feature = "analyzer")]
pub use hsd::{DictError, Dictionary};
#[cfg(feature = "analyzer")]
pub use lattice::Token;
#[cfg(feature = "analyzer")]
pub use pos::CoarsePos;
