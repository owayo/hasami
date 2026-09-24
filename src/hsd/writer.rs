//! v4 辞書の書き出し
//!
//! `DictBuilder` が集めたエントリ・接続行列・文字種定義・未知語テンプレートを、
//! 規範どおりのセクションに組み立てる。同じ入力と同じ hasami からは同じバイト列ができる
//! （文字符号は出現回数の降順・文字の昇順、素性レコードと文字列表は最初に現れた順、
//! 文字カテゴリは名前順）。

use super::container::{self, FLAG_PRUNED_DOMINATED, SectionId};
use super::features::{FeatureInput, FeatureTableBuilder};
use super::meta::{self, Meta};
use super::records::{
    CharCategoryRecord, CharRangeRecord, EntryRecord, LAST_IN_GROUP, LEFT_ID_LIMIT, UnkBucket,
    UnkTemplate,
};
use super::strtab::StringTableBuilder;
use super::{DictError, trie};
use crate::char_class::{ALL_CHAR_TYPES, CharClassifier, CharType};
use crate::dict::{ConnectionMatrix, DictEntry, UnkEntry};
use std::collections::HashMap;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// 品詞・活用型・活用形の番号は u16
const U16_TABLE_LIMIT: usize = u16::MAX as usize + 1;
/// 群の先頭番号は trie の値（30 ビット）に入る
const ENTRY_LIMIT: usize = trie::VALUE_LIMIT as usize;

/// 書き出しの設定
#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// メタデータ（`name`・`pos_scheme` と任意のキー）。`hasami_version`・`pruned_dominated`・
    /// `zero_matrix` は書き出し時に設定し直す
    pub meta: Meta,
    /// 同じ表層形・同じ文脈 ID の中でコストが最小でない（同コストなら後ろの）エントリを除く。
    /// 1-best の解析結果は変わらないが、除いた辞書は repair・merge の入力にできない
    pub prune_dominated: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            meta: Meta::new("hasami", meta::PosScheme::Ipadic),
            prune_dominated: false,
        }
    }
}

/// 書き出した辞書の概要
#[derive(Clone, Debug, Default)]
pub struct WriteStats {
    /// 書き出したエントリ数（除去後）
    pub entries: usize,
    /// 支配エントリとして除いた数
    pub pruned: usize,
    /// ユニークな表層形の数
    pub surfaces: usize,
    /// 重複を除いた素性レコードの数
    pub features: usize,
    pub pos_count: usize,
    pub conj_type_count: usize,
    pub conj_form_count: usize,
    /// 各セクションのバイト数（書き出し順）
    pub sections: Vec<(&'static str, usize)>,
    /// ファイルの長さ
    pub bytes: u64,
}

/// 書き出す元のデータ
pub(crate) struct DictSource<'a> {
    pub entries: &'a [DictEntry],
    pub matrix: Option<&'a ConnectionMatrix>,
    pub classifier: &'a CharClassifier,
    pub unk_entries: &'a HashMap<String, Vec<UnkEntry>>,
}

/// 組み立て済みのセクション
pub(crate) struct Sections {
    flags: u32,
    bodies: Vec<(SectionId, Vec<u8>)>,
    pub stats: WriteStats,
}

impl Sections {
    fn refs(&self) -> Vec<(SectionId, &[u8])> {
        self.bodies
            .iter()
            .map(|(id, b)| (*id, b.as_slice()))
            .collect()
    }

    /// ファイル全体の長さ
    fn file_len(&self) -> usize {
        let mut sink = io::sink();
        container::write(&mut sink, self.flags, &self.refs()).unwrap() as usize
    }

    /// 8 バイト境界に揃えたバッファにファイルの内容を作る（インメモリの辞書用）
    pub fn to_aligned_buffer(&self) -> (Vec<u64>, usize) {
        let len = self.file_len();
        let mut words = vec![0u64; len.div_ceil(8)];
        let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut words);
        let mut cursor = io::Cursor::new(&mut bytes[..len]);
        container::write(&mut cursor, self.flags, &self.refs()).expect("buffer has the exact size");
        (words, len)
    }

    /// ファイルに書き出す。同じディレクトリの一時ファイルに書いてから rename で差し替えるので、
    /// 書き出し中に失敗しても元のファイルは壊れず、mmap で読んでいる他のプロセスにも影響しない
    pub fn write_file(&mut self, path: &Path) -> io::Result<u64> {
        let tmp = temp_path(path);
        let result: io::Result<u64> = (|| {
            let file = std::fs::File::create(&tmp)?;
            let mut out = BufWriter::with_capacity(1 << 20, file);
            let len = container::write(&mut out, self.flags, &self.refs())?;
            out.flush()?;
            out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
            std::fs::rename(&tmp, path)?;
            Ok(len)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        let len = result?;
        self.stats.bytes = len;
        Ok(len)
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "dict.hsd".into());
    path.with_file_name(format!(".{name}.tmp-{}", std::process::id()))
}

/// 未知語テンプレートが無い文字種の既定のコスト
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

/// 未知語テンプレートが無い文字種の既定の品詞
fn default_unk_pos(char_type: CharType) -> &'static str {
    match char_type {
        CharType::Kanji | CharType::Hiragana | CharType::Katakana => "名詞,一般,*,*",
        CharType::Alpha => "名詞,固有名詞,組織,*",
        CharType::Numeric | CharType::NumericWide => "名詞,数,*,*",
        CharType::Symbol => "記号,一般,*,*",
        CharType::Space => "記号,空白,*,*",
        CharType::Default => "名詞,サ変接続,*,*",
    }
}

