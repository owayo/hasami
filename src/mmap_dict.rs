//! カスタム mmap-native バイナリ辞書フォーマット (v1/v2対応)
//!
//! v1: Dense trie output, StrIndex{off,len}, Arc<str>キャッシュ
//! v2: Sparse trie output (bitset+rank), offsets-only文字列index, キャッシュなし
//!
//! v2での改善:
//! - trie_output: 33M*4B=127MB → bitset(4MB)+ranks(0.3MB)+offsets(19MB) ≈ 23MB (-104MB)
//! - str_index:   7M*8B=54MB  → 7M*4B+4B ≈ 27MB (-27MB)
//! - ロード時: Arc<str>キャッシュ構築を廃止 → ロード時間 ~400ms → ~1ms

use bytemuck::{Pod, Zeroable};
use memmap2::Mmap;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, LazyLock, OnceLock};
use std::{mem, slice, str};

// --- 定数 ---

const MAGIC: [u8; 8] = *b"HSMDICT\0";
const FORMAT_VERSION_V1: u32 = 1;
const FORMAT_VERSION_V2: u32 = 2;

// --- オンディスク POD 構造体 ---

/// セクション位置（オフセット＋バイト長）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Section {
    offset: u64,
    bytes: u64,
}

/// v1 ファイルヘッダー (11セクション)
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HeaderV1 {
    magic: [u8; 8],
    version: u32,
    flags: u32,
    entry_count: u32,
    feature_count: u32,
    string_count: u32,
    matrix_left_size: u16,
    matrix_right_size: u16,
    unk_bucket_count: u32,
    unk_template_count: u32,
    // 11 sections
    trie_base: Section,
    trie_check: Section,
    trie_output: Section,
    trie_value_pool: Section,
    entries: Section,
    features: Section,
    str_index: Section,
    str_blob: Section,
    matrix_costs: Section,
    unk_buckets: Section,
    unk_templates: Section,
}

/// v2 ファイルヘッダー (13セクション: sparse trie + offsets-only str)
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HeaderV2 {
    magic: [u8; 8],
    version: u32,
    flags: u32,
    entry_count: u32,
    feature_count: u32,
    string_count: u32,
    matrix_left_size: u16,
    matrix_right_size: u16,
    unk_bucket_count: u32,
    unk_template_count: u32,
    // 13 sections
    trie_base: Section,
    trie_check: Section,
    terminal_bits: Section,
    terminal_ranks: Section,
    terminal_offsets: Section,
    trie_value_pool: Section,
    entries: Section,
    features: Section,
    str_offsets: Section,
    str_blob: Section,
    matrix_costs: Section,
    unk_buckets: Section,
    unk_templates: Section,
}

/// 辞書エントリ（POD、16バイト）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EntryRecord {
    surface_id: u32,
    feature_id: u32,
    left_id: u16,
    right_id: u16,
    cost: i16,
    _pad: u16,
}

/// 特徴量レコード（POD、16バイト、重複排除）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Hash, Eq, PartialEq)]
struct FeatureRecord {
    pos_id: u32,
    base_id: u32,
    reading_id: u32,
    pronunciation_id: u32,
}

/// 文字列インデックス（v1用、POD、8バイト）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StrIndex {
    off: u32,
    len: u32,
}

/// 未知語バケット（CharType → テンプレート範囲）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UnkBucket {
    template_start: u32,
    template_count: u16,
    invoke: u8,
    _pad: u8,
}

/// 未知語テンプレート
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UnkTemplate {
    pos_id: u32,
    left_id: u16,
    right_id: u16,
    cost: i16,
    _pad: u16,
}

// --- 内部フォーマット列挙型 ---

/// Trie output の表現形式
enum TrieOutputFormat {
    /// v1: output[node] = value_pool index, u32::MAX = 非終端
    Dense(Section),
    /// v2: ビットセット + ランクテーブル + 終端オフセット
    Sparse {
        bits: Section,
        ranks: Section,
        offsets: Section,
    },
}

/// 文字列インデックスの表現形式
enum StrIndexFormat {
    /// v1: StrIndex { off, len } の配列
    PairIndex(Section),
    /// v2: u32 オフセット配列 (string_count + 1 エントリ、最後はセンチネル)
    Offsets(Section),
}

// --- ビルダー ---

/// 文字列プール（重複排除 + 連続バッファ）
#[derive(Default)]
struct StringPool {
    ids: HashMap<Box<str>, u32>,
    index: Vec<StrIndex>,
    blob: Vec<u8>,
}

impl StringPool {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.index.len() as u32;
        let off = self.blob.len() as u32;
        self.blob.extend_from_slice(s.as_bytes());
        self.index.push(StrIndex {
            off,
            len: s.len() as u32,
        });
        self.ids.insert(s.into(), id);
        id
    }
}

/// 特徴量プール（重複排除）
#[derive(Default)]
struct FeaturePool {
    ids: HashMap<FeatureRecord, u32>,
    items: Vec<FeatureRecord>,
}

impl FeaturePool {
    fn intern(&mut self, f: FeatureRecord) -> u32 {
        if let Some(&id) = self.ids.get(&f) {
            return id;
        }
        let id = self.items.len() as u32;
        self.items.push(f);
        self.ids.insert(f, id);
        id
    }
}

/// mmap辞書ビルダー（v2形式で出力）
pub struct MmapDictBuilder {
    strings: StringPool,
    features: FeaturePool,
    entries: Vec<EntryRecord>,
    trie_base: Vec<i32>,
    trie_check: Vec<i32>,
    trie_output: Vec<u32>,
    trie_value_pool: Vec<u32>,
    matrix_costs: Vec<i16>,
    matrix_left_size: u16,
    matrix_right_size: u16,
    unk_buckets: Vec<UnkBucket>,
    unk_templates: Vec<UnkTemplate>,
}

