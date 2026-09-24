//! 文字単位 double-array trie（末尾圧縮つき）
//!
//! 辞書の表層形から値（エントリ群の先頭番号）を引く。1 文字を 1 回の遷移で辿る。
//! 文字はキーに現れる回数の多い順に 1..=N の符号へ写し、符号 0 は「辞書にない文字」と END に使う。
//!
//! # 文字の符号 (char map)
//!
//! 2 段の表で引く。`blocks[cp >> 8]` が表の番号で、符号は `tables[表の番号 * 256 + (cp & 0xFF)]`。
//! 文字を 1 つも含まないブロックは、すべて 0 の表 0 を指す。
//!
//! # ノード
//!
//! [`Node`] の配列で、slot 0 が root（`check == 0`、INTERNAL）。`check == UNUSED` のスロットは未使用で、
//! base は 0 で書く。使用中のノードは base の上位 2 ビットが種別を表す。
//!
//! | 上位 2 ビット | 種別 | 残りのビット |
//! | --- | --- | --- |
//! | `00` | INTERNAL | bit29 = HAS_END、下位 29 ビット = 子の base（`1 <= base` かつ `base + N < ノード数`） |
//! | `10` | LEAF | 下位 30 ビット = 値 |
//! | `11` | TAIL | 下位 30 ビット = 末尾レコード（TRIE_TAILS）のバイトオフセット |
//! | `01` | 不正 | |
//!
//! - 符号 `c` の子は `base + c` に置き、`check` に親のスロット番号を持つ
//! - HAS_END なら `base + 0` に、そこで終わるキーの値を持つ LEAF（END 子）がある
//! - 子が END だけになる INTERNAL は作らない（LEAF にする）。root 以外で部分木のキーが 1 つだけになった
//!   ノードは、残りが空なら LEAF、空でなければ TAIL にする。root は END を持たない（空のキーは作らない）
//! - 末尾レコードは `value: u32 (LE)`、`len: varint (LEB128、最大 5 バイト)`、`len` バイトの UTF-8（`len >= 1`）
//!
//! # 読み出しの安全性
//!
//! [`Trie::new`] はロード時の軽い検査だけ行い、全ノードは見ない。検索はすべての添字を範囲検査し、
//! 辿った先で壊れたノードに当たったら [`TrieError::Corrupt`] を返す（panic しない）。
//! 全件の検査は [`Trie::verify`] で行う（`hasami info --verify` 用）。

use std::fmt;

/// char map のブロック数（コードポイント 256 個ずつ、0x110000 >> 8）
pub const NUM_BLOCKS: usize = 0x1100;
/// char map の表 1 つの要素数
pub const TABLE_LEN: usize = 256;
/// 未使用スロットの check
pub const UNUSED: u32 = u32::MAX;
/// LEAF・TAIL が持てる値の上限（この値未満）
pub const VALUE_LIMIT: u32 = 1 << 30;
/// INTERNAL の base の上限（この値未満）
pub const BASE_LIMIT: u32 = 1 << 29;
/// 末尾レコード列（TRIE_TAILS）の長さの上限（この値未満）
pub const TAILS_LIMIT: usize = 1 << 30;
/// 文字符号の最大値 N の上限
pub const MAX_CODE_LIMIT: u32 = u16::MAX as u32;

const KIND_SHIFT: u32 = 30;
const KIND_INTERNAL: u32 = 0b00;
const KIND_LEAF: u32 = 0b10;
const KIND_TAIL: u32 = 0b11;
const HAS_END: u32 = 1 << 29;
const BASE_MASK: u32 = BASE_LIMIT - 1;
const PAYLOAD_MASK: u32 = VALUE_LIMIT - 1;
/// 末尾レコードの長さ（varint）の最大バイト数
const MAX_VARINT_LEN: usize = 5;
/// 構築の進捗を報告する間隔（確定したキー数）
const PROGRESS_STEP: usize = 1 << 16;

#[inline(always)]
const fn kind(base: u32) -> u32 {
    base >> KIND_SHIFT
}

#[inline(always)]
const fn leaf_base(value: u32) -> u32 {
    (KIND_LEAF << KIND_SHIFT) | value
}

/// trie のノード（8 バイト）
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Node {
    /// 上位 2 ビットが種別、残りが種別ごとの値（モジュールの説明を参照）
    pub base: u32,
    /// 親のスロット番号。root は 0、未使用は [`UNUSED`]
    pub check: u32,
}

impl Node {
    /// 未使用スロット
    const EMPTY: Node = Node {
        base: 0,
        check: UNUSED,
    };
}

/// 構築結果（そのまま CHAR_BLOCKS / CHAR_TABLES / TRIE_NODES / TRIE_TAILS に書き出せる）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrieParts {
    pub blocks: Vec<u16>,
    pub tables: Vec<u16>,
    pub nodes: Vec<Node>,
    pub tails: Vec<u8>,
}

impl TrieParts {
    /// 構築結果をそのまま引く
    pub fn as_trie(&self) -> Result<Trie<'_>, TrieError> {
        Trie::new(&self.blocks, &self.tables, &self.nodes, &self.tails)
    }
}

/// trie の構築・読み出しのエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrieError {
    /// 構築の入力が不正、または形式の上限を超えた
    Build(String),
    /// trie のデータが壊れている
    Corrupt(String),
}

impl fmt::Display for TrieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrieError::Build(msg) => write!(f, "trie build failed: {msg}"),
            TrieError::Corrupt(msg) => write!(f, "corrupt trie: {msg}"),
        }
    }
}

impl std::error::Error for TrieError {}

/// エラー経路の文字列組み立てを検索の熱い経路から外す
#[cold]
#[inline(never)]
fn corrupt(msg: String) -> TrieError {
    TrieError::Corrupt(msg)
}

#[cold]
#[inline(never)]
fn build_error(msg: String) -> TrieError {
    TrieError::Build(msg)
}

/// [`Trie::verify`] の集計
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrieStats {
    /// ノード配列の長さ（未使用を含む）
    pub node_count: usize,
    /// 使用中のノード数（root を含む）
    pub used: usize,
    /// INTERNAL の数（root を含む）
    pub internal: usize,
    /// LEAF の数（END 子を含む）
    pub leaves: usize,
    /// TAIL の数
    pub tails: usize,
    /// キーの数（LEAF + TAIL）
    pub keys: usize,
    /// 末尾レコード列のバイト数
    pub tail_bytes: usize,
    /// 文字符号の最大値 N
    pub max_code: u32,
}

// ---------------------------------------------------------------------------
// 構築
// ---------------------------------------------------------------------------

/// ソート済みのキーと値から trie を構築する
///
/// キーはバイト順の昇順・重複なし・空でない・1 個以上、値は [`VALUE_LIMIT`] 未満。
/// 同じ入力からは常に同じバイト列ができる。
pub fn build(keys: &[&str], values: &[u32]) -> Result<TrieParts, TrieError> {
    build_with_progress(keys, values, |_, _| {})
}

/// 進捗コールバック付きで trie を構築する
///
/// `progress(確定したキー数, キーの総数)` を適度な間隔で呼ぶ（最初に 0、最後に総数）。
pub fn build_with_progress(
    keys: &[&str],
    values: &[u32],
    mut progress: impl FnMut(usize, usize),
) -> Result<TrieParts, TrieError> {
    check_input(keys, values)?;
    let total = keys.len();
    progress(0, total);
    let char_map = CharMap::build(keys)?;
    let mut placer = Placer::new(keys, values, char_map);
    placer.run(&mut progress)?;
    let parts = placer.finish()?;
    progress(total, total);
    Ok(parts)
}

fn check_input(keys: &[&str], values: &[u32]) -> Result<(), TrieError> {
    if keys.len() != values.len() {
        return Err(build_error(format!(
            "{} keys but {} values",
            keys.len(),
            values.len()
        )));
    }
    if keys.is_empty() {
        return Err(build_error("no keys".to_string()));
    }
    for (i, key) in keys.iter().enumerate() {
        if key.is_empty() {
            return Err(build_error(format!("key #{i} is empty")));
        }
        if i > 0 && keys[i - 1].as_bytes() >= key.as_bytes() {
            return Err(build_error(format!(
                "keys are not strictly ascending in byte order at #{i}: {:?} then {key:?}",
                keys[i - 1]
            )));
        }
    }
    if let Some((i, v)) = values.iter().enumerate().find(|&(_, &v)| v >= VALUE_LIMIT) {
        return Err(build_error(format!(
            "value {v} of key #{i} is not below 2^30"
        )));
    }
    Ok(())
}

/// 構築時の文字符号
struct CharMap {
    blocks: Vec<u16>,
    tables: Vec<u16>,
    max_code: u32,
}

impl CharMap {
    /// キーに出現する文字を (出現回数の降順, 文字の昇順) に並べて 1..=N を振る
    ///
    /// 表の番号は、文字を含むブロックにブロック番号の昇順で 1 から振る（表 0 は共有のゼロ表）。
    fn build(keys: &[&str]) -> Result<Self, TrieError> {
        // 出現回数はブロックごとに、文字が現れたブロックの分だけ持つ（全コードポイント分の表は 8.9MB になる）
        let mut counts: Vec<Vec<u64>> = vec![Vec::new(); NUM_BLOCKS];
        for key in keys {
            for c in key.chars() {
                let cp = c as usize;
                let block = &mut counts[cp >> 8];
                if block.is_empty() {
                    block.resize(TABLE_LEN, 0);
                }
                block[cp & 0xFF] += 1;
            }
        }
        let mut chars: Vec<(u64, u32)> = Vec::new();
        for (block, block_counts) in counts.iter().enumerate() {
            for (low, &n) in block_counts.iter().enumerate() {
                if n > 0 {
                    chars.push((n, ((block << 8) | low) as u32));
                }
            }
        }
        drop(counts);
        chars.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        if chars.len() > MAX_CODE_LIMIT as usize {
            return Err(build_error(format!(
                "keys contain {} distinct characters; at most {MAX_CODE_LIMIT} can be encoded",
                chars.len()
            )));
        }

        let mut blocks = vec![0u16; NUM_BLOCKS];
        for &(_, cp) in &chars {
            blocks[(cp >> 8) as usize] = 1;
        }
        // ブロックは 0x1100 個なので、表の数（ゼロ表を含めて最大 0x1101）は u16 に収まる
        let mut num_tables = 1u16;
        for table in blocks.iter_mut().filter(|t| **t != 0) {
            *table = num_tables;
            num_tables += 1;
        }
        let mut tables = vec![0u16; num_tables as usize * TABLE_LEN];
        for (i, &(_, cp)) in chars.iter().enumerate() {
            let table = blocks[(cp >> 8) as usize] as usize;
            tables[table * TABLE_LEN + (cp & 0xFF) as usize] = (i + 1) as u16;
        }
        Ok(CharMap {
            blocks,
            tables,
            max_code: chars.len() as u32,
        })
    }

