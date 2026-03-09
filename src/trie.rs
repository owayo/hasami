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

/// 構築時の空きスロット管理
struct SlotAllocator {
    used: Vec<bool>,
    search_start: usize,
}

impl SlotAllocator {
    fn new(initial_size: usize) -> Self {
        SlotAllocator {
            used: vec![false; initial_size],
            search_start: 1,
        }
    }

    fn ensure_size(&mut self, min_len: usize) {
        if self.used.len() < min_len {
            self.used.resize(min_len, false);
        }
    }

    fn mark_used(&mut self, pos: usize) {
        self.ensure_size(pos + 1);
        self.used[pos] = true;
        while self.search_start < self.used.len() && self.used[self.search_start] {
            self.search_start += 1;
        }
    }

    #[inline]
    fn is_free(&self, pos: usize) -> bool {
        pos >= self.used.len() || !self.used[pos]
    }

    fn find_base(&self, labels: &[u8]) -> i32 {
        let min_label = *labels.iter().min().unwrap() as usize;
        let start = if self.search_start > min_label {
            self.search_start - min_label
        } else {
            1
        };

        'outer: for b in start.. {
            for &label in labels {
                let idx = b + label as usize;
                if !self.is_free(idx) {
                    continue 'outer;
                }
            }
            return b as i32;
        }
        unreachable!()
    }
}

impl DoubleArrayTrie {
    /// ソートされたキー・値ペアからDouble-Array Trieを構築
    pub fn build(entries: &[(&[u8], u32)]) -> Self {
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
            if processed % 100_000 == 0 {
                eprintln!("  Trie progress: {}/{} nodes", processed, total);
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
}
