//! Double-Array Trie - 高速な共通接頭辞検索

use std::collections::{BTreeMap, VecDeque};

/// Double-Array Trie によるバイト列の高速検索
#[derive(Clone)]
pub struct DoubleArrayTrie {
    base: Vec<i32>,
    check: Vec<i32>,
    /// output[node] = value_pool 内のインデックス。u32::MAX = 値なし
    output: Vec<u32>,
    /// [count, val1, val2, ..., count, val1, ...] 形式のフラット配列
    value_pool: Vec<u32>,
}

/// 中間トライノード（構築時のみ使用）
struct BuildNode {
    children: BTreeMap<u8, usize>,
    values: Vec<u32>,
}

/// 構築時の空きスロット管理（ビットセットベース）
///
/// 空きスロットを `Vec<u64>` のビットセットで管理し、
/// `trailing_zeros` を使って次の空きスロットを高速に検索する。
/// 従来の `Vec<bool>` + 線形スキャンに比べ、64 スロットを 1 命令で
/// スキップできるため、大規模辞書の Trie 構築が大幅に高速化される。
struct SlotAllocator {
    /// ビットセット: bit i が 1 ならスロット i は使用済み
    words: Vec<u64>,
    /// 最小の未使用スロット（キャッシュ）
    search_start: usize,
}

impl SlotAllocator {
    fn new(initial_size: usize) -> Self {
        let nwords = initial_size.div_ceil(64);
        SlotAllocator {
            words: vec![0u64; nwords],
            search_start: 0,
        }
    }

    fn ensure_size(&mut self, min_len: usize) {
        let nwords = min_len.div_ceil(64);
        if nwords > self.words.len() {
            self.words.resize(nwords, 0);
        }
    }

    fn mark_used(&mut self, pos: usize) {
        self.ensure_size(pos + 1);
        let word_idx = pos / 64;
        self.words[word_idx] |= 1u64 << (pos % 64);
        if pos == self.search_start {
            self.search_start = self.next_free_from(pos + 1);
        }
    }

    #[inline]
    fn is_free(&self, pos: usize) -> bool {
        let word_idx = pos / 64;
        if word_idx >= self.words.len() {
            return true; // 範囲外 = 空き
        }
        self.words[word_idx] & (1u64 << (pos % 64)) == 0
    }

    /// `start` 以降の最初の空きスロットをビット演算で高速検索
    fn next_free_from(&self, start: usize) -> usize {
        let mut word_idx = start / 64;
        let bit_idx = start % 64;

        if word_idx >= self.words.len() {
            return start; // 範囲外 = 空き
        }

        // 最初のワード（部分マスク付き）
        let mask = !0u64 << bit_idx;
        let free_bits = !self.words[word_idx] & mask;
        if free_bits != 0 {
            return word_idx * 64 + free_bits.trailing_zeros() as usize;
        }

        // 後続ワードをスキャン
        word_idx += 1;
        while word_idx < self.words.len() {
            if self.words[word_idx] != u64::MAX {
                return word_idx * 64 + (!self.words[word_idx]).trailing_zeros() as usize;
            }
            word_idx += 1;
        }

        // 全て使用済み → 範囲外の先頭
        self.words.len() * 64
    }

    fn find_base(&self, labels: &[u8]) -> i32 {
        let min_label = *labels.iter().min().unwrap() as usize;

        // b >= 1 なので pos = b + min_label >= min_label + 1
        let start = self.search_start.max(min_label + 1);
        let mut pos = self.next_free_from(start);

        loop {
            let b = pos - min_label;

            let all_free = labels.iter().all(|&label| self.is_free(b + label as usize));

            if all_free {
                return b as i32;
            }

            pos = self.next_free_from(pos + 1);
        }
    }
}

impl DoubleArrayTrie {
    /// ソートされたキー・値ペアからDouble-Array Trieを構築
    pub fn build(entries: &[(&[u8], u32)]) -> Self {
        Self::build_with_progress(entries, |_, _| {})
    }

