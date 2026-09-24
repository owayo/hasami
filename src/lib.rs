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

/// 辞書（.hsd）を実行ファイルに埋め込み、境界をそろえた `&'static [u8]` を返す
///
/// [`Dictionary::from_static`] にそのまま渡せる（複製せずに読む）。パスは `include_bytes!` と同じく、
/// 呼び出したファイルからの相対パス（`concat!(env!("CARGO_MANIFEST_DIR"), "/dict/ipadic.hsd")` も書ける）。
///
/// `from_static` が求めるのは 8 バイト境界だが、64 バイト境界（キャッシュライン）にそろえる。セクションは
/// ファイルの先頭から 64 の倍数の位置にあるので、mmap した辞書と同じくセクションもキャッシュラインの
/// 境界に乗る。
///
/// 呼び出すたびに別の静的領域になる（同じファイルを 2 か所で埋め込むと実行ファイルに 2 つ入る）。
/// 1 か所の `static` に置いて使い回す。
///
/// ```ignore
/// static IPADIC: &[u8] = hasami::include_hsd!("../dict/ipadic.hsd");
///
/// let dict = hasami::Dictionary::from_static(IPADIC)?;
/// ```
#[cfg(feature = "analyzer")]
#[macro_export]
macro_rules! include_hsd {
    ($path:expr $(,)?) => {{
        #[repr(C, align(64))]
        struct __HasamiAligned<T: ?::core::marker::Sized>(T);

        static __HASAMI_HSD: &__HasamiAligned<[u8]> =
            &__HasamiAligned(*::core::include_bytes!($path));
        &__HASAMI_HSD.0
    }};
}

#[cfg(all(test, feature = "analyzer"))]
mod tests {
    /// `static` の初期化式でも使える。パスはこのファイルからの相対パス
    static EMBEDDED: &[u8] = crate::include_hsd!("../Cargo.toml");

    #[test]
    fn include_hsd_embeds_the_file_on_a_64_byte_boundary() {
        let expected: &[u8] = include_bytes!("../Cargo.toml");
        let in_fn: &'static [u8] =
            crate::include_hsd!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),);
        for bytes in [EMBEDDED, in_fn] {
            assert_eq!(bytes, expected);
            assert_eq!(bytes.as_ptr().addr() % 64, 0);
        }
        // 境界はそろっているので、辞書でないことを知らせる
        assert!(matches!(
            crate::Dictionary::from_static(EMBEDDED),
            Err(crate::DictError::NotHsd)
        ));
    }
}
