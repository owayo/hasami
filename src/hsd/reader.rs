//! v4 辞書の読み込みと検証
//!
//! 読み込み（[`Dictionary::load`]）では、ヘッダ・セクション表・小さな表（メタデータ、品詞・活用の
//! 文字列表、文字種定義、未知語テンプレート）と trie の根・行列の寸法だけを検査する
//! （O(セクション数 + 小さな表)）。trie・エントリ・素性の全件は [`Dictionary::verify`]
//! （`hasami info --verify`）で調べる。解析中に不正な参照を見つけたら [`DictError::Corrupt`] を返し、
//! 空文字列でごまかさない。
//!
//! mmap した辞書ファイルを読み込み中に書き換えたり切り詰めたりしてはいけない（未定義動作になりうる）。
//! hasami の書き出しは一時ファイルに書いてから rename で差し替えるので、この規約を守っている。

use super::container::{self, FLAG_PRUNED_DOMINATED, Layout, SectionId};
use super::features;
use super::meta::{self, Meta};
use super::records::{
    CharCategoryRecord, CharRangeRecord, EntryRecord, LEFT_ID_LIMIT, UnkBucket, UnkTemplate,
};
use super::trie::{self, Node, Trie};
use super::{DictError, strtab};
use crate::char_class::{ALL_CHAR_TYPES, CharClass, CharClassifier, UnkGrouping, type_index};
use crate::dict::{ConnectionMatrix, DictEntry, UnkEntry};
use memmap2::Mmap;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

/// 活用型・活用形が無いことを表す辞書上の値
const NO_CONJ: &str = "*";
static EMPTY_ARC: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from(""));
/// 辞書ごとに振る一意な番号（解析器のキャッシュを辞書が変わったら捨てるため）
static NEXT_DICT_ID: AtomicU64 = AtomicU64::new(0);

enum Storage {
    Mmap(Mmap),
    /// 8 バイト境界に揃えた所有バッファ（先頭 len バイトが辞書）
    Owned {
        words: Vec<u64>,
        len: usize,
    },
}

impl Storage {
    fn bytes(&self) -> &[u8] {
        match self {
            Storage::Mmap(m) => m,
            Storage::Owned { words, len } => &bytemuck::cast_slice(words)[..*len],
        }
    }
}

/// 文字種ごとの未知語の設定（先頭のテンプレート）
#[derive(Clone)]
pub(crate) struct UnkInfo {
    pub invoke: bool,
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
    pub pos: Arc<str>,
    /// 同じ文字種の並びから作る候補（char.def の group・length）
    pub grouping: UnkGrouping,
}

/// 解析の最内側で使う型付きスライス（解析 1 回ごとに作る）
#[derive(Clone, Copy)]
pub(crate) struct DictView<'a> {
    pub trie: Trie<'a>,
    pub entries: &'a [EntryRecord],
    pub matrix: &'a [i16],
    pub num_left: usize,
    pub num_right: usize,
}

impl DictView<'_> {
    /// 転置した接続行列の、次の語の left_id の行（前の語の right_id で引く）
    #[inline]
    pub fn matrix_row(&self, left_id: u16) -> &[i16] {
        let start = left_id as usize * self.num_right;
        &self.matrix[start..start + self.num_right]
    }
}

/// `hasami info --verify` の結果
#[derive(Clone, Debug, Default)]
pub struct VerifyReport {
    pub trie: trie::TrieStats,
    pub entries: usize,
    pub groups: usize,
    /// 群の大きさの最大値
    pub max_group: usize,
    /// 群の大きさの分布（大きさ, 群の数）。大きさの昇順
    pub group_sizes: Vec<(usize, usize)>,
    /// 参照されている素性レコードの数（重複を除く）
    pub features: usize,
}

/// v4 辞書（mmap したファイル、またはメモリ上のバッファ）
///
/// 辞書は読み込み専用で、`Arc` で包んで複数のスレッド・[`crate::Analyzer`] から共有できる。
pub struct Dictionary {
    id: u64,
    storage: Storage,
    layout: Layout,
    meta: Meta,
    pos: Vec<Arc<str>>,
    conj_types: Vec<Arc<str>>,
    conj_forms: Vec<Arc<str>>,
    /// `Token` に出す活用型・活用形（`*` を空文字列にしたもの）
    token_conj_types: Vec<Arc<str>>,
    token_conj_forms: Vec<Arc<str>>,
    classifier: CharClassifier,
    /// U+0000〜U+FFFF の文字種（`classifier.classify_char` と同じ結果を表で引く）
    bmp_char_types: Box<[u8]>,
    unk: Vec<UnkInfo>,
    entry_count: usize,
    num_left: usize,
    num_right: usize,
    max_code: u32,
}