    /// プログレスコールバック付きでTrieを構築
    ///
    /// `progress(processed, total)` が定期的に呼び出される。
    pub fn build_with_progress(
        entries: &[(&[u8], u32)],
        mut progress: impl FnMut(usize, usize),
    ) -> Self {
        // Phase 1: 中間トライ構築
        let mut nodes: Vec<BuildNode> = vec![BuildNode {
            children: BTreeMap::new(),
            values: vec![],
        }];

        for &(key, value) in entries {
            let mut current = 0;
            for &byte in key {
                let next = if let Some(&child) = nodes[current].children.get(&byte) {
                    child
                } else {
                    let child = nodes.len();
                    nodes.push(BuildNode {
                        children: BTreeMap::new(),
                        values: vec![],
                    });
                    nodes[current].children.insert(byte, child);
                    child
                };
                current = next;
            }
            nodes[current].values.push(value);
        }

        // Phase 2: Double-Array 変換 (BFS)
        let initial_size = nodes.len() * 2 + 512;
        let mut base = vec![0i32; initial_size];
        let mut check = vec![-1i32; initial_size];
        let mut output = vec![u32::MAX; initial_size];
        let mut value_pool: Vec<u32> = Vec::new();
        let mut allocator = SlotAllocator::new(initial_size);

        allocator.mark_used(0);

        let mut node_map = vec![0usize; nodes.len()];
        let mut queue = VecDeque::new();
        queue.push_back(0usize);

        let mut processed = 0usize;
        let total = nodes.len();

        while let Some(int_node) = queue.pop_front() {
            let da_node = node_map[int_node];

            processed += 1;
            if processed % 10_000 == 0 {
                progress(processed, total);
            }

            if !nodes[int_node].values.is_empty() {
                Self::ensure_size(&mut output, da_node + 1, u32::MAX);
                output[da_node] = value_pool.len() as u32;
                value_pool.push(nodes[int_node].values.len() as u32);
                for &v in &nodes[int_node].values {
                    value_pool.push(v);
                }
            }

            if nodes[int_node].children.is_empty() {
                continue;
            }

            let labels: Vec<u8> = nodes[int_node].children.keys().copied().collect();
            let b = allocator.find_base(&labels);

            let max_label = *labels.last().unwrap() as usize; // labels はソート済み (BTreeMap)
            let max_idx = b as usize + max_label + 1;
            Self::ensure_size(&mut base, max_idx, 0i32);
            Self::ensure_size(&mut check, max_idx, -1i32);
            Self::ensure_size(&mut output, max_idx, u32::MAX);
            allocator.ensure_size(max_idx);

            base[da_node] = b;

            for (&label, &child_int) in &nodes[int_node].children {
                let child_da = b as usize + label as usize;
                check[child_da] = da_node as i32;
                allocator.mark_used(child_da);
                node_map[child_int] = child_da;
                queue.push_back(child_int);
            }
        }

        let len = check
            .iter()
            .rposition(|&c| c >= 0)
            .map(|p| p + 1)
            .unwrap_or(1)
            .max(1);
        base.truncate(len);
        check.truncate(len);
        output.truncate(len);

        DoubleArrayTrie {
            base,
            check,
            output,
            value_pool,
        }
    }

    fn ensure_size<T: Clone>(vec: &mut Vec<T>, min_len: usize, default: T) {
        if vec.len() < min_len {
            vec.resize(min_len, default);
        }
    }

    /// ゼロアロケーション共通接頭辞検索（コールバック方式）
    ///
    /// 各マッチに対して `cb(match_byte_len, value_slice)` を呼び出す。
    /// value_slice は value_pool の参照なのでアロケーションなし。
    #[inline]
    pub fn common_prefix_search_cb(&self, input: &[u8], mut cb: impl FnMut(usize, &[u32])) {
        let base = &self.base;
        let check = &self.check;
        let output = &self.output;
        let base_len = base.len();
        let check_len = check.len();
        let output_len = output.len();

        let mut node = 0usize;

        // ルートノードの出力チェック
        if output_len > 0 && output[0] != u32::MAX {
            cb(0, self.get_values_slice(0));
        }

        for (i, &byte) in input.iter().enumerate() {
            if node >= base_len {
                break;
            }
            let b = base[node];
            if b <= 0 {
                break;
            }
            let next = b as usize + byte as usize;

            if next >= check_len || check[next] != node as i32 {
                break;
            }

            node = next;

            if node < output_len && output[node] != u32::MAX {
                cb(i + 1, self.get_values_slice(node));
            }
        }
    }

