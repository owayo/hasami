//! 文末記号を含む語の例外表と、その照合
//!
//! 「モーニング娘。」「Yahoo!ニュース」のように表層に文末記号を含む語で文を切らないよう、
//! 語の一覧を持ち、入力の文末記号を覆う出現があるかを調べる。
//!
//! - 組み込みの表（`builtin_exceptions.txt`）は、推奨辞書の表層形を [`extract_candidates`] で
//!   絞ったもの。生成手順はファイルの先頭のコメントにある
//! - 照合は文末記号を起点にする。語の中の文末記号ごとに「文末記号とその前」を逆順にした鍵の
//!   トライと、鍵ごとの「文末記号の後ろ」の一覧を持ち、入力の文末記号から前へ・後ろへと調べる。
//!   文末記号から離れた文字は読まないので、入力の大部分を占める文末記号のない区間には手間が
//!   かからない（計算量は [`Matcher`] を参照）

use std::sync::LazyLock;

use super::{ascii_run_is_ender, is_sentence_ender};

/// 組み込みの例外表（1 行 1 語。`#` で始まる行と空行は読み飛ばす）
const BUILTIN_EXCEPTIONS: &str = include_str!("builtin_exceptions.txt");

/// 組み込みの例外表の照合器（初めて使うときに 1 度だけ組み立てる）
static BUILTIN_MATCHER: LazyLock<Matcher> = LazyLock::new(|| Matcher::new(builtin_exceptions()));

/// 組み込みの例外表の語を、表に書かれた順（バイト順）に返す
pub fn builtin_exceptions() -> impl Iterator<Item = &'static str> {
    BUILTIN_EXCEPTIONS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// 組み込みの例外表の照合器
pub(crate) fn builtin_matcher() -> &'static Matcher {
    &BUILTIN_MATCHER
}

/// 辞書の表層形の一覧から、例外表に載せる語を抽出する
///
/// 文末記号（`。！？!?‼⁇⁈⁉．｡`）を含む語だけを残し、次の語を除く。
/// 結果は重複を除いてバイト順（符号位置の順）に並べる。
///
/// - 記号だけの語（英数字・かな・漢字などの文字を 1 字も含まない）
/// - 2 文字未満の語
/// - 文末記号で始まる語
/// - 表の書式（1 行 1 語、`#` で始まる行はコメント）で書けない語: 制御文字（改行を含む）を含む語、
///   `#` で始まる語、前後に空白がある語
pub fn extract_candidates<'a>(surfaces: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut words: Vec<String> = surfaces
        .into_iter()
        .filter(|w| is_candidate(w))
        .map(str::to_owned)
        .collect();
    words.sort_unstable();
    words.dedup();
    words
}

/// [`extract_candidates`] の規則で残す語か
fn is_candidate(word: &str) -> bool {
    let mut chars = word.chars();
    let (Some(first), Some(_)) = (chars.next(), chars.next()) else {
        // 2 文字未満
        return false;
    };
    !is_sentence_ender(first)
        && first != '#'
        && word.trim() == word
        && word.chars().any(is_sentence_ender)
        && word.chars().any(char::is_alphanumeric)
        && !word.chars().any(char::is_control)
}

/// 例外語の末尾の文末記号の直後に来たとき、その文末記号を語の一部とみなす助詞か
#[inline]
fn is_particle(c: char) -> bool {
    matches!(
        c,
        'の' | 'は' | 'が' | 'を' | 'に' | 'と' | 'で' | 'も' | 'や' | 'へ'
    )
}

/// トライの根の節点
const ROOT: u32 = 0;
/// `back_info` の最上位ビット: 文末記号が語の最後の文字になる組がこの鍵で終わる
const END: u32 = 1 << 31;