/// 活用型・活用形が空なら MeCab 形式 CSV と同じ `*` にそろえる
fn or_asterisk(s: &str) -> &str {
    if s.is_empty() { "*" } else { s }
}

fn intern_u16(table: &mut StringTableBuilder, s: &str, what: &str) -> Result<u16, DictError> {
    let id = table.intern(s) as usize;
    if id >= U16_TABLE_LIMIT {
        return Err(DictError::invalid(format!(
            "more than {U16_TABLE_LIMIT} distinct {what} values"
        )));
    }
    Ok(id as u16)
}

/// 同じ表層形の群から、支配されるエントリを除いた残りの添字を返す
///
/// 同じ (left_id, right_id) のうちコスト最小（同コストなら先頭）だけを残す。Viterbi は
/// `total < best` の先勝ちで比べ、同じ文脈 ID なら接続コストも同じなので、除いた側が
/// 最良パスに選ばれることはない。残す側は元の位置に置いたままにするので、文脈 ID の違う
/// 候補との同点の勝ち負けも変わらない。
fn prune_group(entries: &[DictEntry], group: &[u32], kept: &mut Vec<u32>) {
    for (i, &a) in group.iter().enumerate() {
        let ea = &entries[a as usize];
        let dominated = group.iter().enumerate().any(|(j, &b)| {
            let eb = &entries[b as usize];
            j != i
                && eb.left_id == ea.left_id
                && eb.right_id == ea.right_id
                && (eb.cost < ea.cost || (eb.cost == ea.cost && j < i))
        });
        if !dominated {
            kept.push(a);
        }
    }
}

/// 使われている文脈 ID を覆うゼロ行列の寸法（matrix.def なしで作る辞書用）
fn zero_matrix_dims(src: &DictSource<'_>) -> (usize, usize) {
    let lefts = src
        .entries
        .iter()
        .map(|e| e.left_id)
        .chain(src.unk_entries.values().flatten().map(|u| u.left_id));
    let rights = src
        .entries
        .iter()
        .map(|e| e.right_id)
        .chain(src.unk_entries.values().flatten().map(|u| u.right_id));
    (
        lefts.max().map_or(1, |m| m as usize + 1),
        rights.max().map_or(1, |m| m as usize + 1),
    )
}