    /// Vec を返す共通接頭辞検索（互換性のため残す）
    pub fn common_prefix_search(&self, input: &[u8]) -> Vec<(usize, Vec<u32>)> {
        let mut results = Vec::new();
        self.common_prefix_search_cb(input, |len, vals| {
            results.push((len, vals.to_vec()));
        });
        results
    }

    /// value_pool のスライス参照を返す（ゼロコピー）
    #[inline]
    fn get_values_slice(&self, node: usize) -> &[u32] {
        let pool_idx = self.output[node] as usize;
        let count = self.value_pool[pool_idx] as usize;
        &self.value_pool[pool_idx + 1..pool_idx + 1 + count]
    }

    /// ノード数
    pub fn num_nodes(&self) -> usize {
        self.base.len()
    }

    /// メモリ使用量（バイト）
    pub fn memory_usage(&self) -> usize {
        self.base.len() * 4
            + self.check.len() * 4
            + self.output.len() * 4
            + self.value_pool.len() * 4
    }

    /// 内部配列への直接アクセス（mmap辞書ビルダー用）
    pub fn base_slice(&self) -> &[i32] {
        &self.base
    }
    pub fn check_slice(&self) -> &[i32] {
        &self.check
    }
    pub fn output_slice(&self) -> &[u32] {
        &self.output
    }
    pub fn value_pool_slice(&self) -> &[u32] {
        &self.value_pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_trie() {
        let entries = vec![
            ("abc".as_bytes(), 0u32),
            ("abd".as_bytes(), 1),
            ("ab".as_bytes(), 2),
            ("b".as_bytes(), 3),
        ];
        let trie = DoubleArrayTrie::build(&entries);

        let results = trie.common_prefix_search(b"abcdef");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (2, vec![2]));
        assert_eq!(results[1], (3, vec![0]));
    }

    #[test]
    fn test_callback_api() {
        let entries = vec![("abc".as_bytes(), 0u32), ("ab".as_bytes(), 2)];
        let trie = DoubleArrayTrie::build(&entries);

        let mut results = Vec::new();
        trie.common_prefix_search_cb(b"abcdef", |len, vals| {
            results.push((len, vals[0]));
        });
        assert_eq!(results, vec![(2, 2), (3, 0)]);
    }