/// 例外語の照合器
///
/// 語の中の文末記号 1 つごとに、語を「文末記号とその前」と「文末記号の後ろ（後半）」に分けた組を
/// 作る。前者を逆順にした鍵（文末記号、その直前の文字、…）のトライを入力の文末記号から前へ辿り、
/// 鍵が終わる節点ごとに、その鍵の後半のどれかが文末記号の直後に続くかを調べる。
///
/// - 後半が空でない組が一致した: 文末記号は語の内側にある
/// - 後半が空の組が一致した: 文末記号は語の最後の文字（直後が助詞なら語の一部とみなす）
///
/// 後半は鍵ごとにバイト順に並べ、同じ組の中で自分の接頭辞になっている最長の後半への鎖を持つ。
///
/// 計算量: 前へ辿る照合が入力の文字の上を通るのは、その文字より後ろにある文末記号のうち
/// 1 語の中の文末記号の数（組み込みの表では最多 9 個）までなので、前へ辿る手間の合計は入力長に
/// 比例する。後半の照合は 1 回あたり二分探索と後半の長さで抑えられる。利用者が加える語で
/// この定数が膨らまないよう、[`MAX_WORD_ENDERS`] と [`MAX_WORD_CHARS`] を超える語は捨てる。
#[derive(Clone)]
pub(crate) struct Matcher {
    /// 鍵のトライ
    back: Trie,
    /// 節点ごとの情報。最上位ビットは END、残りはその鍵の後半の組の番号 + 1（0 はなし）
    back_info: Box<[u32]>,
    /// 鍵ごとの後半の組（`tail_ranges` 上の範囲）
    groups: Box<[(u32, u32)]>,
    /// 後半の `tails` 上のバイト範囲（組ごとにバイト順）
    tail_ranges: Box<[(u32, u32)]>,
    /// 後半ごとの、同じ組の中で自分の真の接頭辞になっている最長の後半の `tail_ranges` 上の
    /// 位置 + 1（0 はなし）
    tail_links: Box<[u32]>,
    /// 後半をつないだ文字列
    tails: Box<str>,
    /// 語の数
    words: usize,
}

/// 照合に使う語の文末記号の数の上限（組み込みの表の語は最多 9 個）。超える語は捨てる
pub(crate) const MAX_WORD_ENDERS: usize = 16;
/// 照合に使う語の文字数の上限（組み込みの表の語は最長 124 文字）。超える語は捨てる
pub(crate) const MAX_WORD_CHARS: usize = 256;

/// 照合に使える語か（文末記号を含み、上限を超えない）
fn is_matchable(word: &str) -> bool {
    let mut chars = 0;
    let mut enders = 0;
    for c in word.chars() {
        chars += 1;
        enders += usize::from(is_sentence_ender(c));
        if chars > MAX_WORD_CHARS || enders > MAX_WORD_ENDERS {
            return false;
        }
    }
    enders > 0
}

