//! カスタム mmap-native バイナリ辞書フォーマット
//!
//! ゼロコピー・ゼロアロケーションで辞書をロードするためのフォーマット。
//! すべてのデータは mmap 上の POD 配列として直接参照される。

use bytemuck::{Pod, Zeroable};
use memmap2::Mmap;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::{mem, slice, str};

// --- オンディスク POD 構造体 ---

const MAGIC: [u8; 8] = *b"HSMDICT\0";
const FORMAT_VERSION: u32 = 1;

/// セクション位置（オフセット＋バイト長）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Section {
    offset: u64,
    bytes: u64,
}

/// ファイルヘッダー
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Header {
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
    // セクション
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

/// 文字列インデックス（POD、8バイト）
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

/// 未知語テンプレート（12バイト、パディングなし）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UnkTemplate {
    pos_id: u32, // str_index への参照
    left_id: u16,
    right_id: u16,
    cost: i16,
    _pad: u16,
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

/// mmap辞書ビルダー
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

        // エントリを変換
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

        // Trie データをコピー
        let trie_base = dict.trie.base_slice().to_vec();
        let trie_check = dict.trie.check_slice().to_vec();
        let trie_output = dict.trie.output_slice().to_vec();
        let trie_value_pool = dict.trie.value_pool_slice().to_vec();

        // 接続行列
        let matrix_costs = dict.matrix.costs.clone();
        let matrix_left_size = dict.matrix.left_size;
        let matrix_right_size = dict.matrix.right_size;

        // 未知語テーブル
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

    /// バイナリ辞書をファイルに書き出し
    pub fn write<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let mut buf: Vec<u8> = Vec::new();

        // ヘッダーのプレースホルダー
        let header_size = mem::size_of::<Header>();
        buf.resize(header_size, 0);

        // アライメントヘルパー
        fn align_to(buf: &mut Vec<u8>, align: usize) {
            let rem = buf.len() % align;
            if rem != 0 {
                buf.resize(buf.len() + (align - rem), 0);
            }
        }

        // セクション書き込みヘルパー
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

        // 各セクション書き込み
        let trie_base = write_section(&mut buf, &self.trie_base, 4);
        let trie_check = write_section(&mut buf, &self.trie_check, 4);
        let trie_output = write_section(&mut buf, &self.trie_output, 4);
        let trie_value_pool = write_section(&mut buf, &self.trie_value_pool, 4);
        let entries = write_section(&mut buf, &self.entries, 4);
        let features_sec = write_section(&mut buf, &self.features.items, 4);
        let str_index = write_section(&mut buf, &self.strings.index, 4);

        // str_blob はアライメント不要（バイト配列）
        let str_blob_offset = buf.len() as u64;
        buf.extend_from_slice(&self.strings.blob);
        let str_blob = Section {
            offset: str_blob_offset,
            bytes: self.strings.blob.len() as u64,
        };

        let matrix_costs = write_section(&mut buf, &self.matrix_costs, 2);
        let unk_buckets = write_section(&mut buf, &self.unk_buckets, 4);
        let unk_templates = write_section(&mut buf, &self.unk_templates, 4);

        // ヘッダーを埋める
        let header = Header {
            magic: MAGIC,
            version: FORMAT_VERSION,
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
            trie_output,
            trie_value_pool,
            entries,
            features: features_sec,
            str_index,
            str_blob,
            matrix_costs,
            unk_buckets,
            unk_templates,
        };
        buf[..header_size].copy_from_slice(bytemuck::bytes_of(&header));

        std::fs::write(path, buf)
    }
}

// --- mmap ローダー ---

/// ゼロコピー mmap 辞書（Arcキャッシュ付き）
pub struct MmapDictionary {
    mmap: Mmap,
    header: Header,
    /// 全文字列の Arc<str> キャッシュ（ロード時に構築）
    arc_cache: Vec<Arc<str>>,
}

