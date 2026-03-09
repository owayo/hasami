//! ラティス構築 + Viterbi デコーディング（最適化版）

use crate::char_class::{CharType, type_index};
use crate::dict::Dictionary;
use std::sync::{Arc, LazyLock};

/// トークン（形態素解析結果の1単位）
#[derive(Debug, Clone)]
pub struct Token {
    /// 表層形
    pub surface: Arc<str>,
    /// 開始バイト位置
    pub start: usize,
    /// 終了バイト位置
    pub end: usize,
    /// 品詞情報
    pub pos: Arc<str>,
    /// 原形
    pub base_form: Arc<str>,
    /// 読み
    pub reading: Arc<str>,
    /// 発音
    pub pronunciation: Arc<str>,
    /// 単語コスト
    pub word_cost: i16,
    /// 辞書由来 (true=辞書, false=未知語)
    pub is_known: bool,
}

/// 未知語品詞のグローバルArc（一度だけ生成、以降はArc::clone）
static UNK_POS_NOUN_GENERAL: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("名詞,一般,*,*"));
static UNK_POS_NOUN_PROPER_ORG: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::from("名詞,固有名詞,組織,*"));
static UNK_POS_NOUN_NUMBER: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("名詞,数,*,*"));
static UNK_POS_SYMBOL_GENERAL: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("記号,一般,*,*"));
static UNK_POS_SYMBOL_SPACE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("記号,空白,*,*"));
static UNK_POS_NOUN_SAHEN: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("名詞,サ変接続,*,*"));
static EMPTY_ARC: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from(""));

/// ラティスノード（コンパクト表現 - Viterbiデータ分離）
#[derive(Clone, Copy)]
struct LatticeNode {
    start: u32,
    end: u32,
    /// 辞書エントリID。BOUNDARY_ID = BOS/EOS、UNK_FLAG付き = 未知語
    entry_id: u32,
    left_id: u16,
    right_id: u16,
    word_cost: i16,
    char_type: CharType,
    /// 未知語テンプレートインデックス（unk_table内のインデックス）
    unk_template_idx: u8,
}

const BOUNDARY_ID: u32 = u32::MAX;
const NO_PREV: u32 = u32::MAX;
const UNK_FLAG: u32 = 0x8000_0000;

impl LatticeNode {
    #[inline]
    fn is_known(&self) -> bool {
        self.entry_id != BOUNDARY_ID && (self.entry_id & UNK_FLAG) == 0
    }

    #[inline]
    fn is_boundary(&self) -> bool {
        self.entry_id == BOUNDARY_ID
    }
}

/// CharType ごとの未知語パラメータ（事前計算テーブル用）
#[derive(Clone)]
struct UnkParams {
    left_id: u16,
    right_id: u16,
    cost: i16,
    pos: Arc<str>,
}

/// CharType ごとの未知語テンプレート一覧＋invokeフラグ
#[derive(Clone)]
struct UnkTemplates {
    templates: Vec<UnkParams>,
    invoke: bool,
}

const NUM_CHAR_TYPES: usize = 9;

/// 辞書から未知語パラメータの事前計算テーブルを構築（全テンプレートを保持）
fn build_unk_table(dict: &Dictionary) -> Vec<UnkTemplates> {
    let all_types = [
        CharType::Hiragana,
        CharType::Katakana,
        CharType::Kanji,
        CharType::Alpha,
        CharType::Numeric,
        CharType::NumericWide,
        CharType::Symbol,
        CharType::Space,
        CharType::Default,
    ];
    let mut table: Vec<UnkTemplates> = (0..NUM_CHAR_TYPES)
        .map(|_| UnkTemplates {
            templates: Vec::new(),
            invoke: false,
        })
        .collect();

    for &ct in &all_types {
        let idx = type_index(ct);
        let class_name = ct.class_name();
        let invoke = dict
            .char_classifier
            .get_class(class_name)
            .is_some_and(|cl| cl.invoke);

        let templates = if let Some(unk_entries) = dict.unk_entries.get(class_name) {
            unk_entries
                .iter()
                .map(|unk| UnkParams {
                    left_id: unk.left_id,
                    right_id: unk.right_id,
                    cost: unk.cost,
                    pos: Arc::from(unk.pos.as_str()),
                })
                .collect()
        } else {
            // フォールバック: デフォルトコスト＋デフォルトPOSで1テンプレート
            vec![UnkParams {
                left_id: 0,
                right_id: 0,
                cost: LatticeWorkspace::default_unk_cost(ct),
                pos: LatticeWorkspace::unk_pos_arc(ct),
            }]
        };

        table[idx] = UnkTemplates { templates, invoke };
    }
    table
}