/// セクションを組み立てる
///
/// `progress(確定したキー数, キーの総数)` は trie の構築中に呼ばれる。
pub(crate) fn build_sections(
    src: &DictSource<'_>,
    opts: &WriteOptions,
    progress: impl FnMut(usize, usize),
) -> Result<Sections, DictError> {
    let entries = src.entries;
    if entries.is_empty() {
        return Err(DictError::invalid("the dictionary has no entries"));
    }
    if let Some(i) = entries.iter().position(|e| e.surface.is_empty()) {
        return Err(DictError::invalid(format!(
            "entry {i} has an empty surface form"
        )));
    }

    // --- 接続行列 ---
    let (num_left, num_right, zero_matrix) = match src.matrix {
        Some(m) => (m.num_left as usize, m.num_right as usize, false),
        None => {
            let (l, r) = zero_matrix_dims(src);
            (l, r, true)
        }
    };
    if num_left == 0 || num_right == 0 {
        return Err(DictError::invalid("the connection matrix is empty"));
    }
    if num_left > LEFT_ID_LIMIT {
        return Err(DictError::invalid(format!(
            "the connection matrix has {num_left} left context IDs (limit {LEFT_ID_LIMIT})"
        )));
    }
    if let Some(m) = src.matrix {
        if m.costs.len() != num_left * num_right {
            return Err(DictError::invalid(format!(
                "the connection matrix has {} costs, expected {num_left} x {num_right}",
                m.costs.len()
            )));
        }
    }
    let check_ids = |left: u16, right: u16, what: &dyn Fn() -> String| {
        if (left as usize) < num_left && (right as usize) < num_right {
            Ok(())
        } else {
            Err(DictError::invalid(format!(
                "{}: context ID outside the connection matrix: left_id={left} (< {num_left}), \
                 right_id={right} (< {num_right})",
                what()
            )))
        }
    };
    for (i, e) in entries.iter().enumerate() {
        check_ids(e.left_id, e.right_id, &|| {
            format!("entry {i} ({})", e.surface)
        })?;
    }
    for u in src.unk_entries.values().flatten() {
        check_ids(u.left_id, u.right_id, &|| {
            format!("unknown-word template {}", u.char_class)
        })?;
    }

    // --- 表層形のバイト順に並べ、支配エントリを除く ---
    let mut order: Vec<u32> = (0..entries.len() as u32).collect();
    order.sort_by(|&a, &b| {
        entries[a as usize]
            .surface
            .as_bytes()
            .cmp(entries[b as usize].surface.as_bytes())
    });
    let mut kept: Vec<u32> = Vec::with_capacity(order.len());
    let mut keys: Vec<&str> = Vec::new();
    let mut values: Vec<u32> = Vec::new();
    let mut group_ends: Vec<usize> = Vec::new();
    let mut start = 0;
    while start < order.len() {
        let surface = &*entries[order[start] as usize].surface;
        let mut end = start + 1;
        while end < order.len() && &*entries[order[end] as usize].surface == surface {
            end += 1;
        }
        let group_start = kept.len();
        if opts.prune_dominated {
            prune_group(entries, &order[start..end], &mut kept);
        } else {
            kept.extend_from_slice(&order[start..end]);
        }
        keys.push(surface);
        values.push(group_start as u32);
        group_ends.push(kept.len());
        start = end;
    }
    drop(order);
    if kept.len() >= ENTRY_LIMIT {
        return Err(DictError::invalid(format!(
            "{} entries exceed the limit of {ENTRY_LIMIT}",
            kept.len()
        )));
    }

    // --- trie ---
    let trie_parts = trie::build_with_progress(&keys, &values, progress)?;

    // --- エントリと素性 ---
    let mut pos_table = StringTableBuilder::default();
    let mut conj_type_table = StringTableBuilder::default();
    let mut conj_form_table = StringTableBuilder::default();
    let mut features = FeatureTableBuilder::default();
    let mut entry_records: Vec<EntryRecord> = Vec::with_capacity(kept.len());
    let mut feature_offsets: Vec<u32> = Vec::with_capacity(kept.len());
    let mut group_iter = group_ends.iter().peekable();
    for (i, &idx) in kept.iter().enumerate() {
        let e = &entries[idx as usize];
        while group_iter.peek().is_some_and(|&&end| end <= i) {
            group_iter.next();
        }
        let last = group_iter.peek().is_some_and(|&&end| end == i + 1);
        entry_records.push(EntryRecord {
            left_and_last: e.left_id | if last { LAST_IN_GROUP } else { 0 },
            right_id: e.right_id,
            cost: e.cost,
        });
        let input = FeatureInput {
            pos_id: intern_u16(&mut pos_table, &e.pos, "part-of-speech")?,
            conj_type_id: intern_u16(
                &mut conj_type_table,
                or_asterisk(&e.conj_type),
                "conjugation type",
            )?,
            conj_form_id: intern_u16(
                &mut conj_form_table,
                or_asterisk(&e.conj_form),
                "conjugation form",
            )?,
            surface: &e.surface,
            base_form: &e.base_form,
            reading: &e.reading,
            pronunciation: &e.pronunciation,
        };
        feature_offsets.push(features.intern(&input)?);
    }

    // --- 未知語テンプレート（文字種の並びは ALL_CHAR_TYPES） ---
    let mut unk_buckets: Vec<UnkBucket> = Vec::with_capacity(ALL_CHAR_TYPES.len());
    let mut unk_templates: Vec<UnkTemplate> = Vec::new();
    for &ct in &ALL_CHAR_TYPES {
        let class_name = ct.class_name();
        let invoke = src
            .classifier
            .get_class(class_name)
            .is_some_and(|c| c.invoke);
        let template_start = unk_templates.len() as u32;
        match src.unk_entries.get(class_name).filter(|t| !t.is_empty()) {
            Some(templates) => {
                for t in templates {
                    unk_templates.push(UnkTemplate {
                        pos_id: intern_u16(&mut pos_table, &t.pos, "part-of-speech")? as u32,
                        left_id: t.left_id,
                        right_id: t.right_id,
                        cost: t.cost,
                        _pad: 0,
                    });
                }
            }
            // テンプレートが無い文字種は既定値を書いておく（読み込み側は代替値を持たない）
            None => unk_templates.push(UnkTemplate {
                pos_id: intern_u16(&mut pos_table, default_unk_pos(ct), "part-of-speech")? as u32,
                left_id: 0,
                right_id: 0,
                cost: default_unk_cost(ct),
                _pad: 0,
            }),
        }
        let template_count = unk_templates.len() as u32 - template_start;
        unk_buckets.push(UnkBucket {
            template_start,
            template_count: u16::try_from(template_count).map_err(|_| {
                DictError::invalid(format!("too many unknown-word templates for {class_name}"))
            })?,
            invoke: invoke as u8,
            _pad: 0,
        });
    }

    // --- 文字種定義（カテゴリは名前順） ---
    let mut category_names = StringTableBuilder::default();
    let mut classes: Vec<_> = src.classifier.classes.values().collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    let char_categories: Vec<CharCategoryRecord> = classes
        .iter()
        .map(|c| CharCategoryRecord {
            name_id: category_names.intern(&c.name),
            invoke: c.invoke as u8,
            group: c.group as u8,
            _pad0: 0,
            length: c.length,
            _pad1: 0,
        })
        .collect();
    let char_ranges: Vec<CharRangeRecord> = src
        .classifier
        .ranges
        .iter()
        .map(|(start, end, name)| CharRangeRecord {
            start: *start,
            end: *end,
            category_name_id: category_names.intern(name),
            _pad: 0,
        })
        .collect();

    // --- 接続行列（転置: 行 = 次の語の left_id） ---
    let mut matrix = Vec::with_capacity(8 + 2 * num_left * num_right);
    matrix.extend_from_slice(&(num_left as u32).to_le_bytes());
    matrix.extend_from_slice(&(num_right as u32).to_le_bytes());
    match src.matrix {
        Some(m) => matrix.extend_from_slice(bytemuck::cast_slice(&m.costs)),
        None => matrix.resize(8 + 2 * num_left * num_right, 0),
    }

    // --- メタデータ ---
    let mut meta = opts.meta.clone();
    meta.set(meta::KEY_HASAMI_VERSION, env!("CARGO_PKG_VERSION"))?;
    meta.set(
        meta::KEY_PRUNED_DOMINATED,
        if opts.prune_dominated {
            "true"
        } else {
            "false"
        },
    )?;
    if zero_matrix {
        meta.set(meta::KEY_ZERO_MATRIX, "true")?;
    } else {
        meta.remove(meta::KEY_ZERO_MATRIX);
    }

    let stats = WriteStats {
        entries: kept.len(),
        pruned: entries.len() - kept.len(),
        surfaces: keys.len(),
        features: features.len(),
        pos_count: pos_table.len(),
        conj_type_count: conj_type_table.len(),
        conj_form_count: conj_form_table.len(),
        sections: Vec::new(),
        bytes: 0,
    };
    let trie::TrieParts {
        blocks,
        tables,
        nodes,
        tails,
    } = trie_parts;
    let bodies: Vec<(SectionId, Vec<u8>)> = vec![
        (SectionId::Meta, meta.to_bytes()?),
        (
            SectionId::CharBlocks,
            bytemuck::cast_slice(&blocks).to_vec(),
        ),
        (
            SectionId::CharTables,
            bytemuck::cast_slice(&tables).to_vec(),
        ),
        (SectionId::CategoryNames, category_names.to_bytes()),
        (
            SectionId::CharCategories,
            bytemuck::cast_slice(&char_categories).to_vec(),
        ),
        (
            SectionId::CharRanges,
            bytemuck::cast_slice(&char_ranges).to_vec(),
        ),
        (
            SectionId::UnkBuckets,
            bytemuck::cast_slice(&unk_buckets).to_vec(),
        ),
        (
            SectionId::UnkTemplates,
            bytemuck::cast_slice(&unk_templates).to_vec(),
        ),
        (SectionId::PosStrings, pos_table.to_bytes()),
        (SectionId::ConjTypeStrings, conj_type_table.to_bytes()),
        (SectionId::ConjFormStrings, conj_form_table.to_bytes()),
        (SectionId::Matrix, matrix),
        (SectionId::TrieNodes, bytemuck::cast_slice(&nodes).to_vec()),
        (SectionId::TrieTails, tails),
        (
            SectionId::Entries,
            bytemuck::cast_slice(&entry_records).to_vec(),
        ),
        (
            SectionId::FeatureOffsets,
            bytemuck::cast_slice(&feature_offsets).to_vec(),
        ),
        (SectionId::Features, features.into_blob()),
    ];
    let mut sections = Sections {
        flags: if opts.prune_dominated {
            FLAG_PRUNED_DOMINATED
        } else {
            0
        },
        bodies,
        stats,
    };
    sections.stats.sections = sections
        .bodies
        .iter()
        .map(|(id, b)| (id.name(), b.len()))
        .collect();
    sections.stats.bytes = sections.file_len() as u64;
    Ok(sections)
}