impl std::fmt::Debug for Dictionary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dictionary")
            .field("name", &self.meta.name())
            .field("pos_scheme", &self.meta.pos_scheme())
            .field("entries", &self.entry_count)
            .field("matrix", &(self.num_left, self.num_right))
            .field("pruned", &self.is_pruned())
            .field("bytes", &self.byte_len())
            .finish()
    }
}

fn cast<T: bytemuck::Pod>(bytes: &[u8], what: SectionId) -> Result<&[T], DictError> {
    bytemuck::try_cast_slice(bytes).map_err(|e| {
        DictError::corrupt(format!(
            "section {} cannot be read as {}-byte records: {e:?}",
            what.name(),
            size_of::<T>()
        ))
    })
}

fn to_token_conj(values: &[Arc<str>]) -> Vec<Arc<str>> {
    values
        .iter()
        .map(|v| {
            if &**v == NO_CONJ {
                Arc::clone(&EMPTY_ARC)
            } else {
                Arc::clone(v)
            }
        })
        .collect()
}

impl Dictionary {
    /// .hsd ファイルを mmap して読み込む
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, DictError> {
        let file = std::fs::File::open(path)?;
        // SAFETY: 読み込み中のファイルを書き換えない規約（モジュールの説明）を前提にする。
        // 中身は下の検査を経てからしか参照しない
        let mmap = unsafe { Mmap::map(&file)? };
        Self::from_storage(Storage::Mmap(mmap))
    }

    /// メモリ上のバイト列から読み込む（8 バイト境界の所有バッファに複製する）
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DictError> {
        let mut words = vec![0u64; bytes.len().div_ceil(8)];
        bytemuck::cast_slice_mut::<u64, u8>(&mut words)[..bytes.len()].copy_from_slice(bytes);
        Self::from_owned(words, bytes.len())
    }

    pub(crate) fn from_owned(words: Vec<u64>, len: usize) -> Result<Self, DictError> {
        Self::from_storage(Storage::Owned { words, len })
    }

    fn from_storage(storage: Storage) -> Result<Self, DictError> {
        let data = storage.bytes();
        let layout = container::parse(data)?;
        let section = |id: SectionId| {
            let r = layout.get(id);
            &data[r.offset..r.end()]
        };

        let meta = Meta::parse(section(SectionId::Meta))?;
        let pruned = layout.flags & FLAG_PRUNED_DOMINATED != 0;
        if let Some(v) = meta.get(meta::KEY_PRUNED_DOMINATED) {
            if (v == "true") != pruned {
                return Err(DictError::corrupt(
                    "META pruned_dominated disagrees with the header flag",
                ));
            }
        }

        const U16_TABLE: usize = u16::MAX as usize + 1;
        let pos = strtab::parse(section(SectionId::PosStrings), "POS_STRINGS", U16_TABLE)?;
        let conj_types = strtab::parse(
            section(SectionId::ConjTypeStrings),
            "CONJ_TYPE_STRINGS",
            U16_TABLE,
        )?;
        let conj_forms = strtab::parse(
            section(SectionId::ConjFormStrings),
            "CONJ_FORM_STRINGS",
            U16_TABLE,
        )?;
        let category_names = strtab::parse(
            section(SectionId::CategoryNames),
            "CATEGORY_NAMES",
            U16_TABLE,
        )?;

        // trie（char map の全体と根だけ検査する）
        let trie = Trie::new(
            cast(section(SectionId::CharBlocks), SectionId::CharBlocks)?,
            cast(section(SectionId::CharTables), SectionId::CharTables)?,
            cast::<Node>(section(SectionId::TrieNodes), SectionId::TrieNodes)?,
            section(SectionId::TrieTails),
        )?;
        let max_code = trie.max_code();

        let entries: &[EntryRecord] = cast(section(SectionId::Entries), SectionId::Entries)?;
        let feature_offsets: &[u32] = cast(
            section(SectionId::FeatureOffsets),
            SectionId::FeatureOffsets,
        )?;
        if entries.is_empty() {
            return Err(DictError::corrupt("the dictionary has no entries"));
        }
        if entries.len() >= trie::VALUE_LIMIT as usize {
            return Err(DictError::corrupt("too many entries"));
        }
        if entries.len() != feature_offsets.len() {
            return Err(DictError::corrupt(format!(
                "ENTRIES has {} records but FEATURE_OFFSETS has {}",
                entries.len(),
                feature_offsets.len()
            )));
        }

        // 接続行列の寸法
        let matrix = section(SectionId::Matrix);
        if matrix.len() < 8 {
            return Err(DictError::corrupt("MATRIX is shorter than its dimensions"));
        }
        let num_left = u32::from_le_bytes(matrix[..4].try_into().unwrap()) as usize;
        let num_right = u32::from_le_bytes(matrix[4..8].try_into().unwrap()) as usize;
        if num_left == 0 || num_right == 0 || num_left > LEFT_ID_LIMIT || num_right > 1 << 16 {
            return Err(DictError::corrupt(format!(
                "MATRIX dimensions {num_left} x {num_right} are out of range"
            )));
        }
        let expected = num_left
            .checked_mul(num_right)
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(8));
        if expected != Some(matrix.len()) {
            return Err(DictError::corrupt(format!(
                "MATRIX is {} bytes, expected 8 + 2 x {num_left} x {num_right}",
                matrix.len()
            )));
        }
        cast::<i16>(&matrix[8..], SectionId::Matrix)?;