/// 再利用可能なラティスワークスペース
pub struct LatticeWorkspace {
    nodes: Vec<LatticeNode>,
    /// Viterbi用: total_cost[node_idx]
    costs: Vec<i64>,
    /// Viterbi用: prev[node_idx]
    prevs: Vec<u32>,
    /// end_nodes[byte_pos] = そのバイト位置で終了するノードインデックスのリスト
    end_nodes: Vec<Vec<u32>>,
    /// 未知語パラメータの事前計算テーブル（辞書ロード後に初期化、全テンプレート保持）
    unk_table: Option<Vec<UnkTemplates>>,
}

impl LatticeWorkspace {
    pub fn new() -> Self {
        LatticeWorkspace {
            nodes: Vec::with_capacity(4096),
            costs: Vec::with_capacity(4096),
            prevs: Vec::with_capacity(4096),
            end_nodes: Vec::with_capacity(1024),
            unk_table: None,
        }
    }

    fn clear(&mut self, byte_len: usize) {
        self.nodes.clear();
        self.costs.clear();
        self.prevs.clear();
        let positions = byte_len + 1;
        for v in self.end_nodes.iter_mut() {
            v.clear();
        }
        if self.end_nodes.len() < positions {
            self.end_nodes.resize_with(positions, Vec::new);
        } else {
            self.end_nodes.truncate(positions);
        }
    }

    #[inline]
    fn add_node(&mut self, end_pos: usize, node: LatticeNode, total_cost: i64) {
        let idx = self.nodes.len() as u32;
        self.nodes.push(node);
        self.costs.push(total_cost);
        self.prevs.push(NO_PREV);
        self.end_nodes[end_pos].push(idx);
    }