impl MmapDictBuilder {
    /// 既存の Dictionary から MmapDict を構築
    pub fn from_dictionary(dict: &crate::dict::Dictionary) -> Self {
        let mut strings = StringPool::default();
        let mut features = FeaturePool::default();

        let entries: Vec<EntryRecord> = dict
            .entries
            .iter()
            .map(|e| {
                let surface_id = strings.intern(&e.surface);
                let pos_id = strings.intern(&e.pos);
                let base_id = strings.intern(&e.base_form);
                let reading_id = strings.intern(&e.reading);
                let pronunciation_id = strings.intern(&e.pronunciation);
                let feature_id = features.intern(FeatureRecord {
                    pos_id,
                    base_id,
                    reading_id,
                    pronunciation_id,
                });
                EntryRecord {
                    surface_id,
                    feature_id,
                    left_id: e.left_id,
                    right_id: e.right_id,
                    cost: e.cost,
                    _pad: 0,
                }
            })
            .collect();

        let trie_base = dict.trie.base_slice().to_vec();
        let trie_check = dict.trie.check_slice().to_vec();
        let trie_output = dict.trie.output_slice().to_vec();
        let trie_value_pool = dict.trie.value_pool_slice().to_vec();

        let matrix_costs = dict.matrix.costs.clone();
        let matrix_left_size = dict.matrix.left_size;
        let matrix_right_size = dict.matrix.right_size;

        use crate::char_class::CharType;
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
        let mut unk_buckets = Vec::new();
        let mut unk_templates_vec = Vec::new();

        for &ct in &all_types {
            let class_name = ct.class_name();
            let invoke = dict
                .char_classifier
                .get_class(class_name)
                .is_some_and(|cl| cl.invoke);

            let template_start = unk_templates_vec.len() as u32;
            if let Some(unk_entries) = dict.unk_entries.get(class_name) {
                for unk in unk_entries {
                    let pos_id = strings.intern(&unk.pos);
                    unk_templates_vec.push(UnkTemplate {
                        pos_id,
                        left_id: unk.left_id,
                        right_id: unk.right_id,
                        cost: unk.cost,
                        _pad: 0,
                    });
                }
            }
            let template_count = (unk_templates_vec.len() as u32 - template_start) as u16;
            unk_buckets.push(UnkBucket {
                template_start,
                template_count,
                invoke: invoke as u8,
                _pad: 0,
            });
        }

        MmapDictBuilder {
            strings,
            features,
            entries,
            trie_base,
            trie_check,
            trie_output,
            trie_value_pool,
            matrix_costs,
            matrix_left_size,
            matrix_right_size,
            unk_buckets,
            unk_templates: unk_templates_vec,
        }
    }

    pub fn string_count(&self) -> usize {
        self.strings.index.len()
    }

    pub fn feature_count(&self) -> usize {
        self.features.items.len()
    }

    /// バイナリ辞書をv2形式でファイルに書き出し
    pub fn write<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let mut buf: Vec<u8> = Vec::new();

        let header_size = mem::size_of::<HeaderV2>();
        buf.resize(header_size, 0);

        fn align_to(buf: &mut Vec<u8>, align: usize) {
            let rem = buf.len() % align;
            if rem != 0 {
                buf.resize(buf.len() + (align - rem), 0);
            }
        }

        fn write_section<T: Pod>(buf: &mut Vec<u8>, data: &[T], align: usize) -> Section {
            align_to(buf, align);
            let offset = buf.len() as u64;
            let bytes_data = bytemuck::cast_slice::<T, u8>(data);
            buf.extend_from_slice(bytes_data);
            Section {
                offset,
                bytes: bytes_data.len() as u64,
            }
        }

        // Trie base/check (unchanged)
        let trie_base = write_section(&mut buf, &self.trie_base, 4);
        let trie_check = write_section(&mut buf, &self.trie_check, 4);

        // Sparse trie output (v2)
        let (bits, ranks, offsets) = build_sparse_trie_output(&self.trie_output);
        let terminal_bits = write_section(&mut buf, &bits, 8);
        let terminal_ranks = write_section(&mut buf, &ranks, 4);
        let terminal_offsets = write_section(&mut buf, &offsets, 4);

        // Value pool (unchanged)
        let trie_value_pool = write_section(&mut buf, &self.trie_value_pool, 4);

        // Entries & features
        let entries = write_section(&mut buf, &self.entries, 4);
        let features_sec = write_section(&mut buf, &self.features.items, 4);

        // String offsets (v2: offsets-only with sentinel)
        let str_offsets_data = build_str_offsets(&self.strings.index, self.strings.blob.len());
        let str_offsets = write_section(&mut buf, &str_offsets_data, 4);

        // String blob
        let str_blob_offset = buf.len() as u64;
        buf.extend_from_slice(&self.strings.blob);
        let str_blob = Section {
            offset: str_blob_offset,
            bytes: self.strings.blob.len() as u64,
        };

        // Matrix, unk
        let matrix_costs = write_section(&mut buf, &self.matrix_costs, 2);
        let unk_buckets = write_section(&mut buf, &self.unk_buckets, 4);
        let unk_templates = write_section(&mut buf, &self.unk_templates, 4);

        // ヘッダー書き込み
        let header = HeaderV2 {
            magic: MAGIC,
            version: FORMAT_VERSION_V2,
            flags: 0,
            entry_count: self.entries.len() as u32,
            feature_count: self.features.items.len() as u32,
            string_count: self.strings.index.len() as u32,
            matrix_left_size: self.matrix_left_size,
            matrix_right_size: self.matrix_right_size,
            unk_bucket_count: self.unk_buckets.len() as u32,
            unk_template_count: self.unk_templates.len() as u32,
            trie_base,
            trie_check,
            terminal_bits,
            terminal_ranks,
            terminal_offsets,
            trie_value_pool,
            entries,
            features: features_sec,
            str_offsets,
            str_blob,
            matrix_costs,
            unk_buckets,
            unk_templates,
        };
        buf[..header_size].copy_from_slice(bytemuck::bytes_of(&header));

