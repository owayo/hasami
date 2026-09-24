//! 例外表の索引
//!
//! 語は 1 行 1 語で並べたテキスト（以下、語の列）に置いたまま、語の中の文末記号（錨）ごとに
//! 「語の列の上の文末記号のバイト位置」と「後半のリンク」だけを持つ。錨は、鍵（文末記号から語の
//! 先頭へ向かって読んだ字の列）の順、同じ鍵の中では後半（文末記号の後ろから語の末尾まで）の順に
//! 並べる。照合は、入力の文末記号から 1 字ずつ前へ進みながら、鍵がその字の列で始まる錨の範囲を
//! 二分探索で狭めていく（[`key_char`]）。
//!
//! 組み込みの例外表の索引は build.rs が作ってバイト列で埋め込み（[`Index::to_bytes`]）、利用者が
//! 加える語の索引は実行時に作る。build.rs からも `#[path]` で読み込むので、このモジュールは
//! [`super::chars`] と標準ライブラリ以外に依存しない。

use std::cmp::Ordering;

use super::chars::{ender_rank, is_sentence_ender};

/// 照合に使う語の文末記号の数の上限（組み込みの表の語は最多 9 個）。超える語は捨てる
pub const MAX_WORD_ENDERS: usize = 16;
/// 照合に使う語の文字数の上限（組み込みの表の語は最長 124 文字）。超える語は捨てる
pub const MAX_WORD_CHARS: usize = 256;

/// バイト列にした索引の先頭の印と形式の版
const MAGIC: &[u8; 4] = b"HSX3";

/// 照合に使える語か（文末記号を含み、上限を超えず、語の列の書式で書ける）
pub fn is_matchable(word: &str) -> bool {
    let mut chars = 0;
    let mut enders = 0;
    for c in word.chars() {
        chars += 1;
        enders += usize::from(is_sentence_ender(c));
        if chars > MAX_WORD_CHARS || enders > MAX_WORD_ENDERS || c == '\n' || c == '\r' {
            return false;
        }
    }
    enders > 0
}

/// 例外表の索引（錨ごとの文末記号の位置と後半のリンク、鍵の頭の 3 字ごとの錨の範囲）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    /// 錨の文末記号の、語の列の上のバイト位置（鍵・後半の順）
    pub enders: Vec<u32>,
    /// 同じ鍵の錨の中で、自分の後半の真の接頭辞になっている最長の後半を持つ錨の位置 + 1
    /// （0 はなし。後半が空の錨は数えない）
    pub links: Vec<u32>,
    /// 鍵の頭の 3 字（[`head_key`]）の昇順の列と、それぞれの錨の範囲の始まり（終わりは次の始まり）
    ///
    /// 照合の最初の 3 字（文末記号と直前の 2 字）は錨が多く、錨の列を二分探索すると語の列の
    /// ばらばらな位置を読むことになるので、連続した整数の列の二分探索で済ませる。
    pub head_keys: Vec<u64>,
    pub head_starts: Vec<u32>,
    /// 語の数
    pub words: u32,
}

/// 鍵の頭の 3 字（文末記号 `ender`、その直前の字 `c1`、さらに前の字 `c2`）の番号
///
/// 鍵がそこで終わっていれば（語の先頭まで読み終えていれば）None。錨の並び（文末記号、直前の字、
/// …の順。鍵が終わっているものが先）と同じ順になる。文末記号は [`ender_rank`] の 4 ビット、
/// 字は符号位置 + 1（終わりは 0）の 22 ビットずつで入れる。
#[inline]
pub fn head_key(ender: char, c1: Option<char>, c2: Option<char>) -> u64 {
    let code = |c: Option<char>| c.map_or(0, |c| u64::from(c) + 1);
    let rank = ender_rank(ender).expect("文末記号");
    (rank << 44) | (code(c1) << 22) | code(c2)
}