        // 文字種定義
        let categories: &[CharCategoryRecord] = cast(
            section(SectionId::CharCategories),
            SectionId::CharCategories,
        )?;
        let ranges: &[CharRangeRecord] =
            cast(section(SectionId::CharRanges), SectionId::CharRanges)?;
        let name = |id: u32| {
            category_names
                .get(id as usize)
                .ok_or_else(|| DictError::corrupt(format!("category name {id} is out of range")))
        };
        let mut classes = HashMap::new();
        for c in categories {
            let n = name(c.name_id)?.to_string();
            classes.insert(
                n.clone(),
                CharClass {
                    name: n,
                    invoke: c.invoke != 0,
                    group: c.group != 0,
                    length: c.length,
                },
            );
        }
        let mut range_defs = Vec::with_capacity(ranges.len());
        for r in ranges {
            if r.start > r.end || r.end > char::MAX as u32 {
                return Err(DictError::corrupt(format!(
                    "character range {:#x}..{:#x} is invalid",
                    r.start, r.end
                )));
            }
            range_defs.push((r.start, r.end, name(r.category_name_id)?.to_string()));
        }
        let classifier = CharClassifier::from_definitions(classes, range_defs);

        // 未知語テンプレート（文字種ごとに先頭の 1 つを使う）
        let buckets: &[UnkBucket] = cast(section(SectionId::UnkBuckets), SectionId::UnkBuckets)?;
        let templates: &[UnkTemplate] =
            cast(section(SectionId::UnkTemplates), SectionId::UnkTemplates)?;
        if buckets.len() != ALL_CHAR_TYPES.len() {
            return Err(DictError::corrupt(format!(
                "UNK_BUCKETS has {} buckets, expected {}",
                buckets.len(),
                ALL_CHAR_TYPES.len()
            )));
        }
        for t in templates {
            if t.pos_id as usize >= pos.len()
                || t.left_id as usize >= num_left
                || t.right_id as usize >= num_right
            {
                return Err(DictError::corrupt(
                    "unknown-word template refers outside the tables",
                ));
            }
        }
        let mut unk = Vec::with_capacity(buckets.len());
        for (b, &char_type) in buckets.iter().zip(&ALL_CHAR_TYPES) {
            let start = b.template_start as usize;
            let end = start + b.template_count as usize;
            if b.template_count == 0 || end > templates.len() {
                return Err(DictError::corrupt(
                    "unknown-word bucket is empty or out of range",
                ));
            }
            let t = &templates[start];
            unk.push(UnkInfo {
                invoke: b.invoke != 0,
                left_id: t.left_id,
                right_id: t.right_id,
                cost: t.cost,
                pos: Arc::clone(&pos[t.pos_id as usize]),
                grouping: classifier.unk_grouping(char_type),
            });
        }
        let bmp_char_types = classifier.bmp_type_table();