impl Matcher {
    /// 語の一覧から照合器を組み立てる
    ///
    /// 文末記号を含まない語は分割に影響しないので捨てる。文末記号が [`MAX_WORD_ENDERS`] 個を
    /// 超える語と、[`MAX_WORD_CHARS`] 文字を超える語も捨てる。重複は 1 つにまとめる。
    pub(crate) fn new<'a>(words: impl IntoIterator<Item = &'a str>) -> Self {
        let mut words: Vec<&str> = words.into_iter().filter(|w| is_matchable(w)).collect();
        if !words.is_sorted() {
            words.sort_unstable();
        }
        words.dedup();

        // 語の中の文末記号ごとの組。鍵（文末記号とその前を逆順にした文字列）は組ごとに確保せず、
        // 1 つの文字列に並べてバイト範囲で指す。UTF-8 のバイト順は符号位置の順と同じなので、
        // 鍵はバイト列のまま比べて並べる
        let mut keys = String::with_capacity(words.iter().map(|w| w.len()).sum());
        let mut anchors: Vec<Anchor<'_>> = Vec::with_capacity(words.len());
        for word in &words {
            for (k, c) in word.char_indices() {
                if !is_sentence_ender(c) || !can_be_active(word, k, c) {
                    continue;
                }
                let after = k + c.len_utf8();
                let start = keys.len();
                keys.extend(word[..after].chars().rev());
                let mut prefix = [0u8; 8];
                let head = &keys.as_bytes()[start..keys.len().min(start + 8)];
                prefix[..head.len()].copy_from_slice(head);
                anchors.push(Anchor {
                    prefix: u64::from_be_bytes(prefix),
                    start: to_u32(start),
                    end: to_u32(keys.len()),
                    tail: &word[after..],
                });
            }
        }
        let key = |a: &Anchor<'_>| &keys[a.start as usize..a.end as usize];
        // 鍵の先頭 8 バイトで大半の比較が決まる
        anchors.sort_unstable_by(|a, b| {
            a.prefix
                .cmp(&b.prefix)
                .then_with(|| key(a).cmp(key(b)))
                .then_with(|| a.tail.cmp(b.tail))
        });
        anchors.dedup_by(|a, b| a.prefix == b.prefix && key(a) == key(b) && a.tail == b.tail);

        let mut back = TrieBuilder::with_capacity(keys.len() / 2);
        let mut back_info: Vec<(u32, u32)> = Vec::new(); // (節点, 情報)
        let mut groups: Vec<(u32, u32)> = Vec::new();
        let mut tail_ranges: Vec<(u32, u32)> = Vec::new();
        let mut tail_links: Vec<u32> = Vec::new();
        let mut tails = String::new();
        let mut chain: Vec<usize> = Vec::new();
        let mut i = 0;
        while let Some(anchor) = anchors.get(i) {
            let this_key = key(anchor);
            let node = back.insert(this_key);
            let mut info = 0u32;
            let group_start = tail_ranges.len();
            // 同じ鍵の組は後半の昇順に並んでいる（空の後半が先頭）
            while let Some(tail) = anchors
                .get(i)
                .filter(|a| a.prefix == anchor.prefix && key(a) == this_key)
                .map(|a| a.tail)
            {
                if tail.is_empty() {
                    info |= END;
                } else {
                    let start = tails.len();
                    tails.push_str(tail);
                    tail_ranges.push((to_u32(start), to_u32(tails.len())));
                }
                i += 1;
            }
            if tail_ranges.len() > group_start {
                // 各後半の真の接頭辞になっている最長の後半を求める。後半は昇順なので、直前までの
                // 接頭辞の鎖を積みに持ち、接頭辞でなくなったものを降ろせばよい
                chain.clear();
                for j in group_start..tail_ranges.len() {
                    let (start, end) = tail_ranges[j];
                    let t = &tails.as_bytes()[start as usize..end as usize];
                    while let Some(&top) = chain.last() {
                        let (s, e) = tail_ranges[top];
                        if t.starts_with(&tails.as_bytes()[s as usize..e as usize]) {
                            break;
                        }
                        chain.pop();
                    }
                    tail_links.push(chain.last().map_or(0, |&k| to_u32(k + 1)));
                    chain.push(j);
                }
                groups.push((to_u32(group_start), to_u32(tail_ranges.len())));
                info |= to_u32(groups.len());
                debug_assert!(groups.len() < END as usize);
            }
            back_info.push((node, info));
        }

        let back = back.finish();
        let mut info = vec![0u32; back.nodes()];
        for (node, value) in back_info {
            info[node as usize] = value;
        }
        Matcher {
            back,
            back_info: info.into_boxed_slice(),
            groups: groups.into_boxed_slice(),
            tail_ranges: tail_ranges.into_boxed_slice(),
            tail_links: tail_links.into_boxed_slice(),
            tails: tails.into_boxed_str(),
            words: words.len(),
        }
    }

    /// 語の数
    pub(crate) fn len(&self) -> usize {
        self.words
    }

    /// 鍵のトライの節点の数
    #[cfg(test)]
    fn nodes(&self) -> usize {
        self.back.nodes()
    }

    /// `text` の位置 `pos` にある文末記号 `c` で分割してはいけないか
    ///
    /// 語の出現が文末記号を内側に含むか、文末記号で終わる語の出現があって直後が助詞で始まるなら真。
    pub(crate) fn guards(&self, text: &str, pos: usize, c: char) -> bool {
        let Some(mut node) = self.back.child(ROOT, c) else {
            return false;
        };
        let after = pos + c.len_utf8();
        let mut ends_here = false;
        let mut before = text[..pos].chars();
        loop {
            let info = self.back_info[node as usize];
            ends_here |= info & END != 0;
            let group = info & !END;
            if group != 0 && self.tail_follows(group - 1, &text.as_bytes()[after..]) {
                return true;
            }
            let Some(prev) = before.next_back() else {
                break;
            };
            let Some(next) = self.back.child(node, prev) else {
                break;
            };
            node = next;
        }
        ends_here && text[after..].chars().next().is_some_and(is_particle)
    }

    /// 後半の組 `group` のどれかが `text` の先頭に一致するか
    ///
    /// 後半はバイト順に並んでいる。`text` の接頭辞になっている後半 P があれば、`text` 以下で最大の
    /// 後半 t（二分探索で求める）は P と `text` の間に並ぶので P で始まり、P は t と `text` の共通
    /// 接頭辞に収まる。t の接頭辞になっている後半は、t から「真の接頭辞になっている最長の後半」の
    /// 鎖を辿ると長い順にすべて現れるので、共通接頭辞に収まる最初のものを探せばよい。
    /// 手間は二分探索と、後半の長さ以下の鎖の長さで抑えられる。
    fn tail_follows(&self, group: u32, text: &[u8]) -> bool {
        let (lo, hi) = self.groups[group as usize];
        let bytes = |(start, end): (u32, u32)| &self.tails.as_bytes()[start as usize..end as usize];
        let tail = |i: usize| bytes(self.tail_ranges[i]);
        let n = self.tail_ranges[lo as usize..hi as usize].partition_point(|&r| bytes(r) <= text);
        let Some(mut j) = n.checked_sub(1).map(|n| lo as usize + n) else {
            return false;
        };
        let common = tail(j).iter().zip(text).take_while(|(a, b)| a == b).count();
        loop {
            if tail(j).len() <= common {
                return true;
            }
            match self.tail_links[j] {
                0 => return false,
                link => j = link as usize - 1,
            }
        }
    }
}