    fn code(&self, c: char) -> u32 {
        let cp = c as u32;
        let table = self.blocks[(cp >> 8) as usize] as usize;
        self.tables[table * TABLE_LEN + (cp & 0xFF) as usize] as u32
    }
}

/// 構築時の空きスロットの管理（ビット集合）
///
/// 空き探索は 64 スロットずつビット演算で行う。子が複数あるノードは、ラベルごとに
/// 「そのラベルを置く位置が空いている base の候補」を 64 個分のビット列で取り出し、
/// 全ラベルの AND が 0 でない最初の候補を採る。
struct SlotAllocator {
    /// bit i = 1 ならスロット i は使用済み（先に埋めた予約を含む）
    used: Vec<u64>,
    /// 最小の空きスロット
    first_free: usize,
    /// 子が複数あるノードの base を探し始める位置
    ///
    /// 配列が埋まってくると、先頭側には全ラベルを収められない小さな穴だけが残る。子が複数あるノードの
    /// たびにその穴を先頭から試し直さないよう、走査した区間がほぼ埋まっていたら次回はその先から探す
    /// （darts の next_check_pos と同じ考え方）。穴は子が 1 つのノードが先頭から埋めていく。
    multi_start: usize,
}

impl SlotAllocator {
    fn new(capacity: usize) -> Self {
        SlotAllocator {
            used: vec![0; capacity.div_ceil(64)],
            first_free: 0,
            multi_start: 0,
        }
    }

    /// 64 スロット分の空きビット（配列の外は空き）
    #[inline]
    fn free_word(&self, word: usize) -> u64 {
        match self.used.get(word) {
            Some(&w) => !w,
            None => u64::MAX,
        }
    }

    /// bit j = 1 ならスロット pos + j が空き
    #[inline]
    fn free_bits(&self, pos: usize) -> u64 {
        let word = pos / 64;
        let shift = pos % 64;
        let low = self.free_word(word);
        if shift == 0 {
            low
        } else {
            (low >> shift) | (self.free_word(word + 1) << (64 - shift))
        }
    }

    /// start 以降の最初の空きスロット
    fn next_free(&self, start: usize) -> usize {
        let mut word = start / 64;
        let Some(&w) = self.used.get(word) else {
            return start;
        };
        let free = !w & (u64::MAX << (start % 64));
        if free != 0 {
            return word * 64 + free.trailing_zeros() as usize;
        }
        word += 1;
        while let Some(&w) = self.used.get(word) {
            if w != u64::MAX {
                return word * 64 + (!w).trailing_zeros() as usize;
            }
            word += 1;
        }
        word * 64
    }

    fn mark(&mut self, pos: usize) {
        let word = pos / 64;
        if word >= self.used.len() {
            self.used.resize(word + 1, 0);
        }
        self.used[word] |= 1 << (pos % 64);
        if pos == self.first_free {
            self.first_free = self.next_free(pos + 1);
        }
    }

    /// labels（昇順・重複なし・1 個以上）をすべて空きスロットに置ける base（>= 1）を探す
    fn find_base(&mut self, labels: &[u32]) -> usize {
        let first = labels[0] as usize;
        // base >= 1 なので、先頭のラベルを置く位置は first + 1 以上
        if labels.len() == 1 {
            // 子が 1 つなら最初の空きスロットにそのまま入る
            return self.next_free(self.first_free.max(first + 1)) - first;
        }
        let begin = self.next_free(self.first_free.max(self.multi_start).max(first + 1));
        // 先頭のラベルを置く位置の候補 [window, window + 64)
        let mut window = begin;
        // 走査した区間の空きスロットの数（どれも候補として試した）
        let mut tried = 0usize;
        loop {
            let head = self.free_bits(window);
            let mut fit = head;
            for &label in &labels[1..] {
                if fit == 0 {
                    break;
                }
                fit &= self.free_bits(window - first + label as usize);
            }
            if fit != 0 {
                let k = fit.trailing_zeros();
                let pos = window + k as usize;
                tried += (head & (u64::MAX >> (63 - k))).count_ones() as usize;
                // [begin, pos] の使用済みが 95% 以上なら、次からは pos の先を探す
                let span = pos - begin + 1;
                if (span - tried) * 20 >= span * 19 {
                    self.multi_start = pos;
                }
                return pos - first;
            }
            tried += head.count_ones() as usize;
            window += 64;
        }
    }
}

/// 構築中のノードの子（符号と、その子に属するキーの範囲）
struct Child {
    code: u32,
    lo: usize,
    hi: usize,
    /// 遷移する文字のバイト数
    len: usize,
}

/// DFS の作業単位
struct Task {
    /// このノードのスロット
    slot: usize,
    /// このノードの部分木に属するキーの範囲 keys[lo..hi]
    lo: usize,
    hi: usize,
    /// root からこのノードまでに辿ったバイト数（範囲内のキーの共通接頭辞の長さ）
    depth: usize,
}

/// キーを DFS でノード配列に置く
///
/// 配置の順序（再現性のために固定）:
/// - ノードを DFS の前順に処理し、処理したときにそのノードの子（END を含む）の位置をまとめて決める
/// - 子はコードポイントの降順（キーの逆順）に辿る。スタックにはキーの順に積む
/// - 末尾レコードは TAIL を処理した順（DFS の前順）に追記する
///
/// 子を辿る順序で占有率が変わる。配列の先端（使用中の最大スロット）は子の符号が広く散らばるノードが
/// 押し広げ、その後ろの穴は後から置くノードが埋める。構築の終わりに残った穴はそのまま無駄になる。
/// 仮名は出現回数が多く符号が小さいので、仮名で分かれるノードは子が狭い範囲に収まり小さな穴に入る。
/// 仮名は漢字よりコードポイントが小さいので、コードポイントの降順に辿ると各ノードで漢字の子を先に、
/// 仮名の子を後に処理し、構築の後半にそうしたノードが続いて穴が埋まる。符号の昇順に辿ると、
/// 推奨辞書の占有率が 99.4% から 97.2%、IPAdic が 94.9% から 88.7% に下がる。
struct Placer<'a> {
    keys: &'a [&'a str],
    values: &'a [u32],
    char_map: CharMap,
    nodes: Vec<Node>,
    slots: SlotAllocator,
    tails: Vec<u8>,
    /// INTERNAL の base の最大値（末尾を埋める長さの計算に使う）
    max_base: usize,
}

impl<'a> Placer<'a> {
    fn new(keys: &'a [&'a str], values: &'a [u32], char_map: CharMap) -> Self {
        let max_code = char_map.max_code as usize;
        // 末尾圧縮ありの推奨辞書でスロット数はキー数の約 1.45 倍
        let capacity = keys.len() + keys.len() / 2 + max_code + 64;
        let mut nodes = vec![Node::EMPTY; capacity];
        nodes[0].check = 0;
        let mut slots = SlotAllocator::new(capacity);
        slots.mark(0);
        // base >= 1 なので、スロット p に置けるのは符号が p 未満の子だけで、符号の最大値以下のスロットには
        // 置けない子がある。最初の空きスロットがそこで止まると、子が 1 つのノードのたびに配列の終わりまで
        // 走査して構築がノード数 × 配列長になる。スロット 1..=N を先に埋めておく（未使用として書き出す）
        for pos in 1..=max_code {
            slots.mark(pos);
        }
        Placer {
            keys,
            values,
            char_map,
            nodes,
            slots,
            tails: Vec::new(),
            max_base: 0,
        }
    }

    fn run(&mut self, progress: &mut impl FnMut(usize, usize)) -> Result<(), TrieError> {
        let total = self.keys.len();
        let mut done = 0usize;
        let mut next_report = PROGRESS_STEP;
        let mut stack = vec![Task {
            slot: 0,
            lo: 0,
            hi: total,
            depth: 0,
        }];
        let mut children: Vec<Child> = Vec::new();
        let mut labels: Vec<u32> = Vec::new();

        while let Some(task) = stack.pop() {
            if task.slot != 0 && task.hi - task.lo == 1 {
                self.place_last_key(&task)?;
                done += 1;
            } else {
                // キーはソート済みなので、このノードで終わるキーがあれば範囲の先頭にある
                let terminal = self.keys[task.lo].len() == task.depth;
                self.collect_children(&task, terminal, &mut children)?;
                labels.clear();
                if terminal {
                    labels.push(0);
                }
                labels.extend(children.iter().map(|c| c.code));
                labels.sort_unstable();
                let base = self.slots.find_base(&labels);
                if base >= BASE_LIMIT as usize {
                    return Err(build_error(format!(
                        "trie too large: base {base} of slot {} reaches 2^29",
                        task.slot
                    )));
                }
                let last = base + labels.last().map_or(0, |&l| l as usize);
                if last >= self.nodes.len() {
                    let len = (last + 1).max(self.nodes.len() + self.nodes.len() / 4);
                    self.nodes.resize(len, Node::EMPTY);
                }
                for &label in &labels {
                    self.slots.mark(base + label as usize);
                }
                self.max_base = self.max_base.max(base);
                let parent = task.slot as u32;
                self.nodes[task.slot].base = base as u32 | if terminal { HAS_END } else { 0 };
                if terminal {
                    self.nodes[base] = Node {
                        base: leaf_base(self.values[task.lo]),
                        check: parent,
                    };
                    done += 1;
                }
                // コードポイントの降順に辿るよう、キーの順に積む
                for child in &children {
                    let slot = base + child.code as usize;
                    self.nodes[slot].check = parent;
                    stack.push(Task {
                        slot,
                        lo: child.lo,
                        hi: child.hi,
                        depth: task.depth + child.len,
                    });
                }
            }
            if done >= next_report {
                progress(done, total);
                next_report = done + PROGRESS_STEP;
            }
        }
        Ok(())
    }

    /// keys[lo..hi]（終端のキーを除く）を次の文字で分ける（子はキーの順 = コードポイントの昇順に並ぶ）
    fn collect_children(
        &self,
        task: &Task,
        terminal: bool,
        out: &mut Vec<Child>,
    ) -> Result<(), TrieError> {
        out.clear();
        let depth = task.depth;
        let mut i = task.lo + usize::from(terminal);
        while i < task.hi {
            // 範囲内のキーは先頭 depth バイトが共通で、終端のキー以外は depth より長い
            let Some(c) = self.keys[i][depth..].chars().next() else {
                return Err(build_error(format!("internal error: key #{i} ends early")));
            };
            // 同じ文字で続くキーは連続している（UTF-8 のバイト順 = コードポイント順）。
            // 1〜4 バイトの比較で memcmp を呼ばないよう、文字を復号して比べる
            let run = self.keys[i + 1..task.hi]
                .partition_point(|k| k.get(depth..).and_then(|s| s.chars().next()) == Some(c));
            let hi = i + 1 + run;
            out.push(Child {
                code: self.char_map.code(c),
                lo: i,
                hi,
                len: c.len_utf8(),
            });
            i = hi;
        }
        if out.is_empty() {
            return Err(build_error(format!(
                "internal error: INTERNAL slot {} has no children",
                task.slot
            )));
        }
        Ok(())
    }

    /// 部分木のキーが 1 つだけのノードを、残りが空なら LEAF、空でなければ TAIL にする
    fn place_last_key(&mut self, task: &Task) -> Result<(), TrieError> {
        let value = self.values[task.lo];
        let rest = &self.keys[task.lo].as_bytes()[task.depth..];
        self.nodes[task.slot].base = if rest.is_empty() {
            leaf_base(value)
        } else {
            let offset = self.tails.len() as u32;
            self.tails.extend_from_slice(&value.to_le_bytes());
            push_varint(&mut self.tails, rest.len() as u32);
            self.tails.extend_from_slice(rest);
            if self.tails.len() >= TAILS_LIMIT {
                return Err(build_error(format!(
                    "trie too large: tails reach 2^30 bytes at key #{}",
                    task.lo
                )));
            }
            (KIND_TAIL << KIND_SHIFT) | offset
        };
        Ok(())
    }

    fn finish(mut self) -> Result<TrieParts, TrieError> {
        // すべての INTERNAL で base + N < ノード数となるよう、末尾を未使用で埋める。
        // 使用中のスロットはどれも何かの INTERNAL の base + 符号 <= max_base + N なので、切り詰めても失われない
        let node_count = self.max_base + self.char_map.max_code as usize + 1;
        self.nodes.resize(node_count, Node::EMPTY);
        Ok(TrieParts {
            blocks: self.char_map.blocks,
            tables: self.char_map.tables,
            nodes: self.nodes,
            tails: self.tails,
        })
    }
}