        std::fs::write(path, buf)
    }
}

// --- Sparse trie ヘルパー ---

/// Dense trie output を sparse (bitset + rank + offsets) に変換
fn build_sparse_trie_output(output: &[u32]) -> (Vec<u64>, Vec<u32>, Vec<u32>) {
    let num_nodes = output.len();
    let num_words = (num_nodes + 63) / 64;

    let mut bits = vec![0u64; num_words];
    let mut terminal_offsets = Vec::new();

    for (i, &val) in output.iter().enumerate() {
        if val != u32::MAX {
            bits[i / 64] |= 1u64 << (i % 64);
            terminal_offsets.push(val);
        }
    }

    // ランクテーブル: 512ビット(8 u64)ブロックごとの累積ポップカウント
    let num_blocks = (num_words + 7) / 8;
    let mut ranks = Vec::with_capacity(num_blocks);
    let mut cumulative = 0u32;
    for block in 0..num_blocks {
        let start = block * 8;
        let end = (start + 8).min(num_words);
        for w in start..end {
            cumulative += bits[w].count_ones();
        }
        ranks.push(cumulative);
    }

    (bits, ranks, terminal_offsets)
}

/// StrIndex配列 → offsets-only配列 (string_count + 1 エントリ)
fn build_str_offsets(index: &[StrIndex], blob_len: usize) -> Vec<u32> {
    let mut offsets: Vec<u32> = index.iter().map(|s| s.off).collect();
    // センチネル: blob全体の末尾
    offsets.push(blob_len as u32);
    offsets
}

/// ビットセット上の rank(pos) = pos より前の set bit 数
#[inline]
fn compute_rank(bits: &[u64], ranks: &[u32], pos: usize) -> usize {
    let block = pos / 512;
    let base = if block > 0 {
        ranks[block - 1] as usize
    } else {
        0
    };

    let word_start = block * 8;
    let target_word = pos / 64;
    let bit_in_word = pos % 64;

    let mut r = base;
    for w in word_start..target_word {
        r += bits[w].count_ones() as usize;
    }
    if target_word < bits.len() {
        let mask = (1u64 << bit_in_word).wrapping_sub(1);
        r += (bits[target_word] & mask).count_ones() as usize;
    }
    r
}

// --- mmap ローダー ---

/// ゼロコピー mmap 辞書（v1/v2両対応）
pub struct MmapDictionary {
    mmap: Mmap,
    #[allow(dead_code)]
    format_version: u32,

    // スカラーフィールド
    entry_count_val: u32,
    feature_count_val: u32,
    string_count_val: u32,
    matrix_left_size_val: u16,
    matrix_right_size_val: u16,

    // 共通セクション
    trie_base_sec: Section,
    trie_check_sec: Section,
    trie_value_pool_sec: Section,
    entries_sec: Section,
    features_sec: Section,
    str_blob_sec: Section,
    matrix_costs_sec: Section,
    unk_buckets_sec: Section,
    unk_templates_sec: Section,

    // バージョン固有
    trie_output_fmt: TrieOutputFormat,
    str_index_fmt: StrIndexFormat,

    /// 遅延 Arc<str> キャッシュ（初回 arc_at 時に自動構築、warm_cache()で事前構築可）
    arc_cache: OnceLock<Box<[Arc<str>]>>,
}