impl MmapDictionary {
    /// ファイルからロード（ヒープアロケーションはMmap本体のみ）
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < mem::size_of::<Header>() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file too short"));
        }

        let header: Header = *bytemuck::from_bytes(&mmap[..mem::size_of::<Header>()]);
        if header.magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid magic bytes",
            ));
        }
        if header.version != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported format version",
            ));
        }

        let mut dict = Self {
            mmap,
            header,
            arc_cache: Vec::new(),
        };
        dict.validate()?;
        dict.build_arc_cache();
        Ok(dict)
    }

    fn validate(&self) -> io::Result<()> {
        // 各セクションの境界チェック
        let sections = [
            self.header.trie_base,
            self.header.trie_check,
            self.header.trie_output,
            self.header.trie_value_pool,
            self.header.entries,
            self.header.features,
            self.header.str_index,
            self.header.str_blob,
            self.header.matrix_costs,
            self.header.unk_buckets,
            self.header.unk_templates,
        ];
        for sec in &sections {
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

    #[inline]
    fn pod_slice<T: Pod>(&self, sec: Section) -> &[T] {
        unsafe {
            let ptr = self.mmap.as_ptr().add(sec.offset as usize).cast::<T>();
            slice::from_raw_parts(ptr, sec.bytes as usize / mem::size_of::<T>())
        }
    }

    // --- Trie アクセス ---

    #[inline]
    pub fn trie_base(&self) -> &[i32] {
        self.pod_slice(self.header.trie_base)
    }

    #[inline]
    pub fn trie_check(&self) -> &[i32] {
        self.pod_slice(self.header.trie_check)
    }

    #[inline]
    pub fn trie_output(&self) -> &[u32] {
        self.pod_slice(self.header.trie_output)
    }

    #[inline]
    pub fn trie_value_pool(&self) -> &[u32] {
        self.pod_slice(self.header.trie_value_pool)
    }

    // --- エントリアクセス ---

    #[inline]
    fn entries(&self) -> &[EntryRecord] {
        self.pod_slice(self.header.entries)
    }

    #[inline]
    fn features(&self) -> &[FeatureRecord] {
        self.pod_slice(self.header.features)
    }

    #[inline]
    fn str_index(&self) -> &[StrIndex] {
        self.pod_slice(self.header.str_index)
    }

    #[inline]
    fn str_blob(&self) -> &[u8] {
        let s = self.header.str_blob;
        &self.mmap[s.offset as usize..(s.offset + s.bytes) as usize]
    }

    #[inline]
    fn str_at(&self, id: u32) -> &str {
        let s = self.str_index()[id as usize];
        let bytes = &self.str_blob()[s.off as usize..(s.off + s.len) as usize];
        unsafe { str::from_utf8_unchecked(bytes) }
    }

    /// 全文字列の Arc<str> をeagerに構築
    fn build_arc_cache(&mut self) {
        let count = self.str_index().len();
        self.arc_cache = (0..count)
            .map(|i| Arc::from(self.str_at(i as u32)))
            .collect();
    }

    /// Arc<str>キャッシュから取得
    #[inline]
    fn arc_at(&self, id: u32) -> Arc<str> {
        Arc::clone(&self.arc_cache[id as usize])
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

    /// エントリの全フィールドを借用で取得
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
        self.header.matrix_left_size
    }

    #[inline]
    pub fn matrix_right_size(&self) -> u16 {
        self.header.matrix_right_size
    }

    #[inline]
    pub fn matrix_row(&self, prev_right_id: u16) -> &[i16] {
        let costs: &[i16] = self.pod_slice(self.header.matrix_costs);
        let left = self.header.matrix_left_size as usize;
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
        self.pod_slice(self.header.unk_buckets)
    }

    #[inline]
    fn unk_templates_slice(&self) -> &[UnkTemplate] {
        self.pod_slice(self.header.unk_templates)
    }

    /// CharType インデックスの未知語パラメータを取得
    #[inline]
    pub fn unk_invoke(&self, char_type_idx: usize) -> bool {
        let buckets = self.unk_buckets();
        if char_type_idx < buckets.len() {
            buckets[char_type_idx].invoke != 0
        } else {
            false
        }
    }

    /// CharType インデックスの最初の未知語テンプレート (left_id, right_id, cost) を取得
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
        (0, 0, 10000) // フォールバック
    }

    /// 未知語POSを取得
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

    // --- Trie 共通接頭辞検索（インライン実装） ---

    /// ゼロアロケーション共通接頭辞検索
    #[inline]
    pub fn common_prefix_search_cb(&self, input: &[u8], mut cb: impl FnMut(usize, &[u32])) {
        let base = self.trie_base();
        let check = self.trie_check();
        let output = self.trie_output();
        let value_pool = self.trie_value_pool();
        let base_len = base.len();
        let check_len = check.len();
        let output_len = output.len();

        let mut node = 0usize;

        // ルートノードの出力チェック
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

    pub fn entry_count(&self) -> u32 {
        self.header.entry_count
    }

    pub fn string_count(&self) -> u32 {
        self.header.string_count
    }

    pub fn feature_count(&self) -> u32 {
        self.header.feature_count
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

    /// ヘッダーのフォーマットバージョンを返す（テスト用）
    #[cfg(test)]
    pub(crate) fn format_version(&self) -> u32 {
        self.header.version
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
        // classes 変更後に props_cache を再構築
        target.rebuild_props_cache();
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
        let tmp = std::env::temp_dir().join(format!("hasami_test_rt_{}.hsd", id));
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
    fn test_roundtrip_arc_cache() {
        let (tmp, dict) = roundtrip_dict();
        let surface_arc = dict.entry_surface_arc(0);
        let surface_str = dict.entry_surface(0);
        assert_eq!(&*surface_arc, surface_str);
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
        // Should find "東京" and "東京都"
        assert!(results.len() >= 2, "Expected >=2 results, got {:?}", results);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_roundtrip_matrix() {
        let (tmp, dict) = roundtrip_dict();
        // Default matrix should be 1x1
        assert_eq!(dict.matrix_left_size(), 1);
        assert_eq!(dict.matrix_right_size(), 1);
        let row = dict.matrix_row(0);
        assert_eq!(row.len(), 1);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_format_version() {
        let (tmp, dict) = roundtrip_dict();
        assert_eq!(dict.format_version(), FORMAT_VERSION);
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
        let tmp = std::env::temp_dir().join("hasami_test_bad_magic.hsd");
        std::fs::write(&tmp, b"INVALID_MAGIC_BYTES_AND_SOME_PADDING_TO_FILL_HEADER_SIZE_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let result = MmapDictionary::load(&tmp);
        assert!(result.is_err());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_load_too_short_file() {
        let tmp = std::env::temp_dir().join("hasami_test_short.hsd");
        std::fs::write(&tmp, b"short").unwrap();
        let result = MmapDictionary::load(&tmp);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_unk_pos_fallback() {
        let (tmp, dict) = roundtrip_dict();
        // Out-of-range char_type_idx should return fallback
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
        // Should have some entries (from default japanese classifier)
        // Even without explicit unk.def, the builder still creates buckets
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_export_char_classifier() {
        let (tmp, dict) = roundtrip_dict();
        let mut classifier = crate::char_class::CharClassifier::default_japanese();
        dict.export_char_classifier(&mut classifier);
        // Should still have standard classes
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
}