/// 照合に使う組（語の中の文末記号 1 つ）
struct Anchor<'a> {
    /// 鍵の先頭 8 バイト（足りなければ 0 で埋める）をビッグエンディアンで読んだ値。
    /// 鍵の大小と矛盾しないので、並べ替えの比較を先にこれで済ませる
    prefix: u64,
    /// 鍵（文末記号とその前を逆順にした文字列）の、鍵を並べた文字列の上のバイト範囲
    start: u32,
    end: u32,
    /// 文末記号より後ろの部分
    tail: &'a str,
}

/// 語の `k` バイト目の文末記号 `c` が、語の出現の中で文末として働きうるか
///
/// ASCII の `!` `?` の連続が語の中で英数字・ASCII 記号に続くなら、入力の中でも同じ文字が続くので
/// 文末として働かない（規則 4）。そうした文末記号は照合で問われないので組を作らない。
fn can_be_active(word: &str, k: usize, c: char) -> bool {
    if c != '!' && c != '?' {
        return true;
    }
    let run_end = word[k..]
        .find(|ch: char| ch != '!' && ch != '?')
        .map_or(word.len(), |n| k + n);
    run_end == word.len() || ascii_run_is_ender(word[run_end..].chars().next())
}

/// 例外表の大きさを u32 に収める（例外表が 4 GiB を超えることはない）
fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("例外表が大きすぎる")
}

/// 子の数がこれを超える節点の子は、ハッシュ表で引く
const HASHED_DEGREE: usize = 8;
/// ハッシュ表の空きを表す値（節点 < 2^32 - 1、文字 <= U+10FFFF なので鍵と重ならない）
const EMPTY: u64 = u64::MAX;