    /// ラティスを構築してViterbiで最適パスを探索
    pub fn tokenize(&mut self, input: &str, dict: &Dictionary) -> Vec<Token> {
        if input.is_empty() {
            return vec![];
        }

        let byte_len = input.len();
        let bytes = input.as_bytes();

        self.clear(byte_len);

        // BOS ノード
        self.add_node(
            0,
            LatticeNode {
                start: 0,
                end: 0,
                entry_id: BOUNDARY_ID,
                left_id: 0,
                right_id: 0,
                word_cost: 0,
                char_type: CharType::Default,
                unk_template_idx: 0,
            },
            0, // BOS total_cost = 0
        );

        // 未知語テーブルを初期化（初回のみ、以降はキャッシュ）
        // take() で一時的に所有権を移し、借用の競合を回避
        if self.unk_table.is_none() {
            self.unk_table = Some(build_unk_table(dict));
        }
        let unk_table = self.unk_table.take().unwrap();

        // 各文字位置から辞書引き + 未知語生成
        for (byte_pos, c) in input.char_indices() {
            let mut has_known = false;

            // 辞書引き（コールバック方式、ゼロアロケーション）
            dict.trie
                .common_prefix_search_cb(&bytes[byte_pos..], |match_len, entry_ids| {
                    let end = byte_pos + match_len;
                    for &eid in entry_ids {
                        let entry = &dict.entries[eid as usize];
                        self.add_node(
                            end,
                            LatticeNode {
                                start: byte_pos as u32,
                                end: end as u32,
                                entry_id: eid,
                                left_id: entry.left_id,
                                right_id: entry.right_id,
                                word_cost: entry.cost,
                                char_type: CharType::Default,
                                unk_template_idx: 0,
                            },
                            i64::MAX,
                        );
                        has_known = true;
                    }
                });

            // 未知語処理（事前計算テーブルで O(1) ルックアップ、全テンプレート展開）
            let char_type = dict.char_classifier.classify_char(c);
            let unk_templates = &unk_table[type_index(char_type)];

            if unk_templates.invoke || !has_known {
                let single_len = c.len_utf8();

                // 最もコストの低いテンプレート1つだけ使用（速度優先）
                let tmpl = &unk_templates.templates[0];
                let left_id = tmpl.left_id;
                let right_id = tmpl.right_id;
                let cost = tmpl.cost;

                let mut added_single = false;

                // コールバックで直接ノード追加（Vec アロケーション排除）
                dict.char_classifier
                    .group_at_cb(input, byte_pos, |group_len, _| {
                        let end = byte_pos + group_len;
                        if group_len == single_len {
                            added_single = true;
                        }
                        self.add_node(
                            end,
                            LatticeNode {
                                start: byte_pos as u32,
                                end: end as u32,
                                entry_id: UNK_FLAG,
                                left_id,
                                right_id,
                                word_cost: cost,
                                char_type,
                                unk_template_idx: 0,
                            },
                            i64::MAX,
                        );
                    });

                // 1文字未知語がまだなければ追加
                if !added_single {
                    let single_end = byte_pos + single_len;
                    self.add_node(
                        single_end,
                        LatticeNode {
                            start: byte_pos as u32,
                            end: single_end as u32,
                            entry_id: UNK_FLAG,
                            left_id,
                            right_id,
                            word_cost: cost,
                            char_type,
                            unk_template_idx: 0,
                        },
                        i64::MAX,
                    );
                }
            }
        }

        // 未知語テーブルを戻す（次回再利用のため）
        self.unk_table = Some(unk_table);

        // --- Viterbi forward pass (separate costs/prevs arrays for cache locality) ---
        for end_pos in 1..=byte_len {
            let num_nodes = self.end_nodes[end_pos].len();
            if num_nodes == 0 {
                continue;
            }
            for ni_idx in 0..num_nodes {
                let node_idx = self.end_nodes[end_pos][ni_idx] as usize;
                let node_start = self.nodes[node_idx].start as usize;
                let node_left_id = self.nodes[node_idx].left_id as usize;
                let node_word_cost = self.nodes[node_idx].word_cost as i64;

                let mut best_cost = i64::MAX;
                let mut best_prev = NO_PREV;

                let num_prev = self.end_nodes[node_start].len();
                for pi_idx in 0..num_prev {
                    let prev_idx = self.end_nodes[node_start][pi_idx] as usize;
                    let prev_total = self.costs[prev_idx];
                    if prev_total == i64::MAX {
                        continue;
                    }

                    let prev_right_id = self.nodes[prev_idx].right_id;
                    let row = dict.matrix.row(prev_right_id);
                    let conn_cost = if node_left_id < row.len() {
                        unsafe { *row.get_unchecked(node_left_id) as i64 }
                    } else {
                        0i64
                    };
                    let total = prev_total + conn_cost + node_word_cost;

                    if total < best_cost {
                        best_cost = total;
                        best_prev = prev_idx as u32;
                    }
                }

                self.costs[node_idx] = best_cost;
                self.prevs[node_idx] = best_prev;
            }
        }

        // --- EOS 最良前ノード決定 ---
        let num_last = self.end_nodes[byte_len].len();
        let mut best_cost = i64::MAX;
        let mut best_last = NO_PREV;

        for pi_idx in 0..num_last {
            let prev_idx = self.end_nodes[byte_len][pi_idx] as usize;
            let prev_total = self.costs[prev_idx];
            if prev_total == i64::MAX {
                continue;
            }
            let prev_right_id = self.nodes[prev_idx].right_id;
            let row = dict.matrix.row(prev_right_id);
            let conn_cost = if !row.is_empty() { row[0] as i64 } else { 0i64 };
            let total = prev_total + conn_cost;

            if total < best_cost {
                best_cost = total;
                best_last = prev_idx as u32;
            }
        }

        // --- トレースバック ---
        let mut path = Vec::with_capacity(32);
        let mut current = best_last;

        while current != NO_PREV {
            let ci = current as usize;
            if self.nodes[ci].is_boundary() {
                break;
            }
            path.push(current);
            current = self.prevs[ci];
        }

        path.reverse();

        // --- トークン生成 ---
        let empty = Arc::clone(&EMPTY_ARC);
        path.iter()
            .map(|&idx| {
                let node = &self.nodes[idx as usize];

                if node.is_known() {
                    let entry = &dict.entries[node.entry_id as usize];
                    Token {
                        surface: Arc::clone(&entry.surface),
                        start: node.start as usize,
                        end: node.end as usize,
                        pos: Arc::clone(&entry.pos),
                        base_form: Arc::clone(&entry.base_form),
                        reading: Arc::clone(&entry.reading),
                        pronunciation: Arc::clone(&entry.pronunciation),
                        word_cost: node.word_cost,
                        is_known: true,
                    }
                } else {
                    let surface: Arc<str> =
                        Arc::from(&input[node.start as usize..node.end as usize]);
                    let unk_pos = if let Some(ref unk_tbl) = self.unk_table {
                        let ct_idx = type_index(node.char_type);
                        let tmpl_idx = node.unk_template_idx as usize;
                        if ct_idx < unk_tbl.len() && tmpl_idx < unk_tbl[ct_idx].templates.len() {
                            Arc::clone(&unk_tbl[ct_idx].templates[tmpl_idx].pos)
                        } else {
                            Self::unk_pos_arc(node.char_type)
                        }
                    } else {
                        Self::unk_pos_arc(node.char_type)
                    };
                    Token {
                        start: node.start as usize,
                        end: node.end as usize,
                        pos: unk_pos,
                        base_form: Arc::clone(&surface),
                        reading: Arc::clone(&empty),
                        pronunciation: Arc::clone(&empty),
                        surface,
                        word_cost: node.word_cost,
                        is_known: false,
                    }
                }
            })
            .collect()
    }