        let entry_count = entries.len();
        let token_conj_types = to_token_conj(&conj_types);
        let token_conj_forms = to_token_conj(&conj_forms);
        Ok(Dictionary {
            id: NEXT_DICT_ID.fetch_add(1, Ordering::Relaxed),
            storage,
            layout,
            meta,
            pos,
            conj_types,
            conj_forms,
            token_conj_types,
            token_conj_forms,
            classifier,
            bmp_char_types,
            unk,
            entry_count,
            num_left,
            num_right,
            max_code,
        })
    }

    fn section(&self, id: SectionId) -> &[u8] {
        let r = self.layout.get(id);
        &self.storage.bytes()[r.offset..r.end()]
    }

    /// 型付きスライス（ロード時に長さと整列を確かめてあるので失敗しない）
    fn typed<T: bytemuck::Pod>(&self, id: SectionId) -> &[T] {
        bytemuck::cast_slice(self.section(id))
    }

    fn trie(&self) -> Trie<'_> {
        Trie::from_validated(
            self.typed(SectionId::CharBlocks),
            self.typed(SectionId::CharTables),
            self.typed(SectionId::TrieNodes),
            self.section(SectionId::TrieTails),
            self.max_code,
        )
    }

    pub(crate) fn view(&self) -> DictView<'_> {
        DictView {
            trie: self.trie(),
            entries: self.typed(SectionId::Entries),
            matrix: bytemuck::cast_slice(&self.section(SectionId::Matrix)[8..]),
            num_left: self.num_left,
            num_right: self.num_right,
        }
    }

    /// この辞書の一意な番号（プロセス内で読み込んだ順）
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// 文字の文字種（[`type_index`] の値）。char.def の範囲による `classify_char` と同じ結果を、
    /// U+FFFF までは表で引く
    #[inline]
    pub(crate) fn char_type_index(&self, c: char) -> u8 {
        match self.bmp_char_types.get(c as usize) {
            Some(&t) => t,
            None => type_index(self.classifier.classify_char(c)) as u8,
        }
    }

    pub(crate) fn unk_info(&self, char_type_index: usize) -> &UnkInfo {
        &self.unk[char_type_index]
    }

    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// 支配エントリを除いた最終辞書か（repair・merge の入力にできない）
    pub fn is_pruned(&self) -> bool {
        self.layout.flags & FLAG_PRUNED_DOMINATED != 0
    }

    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// 接続行列の寸法（次の語の left_id の数, 前の語の right_id の数）
    pub fn matrix_dims(&self) -> (usize, usize) {
        (self.num_left, self.num_right)
    }

    pub fn pos_count(&self) -> usize {
        self.pos.len()
    }

    pub fn conj_type_count(&self) -> usize {
        self.conj_types.len()
    }

    pub fn conj_form_count(&self) -> usize {
        self.conj_forms.len()
    }

    /// ファイルの長さ
    pub fn byte_len(&self) -> usize {
        self.storage.bytes().len()
    }

    /// 読み飛ばした未知のセクションの数（新しい hasami が足したセクション）
    pub fn unknown_section_count(&self) -> usize {
        self.layout.unknown_sections
    }

    /// 各セクションの (名前, バイト数)
    pub fn section_sizes(&self) -> Vec<(&'static str, usize)> {
        self.layout
            .sections()
            .map(|(id, r)| (id.name(), r.len))
            .collect()
    }

    /// 解析で最初に触れるページ（char map・trie・エントリ・行列）を先に読み込んでおく
    ///
    /// mmap は触れたページから読み込まれるので、起動直後の最初の解析が遅くなるのを避けたいときに使う。
    pub fn prewarm(&self) {
        const PAGE: usize = 4096;
        let mut sum = 0u8;
        for id in [
            SectionId::CharBlocks,
            SectionId::CharTables,
            SectionId::TrieNodes,
            SectionId::TrieTails,
            SectionId::Entries,
            SectionId::Matrix,
        ] {
            for b in self.section(id).iter().step_by(PAGE) {
                sum = sum.wrapping_add(*b);
            }
        }
        std::hint::black_box(sum);
    }

    /// エントリの素性レコード。品詞・活用型・活用形の番号が文字列表の範囲内であることを確かめてある
    pub(crate) fn feature(&self, entry_id: usize) -> Result<features::FeatureRef<'_>, DictError> {
        let offsets: &[u32] = self.typed(SectionId::FeatureOffsets);
        let offset = *offsets
            .get(entry_id)
            .ok_or_else(|| DictError::corrupt(format!("entry {entry_id} is out of range")))?;
        let f = features::decode(self.section(SectionId::Features), offset as usize)?;
        if f.pos_id as usize >= self.pos.len()
            || f.conj_type_id as usize >= self.conj_types.len()
            || f.conj_form_id as usize >= self.conj_forms.len()
        {
            return Err(DictError::corrupt(format!(
                "feature record at offset {offset} refers outside the string tables"
            )));
        }
        Ok(f)
    }

    /// 品詞の文字列（番号は [`Dictionary::feature`] が確かめたもの）
    pub(crate) fn pos_name(&self, id: u16) -> &str {
        &self.pos[id as usize]
    }

    /// `Token` に出す活用型（`*` は空文字列）
    pub(crate) fn token_conj_type(&self, id: u16) -> &str {
        &self.token_conj_types[id as usize]
    }

    /// `Token` に出す活用形（`*` は空文字列）
    pub(crate) fn token_conj_form(&self, id: u16) -> &str {
        &self.token_conj_forms[id as usize]
    }

    /// `text` の接頭辞に一致する語を引く。Returns: (接頭辞の終わりのバイト位置, その表層形のエントリ) の列
    pub fn lookup(&self, text: &str) -> Result<Vec<(usize, Vec<DictEntry>)>, DictError> {
        let mut hits: Vec<(usize, u32)> = Vec::new();
        self.trie()
            .common_prefix_search(text, 0, |end, group| hits.push((end, group)))?;
        let entries: &[EntryRecord] = self.typed(SectionId::Entries);
        let mut out = Vec::with_capacity(hits.len());
        for (end, group) in hits {
            let surface: Arc<str> = Arc::from(&text[..end]);
            let mut list = Vec::new();
            let mut i = group as usize;
            loop {
                let e = entries.get(i).ok_or_else(|| {
                    DictError::corrupt(format!("group at {group} runs past the last entry"))
                })?;
                list.push(self.entry_at(i, e, &surface)?);
                if e.is_last() {
                    break;
                }
                i += 1;
            }
            out.push((end, list));
        }
        Ok(out)
    }

    /// エントリ 1 件を `DictEntry` に復元する
    fn entry_at(
        &self,
        i: usize,
        e: &EntryRecord,
        surface: &Arc<str>,
    ) -> Result<DictEntry, DictError> {
        let f = self.feature(i)?;
        let reading: Arc<str> = Arc::from(f.reading.into_string());
        let pronunciation = f
            .pronunciation
            .map_or_else(|| Arc::clone(&reading), |p| Arc::from(p.into_string()));
        let base_form = f
            .base_form
            .map_or_else(|| Arc::clone(surface), |b| Arc::from(b.into_string()));
        Ok(DictEntry {
            surface: Arc::clone(surface),
            left_id: e.left_id(),
            right_id: e.right_id,
            cost: e.cost,
            pos: Arc::clone(&self.pos[f.pos_id as usize]),
            conj_type: Arc::clone(&self.conj_types[f.conj_type_id as usize]),
            conj_form: Arc::clone(&self.conj_forms[f.conj_form_id as usize]),
            base_form,
            reading,
            pronunciation,
        })
    }

    /// 全表層形（重複なし、バイト順）。文分割の例外表の抽出などに使う
    pub fn surfaces(&self) -> Result<Vec<String>, DictError> {
        Ok(self.groups()?.into_iter().map(|(_, key)| key).collect())
    }

    /// 表層形と群の先頭を集め、群の先頭の昇順（= 表層形のバイト順）に並べる
    fn groups(&self) -> Result<Vec<(u32, String)>, DictError> {
        let mut keys = self.trie().keys()?;
        keys.sort_unstable_by_key(|(v, _)| *v);
        Ok(keys)
    }

    /// 全エントリを表層形のバイト順（同じ表層形の中は群の順）に渡す。export・repair・merge 用
    pub fn for_each_entry(
        &self,
        mut cb: impl FnMut(DictEntry) -> Result<(), DictError>,
    ) -> Result<(), DictError> {
        let entries: &[EntryRecord] = self.typed(SectionId::Entries);
        let groups = self.groups()?;
        for (start, key) in groups {
            let surface: Arc<str> = Arc::from(key);
            let mut i = start as usize;
            loop {
                let e = entries.get(i).ok_or_else(|| {
                    DictError::corrupt(format!("group at {start} runs past the last entry"))
                })?;
                cb(self.entry_at(i, e, &surface)?)?;
                if e.is_last() {
                    break;
                }
                i += 1;
            }
        }
        Ok(())
    }

    /// 接続行列（matrix.def なしで作った辞書のゼロ行列なら None）
    pub fn connection_matrix(&self) -> Option<ConnectionMatrix> {
        if self.meta.get(meta::KEY_ZERO_MATRIX) == Some("true") {
            return None;
        }
        Some(ConnectionMatrix {
            num_left: self.num_left as u32,
            num_right: self.num_right as u32,
            costs: self.view().matrix.to_vec(),
        })
    }

    /// 文字種定義（char.def）
    pub fn char_classifier(&self) -> CharClassifier {
        self.classifier.clone()
    }

    /// 未知語テンプレート（unk.def）。文字種名ごとにまとめる
    pub fn unk_entries(&self) -> HashMap<String, Vec<UnkEntry>> {
        let buckets: &[UnkBucket] = self.typed(SectionId::UnkBuckets);
        let templates: &[UnkTemplate] = self.typed(SectionId::UnkTemplates);
        let mut out: HashMap<String, Vec<UnkEntry>> = HashMap::new();
        for (b, ct) in buckets.iter().zip(ALL_CHAR_TYPES) {
            let class_name = ct.class_name();
            // NUMERIC のように 2 つの文字種が同じ名前を使うときは 1 回だけ取り込む
            if out.contains_key(class_name) {
                continue;
            }
            let start = b.template_start as usize;
            let list = templates[start..start + b.template_count as usize]
                .iter()
                .map(|t| UnkEntry {
                    char_class: class_name.to_string(),
                    left_id: t.left_id,
                    right_id: t.right_id,
                    cost: t.cost,
                    pos: self.pos[t.pos_id as usize].to_string(),
                })
                .collect();
            out.insert(class_name.to_string(), list);
        }
        out
    }

    /// 全体を検証する（全ノード、全末尾レコード、全群、全素性レコード）。`hasami info --verify` 用
    pub fn verify(&self) -> Result<VerifyReport, DictError> {
        let trie = self.trie();
        let trie_stats = trie.verify(self.entry_count as u32)?;
        let entries: &[EntryRecord] = self.typed(SectionId::Entries);

        // 群: trie の値の昇順と、表層形のバイト順が一致し、末尾の印と交互に並ぶ
        let groups = self.groups()?;
        let mut sizes: HashMap<usize, usize> = HashMap::new();
        let mut expected_start = 0usize;
        let mut prev_key: Option<&str> = None;
        for (start, key) in &groups {
            let start = *start as usize;
            if start != expected_start {
                return Err(DictError::corrupt(format!(
                    "groups do not tile the entries: expected a group at {expected_start}, found {start}"
                )));
            }
            if prev_key.is_some_and(|p| p.as_bytes() >= key.as_bytes()) {
                return Err(DictError::corrupt(format!(
                    "surface `{key}` is out of byte order"
                )));
            }
            prev_key = Some(key);
            if trie.get(key)? != Some(start as u32) {
                return Err(DictError::corrupt(format!(
                    "trie lookup of `{key}` does not return its group"
                )));
            }
            let mut end = start;
            loop {
                let e = entries.get(end).ok_or_else(|| {
                    DictError::corrupt(format!("group at {start} runs past the last entry"))
                })?;
                end += 1;
                if e.is_last() {
                    break;
                }
            }
            *sizes.entry(end - start).or_default() += 1;
            expected_start = end;
        }
        if expected_start != entries.len() {
            return Err(DictError::corrupt(format!(
                "entries {expected_start}.. belong to no group"
            )));
        }

        let mut offsets_seen = std::collections::HashSet::new();
        let offsets: &[u32] = self.typed(SectionId::FeatureOffsets);
        for (i, e) in entries.iter().enumerate() {
            if e.left_id() as usize >= self.num_left || e.right_id as usize >= self.num_right {
                return Err(DictError::corrupt(format!(
                    "entry {i} has context IDs outside the matrix"
                )));
            }
            if offsets_seen.insert(offsets[i]) {
                self.feature(i)?;
            }
        }

        let mut group_sizes: Vec<(usize, usize)> = sizes.into_iter().collect();
        group_sizes.sort_unstable();
        Ok(VerifyReport {
            trie: trie_stats,
            entries: entries.len(),
            groups: groups.len(),
            max_group: group_sizes.last().map_or(0, |(s, _)| *s),
            group_sizes,
            features: offsets_seen.len(),
        })
    }
}