    #[test]
    fn test_japanese_utf8() {
        let entries = vec![
            ("東".as_bytes(), 0u32),
            ("東京".as_bytes(), 1),
            ("東京都".as_bytes(), 2),
        ];
        let trie = DoubleArrayTrie::build(&entries);

        let results = trie.common_prefix_search("東京都に住む".as_bytes());
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].1, vec![0]);
        assert_eq!(results[1].1, vec![1]);
        assert_eq!(results[2].1, vec![2]);
    }

    #[test]
    fn test_multiple_values() {
        let entries = vec![
            ("test".as_bytes(), 0u32),
            ("test".as_bytes(), 1),
            ("test".as_bytes(), 2),
        ];
        let trie = DoubleArrayTrie::build(&entries);

        let results = trie.common_prefix_search(b"test");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, vec![0, 1, 2]);
    }

    #[test]
    fn test_no_match() {
        let entries = vec![("hello".as_bytes(), 0u32)];
        let trie = DoubleArrayTrie::build(&entries);

        let results = trie.common_prefix_search(b"world");
        assert!(results.is_empty());
    }

    // --- 追加テスト ---

    #[test]
    fn test_empty_entries() {
        let entries: Vec<(&[u8], u32)> = vec![];
        let trie = DoubleArrayTrie::build(&entries);
        let results = trie.common_prefix_search(b"anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let entries = vec![("test".as_bytes(), 0u32)];
        let trie = DoubleArrayTrie::build(&entries);
        let results = trie.common_prefix_search(b"");
        assert!(results.is_empty());
    }

    #[test]
    fn test_single_byte_key() {
        let entries = vec![("a".as_bytes(), 42u32)];
        let trie = DoubleArrayTrie::build(&entries);
        let results = trie.common_prefix_search(b"abc");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], (1, vec![42]));
    }

    #[test]
    fn test_exact_match() {
        let entries = vec![("hello".as_bytes(), 0u32)];
        let trie = DoubleArrayTrie::build(&entries);
        let results = trie.common_prefix_search(b"hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 5);
    }

    #[test]
    fn test_prefix_chain() {
        // "a" < "ab" < "abc" < "abcd" should all match
        let entries = vec![
            ("a".as_bytes(), 0u32),
            ("ab".as_bytes(), 1),
            ("abc".as_bytes(), 2),
            ("abcd".as_bytes(), 3),
        ];
        let trie = DoubleArrayTrie::build(&entries);
        let results = trie.common_prefix_search(b"abcde");
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], (1, vec![0]));
        assert_eq!(results[1], (2, vec![1]));
        assert_eq!(results[2], (3, vec![2]));
        assert_eq!(results[3], (4, vec![3]));
    }

    #[test]
    fn test_large_trie() {
        // Build with many entries to stress test
        let words: Vec<String> = (0..1000).map(|i| format!("word{:04}", i)).collect();
        let entries: Vec<(&[u8], u32)> = words
            .iter()
            .enumerate()
            .map(|(i, w)| (w.as_bytes(), i as u32))
            .collect();
        let trie = DoubleArrayTrie::build(&entries);

        // Verify a few lookups
        let results = trie.common_prefix_search(b"word0042xxx");
        assert!(!results.is_empty());
        // "word0042" should match with value 42
        let found = results.iter().any(|(_, vals)| vals.contains(&42));
        assert!(found, "Expected value 42 in results: {:?}", results);
    }

    #[test]
    fn test_num_nodes_and_memory() {
        let entries = vec![
            ("cat".as_bytes(), 0u32),
            ("car".as_bytes(), 1),
            ("card".as_bytes(), 2),
        ];
        let trie = DoubleArrayTrie::build(&entries);
        assert!(trie.num_nodes() > 0);
        assert!(trie.memory_usage() > 0);
    }

    #[test]
    fn test_progress_callback() {
        let entries: Vec<String> = (0..100).map(|i| format!("entry{}", i)).collect();
        let entries: Vec<(&[u8], u32)> = entries
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_bytes(), i as u32))
            .collect();

        let mut called = false;
        let _trie = DoubleArrayTrie::build_with_progress(&entries, |processed, total| {
            assert!(processed <= total);
            called = true;
        });
        // With 100 entries, progress might not be called (threshold is 10,000)
        // but the trie should still be valid
        let _ = called;
    }

    #[test]
    fn test_shared_prefix_different_values() {
        let entries = vec![
            ("東京".as_bytes(), 0u32),
            ("東京都".as_bytes(), 1),
            ("東京タワー".as_bytes(), 2),
        ];
        let trie = DoubleArrayTrie::build(&entries);

        let results = trie.common_prefix_search("東京都庁".as_bytes());
        assert_eq!(results.len(), 2); // "東京" and "東京都"
        assert_eq!(results[0].1, vec![0]);
        assert_eq!(results[1].1, vec![1]);
    }

    #[test]
    fn test_internal_slices_accessible() {
        let entries = vec![("abc".as_bytes(), 0u32)];
        let trie = DoubleArrayTrie::build(&entries);
        assert!(!trie.base_slice().is_empty());
        assert!(!trie.check_slice().is_empty());
        assert!(!trie.output_slice().is_empty());
        assert!(!trie.value_pool_slice().is_empty());
    }
}