/// [`head_key`] の番号のうち、直前の字 `c1` がない（語が文末記号で始まる）ものか
#[inline]
pub fn head_has_no_prev(key: u64) -> bool {
    (key >> 22) & ((1 << 22) - 1) == 0
}

impl Index {
    /// 語の列から索引を作る
    ///
    /// `comments` が真なら、`#` で始まる行と空行を語として数えない（組み込みの表の書式）。
    /// 照合に使えない語（[`is_matchable`]）の行は読み飛ばす。語は字幅を畳み（[`super::chars::fold_width`]）、
    /// 重複を除いてあること。
    pub fn build(list: &str, comments: bool) -> Index {
        let mut enders: Vec<u32> = Vec::new();
        let mut words = 0u32;
        let mut line_start = 0;
        for line in list.split_inclusive('\n') {
            let word = line.trim_end_matches(['\n', '\r']);
            let start = line_start;
            line_start += line.len();
            if (comments && (word.is_empty() || word.starts_with('#'))) || !is_matchable(word) {
                continue;
            }
            words += 1;
            for (k, c) in word.char_indices() {
                if is_sentence_ender(c) {
                    enders.push(to_u32(start + k));
                }
            }
        }
        enders.sort_unstable_by(|&a, &b| compare_anchors(list, a, b));

        // 同じ鍵の錨は後半の昇順に並んでいる（空の後半が先頭）。後半の真の接頭辞になっている最長の
        // 後半へのリンクを、接頭辞の鎖を積みに持って求める
        let mut links = vec![0u32; enders.len()];
        let mut chain: Vec<usize> = Vec::new();
        for i in 0..enders.len() {
            if i == 0 || !same_key(list, enders[i - 1], enders[i]) {
                chain.clear();
            }
            let t = tail(list, enders[i] as usize);
            if t.is_empty() {
                continue;
            }
            while let Some(&top) = chain.last() {
                if t.starts_with(tail(list, enders[top] as usize)) {
                    break;
                }
                chain.pop();
            }
            links[i] = chain.last().map_or(0, |&j| to_u32(j + 1));
            chain.push(i);
        }

        let mut head_keys: Vec<u64> = Vec::new();
        let mut head_starts: Vec<u32> = Vec::new();
        for (i, &ender) in enders.iter().enumerate() {
            let mut key = key_chars(list, ender as usize);
            let (c0, c1) = (key.next().expect("文末記号"), key.next());
            let head = head_key(c0, c1, c1.and_then(|_| key.next()));
            if head_keys.last() != Some(&head) {
                head_keys.push(head);
                head_starts.push(to_u32(i));
            }
        }
        Index {
            enders,
            links,
            head_keys,
            head_starts,
            words,
        }
    }

    /// バイト列にする
    ///
    /// 先頭の印、語の数・錨の数・頭の数（u32 LE）、続けて文末記号の位置・リンク（u32 LE）、
    /// 頭の番号（u64 LE）・頭の範囲の始まり（u32 LE）。build.rs が組み込みの表の索引を書き出すのに
    /// 使う（ライブラリの中ではテストだけが使う）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.enders.len() * 8 + self.head_keys.len() * 12);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.words.to_le_bytes());
        out.extend_from_slice(&to_u32(self.enders.len()).to_le_bytes());
        out.extend_from_slice(&to_u32(self.head_keys.len()).to_le_bytes());
        for &e in &self.enders {
            out.extend_from_slice(&e.to_le_bytes());
        }
        for &l in &self.links {
            out.extend_from_slice(&l.to_le_bytes());
        }
        for &k in &self.head_keys {
            out.extend_from_slice(&k.to_le_bytes());
        }
        for &s in &self.head_starts {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }
}

/// [`Index::to_bytes`] のバイト列の各部分（数値は LE の列のまま）
pub struct IndexBytes<'a> {
    pub words: u32,
    pub enders: &'a [u8],
    pub links: &'a [u8],
    pub head_keys: &'a [u8],
    pub head_starts: &'a [u8],
}