fn push_varint(buf: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// LEB128 を読む。最大 5 バイトで、u32 に収まらない値は None
fn read_varint(buf: &[u8], pos: usize) -> Option<(u32, usize)> {
    let mut value = 0u64;
    for i in 0..MAX_VARINT_LEN {
        let byte = *buf.get(pos + i)?;
        value |= u64::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return u32::try_from(value).ok().map(|v| (v, pos + i + 1));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 読み出し
// ---------------------------------------------------------------------------

/// 構築済みの trie（mmap したセクションなどへの参照）
#[derive(Clone, Copy)]
pub struct Trie<'a> {
    blocks: &'a [u16],
    tables: &'a [u16],
    nodes: &'a [Node],
    tails: &'a [u8],
    max_code: u32,
}

/// 親ごとの子のスロット（CSR 形式）。子はスロットの昇順（= 符号の昇順）に並ぶ
struct ChildLists {
    /// ends[p] = p の子の範囲の終わり（p の範囲の始まりは ends[p - 1]、p == 0 なら 0）
    ends: Vec<u32>,
    slots: Vec<u32>,
}

impl ChildLists {
    fn of(&self, parent: usize) -> &[u32] {
        let begin = if parent == 0 {
            0
        } else {
            self.ends[parent - 1] as usize
        };
        &self.slots[begin..self.ends[parent] as usize]
    }
}

impl<'a> Trie<'a> {
    /// ロード時の軽い検査をして trie を作る（全ノードは見ない）
    ///
    /// 検査: 表の長さ、各 block が表の数未満、表 0 がすべて 0、最大符号 N の算出（tables を 1 回走査）、
    /// root が `check == 0` の INTERNAL で END を持たず、`1 <= base && base + N < ノード数`。
    pub fn new(
        blocks: &'a [u16],
        tables: &'a [u16],
        nodes: &'a [Node],
        tails: &'a [u8],
    ) -> Result<Self, TrieError> {
        if blocks.len() != NUM_BLOCKS {
            return Err(corrupt(format!(
                "char map has {} blocks, expected {NUM_BLOCKS}",
                blocks.len()
            )));
        }
        if tables.is_empty() || tables.len() % TABLE_LEN != 0 {
            return Err(corrupt(format!(
                "char tables have {} entries, not a positive multiple of {TABLE_LEN}",
                tables.len()
            )));
        }
        let num_tables = tables.len() / TABLE_LEN;
        if num_tables > 1 << 16 {
            return Err(corrupt(format!(
                "{num_tables} char tables; a block can refer to at most 65536"
            )));
        }
        if let Some((block, &table)) = blocks
            .iter()
            .enumerate()
            .find(|&(_, &t)| t as usize >= num_tables)
        {
            return Err(corrupt(format!(
                "char block {block:#x} refers to table {table}, but there are only {num_tables} tables"
            )));
        }
        if let Some(i) = tables[..TABLE_LEN].iter().position(|&c| c != 0) {
            return Err(corrupt(format!(
                "char table 0 is not all zero (entry {i} is {})",
                tables[i]
            )));
        }
        let max_code = tables.iter().copied().max().unwrap_or(0) as u32;
        if max_code == 0 {
            return Err(corrupt("char map assigns no codes".to_string()));
        }
        // スロット番号は check（u32）に入り、u32::MAX は未使用の印
        if nodes.len() >= UNUSED as usize {
            return Err(corrupt(format!("{} nodes are too many", nodes.len())));
        }
        let Some(&root) = nodes.first() else {
            return Err(corrupt("trie has no nodes".to_string()));
        };
        if root.check != 0 {
            return Err(corrupt(format!(
                "root (slot 0) has check {:#x}, expected 0",
                root.check
            )));
        }
        if kind(root.base) != KIND_INTERNAL {
            return Err(corrupt(format!(
                "root (slot 0) is not INTERNAL (base {:#010x})",
                root.base
            )));
        }
        if root.base & HAS_END != 0 {
            return Err(corrupt(
                "root (slot 0) has an END child (empty key)".to_string(),
            ));
        }
        let base = (root.base & BASE_MASK) as usize;
        if base == 0 || base + max_code as usize >= nodes.len() {
            return Err(corrupt(format!(
                "root (slot 0) has base {base}, out of range for max code {max_code} and {} nodes",
                nodes.len()
            )));
        }
        Ok(Trie {
            blocks,
            tables,
            nodes,
            tails,
            max_code,
        })
    }

    /// [`Trie::new`] で検証済みの部品と、そのとき得た最大符号から作り直す（検査なし、O(1)）
    ///
    /// 解析のたびにビューを作り直す読み込み側のためのもの。検証していない部品を渡しても
    /// 未定義動作や panic にはならず、検索が Err を返すか結果が不定になるだけ。
    /// [`Trie::keys`] と [`Trie::verify`] は [`Trie::new`] と同じ検査からやり直す。
    #[inline]
    pub fn from_validated(
        blocks: &'a [u16],
        tables: &'a [u16],
        nodes: &'a [Node],
        tails: &'a [u8],
        max_code: u32,
    ) -> Self {
        Trie {
            blocks,
            tables,
            nodes,
            tails,
            max_code,
        }
    }

    /// 文字符号の最大値 N
    pub fn max_code(&self) -> u32 {
        self.max_code
    }

    /// ノード配列の長さ（未使用を含む）
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 文字の符号（辞書にない文字は 0）
    #[inline]
    pub fn char_code(&self, c: char) -> u32 {
        let cp = c as u32;
        let Some(&table) = self.blocks.get((cp >> 8) as usize) else {
            return 0;
        };
        match self
            .tables
            .get(table as usize * TABLE_LEN + (cp & 0xFF) as usize)
        {
            Some(&code) => code as u32,
            None => 0,
        }
    }

    /// `text[start..]` の接頭辞でキーに一致するものごとに `cb(終了バイト位置, 値)` を呼ぶ
    ///
    /// 終了位置は text 全体での位置で、昇順に報告する（start より必ず大きい）。start は文字境界で、
    /// そうでなければ何も報告しない。辿った先で壊れたノード（種別 01、範囲外の base・オフセット、
    /// HAS_END の先が `check == 親` の LEAF でない、壊れた TAIL レコード）に当たったら Err を返す。
    /// どんな部品でも panic しない（壊れていない部分で見つけた一致は Err の前に報告済みのことがある）。
    #[inline]
    pub fn common_prefix_search(
        &self,
        text: &str,
        start: usize,
        mut cb: impl FnMut(usize, u32),
    ) -> Result<(), TrieError> {
        let Some(rest) = text.get(start..) else {
            return Ok(());
        };
        let chars = rest.chars().map(|c| (self.char_code(c), c.len_utf8()));
        self.search_prefixes(text, start, chars, |end, _, value| cb(end, value))
    }

    /// [`Trie::common_prefix_search`] の、文字の符号を前もって引いておいた版（解析用）
    ///
    /// `codes[i]` は `text` の i 文字目の符号（[`Trie::char_code`]）、`offsets[i]` はそのバイト位置で、
    /// `offsets` は末尾に `text.len()` を加えた `codes.len() + 1` 要素。`start` 文字目からの接頭辞で
    /// キーに一致するものごとに `cb(終了の文字位置, 値)` を呼ぶ。報告の順序・検査・エラーは
    /// [`Trie::common_prefix_search`] と同じ。入力の文字を検索のたびに復号して符号を引く代わりに、
    /// 呼び出し側が文字ごとに 1 回だけ引いておける。
    #[inline]
    pub fn common_prefix_search_codes(
        &self,
        text: &str,
        codes: &[u16],
        offsets: &[u32],
        start: usize,
        mut cb: impl FnMut(usize, u32),
    ) -> Result<(), TrieError> {
        let (Some(codes), Some(offsets)) = (codes.get(start..), offsets.get(start..)) else {
            return Ok(());
        };
        let Some(&first) = offsets.first() else {
            return Ok(());
        };
        let chars = codes
            .iter()
            .zip(offsets.windows(2))
            .map(|(&code, w)| (code as u32, w[1].wrapping_sub(w[0]) as usize));
        self.search_prefixes(text, first as usize, chars, |_, depth, value| {
            cb(start + depth, value)
        })
    }

    /// 共通接頭辞検索の本体。`chars` は `text[start..]` の文字の (符号, バイト長) を先頭から返す
    ///
    /// 一致ごとに `cb(終了のバイト位置, 一致した文字数, 値)` を呼ぶ。
    #[inline(always)]
    fn search_prefixes(
        &self,
        text: &str,
        start: usize,
        mut chars: impl Iterator<Item = (u32, usize)>,
        mut cb: impl FnMut(usize, usize, u32),
    ) -> Result<(), TrieError> {
        let nodes = self.nodes;
        // INTERNAL の base はこの値未満（base + N < ノード数）
        let base_end = nodes.len().saturating_sub(self.max_code as usize);
        let mut pos = start;
        let mut depth = 0usize;
        let mut slot = 0usize;
        // root が END を持つ・INTERNAL でない trie は new() が拒むが、from_validated() では来うる。
        // 長さ 0 の一致を報告しないよう、ここでも確かめる
        let mut node = match nodes.first() {
            Some(&root) if kind(root.base) == KIND_INTERNAL && root.base & HAS_END == 0 => root,
            _ => return Err(bad_root()),
        };
        loop {
            if kind(node.base) != KIND_INTERNAL {
                return self.search_end(text, pos, depth, slot, node, cb);
            }
            let base = (node.base & BASE_MASK) as usize;
            if base == 0 || base >= base_end {
                return Err(bad_base(slot, base, base_end));
            }
            if node.base & HAS_END != 0 {
                match nodes.get(base) {
                    Some(end) if end.check == slot as u32 && kind(end.base) == KIND_LEAF => {
                        cb(pos, depth, end.base & PAYLOAD_MASK);
                    }
                    _ => return Err(bad_end(slot, base)),
                }
            }
            let Some((code, len)) = chars.next() else {
                return Ok(());
            };
            if code == 0 {
                return Ok(());
            }
            let next = base + code as usize;
            match nodes.get(next) {
                Some(&child) if child.check == slot as u32 => {
                    slot = next;
                    node = child;
                    pos += len;
                    depth += 1;
                }
                Some(_) => return Ok(()),
                // 符号が N 以下なら base + 符号 < ノード数。部品が検証済みでないときだけ来る
                None => return Err(bad_base(slot, base, base_end)),
            }
        }
    }

    /// 辿り着いた LEAF・TAIL を報告する
    #[inline(always)]
    fn search_end(
        &self,
        text: &str,
        pos: usize,
        depth: usize,
        slot: usize,
        node: Node,
        mut cb: impl FnMut(usize, usize, u32),
    ) -> Result<(), TrieError> {
        match kind(node.base) {
            KIND_LEAF => {
                cb(pos, depth, node.base & PAYLOAD_MASK);
                Ok(())
            }
            KIND_TAIL => {
                let (value, suffix) = self.tail_record(slot, node.base & PAYLOAD_MASK)?;
                // 入力の残りが suffix 以上の長さのときだけ比べる。多くは先頭のバイトで外れるので、
                // 先に 1 バイト比べて bcmp の呼び出しを省く
                let end = pos + suffix.len();
                let matched = match text.as_bytes().get(pos..end) {
                    Some(rest) => rest.first() == suffix.first() && rest == suffix,
                    None => false,
                };
                if matched {
                    // suffix が正しい UTF-8 なら一致の終わりは文字境界になる
                    if !text.is_char_boundary(end) {
                        return Err(bad_tail(
                            slot,
                            (node.base & PAYLOAD_MASK) as usize,
                            "the bytes are not valid UTF-8 (a match ends inside a character)",
                        ));
                    }
                    // 一致したバイト列は入力の文字の並びなので、文字の先頭バイト（継続バイトでない
                    // もの）の数が文字数になる
                    let chars = suffix.iter().filter(|&&b| (b as i8) >= -0x40).count();
                    cb(end, depth + chars, value);
                }
                Ok(())
            }
            _ => Err(invalid_kind(slot, node)),
        }
    }

    /// 末尾レコード（値と、残りのバイト列）を読む
    ///
    /// 検索のたびに通るので、Result を値のまま扱えるよう呼び出し側に展開する（エラーの組み立ては cold）
    #[inline(always)]
    fn tail_record(&self, slot: usize, offset: u32) -> Result<(u32, &'a [u8]), TrieError> {
        let tails = self.tails;
        let off = offset as usize;
        let Some(&[b0, b1, b2, b3]) = tails.get(off..off + 4) else {
            return Err(bad_tail(
                slot,
                off,
                "the record runs past the end of the tails",
            ));
        };
        let value = u32::from_le_bytes([b0, b1, b2, b3]);
        if value >= VALUE_LIMIT {
            return Err(bad_tail(slot, off, "the value is not below 2^30"));
        }
        let (len, body) = match tails.get(off + 4) {
            Some(&byte) if byte < 0x80 => (byte as usize, off + 5),
            _ => match read_varint(tails, off + 4) {
                Some((len, body)) => (len as usize, body),
                None => {
                    return Err(bad_tail(
                        slot,
                        off,
                        "the length is not a varint of at most 5 bytes",
                    ));
                }
            },
        };
        if len == 0 {
            return Err(bad_tail(slot, off, "the length is 0"));
        }
        match tails.get(body..).and_then(|r| r.get(..len)) {
            Some(suffix) => Ok((value, suffix)),
            None => Err(bad_tail(
                slot,
                off,
                "the bytes run past the end of the tails",
            )),
        }
    }

    /// 完全一致で値を引く
    pub fn get(&self, key: &str) -> Result<Option<u32>, TrieError> {
        let mut found = None;
        self.common_prefix_search(key, 0, |end, value| {
            if end == key.len() {
                found = Some(value);
            }
        })?;
        Ok(found)
    }

    /// 全キーを (値, キー) で列挙する（順序は不定）。export と verify 用で、O(ノード数) の作業メモリを使う
    pub fn keys(&self) -> Result<Vec<(u32, String)>, TrieError> {
        let trie = self.revalidate()?;
        let chars = trie.inverse_char_map()?;
        let tree = trie.child_lists()?;
        let nodes = trie.nodes;
        let mut out = Vec::new();
        let mut prefix = String::new();
        // (スロット, 親までの接頭辞のバイト数)
        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
        while let Some((slot, parent_len)) = stack.pop() {
            prefix.truncate(parent_len);
            let node = nodes[slot];
            if slot != 0 {
                // child_lists() で親が INTERNAL で、自分 - 親の base が 0..=N だと確かめてある
                let parent_base = (nodes[node.check as usize].base & BASE_MASK) as usize;
                let code = slot - parent_base;
                if code != 0 {
                    prefix.push(chars[code]);
                }
            }
            match kind(node.base) {
                KIND_INTERNAL => {
                    trie.check_internal(slot, node)?;
                    let len = prefix.len();
                    stack.extend(tree.of(slot).iter().rev().map(|&c| (c as usize, len)));
                }
                KIND_LEAF => out.push((node.base & PAYLOAD_MASK, prefix.clone())),
                KIND_TAIL => {
                    let (value, suffix) = trie.tail_record(slot, node.base & PAYLOAD_MASK)?;
                    let suffix = tail_str(slot, node, suffix)?;
                    let mut key = String::with_capacity(prefix.len() + suffix.len());
                    key.push_str(&prefix);
                    key.push_str(suffix);
                    out.push((value, key));
                }
                _ => return Err(invalid_kind(slot, node)),
            }
        }
        Ok(out)
    }

    /// 全件を検査する（O(ノード数)）。値は value_limit 未満（かつ 2^30 未満）だけを許す
    ///
    /// 検査: char map の全単射（1..=N がちょうど 1 文字ずつ。表 0 以外の表はちょうど 1 つのブロックが使う）、
    /// 全スロットの check・base、END・LEAF・TAIL、親子関係（root から全使用ノードに到達でき閉路がない）、
    /// root 以外の INTERNAL は部分木に 2 つ以上のキーを持つ。
    pub fn verify(&self, value_limit: u32) -> Result<TrieStats, TrieError> {
        let trie = self.revalidate()?;
        trie.inverse_char_map()?;
        let value_limit = value_limit.min(VALUE_LIMIT);
        let nodes = trie.nodes;
        let mut stats = TrieStats {
            node_count: nodes.len(),
            tail_bytes: trie.tails.len(),
            max_code: trie.max_code,
            ..TrieStats::default()
        };

        // 全スロットを 1 つずつ検査する
        for (slot, &node) in nodes.iter().enumerate() {
            if node.check == UNUSED {
                if node.base != 0 {
                    return Err(corrupt(format!(
                        "unused slot {slot} has base {:#010x}, expected 0",
                        node.base
                    )));
                }
                continue;
            }
            match kind(node.base) {
                KIND_INTERNAL => {
                    trie.check_internal(slot, node)?;
                    stats.internal += 1;
                }
                KIND_LEAF => {
                    let value = node.base & PAYLOAD_MASK;
                    if value >= value_limit {
                        return Err(corrupt(format!(
                            "LEAF at slot {slot} has value {value}, not below {value_limit}"
                        )));
                    }
                    stats.leaves += 1;
                }
                KIND_TAIL => {
                    let (value, suffix) = trie.tail_record(slot, node.base & PAYLOAD_MASK)?;
                    if value >= value_limit {
                        return Err(corrupt(format!(
                            "TAIL at slot {slot} (offset {}) has value {value}, not below {value_limit}",
                            node.base & PAYLOAD_MASK
                        )));
                    }
                    tail_str(slot, node, suffix)?;
                    stats.tails += 1;
                }
                _ => return Err(invalid_kind(slot, node)),
            }
        }
        stats.used = stats.internal + stats.leaves + stats.tails;
        stats.keys = stats.leaves + stats.tails;

        // root から前順に辿る。各ノードの親は check の 1 つだけなので、どのノードも高々 1 回しか現れない
        let tree = trie.child_lists()?;
        let mut order: Vec<u32> = Vec::with_capacity(stats.used);
        let mut stack: Vec<u32> = vec![0];
        while let Some(slot) = stack.pop() {
            order.push(slot);
            stack.extend_from_slice(tree.of(slot as usize));
        }
        if order.len() != stats.used {
            let mut reached = vec![false; nodes.len()];
            for &slot in &order {
                reached[slot as usize] = true;
            }
            let slot = (0..nodes.len())
                .find(|&s| nodes[s].check != UNUSED && !reached[s])
                .unwrap_or(0);
            return Err(corrupt(format!(
                "slot {slot} is in use but not reachable from the root (cycle or detached subtree)"
            )));
        }

        // 前順の逆（子が親より先）に、部分木のキー数を親へ足し上げる
        let mut below = vec![0u32; nodes.len()];
        for &slot in order.iter().rev() {
            let slot = slot as usize;
            let node = nodes[slot];
            let count = if kind(node.base) == KIND_INTERNAL {
                below[slot]
            } else {
                1
            };
            if slot == 0 {
                if count == 0 {
                    return Err(corrupt("the trie has no keys".to_string()));
                }
            } else {
                if kind(node.base) == KIND_INTERNAL && count < 2 {
                    return Err(corrupt(format!(
                        "INTERNAL slot {slot} has {count} key(s) below it; a node with one key must be a LEAF or TAIL"
                    )));
                }
                below[node.check as usize] += count;
            }
        }
        Ok(stats)
    }

    /// new() と同じ検査をやり直す（from_validated() で作ったビューのため）
    fn revalidate(&self) -> Result<Trie<'a>, TrieError> {
        let trie = Trie::new(self.blocks, self.tables, self.nodes, self.tails)?;
        if trie.max_code != self.max_code {
            return Err(corrupt(format!(
                "max code is {}, but the char tables give {}",
                self.max_code, trie.max_code
            )));
        }
        Ok(trie)
    }

    /// INTERNAL の base の範囲と END 子を確かめ、base を返す
    fn check_internal(&self, slot: usize, node: Node) -> Result<usize, TrieError> {
        let base = (node.base & BASE_MASK) as usize;
        let base_end = self.nodes.len().saturating_sub(self.max_code as usize);
        if base == 0 || base >= base_end {
            return Err(bad_base(slot, base, base_end));
        }
        if node.base & HAS_END != 0 {
            match self.nodes.get(base) {
                Some(end) if end.check == slot as u32 && kind(end.base) == KIND_LEAF => {}
                _ => return Err(bad_end(slot, base)),
            }
        }
        Ok(base)
    }

    /// char map が 1..=N と文字の全単射になっているかを確かめ、符号から文字への表を返す
    ///
    /// new() で検査済みであること（各 block が表の数未満）を前提にする。
    fn inverse_char_map(&self) -> Result<Vec<char>, TrieError> {
        let num_tables = self.tables.len() / TABLE_LEN;
        let max_code = self.max_code as usize;
        let mut owner: Vec<Option<usize>> = vec![None; num_tables];
        let mut chars: Vec<Option<char>> = vec![None; max_code + 1];
        for (block, &table) in self.blocks.iter().enumerate() {
            let table = table as usize;
            if table == 0 {
                continue;
            }
            if let Some(other) = owner[table] {
                return Err(corrupt(format!(
                    "char table {table} is shared by blocks {other:#x} and {block:#x}"
                )));
            }
            owner[table] = Some(block);
            let codes = &self.tables[table * TABLE_LEN..(table + 1) * TABLE_LEN];
            if codes.iter().all(|&c| c == 0) {
                return Err(corrupt(format!(
                    "char table {table} (block {block:#x}) assigns no codes; such blocks must use table 0"
                )));
            }
            for (low, &code) in codes.iter().enumerate() {
                if code == 0 {
                    continue;
                }
                let cp = ((block << 8) | low) as u32;
                let Some(c) = char::from_u32(cp) else {
                    return Err(corrupt(format!(
                        "code {code} is assigned to the surrogate U+{cp:04X}"
                    )));
                };
                let slot = &mut chars[code as usize];
                if let Some(prev) = *slot {
                    return Err(corrupt(format!(
                        "code {code} is assigned to both U+{:04X} and U+{cp:04X}",
                        prev as u32
                    )));
                }
                *slot = Some(c);
            }
        }
        if let Some(table) = (1..num_tables).find(|&t| owner[t].is_none()) {
            return Err(corrupt(format!(
                "char table {table} is not referenced by any block"
            )));
        }
        if let Some(code) = (1..=max_code).find(|&c| chars[c].is_none()) {
            return Err(corrupt(format!(
                "code {code} is not assigned to any character"
            )));
        }
        Ok(chars.into_iter().map(|c| c.unwrap_or('\0')).collect())
    }

    /// 使用中の各ノード（root 以外）の親を確かめ、親ごとの子の一覧を作る
    fn child_lists(&self) -> Result<ChildLists, TrieError> {
        let nodes = self.nodes;
        let mut ends = vec![0u32; nodes.len()];
        let mut total = 0usize;
        for (slot, &node) in nodes.iter().enumerate().skip(1) {
            if node.check != UNUSED {
                let parent = self.parent_of(slot, node)?;
                ends[parent] += 1;
                total += 1;
            }
        }
        // 数を累積して各親の範囲の始まりにする
        let mut acc = 0u32;
        for end in ends.iter_mut() {
            let count = *end;
            *end = acc;
            acc += count;
        }
        let mut slots = vec![0u32; total];
        for (slot, node) in nodes.iter().enumerate().skip(1) {
            if node.check != UNUSED {
                let end = &mut ends[node.check as usize];
                slots[*end as usize] = slot as u32;
                *end += 1;
            }
        }
        // 詰め終えると ends[p] は p の範囲の終わりになる
        Ok(ChildLists { ends, slots })
    }

    /// 使用中のノード（root 以外）の親のスロットを返す
    ///
    /// 親は使用中の INTERNAL で、自分 - 親の base が 0..=N。0 なら親は HAS_END で、自分は LEAF。
    fn parent_of(&self, slot: usize, node: Node) -> Result<usize, TrieError> {
        let parent = node.check as usize;
        let Some(&p) = self.nodes.get(parent) else {
            return Err(corrupt(format!(
                "slot {slot} has check {parent}, beyond the {} nodes",
                self.nodes.len()
            )));
        };
        if p.check == UNUSED || kind(p.base) != KIND_INTERNAL {
            return Err(corrupt(format!(
                "slot {slot} has check {parent}, which is not an INTERNAL node"
            )));
        }
        let base = (p.base & BASE_MASK) as usize;
        if slot < base || slot - base > self.max_code as usize {
            return Err(corrupt(format!(
                "slot {slot} has check {parent}, but is not among its children (base {base}, max code {})",
                self.max_code
            )));
        }
        if slot == base {
            if p.base & HAS_END == 0 {
                return Err(corrupt(format!(
                    "slot {slot} is the END position of slot {parent}, which has no HAS_END"
                )));
            }
            if kind(node.base) != KIND_LEAF {
                return Err(corrupt(format!(
                    "slot {slot} is the END child of slot {parent}, but is not a LEAF"
                )));
            }
        }
        Ok(parent)
    }
}

// 検索の経路で使うエラーは、ここに集めた cold な関数で組み立てる。値だけを受け取るので、
// format! が局所変数の参照を取って検索ループの変数をスタックに退避させることがない。

#[cold]
#[inline(never)]
fn bad_root() -> TrieError {
    corrupt("root (slot 0) is missing, not INTERNAL, or has an END child".to_string())
}

/// INTERNAL の base が範囲外（`1 <= base < base_end` でない。base_end = ノード数 - N）
#[cold]
#[inline(never)]
fn bad_base(slot: usize, base: usize, base_end: usize) -> TrieError {
    corrupt(format!(
        "INTERNAL slot {slot} has base {base}, out of range (must be at least 1 and below {base_end} = node count - max code)"
    ))
}

#[cold]
#[inline(never)]
fn bad_end(slot: usize, base: usize) -> TrieError {
    corrupt(format!(
        "INTERNAL slot {slot} has HAS_END, but slot {base} is not a LEAF whose check is {slot}"
    ))
}

#[cold]
#[inline(never)]
fn bad_tail(slot: usize, offset: usize, what: &str) -> TrieError {
    corrupt(format!("TAIL at slot {slot} (offset {offset}): {what}"))
}

#[cold]
#[inline(never)]
fn invalid_kind(slot: usize, node: Node) -> TrieError {
    corrupt(format!(
        "slot {slot} has the invalid kind 01 (base {:#010x})",
        node.base
    ))
}

fn tail_str(slot: usize, node: Node, suffix: &[u8]) -> Result<&str, TrieError> {
    std::str::from_utf8(suffix).map_err(|e| {
        corrupt(format!(
            "TAIL at slot {slot} (offset {}) is not valid UTF-8: {e}",
            node.base & PAYLOAD_MASK
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

    /// テスト用の決定的な乱数（xorshift64）
    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        /// 0..n の乱数（n == 0 なら 0）
        fn below(&mut self, n: usize) -> usize {
            (self.next() % (n as u64).max(1)) as usize
        }
    }

    /// キーに順番どおり 0.. の値を付けて構築する
    fn build_indexed(keys: &[&str]) -> TrieParts {
        let values: Vec<u32> = (0..keys.len() as u32).collect();
        build(keys, &values).unwrap()
    }

    fn search(trie: &Trie, text: &str, start: usize) -> Vec<(usize, u32)> {
        let mut found = Vec::new();
        trie.common_prefix_search(text, start, |end, value| found.push((end, value)))
            .unwrap();
        found
    }

    /// 文字の符号とバイト位置の列（解析側が前もって作るもの）
    fn codes_of(trie: &Trie, text: &str) -> (Vec<u16>, Vec<u32>) {
        let codes = text.chars().map(|c| trie.char_code(c) as u16).collect();
        let mut offsets: Vec<u32> = text.char_indices().map(|(p, _)| p as u32).collect();
        offsets.push(text.len() as u32);
        (codes, offsets)
    }

    /// [`Trie::common_prefix_search_codes`] で検索し、一致をバイト位置で返す（`start` はバイト位置）
    fn search_codes(
        trie: &Trie,
        text: &str,
        start: usize,
    ) -> (Vec<(usize, u32)>, Result<(), TrieError>) {
        let (codes, offsets) = codes_of(trie, text);
        let Some(start_char) = offsets.iter().position(|&o| o as usize == start) else {
            return (Vec::new(), Ok(()));
        };
        let mut found = Vec::new();
        let result =
            trie.common_prefix_search_codes(text, &codes, &offsets, start_char, |end, v| {
                found.push((offsets[end] as usize, v))
            });
        (found, result)
    }

    /// 総当たりの共通接頭辞検索
    fn brute_force(map: &HashMap<String, u32>, text: &str, start: usize) -> Vec<(usize, u32)> {
        text[start..]
            .char_indices()
            .map(|(i, c)| start + i + c.len_utf8())
            .filter_map(|end| map.get(&text[start..end]).map(|&v| (end, v)))
            .collect()
    }

    /// 構築結果が keys と values の対応どおりに引けることを確かめる
    fn assert_round_trip(keys: &[&str], values: &[u32], parts: &TrieParts) {
        let trie = parts.as_trie().unwrap();
        let stats = trie.verify(VALUE_LIMIT).unwrap();
        assert_eq!(stats.keys, keys.len());
        let mut listed = trie.keys().unwrap();
        listed.sort_by(|a, b| a.1.cmp(&b.1));
        let expected: Vec<(u32, String)> = values
            .iter()
            .zip(keys)
            .map(|(&v, k)| (v, k.to_string()))
            .collect();
        assert_eq!(listed, expected);
        for (key, &value) in keys.iter().zip(values) {
            assert_eq!(trie.get(key).unwrap(), Some(value), "key {key:?}");
        }
    }

    /// key の文字を辿った先のスロットとノード（TAIL の中は辿らない）
    fn node_at<'p>(parts: &'p TrieParts, key: &str) -> (usize, &'p Node) {
        let trie = parts.as_trie().unwrap();
        let mut slot = 0usize;
        for c in key.chars() {
            let base = (parts.nodes[slot].base & BASE_MASK) as usize;
            let next = base + trie.char_code(c) as usize;
            assert_eq!(parts.nodes[next].check, slot as u32, "{key:?} at {c:?}");
            slot = next;
        }
        (slot, &parts.nodes[slot])
    }

    // --- 小さな fixture ---

    #[test]
    fn test_prefix_keys_are_all_reported() {
        let keys = ["東", "東京", "東京都"];
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        assert_eq!(
            search(&trie, "東京都に住む", 0),
            vec![(3, 0), (6, 1), (9, 2)]
        );
        assert_eq!(search(&trie, "東京に住む", 0), vec![(3, 0), (6, 1)]);
        assert_eq!(search(&trie, "京都", 0), vec![]);
        assert_round_trip(&keys, &[0, 1, 2], &parts);
    }

    #[test]
    fn test_node_with_end_and_children() {
        let keys = ["ab", "abc", "abd", "b"];
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        // "ab" は END 子と c・d の子を持つ INTERNAL
        let (slot, node) = node_at(&parts, "ab");
        assert_eq!(kind(node.base), KIND_INTERNAL);
        assert_ne!(node.base & HAS_END, 0);
        let end = parts.nodes[(node.base & BASE_MASK) as usize];
        assert_eq!(end.check, slot as u32);
        assert_eq!(end.base, leaf_base(0));
        // "abc" と "abd" は残りが空の LEAF
        assert_eq!(kind(node_at(&parts, "abc").1.base), KIND_LEAF);
        assert_eq!(search(&trie, "abcd", 0), vec![(2, 0), (3, 1)]);
        assert_eq!(search(&trie, "abx", 0), vec![(2, 0)]);
        assert_eq!(search(&trie, "a", 0), vec![]);
        assert_eq!(trie.get("a").unwrap(), None);
        assert_round_trip(&keys, &[0, 1, 2, 3], &parts);
    }

    #[test]
    fn test_long_tail_uses_multibyte_varint() {
        let long = format!("x{}", "あ".repeat(100));
        let keys = [long.as_str(), "y"];
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        // "x" の先は 300 バイトの TAIL で、長さの varint は 2 バイト (0xAC 0x02)
        let (_, node) = node_at(&parts, "x");
        assert_eq!(kind(node.base), KIND_TAIL);
        let off = (node.base & PAYLOAD_MASK) as usize;
        assert_eq!(&parts.tails[off..off + 4], &0u32.to_le_bytes());
        assert_eq!(&parts.tails[off + 4..off + 6], &[0xAC, 0x02]);
        assert_eq!(parts.tails.len(), 4 + 2 + 300);

        let text = format!("{long}です");
        assert_eq!(search(&trie, &text, 0), vec![(long.len(), 0)]);
        // 入力が suffix より短い・途中で違う
        assert_eq!(search(&trie, &long[..long.len() - 3], 0), vec![]);
        let differ = format!("x{}い", "あ".repeat(99));
        assert_eq!(search(&trie, &differ, 0), vec![]);
        assert_eq!(trie.get(&long).unwrap(), Some(0));
        assert_eq!(trie.get(&differ).unwrap(), None);
        assert_round_trip(&keys, &[0, 1], &parts);
    }

    #[test]
    fn test_four_byte_utf8() {
        let keys = ["😀", "😀😀", "𠮷", "𠮷野家"];
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        assert_eq!(search(&trie, "😀😀😀", 0), vec![(4, 0), (8, 1)]);
        assert_eq!(search(&trie, "𠮷野家で", 0), vec![(4, 2), (10, 3)]);
        assert_eq!(search(&trie, "😀😀😀", 4), vec![(8, 0), (12, 1)]);
        assert_round_trip(&keys, &[0, 1, 2, 3], &parts);
    }

    #[test]
    fn test_char_not_in_dictionary_stops_search() {
        let keys = ["東", "東X京", "東京"];
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        assert_eq!(trie.char_code('Z'), 0);
        assert_eq!(trie.char_code('\u{10FFFF}'), 0);
        assert_eq!(search(&trie, "東Z京", 0), vec![(3, 0)]);
        assert_eq!(search(&trie, "Z東", 0), vec![]);
        assert_eq!(search(&trie, "東X京都", 0), vec![(3, 0), (7, 1)]);
        assert_eq!(trie.get("Z").unwrap(), None);
    }

    #[test]
    fn test_single_key() {
        for key in ["a", "abc", "東京都"] {
            let parts = build(&[key], &[7]).unwrap();
            let trie = parts.as_trie().unwrap();
            let first = key.chars().next().unwrap();
            let (_, node) = node_at(&parts, &first.to_string());
            let expected_kind = if key.chars().count() == 1 {
                KIND_LEAF
            } else {
                KIND_TAIL
            };
            assert_eq!(kind(node.base), expected_kind);
            let text = format!("{key}{key}");
            assert_eq!(search(&trie, &text, 0), vec![(key.len(), 7)]);
            assert_eq!(search(&trie, &text, key.len()), vec![(text.len(), 7)]);
            assert_eq!(
                search(&trie, &key[..first.len_utf8()], 0).len(),
                usize::from(key.chars().count() == 1)
            );
            assert_round_trip(&[key], &[7], &parts);
        }
    }

    #[test]
    fn test_many_keys_with_same_prefix() {
        let mut owned: Vec<String> = (0..1000).map(|i| format!("接頭辞{i}")).collect();
        owned.push("接頭辞".to_string());
        owned.push("接頭".to_string());
        owned.sort();
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let values: Vec<u32> = (0..keys.len() as u32).map(|v| v * 3).collect();
        let parts = build(&keys, &values).unwrap();
        assert_round_trip(&keys, &values, &parts);
        let trie = parts.as_trie().unwrap();
        let map: HashMap<String, u32> = owned.iter().cloned().zip(values.iter().copied()).collect();
        for text in ["接頭辞9990", "接頭辞12x", "接頭x", "接頭辞"] {
            assert_eq!(
                search(&trie, text, 0),
                brute_force(&map, text, 0),
                "{text:?}"
            );
        }
    }

    #[test]
    fn test_start_is_absolute_and_must_be_char_boundary() {
        let keys = ["京都", "都"];
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        let text = "東京都";
        assert_eq!(search(&trie, text, 3), vec![(9, 0)]);
        assert_eq!(search(&trie, text, 6), vec![(9, 1)]);
        assert_eq!(search(&trie, text, 9), vec![]);
        // 文字境界でない・範囲外の start は何も報告しない
        assert_eq!(search(&trie, text, 4), vec![]);
        assert_eq!(search(&trie, text, 100), vec![]);
    }

    // --- ランダム ---

    const ALPHABET: &[char] = &['a', 'b', 'c', 'é', 'あ', 'い', 'ー', '東', '京', '𠮷', '😀'];

    fn random_keys(rng: &mut XorShift, count: usize, max_len: usize) -> Vec<String> {
        let mut keys: Vec<String> = (0..count)
            .map(|_| {
                let len = 1 + rng.below(max_len);
                (0..len)
                    .map(|_| ALPHABET[rng.below(ALPHABET.len())])
                    .collect()
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    fn random_text(rng: &mut XorShift, len: usize) -> String {
        // 辞書にない文字 'Z' も混ぜる
        (0..len)
            .map(|_| {
                let i = rng.below(ALPHABET.len() + 1);
                ALPHABET.get(i).copied().unwrap_or('Z')
            })
            .collect()
    }

    #[test]
    fn test_random_search_matches_brute_force() {
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        for round in 0..300 {
            let count = 1 + rng.below(200);
            let max_len = 1 + rng.below(if round % 10 == 0 { 40 } else { 6 });
            let owned = random_keys(&mut rng, count, max_len);
            let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
            let values: Vec<u32> = (0..keys.len()).map(|_| rng.below(1 << 30) as u32).collect();
            let parts = build(&keys, &values).unwrap();
            assert_round_trip(&keys, &values, &parts);
            let trie = parts.as_trie().unwrap();
            let map: HashMap<String, u32> =
                owned.iter().cloned().zip(values.iter().copied()).collect();
            for _ in 0..20 {
                let len = rng.below(12);
                let text = random_text(&mut rng, len);
                // キーそのものや、キーを伸ばした文も試す
                let text = if rng.below(2) == 0 {
                    let k = &owned[rng.below(owned.len())];
                    format!("{k}{text}")
                } else {
                    text
                };
                for (start, _) in text.char_indices() {
                    let expected = brute_force(&map, &text, start);
                    assert_eq!(
                        search(&trie, &text, start),
                        expected,
                        "round {round} text {text:?} start {start}"
                    );
                    assert_eq!(
                        search_codes(&trie, &text, start),
                        (expected, Ok(())),
                        "codes: round {round} text {text:?} start {start}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_build_is_deterministic() {
        let mut rng = XorShift(12345);
        let owned = random_keys(&mut rng, 500, 8);
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let values: Vec<u32> = (0..keys.len() as u32).collect();
        let a = build(&keys, &values).unwrap();
        let b = build(&keys, &values).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_char_codes_follow_frequency_then_char_order() {
        // 出現回数: い 3、あ 2、う 2、え 1 → い=1, あ=2, う=3, え=4
        let keys = ["あい", "いう", "いえ", "うあ"];
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        assert_eq!(trie.char_code('い'), 1);
        assert_eq!(trie.char_code('あ'), 2);
        assert_eq!(trie.char_code('う'), 3);
        assert_eq!(trie.char_code('え'), 4);
        assert_eq!(trie.max_code(), 4);
        // 表の番号はブロック番号の昇順
        let keys = ["a", "é", "あ"];
        let parts = build_indexed(&keys);
        assert_eq!(parts.blocks[0x00], 1);
        assert_eq!(parts.blocks[0x30], 2);
        assert_eq!(parts.tables.len(), 3 * TABLE_LEN);
    }

    #[test]
    fn test_layout_invariants() {
        let mut rng = XorShift(777);
        let owned = random_keys(&mut rng, 300, 10);
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let parts = build_indexed(&keys);
        let trie = parts.as_trie().unwrap();
        let n = trie.max_code() as usize;
        // 予約したスロット 1..=N は未使用として書く
        for slot in 1..=n {
            assert_eq!(parts.nodes[slot], Node::EMPTY, "slot {slot}");
        }
        // 未使用の base は 0、INTERNAL は base + N < ノード数（末尾を埋めた長さがちょうど max_base + N + 1）
        let mut max_base = 0;
        for node in &parts.nodes {
            if node.check == UNUSED {
                assert_eq!(node.base, 0);
            } else if kind(node.base) == KIND_INTERNAL {
                let base = (node.base & BASE_MASK) as usize;
                assert!(base >= 1 && base + n < parts.nodes.len());
                max_base = max_base.max(base);
            }
        }
        assert_eq!(parts.nodes.len(), max_base + n + 1);
    }

    #[test]
    fn test_progress_reports() {
        let owned: Vec<String> = (0..200_000).map(|i| format!("{i:08}")).collect();
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let values: Vec<u32> = (0..keys.len() as u32).collect();
        let mut calls = Vec::new();
        let parts =
            build_with_progress(&keys, &values, |done, total| calls.push((done, total))).unwrap();
        assert_eq!(calls.first(), Some(&(0, keys.len())));
        assert_eq!(calls.last(), Some(&(keys.len(), keys.len())));
        assert!(calls.len() >= 4);
        assert!(calls.windows(2).all(|w| w[0].0 <= w[1].0));
        let trie = parts.as_trie().unwrap();
        assert_eq!(trie.verify(VALUE_LIMIT).unwrap().keys, keys.len());
    }

    // --- 構築エラー ---

    #[test]
    fn test_build_errors() {
        let is_build_err = |r: Result<TrieParts, TrieError>| matches!(r, Err(TrieError::Build(_)));
        assert!(is_build_err(build(&[], &[])));
        assert!(is_build_err(build(&[""], &[0])));
        assert!(is_build_err(build(&["a", ""], &[0, 1])));
        assert!(is_build_err(build(&["b", "a"], &[0, 1])));
        assert!(is_build_err(build(&["a", "a"], &[0, 1])));
        assert!(is_build_err(build(&["a"], &[VALUE_LIMIT])));
        assert!(is_build_err(build(&["a"], &[u32::MAX])));
        assert!(is_build_err(build(&["a", "b"], &[0])));
        assert!(build(&["a"], &[VALUE_LIMIT - 1]).is_ok());
    }

    #[test]
    fn test_too_many_distinct_chars() {
        // 補助面の 65536 文字は符号 1..=65535 に収まらない
        let owned: Vec<String> = (0x10000u32..0x20000)
            .map(|cp| char::from_u32(cp).unwrap().to_string())
            .collect();
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let values = vec![0u32; keys.len()];
        let err = build(&keys, &values).unwrap_err();
        assert!(
            matches!(err, TrieError::Build(ref m) if m.contains("distinct characters")),
            "{err}"
        );
        // 65535 文字なら構築できる
        let parts = build(&keys[..65535], &values[..65535]).unwrap();
        let trie = parts.as_trie().unwrap();
        assert_eq!(trie.max_code(), 65535);
        assert_eq!(trie.verify(VALUE_LIMIT).unwrap().keys, 65535);
    }

    // --- verify が壊れた trie を検出する ---

    /// 検証用の fixture: END と子を持つノード、長い TAIL、4 バイト文字を含む
    fn fixture() -> (Vec<String>, TrieParts) {
        let mut owned: Vec<String> = [
            "ab",
            "abc",
            "abd",
            "b",
            "東",
            "東京",
            "東京都",
            "😀",
            "𠮷野家",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        owned.push(format!("z{}", "字".repeat(60)));
        owned.sort();
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let parts = build_indexed(&keys);
        (owned, parts)
    }

    fn verify_err(parts: &TrieParts) -> String {
        let trie = match parts.as_trie() {
            Ok(trie) => trie,
            Err(e) => return e.to_string(),
        };
        match trie.verify(VALUE_LIMIT) {
            Ok(stats) => panic!("verify passed: {stats:?}"),
            Err(e) => {
                assert!(matches!(e, TrieError::Corrupt(_)));
                e.to_string()
            }
        }
    }

    fn search_err(parts: &TrieParts, text: &str) -> String {
        let trie = parts.as_trie().unwrap();
        trie.common_prefix_search(text, 0, |_, _| {})
            .unwrap_err()
            .to_string()
    }

    fn first_slot(parts: &TrieParts, pred: impl Fn(usize, &Node) -> bool) -> usize {
        parts
            .nodes
            .iter()
            .enumerate()
            .position(|(i, n)| n.check != UNUSED && pred(i, n))
            .unwrap()
    }

    #[test]
    fn test_verify_accepts_fixture() {
        let (owned, parts) = fixture();
        let stats = parts.as_trie().unwrap().verify(VALUE_LIMIT).unwrap();
        assert_eq!(stats.keys, owned.len());
        assert_eq!(stats.used, stats.internal + stats.leaves + stats.tails);
        assert_eq!(stats.node_count, parts.nodes.len());
        assert_eq!(stats.tail_bytes, parts.tails.len());
        assert!(stats.tails > 0 && stats.internal > 1);
    }

    #[test]
    fn test_verify_detects_bad_root() {
        let (_, mut parts) = fixture();
        parts.nodes[0].base = leaf_base(0);
        assert!(verify_err(&parts).contains("root"));
        let (_, mut parts) = fixture();
        parts.nodes[0].base |= HAS_END;
        assert!(verify_err(&parts).contains("END"));
        let (_, mut parts) = fixture();
        parts.nodes[0].check = 5;
        assert!(verify_err(&parts).contains("root"));
    }

    #[test]
    fn test_verify_detects_kind_01() {
        let (_, mut parts) = fixture();
        let (slot, _) = node_at(&parts, "東");
        parts.nodes[slot].base = (0b01 << KIND_SHIFT) | 3;
        assert!(verify_err(&parts).contains("kind 01"));
        assert!(search_err(&parts, "東京").contains("kind 01"));
    }

    #[test]
    fn test_verify_detects_bad_end_child() {
        let (_, parts) = fixture();
        let (slot, node) = node_at(&parts, "ab");
        let end = (node.base & BASE_MASK) as usize;
        // END 子が LEAF でない
        let mut broken = parts.clone();
        broken.nodes[end].base = KIND_TAIL << KIND_SHIFT;
        assert!(verify_err(&broken).contains("END"));
        assert!(search_err(&broken, "abc").contains("HAS_END"));
        // END 子の check が親でない
        let mut broken = parts.clone();
        broken.nodes[end].check = slot as u32 + 1;
        assert!(!verify_err(&broken).is_empty());
        assert!(search_err(&broken, "abc").contains("HAS_END"));
        // HAS_END を持たない親の END 位置に LEAF がある
        let mut broken = parts.clone();
        broken.nodes[slot].base &= !HAS_END;
        assert!(verify_err(&broken).contains("no HAS_END"));
    }

    #[test]
    fn test_verify_detects_base_out_of_range() {
        let (_, mut parts) = fixture();
        let n = parts.as_trie().unwrap().max_code() as usize;
        let (slot, _) = node_at(&parts, "東");
        let len = parts.nodes.len();
        parts.nodes[slot].base = (len - n) as u32; // base + N == ノード数
        assert!(verify_err(&parts).contains("out of range"));
        assert!(search_err(&parts, "東京").contains("out of range"));
        parts.nodes[slot].base = 0;
        assert!(verify_err(&parts).contains("out of range"));
    }

    #[test]
    fn test_verify_detects_broken_tail() {
        let (owned, parts) = fixture();
        let long = owned.iter().find(|k| k.starts_with('z')).unwrap().clone();
        let (slot, node) = node_at(&parts, "z");
        assert_eq!(kind(node.base), KIND_TAIL);
        let off = (node.base & PAYLOAD_MASK) as usize;

        // オフセットが範囲外
        let mut broken = parts.clone();
        broken.nodes[slot].base = (KIND_TAIL << KIND_SHIFT) | broken.tails.len() as u32;
        assert!(verify_err(&broken).contains("past the end"));
        assert!(search_err(&broken, &long).contains("past the end"));

        // varint が 5 バイトを超える
        let mut broken = parts.clone();
        broken.tails[off + 4..off + 9].fill(0x80);
        assert!(verify_err(&broken).contains("varint"));
        assert!(search_err(&broken, &long).contains("varint"));

        // 長さが tails を越える
        let mut broken = parts.clone();
        broken.tails[off + 4..off + 7].copy_from_slice(&[0xFF, 0xFF, 0x03]);
        assert!(verify_err(&broken).contains("past the end"));

        // 長さ 0
        let mut broken = parts.clone();
        broken.tails[off + 4] = 0;
        assert!(verify_err(&broken).contains("length is 0"));

        // UTF-8 として不正（検索は一致しなければ気づかない）
        let mut broken = parts.clone();
        let last = off + 4 + 2 + (long.len() - 1) - 1;
        broken.tails[last] = 0xFF;
        assert!(verify_err(&broken).contains("UTF-8"));
        // 文字の途中で切れた suffix は、一致したときに検索が検出する
        let mut broken = parts.clone();
        let len = long.len() - 1 - 1;
        broken.tails[off + 4..off + 6]
            .copy_from_slice(&[(len as u8 & 0x7F) | 0x80, (len >> 7) as u8]);
        assert!(verify_err(&broken).contains("UTF-8"));
        assert!(search_err(&broken, &long).contains("UTF-8"));

        // 値が 2^30 以上
        let mut broken = parts.clone();
        broken.tails[off..off + 4].copy_from_slice(&VALUE_LIMIT.to_le_bytes());
        assert!(verify_err(&broken).contains("2^30"));
        assert!(search_err(&broken, &long).contains("2^30"));
    }

    #[test]
    fn test_verify_detects_value_limit() {
        let (owned, parts) = fixture();
        let trie = parts.as_trie().unwrap();
        let n = owned.len() as u32;
        assert!(trie.verify(n).is_ok());
        let err = trie.verify(n - 1).unwrap_err().to_string();
        assert!(err.contains("not below"), "{err}");
    }

    #[test]
    fn test_verify_detects_unused_slot_with_base() {
        let (_, mut parts) = fixture();
        let slot = parts.nodes.iter().position(|n| n.check == UNUSED).unwrap();
        parts.nodes[slot].base = 1;
        assert!(verify_err(&parts).contains("unused slot"));
    }

    #[test]
    fn test_verify_detects_cycle_and_detached_nodes() {
        // 予約スロット 2・3 で互いを親とする閉路を作る（root からは到達できない）
        let (_, mut parts) = fixture();
        assert!(parts.as_trie().unwrap().max_code() >= 3);
        parts.nodes[2] = Node { base: 2, check: 3 };
        parts.nodes[3] = Node { base: 1, check: 2 };
        assert!(verify_err(&parts).contains("not reachable"));

        // 自分自身を親とする
        let (_, mut parts) = fixture();
        parts.nodes[2] = Node { base: 1, check: 2 };
        assert!(verify_err(&parts).contains("not reachable"));

        // 親が LEAF
        let (_, mut parts) = fixture();
        let leaf = first_slot(&parts, |_, n| kind(n.base) == KIND_LEAF);
        parts.nodes[2] = Node {
            base: leaf_base(0),
            check: leaf as u32,
        };
        assert!(verify_err(&parts).contains("not an INTERNAL"));

        // 親の子の範囲の外を指す
        let (_, mut parts) = fixture();
        parts.nodes[2] = Node {
            base: leaf_base(0),
            check: 0,
        };
        assert!(verify_err(&parts).contains("not among its children"));
    }

    #[test]
    fn test_verify_detects_internal_with_one_key() {
        // 子が END だけの INTERNAL（LEAF にすべきもの）
        let (_, mut parts) = fixture();
        let (slot, node) = node_at(&parts, "b");
        assert_eq!(kind(node.base), KIND_LEAF);
        let value = node.base & PAYLOAD_MASK;
        parts.nodes[slot].base = HAS_END | 1;
        parts.nodes[1] = Node {
            base: leaf_base(value),
            check: slot as u32,
        };
        // 検索としては正しく引ける
        assert_eq!(search(&parts.as_trie().unwrap(), "bb", 0), vec![(1, value)]);
        assert!(verify_err(&parts).contains("must be a LEAF or TAIL"));
    }

    #[test]
    fn test_verify_detects_char_map_errors() {
        let (_, parts) = fixture();
        let trie = parts.as_trie().unwrap();
        let num_tables = (parts.tables.len() / TABLE_LEN) as u16;
        let table_a = parts.blocks[0] as usize;
        let code_a = trie.char_code('a') as usize;
        let code_b = trie.char_code('b') as usize;

        // block が表の範囲外
        let mut broken = parts.clone();
        broken.blocks[0x41] = num_tables;
        assert!(verify_err(&broken).contains("refers to table"));
        // 表 0 が非ゼロ
        let mut broken = parts.clone();
        broken.tables[5] = 1;
        assert!(verify_err(&broken).contains("table 0"));
        // 符号の重複
        let mut broken = parts.clone();
        broken.tables[table_a * TABLE_LEN + 'c' as usize] = code_a as u16;
        assert!(verify_err(&broken).contains("assigned to both"));
        // 符号の欠番（最大でない符号を消す）
        let mut broken = parts.clone();
        let max = trie.max_code() as usize;
        let victim = if code_b != max { 'b' } else { 'a' };
        broken.tables[table_a * TABLE_LEN + victim as usize] = 0;
        assert!(verify_err(&broken).contains("not assigned"));
        // 表の共有・参照されない表
        let mut broken = parts.clone();
        broken.blocks[0x41] = broken.blocks[0];
        assert!(verify_err(&broken).contains("shared"));
        let mut broken = parts.clone();
        broken.tables.extend_from_slice(&[0; TABLE_LEN]);
        assert!(verify_err(&broken).contains("not referenced"));
        // サロゲートへの割り当て
        let mut broken = parts.clone();
        broken.blocks[0xD8] = num_tables;
        broken
            .tables
            .extend((0..TABLE_LEN).map(|i| if i == 0 { max as u16 + 1 } else { 0 }));
        assert!(verify_err(&broken).contains("surrogate"));
        // 長さの不正
        let mut broken = parts.clone();
        broken.blocks.pop();
        assert!(verify_err(&broken).contains("blocks"));
        let mut broken = parts.clone();
        broken.tables.pop();
        assert!(verify_err(&broken).contains("multiple"));
    }

    // --- 壊れた入力で panic しない ---

    /// ランダムに 1〜4 か所を書き換える
    fn mutate(parts: &mut TrieParts, rng: &mut XorShift) {
        for _ in 0..1 + rng.below(4) {
            match rng.below(10) {
                0 if !parts.blocks.is_empty() => {
                    let i = rng.below(parts.blocks.len());
                    parts.blocks[i] = rng.below(parts.tables.len() / TABLE_LEN + 2) as u16;
                }
                1 if !parts.tables.is_empty() => {
                    let i = rng.below(parts.tables.len());
                    parts.tables[i] = if rng.below(2) == 0 {
                        rng.next() as u16
                    } else {
                        rng.below(40) as u16
                    };
                }
                2..=7 if !parts.nodes.is_empty() => {
                    let len = parts.nodes.len();
                    let node = &mut parts.nodes[rng.below(len)];
                    match rng.below(6) {
                        0 => node.base = rng.next() as u32,
                        1 => node.base ^= 1 << rng.below(32),
                        2 => node.check = rng.below(len + 2) as u32,
                        3 => node.check = UNUSED,
                        4 => node.base = (node.base & 0x3FFF_FFFF) | ((rng.below(4) as u32) << 30),
                        _ => node.base = (node.base & !BASE_MASK) | rng.below(len + 2) as u32,
                    }
                }
                8 => {
                    // バイト列として書き換える（型をまたいだ任意の位置）
                    let bytes: &mut [u8] = match rng.below(4) {
                        0 => bytemuck::cast_slice_mut(&mut parts.nodes),
                        1 => bytemuck::cast_slice_mut(&mut parts.tables),
                        2 => bytemuck::cast_slice_mut(&mut parts.blocks),
                        _ => &mut parts.tails,
                    };
                    if !bytes.is_empty() {
                        let i = rng.below(bytes.len());
                        bytes[i] = rng.next() as u8;
                    }
                }
                _ => match rng.below(4) {
                    0 => parts.nodes.truncate(rng.below(parts.nodes.len() + 1)),
                    1 => parts.tails.truncate(rng.below(parts.tails.len() + 1)),
                    2 => parts.tables.truncate(rng.below(parts.tables.len() + 1)),
                    _ => parts.blocks.truncate(rng.below(parts.blocks.len() + 1)),
                },
            }
        }
    }

    /// verify を通った trie は、keys() の集合どおりに検索できる
    fn assert_consistent(trie: &Trie, rng: &mut XorShift) {
        let keys = trie.keys().unwrap();
        let map: HashMap<String, u32> = keys.iter().map(|(v, k)| (k.clone(), *v)).collect();
        assert_eq!(map.len(), keys.len(), "keys are unique");
        for (value, key) in &keys {
            assert_eq!(trie.get(key).unwrap(), Some(*value));
        }
        for _ in 0..5 {
            let len = rng.below(10);
            let text = random_text(rng, len);
            for (start, _) in text.char_indices() {
                let mut found = Vec::new();
                trie.common_prefix_search(&text, start, |e, v| found.push((e, v)))
                    .unwrap();
                assert_eq!(found, brute_force(&map, &text, start));
            }
        }
    }

    #[test]
    fn test_corrupted_tries_never_panic() {
        let mut rng = XorShift(0xDEAD_BEEF_CAFE_F00D);
        let mut owned = random_keys(&mut rng, 120, 5);
        owned.push(format!("😀{}", "ー".repeat(50)));
        owned.sort();
        owned.dedup();
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let original = build_indexed(&keys);
        let max_code = original.as_trie().unwrap().max_code();
        let texts: Vec<String> = (0..8)
            .map(|i| {
                if i % 2 == 0 {
                    format!(
                        "{}{}",
                        owned[rng.below(owned.len())],
                        random_text(&mut rng, 4)
                    )
                } else {
                    random_text(&mut rng, 10)
                }
            })
            .collect();
        let mut verified = 0;
        for _ in 0..4000 {
            let mut parts = original.clone();
            mutate(&mut parts, &mut rng);
            let loaded = parts.as_trie();
            // 検証していない部品と任意の最大符号でも panic しない
            let claimed = match rng.below(4) {
                0 => max_code,
                1 => rng.below(70_000) as u32,
                2 => 0,
                _ => u32::MAX,
            };
            let unchecked = Trie::from_validated(
                &parts.blocks,
                &parts.tables,
                &parts.nodes,
                &parts.tails,
                claimed,
            );
            for trie in loaded.iter().chain(std::iter::once(&unchecked)) {
                for text in &texts {
                    for (start, _) in text.char_indices() {
                        let mut last = start;
                        let mut found = Vec::new();
                        let result = trie.common_prefix_search(text, start, |end, v| {
                            // 報告する終了位置は start より大きい文字境界で、昇順
                            assert!(end > last && text.is_char_boundary(end));
                            last = end;
                            found.push((end, v));
                        });
                        // 符号の列で辿っても、同じ一致・同じエラーになる
                        assert_eq!(search_codes(trie, text, start), (found, result));
                    }
                    let _ = trie.get(text);
                }
                let _ = trie.keys();
                if trie.verify(VALUE_LIMIT).is_ok() {
                    verified += 1;
                    assert_consistent(trie, &mut rng);
                }
            }
        }
        // 値や TAIL の中身だけを書き換えたものなど、正しい trie のままの変異もある
        assert!(verified > 0);
    }

    #[test]
    fn test_from_validated_with_garbage_does_not_panic() {
        let (owned, parts) = fixture();
        let bad_root = [Node {
            base: leaf_base(1),
            check: 0,
        }];
        let self_loop = [Node { base: 1, check: 0 }, Node { base: 1, check: 0 }];
        let empty_nodes: &[Node] = &[];
        type Raw<'p> = (&'p [u16], &'p [u16], &'p [Node], &'p [u8]);
        let cases: [Raw; 5] = [
            (&[], &[], empty_nodes, &[]),
            (&parts.blocks, &parts.tables, empty_nodes, &[]),
            (&parts.blocks, &parts.tables, &bad_root, &parts.tails),
            (&parts.blocks, &parts.tables, &self_loop, &[]),
            (
                &parts.blocks[..10],
                &parts.tables[..300],
                &parts.nodes,
                &parts.tails[..5],
            ),
        ];
        let texts: Vec<String> = owned.iter().map(|k| format!("{k}{k}")).collect();
        for (blocks, tables, nodes, tails) in cases {
            for max_code in [0, 1, 3, 65535, u32::MAX] {
                let view = Trie::from_validated(blocks, tables, nodes, tails, max_code);
                for text in &texts {
                    for (start, _) in text.char_indices() {
                        let mut last = start;
                        let _ = view.common_prefix_search(text, start, |end, _| {
                            assert!(end > last && text.is_char_boundary(end));
                            last = end;
                        });
                    }
                    let _ = view.get(text);
                }
                assert!(view.keys().is_err());
                assert!(view.verify(VALUE_LIMIT).is_err());
            }
        }
    }

    #[test]
    fn test_from_validated_matches_new() {
        let (owned, parts) = fixture();
        let trie = parts.as_trie().unwrap();
        let view = Trie::from_validated(
            &parts.blocks,
            &parts.tables,
            &parts.nodes,
            &parts.tails,
            trie.max_code(),
        );
        for key in &owned {
            assert_eq!(view.get(key).unwrap(), trie.get(key).unwrap());
        }
        assert_eq!(
            view.verify(VALUE_LIMIT).unwrap(),
            trie.verify(VALUE_LIMIT).unwrap()
        );
        // 最大符号が表と食い違うビューは keys・verify が拒む
        let wrong = Trie::from_validated(
            &parts.blocks,
            &parts.tables,
            &parts.nodes,
            &parts.tails,
            trie.max_code() + 1,
        );
        assert!(wrong.verify(VALUE_LIMIT).is_err());
        assert!(wrong.keys().is_err());
    }

    #[test]
    fn test_keys_of_prefix_heavy_set() {
        // 同じ接頭辞・END・TAIL が入り組んだ集合で keys() が元に戻る
        let mut rng = XorShift(4242);
        let mut owned = random_keys(&mut rng, 800, 4);
        let extra: Vec<String> = owned
            .iter()
            .take(50)
            .map(|k| format!("{k}{k}{k}"))
            .collect();
        owned.extend(extra);
        owned.sort();
        owned.dedup();
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let values: Vec<u32> = (0..keys.len() as u32).rev().collect();
        let parts = build(&keys, &values).unwrap();
        assert_round_trip(&keys, &values, &parts);
        let listed: BTreeMap<String, u32> = parts
            .as_trie()
            .unwrap()
            .keys()
            .unwrap()
            .into_iter()
            .map(|(v, k)| (k, v))
            .collect();
        assert_eq!(listed.len(), keys.len());
    }
}
