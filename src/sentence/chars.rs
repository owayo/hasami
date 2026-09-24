//! 文分割と例外表の索引で使う字の分類
//!
//! build.rs からも `#[path]` で読み込んで、組み込みの例外表の索引を作るのに使う。そのため
//! このモジュールは標準ライブラリ以外に依存しない。

/// 文末記号か（規則 1 の `。！？!?‼⁇⁈⁉．｡`）
///
/// 字だけで決まる集合で、前後の字による判定（規則 4 の ASCII の `!` `?`、規則 9 の数字に挟まれた
/// `．` `｡`）は含まない。ASCII の `!` `?` の連続が文末として働くかは
/// [`ascii_run_is_ender`](super::ascii_run_is_ender) で判定する。
#[inline]
pub fn is_sentence_ender(c: char) -> bool {
    matches!(
        c,
        '。' | '！' | '？' | '!' | '?' | '‼' | '⁇' | '⁈' | '⁉' | '．' | '｡'
    )
}

/// 畳んだ文末記号（[`fold_width`] の後）の、符号位置の順を保った番号（文末記号でなければ None）
///
/// 例外表の索引の頭の番号（[`super::index::head_key`]）に文末記号を 4 ビットで入れるのに使う。
#[inline]
pub fn ender_rank(c: char) -> Option<u64> {
    Some(match c {
        '!' => 0,
        '?' => 1,
        '‼' => 2,
        '⁇' => 3,
        '⁈' => 4,
        '⁉' => 5,
        '。' => 6,
        '．' => 7,
        '｡' => 8,
        _ => return None,
    })
}

/// 例外語の照合のために、全角の英数字・記号（U+FF01〜U+FF5E）を半角に畳む
///
/// NFKC のうち字幅の部分だけで、大文字・小文字は畳まない。全角ピリオド `．` は畳むと文末記号で
/// なくなる（`.` は文末記号でない）ので畳まない。
#[inline]
pub fn fold_width(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' if c != '．' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        _ => c,
    }
}

/// 例外語の左の境界を決める字種
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Hiragana,
    Katakana,
    Kanji,
    /// 英字・数字（全角は畳んでから分類する）
    Alphanumeric,
    Other,
}

/// 字種（全角の英数字も英数字に分類する）
pub fn script(c: char) -> Script {
    let c = fold_width(c);
    match c {
        c if c.is_ascii_alphanumeric() => Script::Alphanumeric,
        '\u{3041}'..='\u{3096}' | '\u{309D}'..='\u{309F}' => Script::Hiragana,
        // 中黒 `・`（U+30FB）は区切りなので含めない。長音符 `ー` と半角カタカナは含める
        '\u{30A1}'..='\u{30FA}' | '\u{30FC}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' => {
            Script::Katakana
        }
        '\u{FF66}'..='\u{FF9F}' => Script::Katakana,
        '々' | '〆' | '〇' => Script::Kanji,
        '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{20000}'..='\u{3FFFF}' => Script::Kanji,
        _ => Script::Other,
    }
}