/// [`Index::to_bytes`] のバイト列を各部分に分ける（形式が合わなければ None）
pub fn split_bytes(bytes: &[u8]) -> Option<IndexBytes<'_>> {
    let (magic, rest) = bytes.split_at_checked(4)?;
    if magic != MAGIC {
        return None;
    }
    fn read(rest: &[u8]) -> Option<(usize, &[u8])> {
        let (n, rest) = rest.split_at_checked(4)?;
        Some((u32::from_le_bytes(n.try_into().ok()?) as usize, rest))
    }
    let (words, rest) = read(rest)?;
    let (count, rest) = read(rest)?;
    let (heads, rest) = read(rest)?;
    let (enders, rest) = rest.split_at_checked(count.checked_mul(4)?)?;
    let (links, rest) = rest.split_at_checked(count.checked_mul(4)?)?;
    let (head_keys, head_starts) = rest.split_at_checked(heads.checked_mul(8)?)?;
    (head_starts.len() == heads * 4).then_some(IndexBytes {
        words: u32::try_from(words).ok()?,
        enders,
        links,
        head_keys,
        head_starts,
    })
}

/// 錨の鍵の `back` バイト目（文末記号の直前から語の先頭へ向かって数える）で終わる字
///
/// 鍵の文末記号より前の字のうち、すでに比べた字の語の列の上のバイト数が `back`。語の先頭まで
/// 比べ終わっていれば None（鍵がここで終わる）。
#[inline]
pub fn key_char(list: &str, ender: usize, back: usize) -> Option<char> {
    // 照合の内側で最も多く呼ばれるので、str を切り出さずにバイト列から直接復号する
    // （`end` は字の境界にあり、その前は正しい UTF-8）
    let bytes = list.as_bytes();
    let end = ender - back;
    let last = *bytes.get(end.checked_sub(1)?)?;
    if last < 0x80 {
        return (last != b'\n').then_some(last as char);
    }
    let mut start = end - 1;
    while bytes[start] & 0xC0 == 0x80 {
        start -= 1;
    }
    let tail = |i: usize| u32::from(bytes[i] & 0x3F);
    let lead = u32::from(bytes[start]);
    let code = match end - start {
        2 => ((lead & 0x1F) << 6) | tail(start + 1),
        3 => ((lead & 0x0F) << 12) | (tail(start + 1) << 6) | tail(start + 2),
        _ => {
            ((lead & 0x07) << 18)
                | (tail(start + 1) << 12)
                | (tail(start + 2) << 6)
                | tail(start + 3)
        }
    };
    char::from_u32(code)
}

/// 錨の後半（文末記号の直後から語の末尾まで）
#[inline]
pub fn tail(list: &str, ender: usize) -> &str {
    let rest = &list[ender..];
    let c = rest.chars().next().map_or(0, char::len_utf8);
    let rest = &rest[c..];
    let end = rest.find('\n').unwrap_or(rest.len());
    rest[..end].trim_end_matches('\r')
}

/// 2 つの錨の順序（鍵、次に後半）
fn compare_anchors(list: &str, a: u32, b: u32) -> Ordering {
    key_chars(list, a as usize)
        .cmp(key_chars(list, b as usize))
        .then_with(|| tail(list, a as usize).cmp(tail(list, b as usize)))
}

/// 2 つの錨の鍵が同じか
fn same_key(list: &str, a: u32, b: u32) -> bool {
    key_chars(list, a as usize).eq(key_chars(list, b as usize))
}

/// 錨の鍵（文末記号、その直前の字、…、語の先頭の字）
fn key_chars(list: &str, ender: usize) -> impl Iterator<Item = char> + '_ {
    let line_start = list[..ender].rfind('\n').map_or(0, |i| i + 1);
    let c = list[ender..].chars().next();
    c.into_iter().chain(list[line_start..ender].chars().rev())
}

/// 例外表の大きさを u32 に収める（例外表が 4 GiB を超えることはない）
fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("例外表が大きすぎる")
}