/// 文字で子を引くトライ
///
/// 子は節点ごとに連続して並べ、子の少ない節点は並びをそのまま探す。子の多い節点（文末記号の
/// 直前の文字で分かれる浅い節点など）は、(節点, 文字) を鍵にした開番地法のハッシュ表で引く。
#[derive(Clone)]
struct Trie {
    /// 節点 `n` の子は `edge_char[edge_start[n]..edge_start[n + 1]]` と、同じ位置の `edge_target`
    edge_start: Box<[u32]>,
    edge_char: Box<[char]>,
    edge_target: Box<[u32]>,
    /// 子の多い節点の子のハッシュ表の鍵（`節点 << 32 | 文字`。空きは EMPTY）と行き先
    hash_keys: Box<[u64]>,
    hash_targets: Box<[u32]>,
    /// ハッシュ値を表の大きさに縮めるシフト量（64 - 表の大きさの log2）
    hash_shift: u32,
}

impl Trie {
    /// 節点の数
    fn nodes(&self) -> usize {
        self.edge_start.len() - 1
    }

    /// 節点 `node` から文字 `c` で辿った子
    #[inline]
    fn child(&self, node: u32, c: char) -> Option<u32> {
        let lo = self.edge_start[node as usize] as usize;
        let hi = self.edge_start[node as usize + 1] as usize;
        if hi - lo <= HASHED_DEGREE {
            let found = self.edge_char[lo..hi].iter().position(|&x| x == c);
            return found.map(|i| self.edge_target[lo + i]);
        }
        let key = u64::from(node) << 32 | u64::from(c);
        let mask = self.hash_keys.len() - 1;
        let mut i = hash_slot(key, self.hash_shift);
        loop {
            let k = self.hash_keys[i];
            if k == key {
                return Some(self.hash_targets[i]);
            }
            if k == EMPTY {
                return None;
            }
            i = (i + 1) & mask;
        }
    }
}

/// ハッシュ表の位置（乗算で混ぜた上位ビットを使う）
#[inline]
fn hash_slot(key: u64, shift: u32) -> usize {
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> shift) as usize
}

/// 昇順に並べた鍵からトライを作る
///
/// 直前の鍵との共通接頭辞までを使い回し、残りの文字の節点を作る。鍵を昇順に加えるので、
/// 同じ親の子は文字の昇順に作られる。
struct TrieBuilder {
    /// (親, 文字, 子)
    edges: Vec<(u32, char, u32)>,
    /// 節点の数
    nodes: u32,
    /// 直前に加えた鍵の節点の列（先頭は根）
    path: Vec<u32>,
    /// 直前に加えた鍵
    prev: String,
}

impl TrieBuilder {
    /// 根だけのトライを作り始める（`edges` は見込みの辺の数）
    fn with_capacity(edges: usize) -> Self {
        TrieBuilder {
            edges: Vec::with_capacity(edges),
            nodes: 1,
            path: vec![ROOT],
            prev: String::new(),
        }
    }

    /// 鍵を加え、鍵の終わりの節点を返す（鍵は昇順に加える）
    fn insert(&mut self, key: &str) -> u32 {
        debug_assert!(key >= self.prev.as_str(), "鍵が昇順でない");
        let common = self
            .prev
            .chars()
            .zip(key.chars())
            .take_while(|(a, b)| a == b)
            .count();
        self.path.truncate(common + 1);
        let mut node = self.path[common];
        for c in key.chars().skip(common) {
            let child = self.nodes;
            self.nodes = child.checked_add(1).expect("例外表が大きすぎる");
            self.edges.push((node, c, child));
            self.path.push(child);
            node = child;
        }
        self.prev.clear();
        self.prev.push_str(key);
        node
    }

