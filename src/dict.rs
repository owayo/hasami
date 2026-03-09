//! 辞書モジュール - エントリ、接続コスト行列、辞書構築

use crate::char_class::{CharClass, CharClassifier};
use crate::trie::DoubleArrayTrie;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

/// 辞書エントリ（1形態素に対応）
#[derive(Clone, Debug)]
pub struct DictEntry {
    /// 表層形（Arc<str>で共有参照）
    pub surface: Arc<str>,
    /// 左文脈ID
    pub left_id: u16,
    /// 右文脈ID
    pub right_id: u16,
    /// 単語コスト
    pub cost: i16,
    /// 品詞情報（カンマ区切り）- Arc<str>で共有参照（クローンコスト最小）
    pub pos: Arc<str>,
    /// 原形
    pub base_form: Arc<str>,
    /// 読み
    pub reading: Arc<str>,
    /// 発音
    pub pronunciation: Arc<str>,
}

/// 未知語テンプレート
#[derive(Clone, Debug)]
pub struct UnkEntry {
    pub char_class: String,
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
    pub pos: String,
}

/// 接続コスト行列
#[derive(Clone)]
pub struct ConnectionMatrix {
    pub left_size: u16,
    pub right_size: u16,
    /// costs[right_id * left_size + left_id] = cost
    pub costs: Vec<i16>,
}

impl ConnectionMatrix {
    /// 接続コストを取得
    /// prev_right_id: 前のトークンの right_id
    /// next_left_id: 次のトークンの left_id
    #[inline(always)]
    pub fn cost(&self, prev_right_id: u16, next_left_id: u16) -> i32 {
        let row_start = prev_right_id as usize * self.left_size as usize;
        let idx = row_start + next_left_id as usize;
        if idx < self.costs.len() {
            // SAFETY: bounds checked above
            unsafe { *self.costs.get_unchecked(idx) as i32 }
        } else {
            0
        }
    }

    /// 指定 right_id の行スライスを取得（同じ prev ノードから複数の next ノードへ接続する場合に有用）
    #[inline(always)]
    pub fn row(&self, prev_right_id: u16) -> &[i16] {
        let row_start = prev_right_id as usize * self.left_size as usize;
        let row_end = row_start + self.left_size as usize;
        if row_end <= self.costs.len() {
            // SAFETY: bounds checked above
            unsafe { self.costs.get_unchecked(row_start..row_end) }
        } else {
            &[]
        }
    }
}

/// コンパイル済み辞書（ビルド時の中間構造体）
pub struct Dictionary {
    /// Double-Array Trie（表層形 → エントリID）
    pub trie: DoubleArrayTrie,
    /// 全辞書エントリ
    pub entries: Vec<DictEntry>,
    /// 接続コスト行列
    pub matrix: ConnectionMatrix,
    /// 文字クラス分類器
    pub char_classifier: CharClassifier,
    /// 未知語テンプレート（文字クラス名 → テンプレートリスト）
    pub unk_entries: HashMap<String, Vec<UnkEntry>>,
}

impl Dictionary {
    /// 表層形の共通接頭辞検索
    pub fn lookup(&self, input: &[u8]) -> Vec<(usize, Vec<&DictEntry>)> {
        self.trie
            .common_prefix_search(input)
            .into_iter()
            .map(|(len, ids)| {
                let entries: Vec<&DictEntry> =
                    ids.iter().map(|&id| &self.entries[id as usize]).collect();
                (len, entries)
            })
            .collect()
    }
}

/// 辞書ビルダー: MeCab形式のCSVから辞書を構築
pub struct DictBuilder {
    entries: Vec<DictEntry>,
    matrix: Option<ConnectionMatrix>,
    char_classifier: CharClassifier,
    unk_entries: HashMap<String, Vec<UnkEntry>>,
}

impl Default for DictBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DictBuilder {
    /// エントリを直接追加
    pub fn add_entry(&mut self, entry: DictEntry) {
        self.entries.push(entry);
    }

    pub fn new() -> Self {
        DictBuilder {
            entries: Vec::new(),
            matrix: None,
            char_classifier: CharClassifier::default_japanese(),
            unk_entries: HashMap::new(),
        }
    }

    /// 既存の .hsd 辞書からエントリをインポート（マージ用）
    pub fn load_hsd<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let dict = crate::mmap_dict::MmapDictionary::load(path)?;
        let count = dict.entry_count() as usize;

        for i in 0..count {
            let id = i as u32;
            let (left_id, right_id, cost) = dict.entry_cost_info(id);
            let surface: Arc<str> = Arc::from(dict.entry_surface(id));
            let pos: Arc<str> = Arc::from(dict.entry_pos(id));
            let base_form: Arc<str> = Arc::from(dict.entry_base_form(id));
            let reading: Arc<str> = Arc::from(dict.entry_reading(id));
            let pronunciation: Arc<str> = Arc::from(dict.entry_pronunciation(id));

            self.entries.push(DictEntry {
                surface,
                left_id,
                right_id,
                cost,
                pos,
                base_form,
                reading,
                pronunciation,
            });
        }