    fn default_unk_cost(char_type: CharType) -> i16 {
        match char_type {
            CharType::Kanji => 7000,
            CharType::Hiragana => 8000,
            CharType::Katakana => 5000,
            CharType::Alpha => 6000,
            CharType::Numeric | CharType::NumericWide => 6000,
            CharType::Symbol => 9000,
            CharType::Space => 3000,
            CharType::Default => 10000,
        }
    }

    fn unk_pos_arc(char_type: CharType) -> Arc<str> {
        Arc::clone(match char_type {
            CharType::Kanji | CharType::Hiragana | CharType::Katakana => &UNK_POS_NOUN_GENERAL,
            CharType::Alpha => &UNK_POS_NOUN_PROPER_ORG,
            CharType::Numeric | CharType::NumericWide => &UNK_POS_NOUN_NUMBER,
            CharType::Symbol => &UNK_POS_SYMBOL_GENERAL,
            CharType::Space => &UNK_POS_SYMBOL_SPACE,
            CharType::Default => &UNK_POS_NOUN_SAHEN,
        })
    }

    /// v2 mmap辞書でラティス構築+Viterbi（ゼロコピー）
    pub fn tokenize_v2(
        &mut self,
        input: &str,
        dict: &crate::mmap_dict::MmapDictionary,
        classifier: &crate::char_class::CharClassifier,
    ) -> Vec<Token> {
        if input.is_empty() {
            return vec![];
        }

        let byte_len = input.len();
        let bytes = input.as_bytes();
        self.clear(byte_len);

        // BOS ノード
        self.add_node(
            0,
            LatticeNode {
                start: 0,
                end: 0,
                entry_id: BOUNDARY_ID,
                left_id: 0,
                right_id: 0,
                word_cost: 0,
                char_type: CharType::Default,
                unk_template_idx: 0,
            },
            0,
        );

        // 各文字位置から辞書引き + 未知語生成
        for (byte_pos, c) in input.char_indices() {
            let mut has_known = false;

            // 辞書引き（v2 mmap辞書のTrie直接参照）
            dict.common_prefix_search_cb(&bytes[byte_pos..], |match_len, entry_ids| {
                let end = byte_pos + match_len;
                for &eid in entry_ids {
                    let (left_id, right_id, cost) = dict.entry_cost_info(eid);
                    self.add_node(
                        end,
                        LatticeNode {
                            start: byte_pos as u32,
                            end: end as u32,
                            entry_id: eid,
                            left_id,
                            right_id,
                            word_cost: cost,
                            char_type: CharType::Default,
                            unk_template_idx: 0,
                        },
                        i64::MAX,
                    );
                    has_known = true;
                }
            });

            // 未知語処理
            let char_type = classifier.classify_char(c);
            let ct_idx = type_index(char_type);
            let invoke = dict.unk_invoke(ct_idx);

            if invoke || !has_known {
                let single_len = c.len_utf8();
                let (left_id, right_id, cost) = dict.unk_first_template(ct_idx);

                let mut added_single = false;
                classifier.group_at_cb(input, byte_pos, |group_len, _| {
                    let end = byte_pos + group_len;
                    if group_len == single_len {
                        added_single = true;
                    }
                    self.add_node(
                        end,
                        LatticeNode {
                            start: byte_pos as u32,
                            end: end as u32,
                            entry_id: UNK_FLAG,
                            left_id,
                            right_id,
                            word_cost: cost,
                            char_type,
                            unk_template_idx: 0,
                        },
                        i64::MAX,
                    );
                });

                if !added_single {
                    let single_end = byte_pos + single_len;
                    self.add_node(
                        single_end,
                        LatticeNode {
                            start: byte_pos as u32,
                            end: single_end as u32,
                            entry_id: UNK_FLAG,
                            left_id,
                            right_id,
                            word_cost: cost,
                            char_type,
                            unk_template_idx: 0,
                        },
                        i64::MAX,
                    );
                }
            }
        }

        // --- Viterbi forward pass ---
        for end_pos in 1..=byte_len {
            let num_nodes = self.end_nodes[end_pos].len();
            if num_nodes == 0 {
                continue;
            }
            for ni_idx in 0..num_nodes {
                let node_idx = self.end_nodes[end_pos][ni_idx] as usize;
                let node_start = self.nodes[node_idx].start as usize;
                let node_left_id = self.nodes[node_idx].left_id as usize;
                let node_word_cost = self.nodes[node_idx].word_cost as i64;

                let mut best_cost = i64::MAX;
                let mut best_prev = NO_PREV;

                let num_prev = self.end_nodes[node_start].len();
                for pi_idx in 0..num_prev {
                    let prev_idx = self.end_nodes[node_start][pi_idx] as usize;
                    let prev_total = self.costs[prev_idx];
                    if prev_total == i64::MAX {
                        continue;
                    }

                    let prev_right_id = self.nodes[prev_idx].right_id;
                    let row = dict.matrix_row(prev_right_id);
                    let conn_cost = if node_left_id < row.len() {
                        unsafe { *row.get_unchecked(node_left_id) as i64 }
                    } else {
                        0i64
                    };
                    let total = prev_total + conn_cost + node_word_cost;

                    if total < best_cost {
                        best_cost = total;
                        best_prev = prev_idx as u32;
                    }
                }

                self.costs[node_idx] = best_cost;
                self.prevs[node_idx] = best_prev;
            }
        }

        // --- EOS ---
        let num_last = self.end_nodes[byte_len].len();
        let mut best_cost = i64::MAX;
        let mut best_last = NO_PREV;

        for pi_idx in 0..num_last {
            let prev_idx = self.end_nodes[byte_len][pi_idx] as usize;
            let prev_total = self.costs[prev_idx];
            if prev_total == i64::MAX {
                continue;
            }
            let prev_right_id = self.nodes[prev_idx].right_id;
            let row = dict.matrix_row(prev_right_id);
            let conn_cost = if !row.is_empty() { row[0] as i64 } else { 0i64 };
            let total = prev_total + conn_cost;
            if total < best_cost {
                best_cost = total;
                best_last = prev_idx as u32;
            }
        }

        // --- トレースバック ---
        let mut path = Vec::with_capacity(32);
        let mut current = best_last;
        while current != NO_PREV {
            let ci = current as usize;
            if self.nodes[ci].is_boundary() {
                break;
            }
            path.push(current);
            current = self.prevs[ci];
        }
        path.reverse();

        // --- トークン生成（v2: Arcキャッシュから Arc::clone で取得） ---
        let empty = Arc::clone(&EMPTY_ARC);
        path.iter()
            .map(|&idx| {
                let node = &self.nodes[idx as usize];
                if node.is_known() {
                    let (surface, pos, base_form, reading, pronunciation) =
                        dict.entry_arcs(node.entry_id);
                    Token {
                        surface,
                        start: node.start as usize,
                        end: node.end as usize,
                        pos,
                        base_form,
                        reading,
                        pronunciation,
                        word_cost: node.word_cost,
                        is_known: true,
                    }
                } else {
                    let surface: Arc<str> =
                        Arc::from(&input[node.start as usize..node.end as usize]);
                    let unk_pos = dict.unk_pos_arc(type_index(node.char_type));
                    Token {
                        start: node.start as usize,
                        end: node.end as usize,
                        pos: unk_pos,
                        base_form: Arc::clone(&surface),
                        reading: Arc::clone(&empty),
                        pronunciation: Arc::clone(&empty),
                        surface,
                        word_cost: node.word_cost,
                        is_known: false,
                    }
                }
            })
            .collect()
    }
}