    /// 子を親ごとにまとめ（数え上げソート）、子の多い節点の子をハッシュ表に入れたトライにする
    fn finish(self) -> Trie {
        let nodes = self.nodes as usize;
        let mut edge_start = vec![0u32; nodes + 1];
        for &(parent, _, _) in &self.edges {
            edge_start[parent as usize + 1] += 1;
        }
        for i in 0..nodes {
            edge_start[i + 1] += edge_start[i];
        }
        let mut cursor = edge_start.clone();
        let mut edge_char = vec!['\0'; self.edges.len()];
        let mut edge_target = vec![0u32; self.edges.len()];
        for &(parent, c, child) in &self.edges {
            let i = cursor[parent as usize] as usize;
            edge_char[i] = c;
            edge_target[i] = child;
            cursor[parent as usize] += 1;
        }

        // ハッシュ表の大きさは入れる子の数の 2 倍以上の 2 のべき（埋まり具合は半分以下）
        let degree = |n: usize| (edge_start[n + 1] - edge_start[n]) as usize;
        let hashed: usize = (0..nodes).map(degree).filter(|&d| d > HASHED_DEGREE).sum();
        let bits = (hashed * 2).max(2).next_power_of_two().trailing_zeros();
        let mut hash_keys = vec![EMPTY; 1 << bits];
        let mut hash_targets = vec![0u32; 1 << bits];
        let mask = hash_keys.len() - 1;
        for n in (0..nodes).filter(|&n| degree(n) > HASHED_DEGREE) {
            for e in edge_start[n] as usize..edge_start[n + 1] as usize {
                let key = (n as u64) << 32 | u64::from(edge_char[e]);
                let mut i = hash_slot(key, 64 - bits);
                while hash_keys[i] != EMPTY {
                    i = (i + 1) & mask;
                }
                hash_keys[i] = key;
                hash_targets[i] = edge_target[e];
            }
        }
        Trie {
            edge_start: edge_start.into_boxed_slice(),
            edge_char: edge_char.into_boxed_slice(),
            edge_target: edge_target.into_boxed_slice(),
            hash_keys: hash_keys.into_boxed_slice(),
            hash_targets: hash_targets.into_boxed_slice(),
            hash_shift: 64 - bits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 語の出現をすべて調べて、位置 `pos` の文末記号で分割してはいけないかを総当たりで求める
    fn guards_naive(words: &[&str], text: &str, pos: usize) -> bool {
        let c = text[pos..].chars().next().unwrap();
        let after = pos + c.len_utf8();
        let mut ends_here = false;
        for word in words.iter().filter(|w| w.chars().any(is_sentence_ender)) {
            // 重なり合う出現もすべて数える
            let starts = text
                .char_indices()
                .map(|(i, _)| i)
                .filter(|&i| text[i..].starts_with(word));
            for start in starts {
                let end = start + word.len();
                if start <= pos && after < end {
                    return true;
                }
                if after == end {
                    ends_here = true;
                }
            }
        }
        ends_here && text[after..].chars().next().is_some_and(is_particle)
    }

    /// 規則 4 で文末として働く文末記号の位置
    fn active_enders(text: &str) -> Vec<(usize, char)> {
        text.char_indices()
            .filter(|&(pos, c)| is_sentence_ender(c) && can_be_active(text, pos, c))
            .collect()
    }

    #[test]
    fn test_extract_candidates_keeps_words_with_enders() {
        let words = extract_candidates([
            "モーニング娘。",
            "Yahoo!",
            "Yahoo!ニュース",
            "Hey!Say!JUMP",
            "けいおん!",
            "ご注文はうさぎですか?",
            "猫",
            "東京都",
        ]);
        assert_eq!(
            words,
            vec![
                "Hey!Say!JUMP",
                "Yahoo!",
                "Yahoo!ニュース",
                "けいおん!",
                "ご注文はうさぎですか?",
                "モーニング娘。",
            ]
        );
    }

    #[test]
    fn test_extract_candidates_excludes_symbols_short_and_leading_enders() {
        let words = extract_candidates([
            "!?",         // 記号だけ
            "(^^)!",      // 記号だけ
            "…！",        // 記号だけ
            "娘",         // 文末記号を含まない
            "!",          // 2 文字未満
            "！ニュース", // 文末記号で始まる
            "。はい",     // 文末記号で始まる
            "#タグ!",     // 表の書式で書けない
            " Yahoo!",    // 表の書式で書けない
            "改\n行!",    // 表の書式で書けない
            "〇〇!",      // 漢数字は英数字として数える
            "ｵｯｹｰ｡",      // 半角の句点も文末記号
        ]);
        assert_eq!(words, vec!["〇〇!", "ｵｯｹｰ｡"]);
    }

    #[test]
    fn test_extract_candidates_dedups_and_sorts() {
        let words = extract_candidates(["b!", "a!", "b!", "a!"]);
        assert_eq!(words, vec!["a!", "b!"]);
    }

    #[test]
    fn test_builtin_exceptions_follow_extraction_rules() {
        let words: Vec<&str> = builtin_exceptions().collect();
        assert!(!words.is_empty());
        // 組み込みの表は extract_candidates の出力そのもの（規則を満たし、重複がなく、並んでいる）
        assert_eq!(extract_candidates(words.iter().copied()), words);
    }

    #[test]
    fn test_builtin_exceptions_contain_known_words() {
        let words: Vec<&str> = builtin_exceptions().collect();
        for word in [
            "モーニング娘。",
            "Yahoo!",
            "Yahoo!ニュース",
            "Hey!Say!JUMP",
            "けいおん!",
            "ご注文はうさぎですか?",
            "やはり俺の青春ラブコメはまちがっている。",
        ] {
            assert!(words.binary_search(&word).is_ok(), "{word} がない");
        }
    }

    #[test]
    fn test_guards_inside_and_at_end_of_words() {
        let words = ["Yahoo!", "Yahoo!ニュース", "モーニング娘。", "けいおん!!"];
        let matcher = Matcher::new(words);
        assert_eq!(matcher.len(), 4);
        let cases = [
            ("Yahoo!ニュースを見た", "Yahoo".len(), true), // 語の内側
            ("Yahoo!の株価", "Yahoo".len(), true),         // 語の末尾 + 助詞
            ("Yahoo!次", "Yahoo".len(), false),            // 語の末尾 + 助詞以外
            ("Yahoo!", "Yahoo".len(), false),              // 語の末尾 + 入力の終わり
            ("モーニング娘。のライブ", "モーニング娘".len(), true),
            ("モーニング娘。次", "モーニング娘".len(), false),
            ("娘。のライブ", "娘".len(), false), // 語の途中から始まる
            ("けいおん!!次", "けいおん".len(), true), // 1 つ目は内側
            ("けいおん!!次", "けいおん!".len(), false), // 2 つ目は末尾
            ("けいおん!!の話", "けいおん!".len(), true),
        ];
        for (text, pos, expected) in cases {
            let c = text[pos..].chars().next().unwrap();
            assert_eq!(matcher.guards(text, pos, c), expected, "{text} @ {pos}");
            assert_eq!(guards_naive(&words, text, pos), expected, "{text} @ {pos}");
        }
    }

    #[test]
    fn test_matcher_ignores_words_without_enders() {
        let matcher = Matcher::new(["猫", "", "犬!"]);
        assert_eq!(matcher.len(), 1);
        assert!(matcher.guards("犬!の", "犬".len(), '!'));
        assert!(!matcher.guards("猫!の", "猫".len(), '!'));
    }

    #[test]
    fn test_empty_matcher_never_guards() {
        let matcher = Matcher::new([]);
        assert_eq!(matcher.len(), 0);
        assert!(!matcher.guards("Yahoo!の", 5, '!'));
    }

    #[test]
    fn test_matcher_agrees_with_naive_search_on_random_input() {
        // 小さな字母で語と入力を作り、語が入れ子・部分的に重なる状況で総当たりと突き合わせる
        let alphabet = ['a', 'b', '!', '?', '。', 'の', 'x'];
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for _ in 0..2000 {
            let words: Vec<String> = (0..next(8) + 1)
                .map(|_| {
                    (0..next(5) + 1)
                        .map(|_| alphabet[next(alphabet.len())])
                        .collect()
                })
                .collect();
            let text: String = (0..next(40))
                .map(|_| alphabet[next(alphabet.len())])
                .collect();
            let refs: Vec<&str> = words.iter().map(String::as_str).collect();
            let matcher = Matcher::new(refs.iter().copied());
            for (pos, c) in active_enders(&text) {
                assert_eq!(
                    matcher.guards(&text, pos, c),
                    guards_naive(&refs, &text, pos),
                    "words={refs:?} text={text} pos={pos}"
                );
            }
        }
    }

    #[test]
    fn test_many_tails_after_the_same_key() {
        // 同じ「それいけ!」に続く後半が多く、互いに接頭辞を共有する
        let tails = [
            "ア",
            "アン",
            "アンパン",
            "アンパンマン",
            "アンパンマンの歌",
            "アイ",
            "カ",
            "カレー",
            "ン",
            "アンコ",
            "アンパ",
            "アンパンマン2",
        ];
        let words: Vec<String> = tails.iter().map(|t| format!("それいけ!{t}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let matcher = Matcher::new(refs.iter().copied());
        let pos = "それいけ".len();
        for after in [
            "アンパンマンの歌を歌う",
            "アンパンマンを見る",
            "アンパを",
            "アンパ",
            "アンダー",
            "アウト",
            "カレーパン",
            "キャラ",
            "ンン",
            "",
            "Z",
        ] {
            let text = format!("それいけ!{after}");
            assert_eq!(
                matcher.guards(&text, pos, '!'),
                guards_naive(&refs, &text, pos),
                "{text}"
            );
        }
    }

    #[test]
    fn test_tails_sharing_long_prefixes_are_checked_quickly() {
        // 後半 "a", "za", "zza", … と続き "zzz…" の組み合わせは、共通接頭辞を 1 文字ずつ
        // 縮める探し方だと二乗の手間になる。接頭辞の鎖で辿れば一度の二分探索で済む
        let m = 200;
        let words: Vec<String> = (0..m).map(|k| format!("X。{}a", "z".repeat(k))).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let matcher = Matcher::new(refs.iter().copied());
        for tail in [
            "z".repeat(m),
            format!("{}a", "z".repeat(m / 2)),
            "zz".to_string(),
            "a".to_string(),
        ] {
            let text = format!("X。{tail}");
            let pos = "X".len();
            assert_eq!(
                matcher.guards(&text, pos, '。'),
                guards_naive(&refs, &text, pos),
                "{text}"
            );
        }
    }

    #[test]
    fn test_words_over_the_limits_are_ignored() {
        let enders_ok = format!("語{}", "!".repeat(MAX_WORD_ENDERS));
        let enders_over = format!("語{}", "!".repeat(MAX_WORD_ENDERS + 1));
        let chars_ok = format!("{}!", "あ".repeat(MAX_WORD_CHARS - 1));
        let chars_over = format!("{}!", "あ".repeat(MAX_WORD_CHARS));
        assert!(is_matchable(&enders_ok));
        assert!(!is_matchable(&enders_over));
        assert!(is_matchable(&chars_ok));
        assert!(!is_matchable(&chars_over));
        let matcher = Matcher::new([
            enders_ok.as_str(),
            enders_over.as_str(),
            chars_ok.as_str(),
            chars_over.as_str(),
        ]);
        assert_eq!(matcher.len(), 2);
        // 組み込みの表の語はどれも上限に収まる
        assert!(builtin_exceptions().all(is_matchable));
    }

    #[test]
    fn test_dense_enders_with_ender_heavy_word_stay_linear() {
        // 文末記号だけの語と、文末記号だけの長い入力（前へ辿る手間は語の文末記号の数で抑えられる）
        let word = "。".repeat(MAX_WORD_ENDERS);
        let matcher = Matcher::new([word.as_str()]);
        let text = "。".repeat(200_000);
        let guarded = text
            .char_indices()
            .filter(|&(pos, c)| matcher.guards(&text, pos, c))
            .count();
        // 最後の 1 つ以外は語の内側にある
        assert_eq!(guarded, 200_000 - 1);
    }

    #[test]
    fn test_inactive_ascii_enders_in_words_are_not_indexed() {
        // 「Hey!Say!JUMP」の `!` は英字が続くので文末にならず、組を作らない
        let matcher = Matcher::new(["Hey!Say!JUMP"]);
        assert_eq!(matcher.nodes(), 1);
        // 語の末尾・日本語の前の `!` は文末になりうるので組を作る
        assert!(Matcher::new(["Yahoo!"]).nodes() > 1);
        assert!(Matcher::new(["Yahoo!ニュース"]).nodes() > 1);
    }
}