impl MmapDictionary {
    /// ファイルからロード（v1/v2自動判定）
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < 16 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file too short"));
        }

        // magic チェック
        if mmap[..8] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid magic bytes",
            ));
        }

        let version = u32::from_ne_bytes(mmap[8..12].try_into().unwrap());

        match version {
            FORMAT_VERSION_V1 => Self::load_v1(mmap),
            FORMAT_VERSION_V2 => Self::load_v2(mmap),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported format version: {}", version),
            )),
        }
    }

    fn load_v1(mmap: Mmap) -> io::Result<Self> {
        let hdr_size = mem::size_of::<HeaderV1>();
        if mmap.len() < hdr_size {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file too short for v1 header"));
        }
        let h: HeaderV1 = *bytemuck::from_bytes(&mmap[..hdr_size]);

        let dict = Self {
            mmap,
            format_version: FORMAT_VERSION_V1,
            entry_count_val: h.entry_count,
            feature_count_val: h.feature_count,
            string_count_val: h.string_count,
            matrix_left_size_val: h.matrix_left_size,
            matrix_right_size_val: h.matrix_right_size,
            trie_base_sec: h.trie_base,
            trie_check_sec: h.trie_check,
            trie_value_pool_sec: h.trie_value_pool,
            entries_sec: h.entries,
            features_sec: h.features,
            str_blob_sec: h.str_blob,
            matrix_costs_sec: h.matrix_costs,
            unk_buckets_sec: h.unk_buckets,
            unk_templates_sec: h.unk_templates,
            trie_output_fmt: TrieOutputFormat::Dense(h.trie_output),
            str_index_fmt: StrIndexFormat::PairIndex(h.str_index),
            arc_cache: OnceLock::new(),
        };
        dict.validate()?;
        Ok(dict)
    }

    fn load_v2(mmap: Mmap) -> io::Result<Self> {
        let hdr_size = mem::size_of::<HeaderV2>();
        if mmap.len() < hdr_size {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file too short for v2 header"));
        }
        let h: HeaderV2 = *bytemuck::from_bytes(&mmap[..hdr_size]);

        let dict = Self {
            mmap,
            format_version: FORMAT_VERSION_V2,
            entry_count_val: h.entry_count,
            feature_count_val: h.feature_count,
            string_count_val: h.string_count,
            matrix_left_size_val: h.matrix_left_size,
            matrix_right_size_val: h.matrix_right_size,
            trie_base_sec: h.trie_base,
            trie_check_sec: h.trie_check,
            trie_value_pool_sec: h.trie_value_pool,
            entries_sec: h.entries,
            features_sec: h.features,
            str_blob_sec: h.str_blob,
            matrix_costs_sec: h.matrix_costs,
            unk_buckets_sec: h.unk_buckets,
            unk_templates_sec: h.unk_templates,
            trie_output_fmt: TrieOutputFormat::Sparse {
                bits: h.terminal_bits,
                ranks: h.terminal_ranks,
                offsets: h.terminal_offsets,
            },
            str_index_fmt: StrIndexFormat::Offsets(h.str_offsets),
            arc_cache: OnceLock::new(),
        };
        dict.validate()?;
        Ok(dict)
    }

    fn validate(&self) -> io::Result<()> {
        let all_sections = self.all_sections();
        for sec in &all_sections {
            let end = sec.offset as usize + sec.bytes as usize;
            if end > self.mmap.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "section exceeds file bounds",
                ));
            }
        }
        Ok(())
    }

    fn all_sections(&self) -> Vec<Section> {
        let mut secs = vec![
            self.trie_base_sec,
            self.trie_check_sec,
            self.trie_value_pool_sec,
            self.entries_sec,
            self.features_sec,
            self.str_blob_sec,
            self.matrix_costs_sec,
            self.unk_buckets_sec,
            self.unk_templates_sec,
        ];
        match &self.trie_output_fmt {
            TrieOutputFormat::Dense(s) => secs.push(*s),
            TrieOutputFormat::Sparse { bits, ranks, offsets } => {
                secs.push(*bits);
                secs.push(*ranks);
                secs.push(*offsets);
            }
        }
        match &self.str_index_fmt {
            StrIndexFormat::PairIndex(s) => secs.push(*s),
            StrIndexFormat::Offsets(s) => secs.push(*s),
        }
        secs
    }

    #[inline]
    fn pod_slice<T: Pod>(&self, sec: Section) -> &[T] {
        unsafe {
            let ptr = self.mmap.as_ptr().add(sec.offset as usize).cast::<T>();
            slice::from_raw_parts(ptr, sec.bytes as usize / mem::size_of::<T>())
        }
    }

    #[inline]
    fn raw_section(&self, sec: Section) -> &[u8] {
        &self.mmap[sec.offset as usize..(sec.offset + sec.bytes) as usize]
    }

    // --- Trie アクセス ---

    #[inline]
    pub fn trie_base(&self) -> &[i32] {
        self.pod_slice(self.trie_base_sec)
    }

    #[inline]
    pub fn trie_check(&self) -> &[i32] {
        self.pod_slice(self.trie_check_sec)
    }

    #[inline]
    pub fn trie_value_pool(&self) -> &[u32] {
        self.pod_slice(self.trie_value_pool_sec)
    }

    /// v1互換: dense trie output (v2ではpanic)
    #[inline]
    pub fn trie_output(&self) -> &[u32] {
        match &self.trie_output_fmt {
            TrieOutputFormat::Dense(sec) => self.pod_slice(*sec),
            TrieOutputFormat::Sparse { .. } => panic!("trie_output() not available in v2 format"),
        }
    }

    // --- エントリアクセス ---

    #[inline]
    fn entries(&self) -> &[EntryRecord] {
        self.pod_slice(self.entries_sec)
    }

    #[inline]
    fn features(&self) -> &[FeatureRecord] {
        self.pod_slice(self.features_sec)
    }

    #[inline]
    fn str_blob(&self) -> &[u8] {
        let s = self.str_blob_sec;
        &self.mmap[s.offset as usize..(s.offset + s.bytes) as usize]
    }

    #[inline]
    fn str_at(&self, id: u32) -> &str {
        let blob = self.str_blob();
        match &self.str_index_fmt {
            StrIndexFormat::PairIndex(sec) => {
                let idx: &[StrIndex] = self.pod_slice(*sec);
                let s = idx[id as usize];
                unsafe { str::from_utf8_unchecked(&blob[s.off as usize..(s.off + s.len) as usize]) }
            }
            StrIndexFormat::Offsets(sec) => {
                let offsets: &[u32] = self.pod_slice(*sec);
                let start = offsets[id as usize] as usize;
                let end = offsets[id as usize + 1] as usize;
                unsafe { str::from_utf8_unchecked(&blob[start..end]) }
            }
        }
    }

    /// Arc<str> キャッシュを遅延構築（初回アクセス時）
    fn ensure_arc_cache(&self) -> &[Arc<str>] {
        self.arc_cache.get_or_init(|| {
            let count = self.string_count_val as usize;
            (0..count)
                .map(|i| Arc::from(self.str_at(i as u32)))
                .collect()
        })
    }

    /// Arc<str> をキャッシュから取得
    #[inline]
    fn arc_at(&self, id: u32) -> Arc<str> {
        Arc::clone(&self.ensure_arc_cache()[id as usize])
    }

    /// エントリの surface を Arc<str> で取得
    #[inline]
    pub fn entry_surface_arc(&self, id: u32) -> Arc<str> {
        let e = &self.entries()[id as usize];
        self.arc_at(e.surface_id)
    }

    /// エントリの全フィールドを Arc<str> で取得（トークン生成用）
    #[inline]
    #[allow(clippy::type_complexity)]
    pub fn entry_arcs(&self, id: u32) -> (Arc<str>, Arc<str>, Arc<str>, Arc<str>, Arc<str>) {
        let e = &self.entries()[id as usize];
        let f = &self.features()[e.feature_id as usize];
        (
            self.arc_at(e.surface_id),
            self.arc_at(f.pos_id),
            self.arc_at(f.base_id),
            self.arc_at(f.reading_id),
            self.arc_at(f.pronunciation_id),
        )
    }

    /// 未知語POSを Arc<str> で取得
    #[inline]
    pub fn unk_pos_arc(&self, char_type_idx: usize) -> Arc<str> {
        let buckets = self.unk_buckets();
        if char_type_idx < buckets.len() {
            let bucket = &buckets[char_type_idx];
            if bucket.template_count > 0 {
                let templates = self.unk_templates_slice();
                let t = &templates[bucket.template_start as usize];
                return self.arc_at(t.pos_id);
            }
        }
        static UNK_POS_FALLBACK: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("名詞,一般,*,*"));
        Arc::clone(&UNK_POS_FALLBACK)
    }

    /// エントリの left_id, right_id, cost を取得（Viterbi用、ゼロコピー）
    #[inline]
    pub fn entry_cost_info(&self, id: u32) -> (u16, u16, i16) {
        let e = &self.entries()[id as usize];
        (e.left_id, e.right_id, e.cost)
    }

    /// エントリの surface を取得
    #[inline]
    pub fn entry_surface(&self, id: u32) -> &str {
        let e = &self.entries()[id as usize];
        self.str_at(e.surface_id)
    }

    #[inline]
    pub fn entry_pos(&self, id: u32) -> &str {
        let e = &self.entries()[id as usize];
        let f = &self.features()[e.feature_id as usize];
        self.str_at(f.pos_id)
    }

    #[inline]
    pub fn entry_base_form(&self, id: u32) -> &str {
        let e = &self.entries()[id as usize];
        let f = &self.features()[e.feature_id as usize];
        self.str_at(f.base_id)
    }

    #[inline]
    pub fn entry_reading(&self, id: u32) -> &str {
        let e = &self.entries()[id as usize];
        let f = &self.features()[e.feature_id as usize];
        self.str_at(f.reading_id)
    }

    #[inline]
    pub fn entry_pronunciation(&self, id: u32) -> &str {
        let e = &self.entries()[id as usize];
        let f = &self.features()[e.feature_id as usize];
        self.str_at(f.pronunciation_id)
    }

    // --- 接続行列アクセス ---

    #[inline]
    pub fn matrix_left_size(&self) -> u16 {
        self.matrix_left_size_val
    }

    #[inline]
    pub fn matrix_right_size(&self) -> u16 {
        self.matrix_right_size_val
    }

    #[inline]
    pub fn matrix_row(&self, prev_right_id: u16) -> &[i16] {
        let costs: &[i16] = self.pod_slice(self.matrix_costs_sec);
        let left = self.matrix_left_size_val as usize;
        let start = prev_right_id as usize * left;
        if start + left <= costs.len() {
            unsafe { costs.get_unchecked(start..start + left) }
        } else {
            &[]
        }
    }

    // --- 未知語テーブルアクセス ---

    #[inline]
    fn unk_buckets(&self) -> &[UnkBucket] {
        self.pod_slice(self.unk_buckets_sec)
    }

    #[inline]
    fn unk_templates_slice(&self) -> &[UnkTemplate] {
        self.pod_slice(self.unk_templates_sec)
    }

    #[inline]
    pub fn unk_invoke(&self, char_type_idx: usize) -> bool {
        let buckets = self.unk_buckets();
        if char_type_idx < buckets.len() {
            buckets[char_type_idx].invoke != 0
        } else {
            false
        }
    }

    #[inline]
    pub fn unk_first_template(&self, char_type_idx: usize) -> (u16, u16, i16) {
        let buckets = self.unk_buckets();
        if char_type_idx < buckets.len() {
            let bucket = &buckets[char_type_idx];
            if bucket.template_count > 0 {
                let templates = self.unk_templates_slice();
                let t = &templates[bucket.template_start as usize];
                return (t.left_id, t.right_id, t.cost);
            }
        }
        (0, 0, 10000)
    }

    #[inline]
    pub fn unk_pos(&self, char_type_idx: usize) -> &str {
        let buckets = self.unk_buckets();
        if char_type_idx < buckets.len() {
            let bucket = &buckets[char_type_idx];
            if bucket.template_count > 0 {
                let templates = self.unk_templates_slice();
                let t = &templates[bucket.template_start as usize];
                return self.str_at(t.pos_id);
            }
        }
        "名詞,一般,*,*"
    }

    // --- Trie 共通接頭辞検索 ---

    /// ゼロアロケーション共通接頭辞検索（v1/v2自動ディスパッチ）
    #[inline]
    pub fn common_prefix_search_cb(&self, input: &[u8], cb: impl FnMut(usize, &[u32])) {
        match &self.trie_output_fmt {
            TrieOutputFormat::Dense(sec) => self.cps_dense(input, *sec, cb),
            TrieOutputFormat::Sparse { bits, ranks, offsets } => {
                self.cps_sparse(input, *bits, *ranks, *offsets, cb)
            }
        }
    }

    /// v1: Dense trie output による共通接頭辞検索
    #[inline(always)]
    fn cps_dense(&self, input: &[u8], output_sec: Section, mut cb: impl FnMut(usize, &[u32])) {
        let base = self.trie_base();
        let check = self.trie_check();
        let output: &[u32] = self.pod_slice(output_sec);
        let value_pool = self.trie_value_pool();
        let base_len = base.len();
        let check_len = check.len();
        let output_len = output.len();

        let mut node = 0usize;

        if output_len > 0 && output[0] != u32::MAX {
            let pool_idx = output[0] as usize;
            let count = value_pool[pool_idx] as usize;
            cb(0, &value_pool[pool_idx + 1..pool_idx + 1 + count]);
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
                let pool_idx = output[node] as usize;
                let count = value_pool[pool_idx] as usize;
                cb(i + 1, &value_pool[pool_idx + 1..pool_idx + 1 + count]);
            }
        }
    }

    /// v2: Sparse trie output (bitset+rank) による共通接頭辞検索
    #[inline(always)]
    fn cps_sparse(
        &self,
        input: &[u8],
        bits_sec: Section,
        ranks_sec: Section,
        offsets_sec: Section,
        mut cb: impl FnMut(usize, &[u32]),
    ) {
        let base = self.trie_base();
        let check = self.trie_check();
        let bits: &[u64] = self.pod_slice(bits_sec);
        let ranks: &[u32] = self.pod_slice(ranks_sec);
        let term_offsets: &[u32] = self.pod_slice(offsets_sec);
        let value_pool = self.trie_value_pool();
        let base_len = base.len();
        let check_len = check.len();

        #[inline(always)]
        fn check_terminal(
            node: usize,
            bits: &[u64],
            ranks: &[u32],
            term_offsets: &[u32],
            value_pool: &[u32],
            len: usize,
            cb: &mut impl FnMut(usize, &[u32]),
        ) {
            let word = node / 64;
            let bit = node % 64;
            if word < bits.len() && (bits[word] >> bit) & 1 == 1 {
                let rank = compute_rank(bits, ranks, node);
                let pool_idx = term_offsets[rank] as usize;
                let count = value_pool[pool_idx] as usize;
                cb(len, &value_pool[pool_idx + 1..pool_idx + 1 + count]);
            }
        }

        let mut node = 0usize;
        check_terminal(node, bits, ranks, term_offsets, value_pool, 0, &mut cb);

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
            check_terminal(node, bits, ranks, term_offsets, value_pool, i + 1, &mut cb);
        }
    }

    // --- メタデータ ---

    pub fn entry_count(&self) -> u32 {
        self.entry_count_val
    }

    pub fn string_count(&self) -> u32 {
        self.string_count_val
    }

    pub fn feature_count(&self) -> u32 {
        self.feature_count_val
    }

    /// 未知語エントリをエクスポート（マージ用）
    pub fn export_unk_entries(&self, target: &mut HashMap<String, Vec<crate::dict::UnkEntry>>) {
        use crate::char_class::CharType;
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
        let buckets = self.unk_buckets();
        let templates = self.unk_templates_slice();

        for (i, &ct) in all_types.iter().enumerate() {
            if i >= buckets.len() {
                break;
            }
            let bucket = &buckets[i];
            let class_name = ct.class_name().to_string();
            let start = bucket.template_start as usize;
            let count = bucket.template_count as usize;

            for j in start..start + count {
                if j < templates.len() {
                    let t = &templates[j];
                    target
                        .entry(class_name.clone())
                        .or_default()
                        .push(crate::dict::UnkEntry {
                            char_class: class_name.clone(),
                            left_id: t.left_id,
                            right_id: t.right_id,
                            cost: t.cost,
                            pos: self.str_at(t.pos_id).to_string(),
                        });
                }
            }
        }
    }

    /// ヘッダーのフォーマットバージョンを返す
    #[cfg(test)]
    pub(crate) fn format_version(&self) -> u32 {
        self.format_version
    }

    /// CharClassifier をエクスポート（マージ用）
    pub fn export_char_classifier(&self, target: &mut crate::char_class::CharClassifier) {
        use crate::char_class::{CharClass, CharType};
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
        let buckets = self.unk_buckets();

        for (i, &ct) in all_types.iter().enumerate() {
            if i >= buckets.len() {
                break;
            }
            let bucket = &buckets[i];
            let class_name = ct.class_name().to_string();
            target
                .classes
                .entry(class_name.clone())
                .or_insert_with(|| CharClass {
                    name: class_name,
                    invoke: bucket.invoke != 0,
                    group: true,
                    length: 0,
                });
        }
        target.rebuild_props_cache();
    }

    // --- フォーマット変換 ---

    /// v1/v2辞書をv2形式で保存（Trieリビルド不要）
    pub fn save_as_v2<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let mut buf: Vec<u8> = Vec::new();
        let header_size = mem::size_of::<HeaderV2>();
        buf.resize(header_size, 0);

        fn align_to(buf: &mut Vec<u8>, align: usize) {
            let rem = buf.len() % align;
            if rem != 0 {
                buf.resize(buf.len() + (align - rem), 0);
            }
        }

        fn write_raw(buf: &mut Vec<u8>, data: &[u8], align: usize) -> Section {
            align_to(buf, align);
            let offset = buf.len() as u64;
            buf.extend_from_slice(data);
            Section {
                offset,
                bytes: data.len() as u64,
            }
        }

        fn write_pod<T: Pod>(buf: &mut Vec<u8>, data: &[T], align: usize) -> Section {
            write_raw(buf, bytemuck::cast_slice(data), align)
        }

        // Trie base/check (そのままコピー)
        let trie_base = write_raw(&mut buf, self.raw_section(self.trie_base_sec), 4);
        let trie_check = write_raw(&mut buf, self.raw_section(self.trie_check_sec), 4);

        // Sparse trie output
        let (terminal_bits, terminal_ranks, terminal_offsets) = match &self.trie_output_fmt {
            TrieOutputFormat::Dense(sec) => {
                let output: &[u32] = self.pod_slice(*sec);
                let (bits, ranks, offsets) = build_sparse_trie_output(output);
                (
                    write_pod(&mut buf, &bits, 8),
                    write_pod(&mut buf, &ranks, 4),
                    write_pod(&mut buf, &offsets, 4),
                )
            }
            TrieOutputFormat::Sparse { bits, ranks, offsets } => {
                (
                    write_raw(&mut buf, self.raw_section(*bits), 8),
                    write_raw(&mut buf, self.raw_section(*ranks), 4),
                    write_raw(&mut buf, self.raw_section(*offsets), 4),
                )
            }
        };

        // Value pool
        let trie_value_pool = write_raw(&mut buf, self.raw_section(self.trie_value_pool_sec), 4);

        // Entries, features
        let entries = write_raw(&mut buf, self.raw_section(self.entries_sec), 4);
        let features = write_raw(&mut buf, self.raw_section(self.features_sec), 4);

        // String offsets
        let str_offsets = match &self.str_index_fmt {
            StrIndexFormat::PairIndex(sec) => {
                let idx: &[StrIndex] = self.pod_slice(*sec);
                let offsets_data = build_str_offsets(idx, self.str_blob_sec.bytes as usize);
                write_pod(&mut buf, &offsets_data, 4)
            }
            StrIndexFormat::Offsets(sec) => {
                write_raw(&mut buf, self.raw_section(*sec), 4)
            }
        };

        // String blob, matrix, unk
        let str_blob = write_raw(&mut buf, self.raw_section(self.str_blob_sec), 1);
        let matrix_costs = write_raw(&mut buf, self.raw_section(self.matrix_costs_sec), 2);
        let unk_buckets = write_raw(&mut buf, self.raw_section(self.unk_buckets_sec), 4);
        let unk_templates = write_raw(&mut buf, self.raw_section(self.unk_templates_sec), 4);

        let header = HeaderV2 {
            magic: MAGIC,
            version: FORMAT_VERSION_V2,
            flags: 0,
            entry_count: self.entry_count_val,
            feature_count: self.feature_count_val,
            string_count: self.string_count_val,
            matrix_left_size: self.matrix_left_size_val,
            matrix_right_size: self.matrix_right_size_val,
            unk_bucket_count: self.unk_buckets().len() as u32,
            unk_template_count: self.unk_templates_slice().len() as u32,
            trie_base,
            trie_check,
            terminal_bits,
            terminal_ranks,
            terminal_offsets,
            trie_value_pool,
            entries,
            features,
            str_offsets,
            str_blob,
            matrix_costs,
            unk_buckets,
            unk_templates,
        };
        buf[..header_size].copy_from_slice(bytemuck::bytes_of(&header));

        std::fs::write(path, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};
    use std::collections::HashMap;

    fn make_test_dict() -> crate::dict::Dictionary {
        let mut builder = DictBuilder::new();
        let words = vec![
            ("東京", 1, 1, 3000, "名詞,固有名詞,地域,一般", "トウキョウ"),
            ("都", 2, 2, 5000, "名詞,接尾,地域,*", "ト"),
            ("東京都", 3, 3, 2000, "名詞,固有名詞,地域,一般", "トウキョウト"),
        ];
        for (surface, lid, rid, cost, pos, reading) in words {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: lid,
                right_id: rid,
                cost,
                pos: pos.into(),
                base_form: surface.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
            });
        }
        builder.build()
    }

    use std::sync::atomic::{AtomicU32, Ordering};
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn roundtrip_dict() -> (std::path::PathBuf, MmapDictionary) {
        let dict = make_test_dict();
        let mmap_builder = MmapDictBuilder::from_dictionary(&dict);
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = std::env::temp_dir().join(format!("hasami_test_v2_{}.hsd", id));
        mmap_builder.write(&tmp).unwrap();
        let loaded = MmapDictionary::load(&tmp).unwrap();
        (tmp, loaded)
    }

    #[test]
    fn test_roundtrip_entry_count() {
        let (tmp, dict) = roundtrip_dict();
        assert_eq!(dict.entry_count(), 3);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_surfaces() {
        let (tmp, dict) = roundtrip_dict();
        let surfaces: Vec<&str> = (0..dict.entry_count())
            .map(|i| dict.entry_surface(i))
            .collect();
        assert!(surfaces.contains(&"東京"));
        assert!(surfaces.contains(&"都"));
        assert!(surfaces.contains(&"東京都"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_pos() {
        let (tmp, dict) = roundtrip_dict();
        let pos = dict.entry_pos(0);
        assert!(pos.contains("名詞"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_reading() {
        let (tmp, dict) = roundtrip_dict();
        let reading = dict.entry_reading(0);
        assert!(!reading.is_empty());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_cost_info() {
        let (tmp, dict) = roundtrip_dict();
        let (left, right, cost) = dict.entry_cost_info(0);
        assert!(left > 0 || right > 0 || cost != 0);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_entry_arcs() {
        let (tmp, dict) = roundtrip_dict();
        let (surface, pos, base, reading, pronunciation) = dict.entry_arcs(0);
        assert!(!surface.is_empty());
        assert!(!pos.is_empty());
        assert!(!base.is_empty());
        assert!(!reading.is_empty());
        assert!(!pronunciation.is_empty());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_trie_search() {
        let (tmp, dict) = roundtrip_dict();
        let mut results = Vec::new();
        dict.common_prefix_search_cb("東京都庁".as_bytes(), |len, ids| {
            results.push((len, ids.to_vec()));
        });
        assert!(results.len() >= 2, "Expected >=2 results, got {:?}", results);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_matrix() {
        let (tmp, dict) = roundtrip_dict();
        assert_eq!(dict.matrix_left_size(), 1);
        assert_eq!(dict.matrix_right_size(), 1);
        let row = dict.matrix_row(0);
        assert_eq!(row.len(), 1);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_format_version() {
        let (tmp, dict) = roundtrip_dict();
        assert_eq!(dict.format_version(), FORMAT_VERSION_V2);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_string_and_feature_counts() {
        let (tmp, dict) = roundtrip_dict();
        assert!(dict.string_count() > 0);
        assert!(dict.feature_count() > 0);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_builder_string_feature_counts() {
        let dict = make_test_dict();
        let builder = MmapDictBuilder::from_dictionary(&dict);
        assert!(builder.string_count() > 0);
        assert!(builder.feature_count() > 0);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = MmapDictionary::load("/nonexistent/path/dict.hsd");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_magic() {
        let tmp = std::env::temp_dir().join("hasami_test_bad_magic_v2.hsd");
        std::fs::write(&tmp, b"INVALID_MAGIC_BYTES_AND_SOME_PADDING_TO_FILL_HEADER_SIZE_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let result = MmapDictionary::load(&tmp);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_load_too_short_file() {
        let tmp = std::env::temp_dir().join("hasami_test_short_v2.hsd");
        std::fs::write(&tmp, b"short").unwrap();
        let result = MmapDictionary::load(&tmp);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_unk_pos_fallback() {
        let (tmp, dict) = roundtrip_dict();
        let pos = dict.unk_pos(100);
        assert_eq!(pos, "名詞,一般,*,*");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_unk_invoke_out_of_range() {
        let (tmp, dict) = roundtrip_dict();
        assert!(!dict.unk_invoke(100));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_unk_first_template_fallback() {
        let (tmp, dict) = roundtrip_dict();
        let (left, right, cost) = dict.unk_first_template(100);
        assert_eq!(left, 0);
        assert_eq!(right, 0);
        assert_eq!(cost, 10000);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_matrix_row_out_of_range() {
        let (tmp, dict) = roundtrip_dict();
        let row = dict.matrix_row(9999);
        assert!(row.is_empty());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_export_unk_entries() {
        let (tmp, dict) = roundtrip_dict();
        let mut unk = HashMap::new();
        dict.export_unk_entries(&mut unk);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_export_char_classifier() {
        let (tmp, dict) = roundtrip_dict();
        let mut classifier = crate::char_class::CharClassifier::default_japanese();
        dict.export_char_classifier(&mut classifier);
        assert!(classifier.get_class("HIRAGANA").is_some() || classifier.get_class("DEFAULT").is_some());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_string_pool_deduplication() {
        let mut pool = StringPool::default();
        let id1 = pool.intern("hello");
        let id2 = pool.intern("hello");
        let id3 = pool.intern("world");
        assert_eq!(id1, id2, "Same string should get same ID");
        assert_ne!(id1, id3, "Different strings should get different IDs");
    }

    #[test]
    fn test_feature_pool_deduplication() {
        let mut pool = FeaturePool::default();
        let f = FeatureRecord {
            pos_id: 1,
            base_id: 2,
            reading_id: 3,
            pronunciation_id: 4,
        };
        let id1 = pool.intern(f);
        let id2 = pool.intern(f);
        let id3 = pool.intern(FeatureRecord {
            pos_id: 5,
            base_id: 6,
            reading_id: 7,
            pronunciation_id: 8,
        });
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_sparse_trie_output_roundtrip() {
        // Dense output を sparse に変換して正しく検索できるかテスト
        let dict = make_test_dict();
        let mmap_builder = MmapDictBuilder::from_dictionary(&dict);
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = std::env::temp_dir().join(format!("hasami_test_sparse_{}.hsd", id));
        mmap_builder.write(&tmp).unwrap();
        let loaded = MmapDictionary::load(&tmp).unwrap();

        // v2 format should use sparse
        assert_eq!(loaded.format_version, FORMAT_VERSION_V2);

        // 検索結果を検証
        let mut results = Vec::new();
        loaded.common_prefix_search_cb("東京都庁".as_bytes(), |len, ids| {
            results.push((len, ids.to_vec()));
        });
        assert!(results.len() >= 2);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_compute_rank() {
        // 0b1010_1010 = bits at positions 1,3,5,7 are set
        let bits: Vec<u64> = vec![0b1010_1010, 0b1111_0000, 0];
        let ranks: Vec<u32> = vec![12]; // cumulative for first 512-bit block

        assert_eq!(compute_rank(&bits, &ranks, 0), 0); // no bits before pos 0
        assert_eq!(compute_rank(&bits, &ranks, 1), 0); // bit 0 = 0
        assert_eq!(compute_rank(&bits, &ranks, 2), 1); // bit 1 = 1
        assert_eq!(compute_rank(&bits, &ranks, 3), 1); // bit 2 = 0
        assert_eq!(compute_rank(&bits, &ranks, 4), 2); // bits 1,3 set
        assert_eq!(compute_rank(&bits, &ranks, 8), 4); // bits 1,3,5,7 set
    }

    #[test]
    fn test_save_as_v2() {
        let (tmp_v2, dict) = roundtrip_dict();

        // v2 → v2 save should work
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_v2b = std::env::temp_dir().join(format!("hasami_test_v2b_{}.hsd", id));
        dict.save_as_v2(&tmp_v2b).unwrap();

        let reloaded = MmapDictionary::load(&tmp_v2b).unwrap();
        assert_eq!(reloaded.entry_count(), 3);
        assert_eq!(reloaded.format_version(), FORMAT_VERSION_V2);

        // 検索も動作するか
        let mut results = Vec::new();
        reloaded.common_prefix_search_cb("東京都庁".as_bytes(), |len, ids| {
            results.push((len, ids.to_vec()));
        });
        assert!(results.len() >= 2);

        let _ = std::fs::remove_file(&tmp_v2);
        let _ = std::fs::remove_file(&tmp_v2b);
    }
}