impl Default for LatticeWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};

    fn make_test_dict() -> Dictionary {
        let mut builder = DictBuilder::new();
        let words = vec![
            ("東京", 1, 1, 3000, "名詞,固有名詞,地域,一般"),
            ("都", 2, 2, 5000, "名詞,接尾,地域,*"),
            ("東京都", 3, 3, 2000, "名詞,固有名詞,地域,一般"),
            ("に", 4, 4, 4000, "助詞,格助詞,一般,*"),
            ("住む", 5, 5, 4500, "動詞,自立,*,*"),
            ("住ん", 5, 5, 4500, "動詞,自立,*,*"),
            ("で", 6, 6, 4000, "助詞,接続助詞,*,*"),
            ("いる", 7, 7, 4500, "動詞,非自立,*,*"),
        ];

        for (surface, lid, rid, cost, pos) in words {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: lid,
                right_id: rid,
                cost,
                pos: pos.into(),
                base_form: surface.into(),
                reading: "".into(),
                pronunciation: "".into(),
            });
        }

        builder.build()
    }

    #[test]
    fn test_lattice_tokenize() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("東京都に住んでいる", &dict);

        assert!(!tokens.is_empty());
        let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(reconstructed, "東京都に住んでいる");
    }

    #[test]
    fn test_workspace_reuse() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();

        let t1 = ws.tokenize("東京都", &dict);
        let t2 = ws.tokenize("東京都に住んでいる", &dict);

        assert!(!t1.is_empty());
        assert!(!t2.is_empty());
    }
}