        // 接続行列もインポート（まだ設定されていなければ）
        if self.matrix.is_none() {
            let left_size = dict.matrix_left_size();
            let right_size = dict.matrix_right_size();
            let total = left_size as usize * right_size as usize;
            let mut costs = Vec::with_capacity(total);
            for r in 0..right_size {
                let row = dict.matrix_row(r);
                costs.extend_from_slice(row);
            }
            self.matrix = Some(ConnectionMatrix {
                left_size,
                right_size,
                costs,
            });
        }

        // 未知語テンプレートもインポート
        dict.export_unk_entries(&mut self.unk_entries);

        // CharClassifier もインポート
        dict.export_char_classifier(&mut self.char_classifier);

        eprintln!("Imported {} entries from existing dictionary", count);
        Ok(())
    }

    /// 現在のエントリ数
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// 接続行列を直接設定
    pub fn set_matrix(&mut self, matrix: ConnectionMatrix) {
        self.matrix = Some(matrix);
    }

    /// CharClassifier を直接設定
    pub fn set_char_classifier(&mut self, classifier: CharClassifier) {
        self.char_classifier = classifier;
    }

    /// MeCab形式のCSVファイルからエントリを追加
    /// 形式: surface,left_id,right_id,cost,pos1,pos2,pos3,pos4,conj_type,conj_form,base_form,reading,pronunciation
    pub fn add_csv<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();

        // ファイルをバイト列として読み込み、エンコーディングを検出
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(content.as_bytes());

        for result in rdr.records() {
            let record = match result {
                Ok(r) => r,
                Err(_) => continue,
            };
            if record.len() < 5 {
                continue;
            }

            let surface: Arc<str> = Arc::from(&record[0]);
            let left_id: u16 = record[1].parse().unwrap_or(0);
            let right_id: u16 = record[2].parse().unwrap_or(0);
            let cost: i16 = record[3].parse().unwrap_or(0);

            // 品詞情報を結合
            let pos_parts: Vec<&str> = (4..record.len().min(8))
                .filter_map(|i| record.get(i))
                .collect();
            let pos = pos_parts.join(",");

            let base_form: Arc<str> = record.get(10).unwrap_or(&surface).into();
            let reading: Arc<str> = record.get(11).unwrap_or("").into();
            let pronunciation: Arc<str> = record.get(12).unwrap_or("").into();

            self.entries.push(DictEntry {
                surface,
                left_id,
                right_id,
                cost,
                pos: pos.into(),
                base_form,
                reading,
                pronunciation,
            });
        }

        Ok(())
    }

    /// CSV ディレクトリ内の全CSVファイルを読み込み
    pub fn add_csv_dir<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        let pattern = format!("{}/*.csv", dir.as_ref().display());
        let paths: Vec<_> = glob::glob(&pattern)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .filter_map(|r| r.ok())
            .collect();

        if paths.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No CSV files found in {}", dir.as_ref().display()),
            ));
        }

        for path in &paths {
            eprintln!("Loading: {}", path.display());
            self.add_csv(path)?;
        }
        eprintln!(
            "Loaded {} entries from {} files",
            self.entries.len(),
            paths.len()
        );

        Ok(())
    }

    /// matrix.def を読み込み
    pub fn load_matrix<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);
        let mut lines = content.lines();

        let header = lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Empty matrix file"))?;
        let parts: Vec<&str> = header.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid matrix header",
            ));
        }

        let left_size: u16 = parts[0].parse().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid left_size: {}", e),
            )
        })?;
        let right_size: u16 = parts[1].parse().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid right_size: {}", e),
            )
        })?;

        let total = left_size as usize * right_size as usize;
        let mut costs = vec![0i16; total];

        for line in lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let right_id: usize = parts[0].parse().unwrap_or(0);
            let left_id: usize = parts[1].parse().unwrap_or(0);
            let cost: i16 = parts[2].parse().unwrap_or(0);

            let idx = right_id * left_size as usize + left_id;
            if idx < costs.len() {
                costs[idx] = cost;
            }
        }

        self.matrix = Some(ConnectionMatrix {
            left_size,
            right_size,
            costs,
        });

        eprintln!(
            "Loaded matrix: {}x{} ({} entries)",
            right_size, left_size, total
        );

        Ok(())
    }

    /// unk.def を読み込み（未知語テンプレート）
    pub fn load_unk<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(content.as_bytes());

        for result in rdr.records() {
            let record = match result {
                Ok(r) => r,
                Err(_) => continue,
            };
            if record.len() < 5 {
                continue;
            }

            let char_class = record[0].to_string();
            let left_id: u16 = record[1].parse().unwrap_or(0);
            let right_id: u16 = record[2].parse().unwrap_or(0);
            let cost: i16 = record[3].parse().unwrap_or(0);
            let pos_parts: Vec<&str> = (4..record.len().min(8))
                .filter_map(|i| record.get(i))
                .collect();
            let pos = pos_parts.join(",");

            self.unk_entries
                .entry(char_class.clone())
                .or_default()
                .push(UnkEntry {
                    char_class,
                    left_id,
                    right_id,
                    cost,
                    pos,
                });
        }

        eprintln!("Loaded {} unknown word categories", self.unk_entries.len());

        Ok(())
    }

    /// char.def を読み込み
    pub fn load_char_def<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);

        let mut categories: HashMap<String, CharClass> = HashMap::new();
        let mut ranges: Vec<(u32, u32, String)> = Vec::new();
        let mut in_category_section = true;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Unicode範囲マッピング: 0xHHHH or 0xHHHH..0xHHHH CATEGORY
            if line.starts_with("0x") {
                in_category_section = false;
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 2 {
                    continue;
                }
                let range_part = parts[0];
                let category = parts[1].to_string();

                if let Some((start, end)) = Self::parse_range(range_part) {
                    ranges.push((start, end, category));
                }
                continue;
            }

            // カテゴリ定義: CATEGORY_NAME invoke group length
            if in_category_section {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let name = parts[0].to_string();
                    let invoke: bool = parts[1] == "1";
                    let group: bool = parts[2] == "1";
                    let length: u32 = parts[3].parse().unwrap_or(0);

                    categories.insert(
                        name.clone(),
                        CharClass {
                            name,
                            invoke,
                            group,
                            length,
                        },
                    );
                }
            }
        }

        if !categories.is_empty() {
            self.char_classifier = CharClassifier::from_definitions(categories, ranges);
        }

        Ok(())
    }

    fn parse_range(s: &str) -> Option<(u32, u32)> {
        if let Some(pos) = s.find("..") {
            let start = u32::from_str_radix(s[2..pos].trim(), 16).ok()?;
            let end_str = &s[pos + 2..];
            let end = u32::from_str_radix(end_str.strip_prefix("0x").unwrap_or(end_str).trim(), 16)
                .ok()?;
            Some((start, end))
        } else {
            let val = u32::from_str_radix(s[2..].trim(), 16).ok()?;
            Some((val, val))
        }
    }

    /// バイト列をUTF-8にデコード（EUC-JP自動検出対応）
    fn decode_to_utf8(bytes: &[u8]) -> String {
        // まずUTF-8として試す
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
        // EUC-JPとしてデコード
        let (cow, _, _) = encoding_rs::EUC_JP.decode(bytes);
        cow.into_owned()
    }

    /// 辞書をビルド
    pub fn build(self) -> Dictionary {
        // エントリからTrieを構築
        let mut trie_entries: Vec<(&[u8], u32)> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.surface.as_bytes(), i as u32))
            .collect();

        // バイト列でソート（Trie構築に必要ではないが効率向上）
        trie_entries.sort_by(|a, b| a.0.cmp(b.0));

        eprintln!("Building trie with {} entries...", trie_entries.len());
        let trie = DoubleArrayTrie::build(&trie_entries);
        eprintln!(
            "Trie built: {} nodes, {} bytes",
            trie.num_nodes(),
            trie.memory_usage()
        );

        let matrix = self.matrix.unwrap_or(ConnectionMatrix {
            left_size: 1,
            right_size: 1,
            costs: vec![0],
        });

        Dictionary {
            trie,
            entries: self.entries,
            matrix,
            char_classifier: self.char_classifier,
            unk_entries: self.unk_entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_and_lookup() {
        let mut builder = DictBuilder::new();
        builder.add_entry(DictEntry {
            surface: "東京".into(),
            left_id: 1,
            right_id: 1,
            cost: 3000,
            pos: "名詞,固有名詞,地域,一般".into(),
            base_form: "東京".into(),
            reading: "トウキョウ".into(),
            pronunciation: "トーキョー".into(),
        });
        builder.add_entry(DictEntry {
            surface: "都".into(),
            left_id: 2,
            right_id: 2,
            cost: 4000,
            pos: "名詞,接尾,地域,*".into(),
            base_form: "都".into(),
            reading: "ト".into(),
            pronunciation: "ト".into(),
        });

        let dict = builder.build();
        let results = dict.lookup("東京都".as_bytes());
        assert!(!results.is_empty());
        assert_eq!(&*results[0].1[0].surface, "東京");
    }
}
