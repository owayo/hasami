//! 辞書モジュール - エントリ、接続コスト行列、辞書構築

use crate::char_class::{CharClass, CharClassifier};
use crate::trie::DoubleArrayTrie;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::Arc;

/// フィールドのパースヘルパー（エラー時にファイル名・行番号・フィールド名を含む）
fn parse_field<T: std::str::FromStr>(
    path: &Path,
    line_no: usize,
    field: &str,
    raw: &str,
) -> io::Result<T>
where
    T::Err: std::fmt::Display,
{
    raw.parse().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}:{}: invalid {} `{}`: {}",
                path.display(),
                line_no,
                field,
                raw,
                e
            ),
        )
    })
}

/// 文字列が全てカタカナ（U+30A0〜U+30FF）かどうか判定する
fn is_katakana_str(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| ('\u{30A0}'..='\u{30FF}').contains(&c))
}

/// 発音の合成に使う部品の最大文字数
///
/// 熟語の構成要素はほぼ 1〜2 字で、4 字あれば「アメリカ国防総省」のような
/// 長い複合語も分けられる。これより長い部品を許すと分割の候補が増えるだけで当たらない。
const MAX_PART_CHARS: usize = 4;

/// 仮名の母音を返す（拗音の小書き仮名は直前の仮名に従うので None）
fn kana_vowel(c: char) -> Option<char> {
    Some(match c {
        'ア' | 'カ' | 'サ' | 'タ' | 'ナ' | 'ハ' | 'マ' | 'ヤ' | 'ラ' | 'ワ' | 'ガ' | 'ザ'
        | 'ダ' | 'バ' | 'パ' | 'ャ' | 'ァ' => 'ア',
        'イ' | 'キ' | 'シ' | 'チ' | 'ニ' | 'ヒ' | 'ミ' | 'リ' | 'ヰ' | 'ギ' | 'ジ' | 'ヂ'
        | 'ビ' | 'ピ' | 'ィ' => 'イ',
        'ウ' | 'ク' | 'ス' | 'ツ' | 'ヌ' | 'フ' | 'ム' | 'ユ' | 'ル' | 'グ' | 'ズ' | 'ヅ'
        | 'ブ' | 'プ' | 'ュ' | 'ゥ' | 'ヴ' => 'ウ',
        'エ' | 'ケ' | 'セ' | 'テ' | 'ネ' | 'ヘ' | 'メ' | 'レ' | 'ヱ' | 'ゲ' | 'ゼ' | 'デ'
        | 'ベ' | 'ペ' | 'ェ' => 'エ',
        'オ' | 'コ' | 'ソ' | 'ト' | 'ノ' | 'ホ' | 'モ' | 'ヨ' | 'ロ' | 'ヲ' | 'ゴ' | 'ゾ'
        | 'ド' | 'ボ' | 'ポ' | 'ョ' | 'ォ' => 'オ',
        _ => return None,
    })
}

/// 読みに歴史的仮名遣いの長音（オ段・ウ段 + ウ）が含まれるかどうか
///
/// 「ホウホウ」「ジュウヨウ」のようにここに当たる語だけが、発音の長音表記
/// （ホーホー / ジューヨー）を持つべき語になる。
fn has_unmarked_long_vowel(reading: &str) -> bool {
    let mut prev: Option<char> = None;
    for c in reading.chars() {
        if c == 'ウ' && matches!(prev, Some('オ') | Some('ウ')) {
            return true;
        }
        prev = kana_vowel(c);
    }
    false
}

/// 表層形を辞書内の短い既知語に分けて、部品の発音を連結した発音を組み立てる。
///
/// 借用元（同じ表層形・読みを持つ健全なエントリ）が無い語の受け皿。「商材(ショウザイ)」は
/// 「商(ショウ→ショー)」と「材(ザイ)」に分かれるので「ショーザイ」になる。
///
/// 部品の境界をまたぐ「オ段 + ウ」は連結しても長音にならないので、
/// 「小売(コウリ)」は「小(コ)」+「売(ウリ)」となり「コーリ」にはならない。
/// かな列だけを見て機械的に長音化すると、この区別が付かない。
///
/// 分割は長い部品を優先して探し、最初に見つかった分割を採る。
fn compose_pronunciation(
    surface: &str,
    reading: &str,
    parts: &HashMap<&str, Vec<(&str, Arc<str>)>>,
) -> Option<String> {
    let s: Vec<char> = surface.chars().collect();
    let r: Vec<char> = reading.chars().collect();
    if s.len() < 2 {
        return None;
    }
    let mut memo: HashMap<(usize, usize), Option<String>> = HashMap::new();
    // 語全体を 1 部品にすると自分自身の（直したい）発音がそのまま返るので、
    // 先頭の部品は語より短いものに限る
    for len in (1..=MAX_PART_CHARS.min(s.len() - 1)).rev() {
        let head: String = s[..len].iter().collect();
        let Some(candidates) = parts.get(head.as_str()) else {
            continue;
        };
        for (part_reading, part_pron) in candidates {
            let n = part_reading.chars().count();
            if n > r.len() || !part_reading.chars().eq(r[..n].iter().copied()) {
                continue;
            }
            if let Some(rest) = compose_at(&s, &r, len, n, parts, &mut memo) {
                let composed = format!("{part_pron}{rest}");
                // 分割できても発音が読みと同じなら直すものが無い
                return (composed != reading).then_some(composed);
            }
        }
    }
    None
}

/// `compose_pronunciation` の本体。表層形 i 文字目・読み j 文字目から先を組み立てる。
fn compose_at(
    s: &[char],
    r: &[char],
    i: usize,
    j: usize,
    parts: &HashMap<&str, Vec<(&str, Arc<str>)>>,
    memo: &mut HashMap<(usize, usize), Option<String>>,
) -> Option<String> {
    if i == s.len() {
        return (j == r.len()).then(String::new);
    }
    if j >= r.len() {
        return None;
    }
    if let Some(cached) = memo.get(&(i, j)) {
        return cached.clone();
    }
    let mut found = None;
    'outer: for len in (1..=MAX_PART_CHARS.min(s.len() - i)).rev() {
        let sub: String = s[i..i + len].iter().collect();
        let Some(candidates) = parts.get(sub.as_str()) else {
            continue;
        };
        for (part_reading, part_pron) in candidates {
            let n = part_reading.chars().count();
            if j + n > r.len() || !part_reading.chars().eq(r[j..j + n].iter().copied()) {
                continue;
            }
            if let Some(rest) = compose_at(s, r, i + len, j + n, parts, memo) {
                found = Some(format!("{part_pron}{rest}"));
                break 'outer;
            }
        }
    }
    memo.insert((i, j), found.clone());
    found
}

/// 発音の合成を試すエントリかどうか
///
/// 分割で組み立てた発音が元より確かなのは、漢字表記の名詞に限られる。活用語は語尾が
/// 辞書の部品と合わない。人名・地名は特殊な読みを部品から組み立てられないので避けるが、
/// NEologd は「高品質」のような普通名詞も「固有名詞,一般」で登録しているので、
/// そちらは対象に含める。
fn is_composable(entry: &DictEntry) -> bool {
    entry.pos.starts_with("名詞")
        && !is_name_like_proper_noun(&entry.pos)
        && !entry.pos.starts_with("名詞,数")
        && is_katakana_str(&entry.reading)
        && has_unmarked_long_vowel(&entry.reading)
        && entry.surface.chars().any(is_kanji)
}

/// 人名・組織・地域の固有名詞かどうか（「固有名詞,一般」は普通名詞が多いので含めない）
fn is_name_like_proper_noun(pos: &str) -> bool {
    pos.starts_with("名詞,固有名詞") && !pos.starts_with("名詞,固有名詞,一般")
}

/// 漢字（CJK統合漢字と拡張A、繰り返し記号）かどうか判定する
fn is_kanji(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c) || c == '々'
}

/// カタカナ（長音記号を含む）かどうか判定する
fn is_katakana(c: char) -> bool {
    ('\u{30A0}'..='\u{30FF}').contains(&c)
}

/// 漢数字（位取りを含む）かどうか判定する
fn is_kansuji(c: char) -> bool {
    matches!(
        c,
        '〇' | '零'
            | '一'
            | '二'
            | '三'
            | '四'
            | '五'
            | '六'
            | '七'
            | '八'
            | '九'
            | '十'
            | '百'
            | '千'
            | '万'
            | '億'
            | '兆'
    )
}

/// 異表記エントリ削除時にログへ出すサンプル件数
const ORTHO_VARIANT_SAMPLE: usize = 20;

/// 品詞が「本来の語」（表記ゆれ正規化エントリではありえない語）かどうか判定する
///
/// 活用語と機能語、および代名詞を対象とする。これらの表層形と衝突する名詞エントリは
/// 表記ゆれ由来の可能性が高い。
fn is_canonical_word(pos: &str) -> bool {
    pos.starts_with("動詞")
        || pos.starts_with("形容詞")
        || pos.starts_with("副詞")
        || pos.starts_with("助詞")
        || pos.starts_with("助動詞")
        || pos.starts_with("連体詞")
        || pos.starts_with("接続詞")
        || pos.starts_with("感動詞")
        || pos.starts_with("名詞,代名詞")
        || pos.starts_with("名詞,非自立")
}

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
        let mut results = Vec::new();
        self.trie.common_prefix_search_cb(input, |len, ids| {
            let entries: Vec<&DictEntry> =
                ids.iter().map(|&id| &self.entries[id as usize]).collect();
            results.push((len, entries));
        });
        results
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

    /// 現在のエントリ一覧
    pub fn entries(&self) -> &[DictEntry] {
        &self.entries
    }

    /// pronunciation が壊れている（カタカナでない）エントリを修復する
    ///
    /// SudachiDict 由来のエントリは pronunciation に表層形（漢字・ひらがな・ラテン文字）が
    /// 入っているため、そのままでは音声合成で長音が失われたり読みが消えたりする。
    ///
    /// 修復は以下の優先順で行う:
    /// 1. 同じ (表層形, 読み) を持つ健全なエントリの発音形を借用する。
    ///    IPAdic 由来のエントリは人手で作られた発音形（ホウホウ→ホーホー）を持つため、
    ///    マージ辞書の中で最も信頼できる長音表記の供給源になる。
    /// 2. 借用元が無い語は、辞書内の短い既知語に分けて部品の発音を連結する。
    ///    「商材(ショウザイ)」は借用元が無いが「商(ショー)」+「材(ザイ)」に分かれる。
    ///    部品の境界をまたぐ「オ段 + ウ」は長音にならないので、「小売」は「コウリ」のまま残る。
    /// 3. どちらもできない場合は読みをそのまま発音とする（長音化はされないが読みは保たれる）。
    /// 4. 読みも壊れている（ラテン文字のまま等）場合は発音・読みを空にする。
    ///    空にしておくと解析時の読み補完（スペルアウト等）が働く。
    ///
    /// Returns: 修復されたエントリ数
    pub fn repair_pronunciation(&mut self) -> usize {
        // 健全なエントリから (表層形, 読み) → 発音 の対応表を作る。
        // 読みと違う発音（「リョウ」に対する「リョー」のように長音表記を持つ形）を優先する。
        let mut trusted: HashMap<(&str, &str), &Arc<str>> = HashMap::new();
        for entry in &self.entries {
            if !is_katakana_str(&entry.pronunciation) || !is_katakana_str(&entry.reading) {
                continue;
            }
            let best = trusted.entry((&entry.surface, &entry.reading)).or_insert(&entry.pronunciation);
            if ***best == *entry.reading && entry.pronunciation != entry.reading {
                *best = &entry.pronunciation;
            }
        }
        // 合成に使う部品の索引。健全な発音を持つ短い語だけを集める。
        // ひらがなを含む語を部品にすると、助詞や活用語尾の短い読みが噛み合って
        // 「雲の上(クモノウエ)」が「雲」+「の上(ノーエ)」に分かれるような誤分割を招く
        let mut parts: HashMap<&str, Vec<(&str, Arc<str>)>> = HashMap::new();
        for ((surface, reading), pron) in &trusted {
            if surface.chars().count() <= MAX_PART_CHARS
                && surface.chars().all(|c| is_kanji(c) || is_katakana(c))
            {
                parts
                    .entry(surface)
                    .or_default()
                    .push((reading, Arc::clone(pron)));
            }
        }

        // 借用する発音を先に決める（trusted が entries を借用しているため参照を分離する）
        let borrowed: Vec<Option<Arc<str>>> = self
            .entries
            .iter()
            .map(|entry| {
                // 発音がカタカナで、かつ読みと違う形なら辞書の値をそのまま信じる。
                // 発音が読みと同じ場合は、長音表記を持つ形が他にあるかもしれないので探す。
                let dict_pron_is_sound =
                    is_katakana_str(&entry.pronunciation) && entry.pronunciation != entry.reading;
                let mut result = if dict_pron_is_sound {
                    None
                } else {
                    trusted
                        .get(&(&*entry.surface, &*entry.reading))
                        .map(|p| Arc::clone(p))
                        .filter(|p| *p != entry.pronunciation)
                };
                // 借りた発音（借りられなければ今の発音）にまだ長音化されていない
                // 「オ段・ウ段 + ウ」が残っているなら、分割して組み立て直す。
                // 「機密情報」は借用で末尾が「ジョウホオ」まで直るが前半が残る
                let current = result.as_deref().unwrap_or(&entry.pronunciation);
                if has_unmarked_long_vowel(current) && is_composable(entry) {
                    if let Some(composed) =
                        compose_pronunciation(&entry.surface, &entry.reading, &parts)
                    {
                        result = Some(Arc::from(composed));
                    }
                }
                result
            })
            .collect();
        drop(parts);
        drop(trusted);

        let empty: Arc<str> = Arc::from("");
        let mut fixed = 0;
        for (entry, borrowed) in self.entries.iter_mut().zip(borrowed) {
            if is_katakana_str(&entry.pronunciation) && borrowed.is_none() {
                continue;
            }
            if let Some(pron) = borrowed {
                entry.pronunciation = pron;
            } else if is_katakana_str(&entry.reading) {
                entry.pronunciation = Arc::clone(&entry.reading);
            } else if entry.pronunciation.is_empty() && entry.reading.is_empty() {
                continue;
            } else {
                // 読みも発音もカタカナでない（ラテン文字の辞書エントリ等）。
                // 空にして解析時の読み補完に委ねる。
                entry.pronunciation = Arc::clone(&empty);
                entry.reading = Arc::clone(&empty);
            }
            fixed += 1;
        }
        fixed
    }

    /// 活用語・機能語と衝突する名詞エントリを取り除く
    ///
    /// NEologd / SudachiDict には表記ゆれを正規化するためのエントリが含まれており、
    /// 活用語の語形をそのまま名詞として登録しているものがある（「高い」→「高位(コウイ)」、
    /// 「学ぶ」→「学部(ガクブ)」等）。これらは Viterbi のコスト次第で本来の形容詞・動詞に
    /// 勝ってしまい、「質の高い」が「シツノコウイ」のように誤読される。
    ///
    /// そこで、同じ表層形に本来の語（動詞・形容詞・副詞・助詞・代名詞など）が存在し、
    /// かつ読みが食い違う名詞エントリを異表記由来とみなして落とす。読みが一致する
    /// エントリ（名詞「位(クライ)」と助詞「くらい」等）は誤読にならないため残す。
    ///
    /// 表層形と原形が同じエントリは表記ゆれではないので原則として残すが、代名詞と
    /// 衝突する 1 文字の人名だけは例外として落とす（「何」を姓の「ガ」「カ」と読ませる
    /// エントリが「何なのか」を「ガナノカ」に変えてしまうため）。
    ///
    /// Returns: 削除されたエントリ数
    pub fn drop_conflicting_ortho_variants(&mut self) -> usize {
        // 表層形ごとに「本来の語」の読みを集める
        let mut canonical: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut pronouns: HashMap<&str, Vec<&str>> = HashMap::new();
        for entry in &self.entries {
            if is_canonical_word(&entry.pos) {
                canonical
                    .entry(&entry.surface)
                    .or_default()
                    .push(&entry.reading);
            }
            if entry.pos.starts_with("名詞,代名詞") {
                pronouns
                    .entry(&entry.surface)
                    .or_default()
                    .push(&entry.reading);
            }
        }
        let doomed: Vec<bool> = self
            .entries
            .iter()
            .map(|entry| {
                if !entry.pos.starts_with("名詞") {
                    return false;
                }
                if entry.surface == entry.base_form {
                    return entry.pos.starts_with("名詞,固有名詞,人名")
                        && entry.surface.chars().count() == 1
                        && pronouns
                            .get(&*entry.surface)
                            .is_some_and(|readings| !readings.contains(&&*entry.reading));
                }
                canonical
                    .get(&*entry.surface)
                    .is_some_and(|readings| !readings.contains(&&*entry.reading))
            })
            .collect();
        drop(canonical);
        drop(pronouns);

        let removed = doomed.iter().filter(|d| **d).count();
        if removed > 0 {
            for (entry, _) in self
                .entries
                .iter()
                .zip(&doomed)
                .filter(|(_, d)| **d)
                .take(ORTHO_VARIANT_SAMPLE)
            {
                eprintln!(
                    "  drop: {} ({}) -> {} [{}]",
                    entry.surface, entry.reading, entry.base_form, entry.pos
                );
            }
            let mut doomed = doomed.into_iter();
            self.entries.retain(|_| !doomed.next().unwrap_or(false));
        }
        removed
    }

    /// 漢数字を数以外に読ませる固有名詞エントリを取り除く
    ///
    /// NEologd / SudachiDict には漢数字だけで綴られた人名・地名が登録されており
    /// （「十五(トウゴ)」「二十八(ツチヤ)」「五百(イホ)」等）、Viterbi のコスト次第で
    /// 数詞に勝ってしまう。読み上げでは数として読むのが安全なため、漢数字のみからなる
    /// 2 文字以上の表層形について固有名詞エントリを落とす。
    ///
    /// 対象を固有名詞に限るのは、「万一(マンイチ)」「二三(ニサン)」「八百万(ヤオヨロズ)」
    /// のように漢数字で綴る一般語・副詞が存在するため。1 文字の漢数字（「一」を「はじめ」と
    /// 読む人名等）も数詞との共存が必要な場面があるため対象にしない。
    ///
    /// Returns: 削除されたエントリ数
    pub fn drop_numeral_misreadings(&mut self) -> usize {
        let doomed: Vec<bool> = self
            .entries
            .iter()
            .map(|entry| {
                entry.surface.chars().count() >= 2
                    && entry.surface.chars().all(is_kansuji)
                    && entry.pos.starts_with("名詞,固有名詞")
            })
            .collect();

        let removed = doomed.iter().filter(|d| **d).count();
        if removed > 0 {
            for (entry, _) in self
                .entries
                .iter()
                .zip(&doomed)
                .filter(|(_, d)| **d)
                .take(ORTHO_VARIANT_SAMPLE)
            {
                eprintln!(
                    "  drop: {} ({}) [{}]",
                    entry.surface, entry.reading, entry.pos
                );
            }
            let mut doomed = doomed.into_iter();
            self.entries.retain(|_| !doomed.next().unwrap_or(false));
        }
        removed
    }

    /// CSV で指定されたエントリを辞書から削除する
    ///
    /// 汎用の異表記フィルタ（[`Self::drop_conflicting_ortho_variants`]）では拾えない
    /// 個別の誤読エントリを落とすために使う。CSV は `表層形,読み` の 2 列で、
    /// 3 列目以降があっても無視する。`#` で始まる行と空行はコメントとして読み飛ばす。
    ///
    /// Returns: 削除されたエントリ数
    pub fn drop_entries_from_csv<P: AsRef<Path>>(&mut self, path: P) -> io::Result<usize> {
        let path = path.as_ref();
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);

        let mut targets: HashSet<(String, String)> = HashSet::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.splitn(3, ',');
            let (Some(surface), Some(reading)) = (fields.next(), fields.next()) else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: expected `surface,reading`: {}", path.display(), line),
                ));
            };
            targets.insert((surface.trim().to_string(), reading.trim().to_string()));
        }

        let before = self.entries.len();
        self.entries
            .retain(|e| !targets.contains(&(e.surface.to_string(), e.reading.to_string())));
        Ok(before - self.entries.len())
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

        for (idx, result) in rdr.records().enumerate() {
            let line_no = idx + 1;
            let record = result.map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}:{}: CSV parse error: {}", path.display(), line_no, e),
                )
            })?;
            if record.len() < 5 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{}:{}: too few columns ({})",
                        path.display(),
                        line_no,
                        record.len()
                    ),
                ));
            }

            let surface: Arc<str> = Arc::from(&record[0]);
            let left_id: u16 = parse_field(path, line_no, "left_id", &record[1])?;
            let right_id: u16 = parse_field(path, line_no, "right_id", &record[2])?;
            let cost: i16 = parse_field(path, line_no, "cost", &record[3])?;

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
        let mut paths: Vec<_> = glob::glob(&pattern)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .filter_map(|r| r.ok())
            .collect();
        // 同じ表層形のエントリはコストが同じなら先勝ちになるため、
        // 辞書が読み込み順に依存しないようパスを固定順にする
        paths.sort();

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
        let path = path.as_ref();
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

        for (line_no, line) in lines.enumerate().map(|(i, l)| (i + 2, l)) {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let right_id: usize = parse_field(path, line_no, "right_id", parts[0])?;
            let left_id: usize = parse_field(path, line_no, "left_id", parts[1])?;
            let cost: i16 = parse_field(path, line_no, "cost", parts[2])?;

            let idx = right_id * left_size as usize + left_id;
            if idx >= costs.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "matrix id out of range: right_id={}, left_id={} (max {}x{})",
                        right_id, left_id, right_size, left_size
                    ),
                ));
            }
            costs[idx] = cost;
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
        let path = path.as_ref();
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(content.as_bytes());

        for (idx, result) in rdr.records().enumerate() {
            let line_no = idx + 1;
            let record = result.map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unk.def:{}: CSV parse error: {}", line_no, e),
                )
            })?;
            if record.len() < 5 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unk.def:{}: too few columns ({})", line_no, record.len()),
                ));
            }

            let char_class = record[0].to_string();
            let left_id: u16 = parse_field(path, line_no, "left_id", &record[1])?;
            let right_id: u16 = parse_field(path, line_no, "right_id", &record[2])?;
            let cost: i16 = parse_field(path, line_no, "cost", &record[3])?;
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
        let path = path.as_ref();
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
                    let length: u32 = parts[3].parse().map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("char.def: invalid length `{}`: {}", parts[3], e),
                        )
                    })?;

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

    pub(crate) fn parse_range(s: &str) -> Option<(u32, u32)> {
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
    pub(crate) fn decode_to_utf8(bytes: &[u8]) -> String {
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
        self.build_with_progress(|_, _| {})
    }

    /// プログレスコールバック付きで辞書をビルド
    ///
    /// `progress(processed, total)` が Trie 構築中に定期的に呼び出される。
    pub fn build_with_progress(self, progress: impl FnMut(usize, usize)) -> Dictionary {
        // エントリからTrieを構築
        let mut trie_entries: Vec<(&[u8], u32)> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.surface.as_bytes(), i as u32))
            .collect();

        // バイト列でソート（Trie構築に必要ではないが効率向上）
        trie_entries.sort_by(|a, b| a.0.cmp(b.0));

        let trie = DoubleArrayTrie::build_with_progress(&trie_entries, progress);

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

    // --- 追加テスト ---

    #[test]
    fn test_connection_matrix_cost() {
        let matrix = ConnectionMatrix {
            left_size: 3,
            right_size: 3,
            // 3x3 matrix: costs[right_id * left_size + left_id]
            costs: vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
        };
        assert_eq!(matrix.cost(0, 0), 0);
        assert_eq!(matrix.cost(0, 1), 1);
        assert_eq!(matrix.cost(0, 2), 2);
        assert_eq!(matrix.cost(1, 0), 3);
        assert_eq!(matrix.cost(2, 2), 8);
    }

    #[test]
    fn test_connection_matrix_row() {
        let matrix = ConnectionMatrix {
            left_size: 3,
            right_size: 2,
            costs: vec![10, 20, 30, 40, 50, 60],
        };
        assert_eq!(matrix.row(0), &[10, 20, 30]);
        assert_eq!(matrix.row(1), &[40, 50, 60]);
    }

    #[test]
    fn test_connection_matrix_out_of_bounds() {
        let matrix = ConnectionMatrix {
            left_size: 2,
            right_size: 2,
            costs: vec![0, 1, 2, 3],
        };
        // Out of bounds should return 0 or empty
        assert_eq!(matrix.cost(10, 0), 0);
        assert!(matrix.row(10).is_empty());
    }

    #[test]
    fn test_dict_builder_default() {
        let builder = DictBuilder::default();
        assert_eq!(builder.entry_count(), 0);
    }

    #[test]
    fn test_dict_builder_entry_count() {
        let mut builder = DictBuilder::new();
        assert_eq!(builder.entry_count(), 0);

        builder.add_entry(DictEntry {
            surface: "test".into(),
            left_id: 0,
            right_id: 0,
            cost: 0,
            pos: "名詞,一般,*,*".into(),
            base_form: "test".into(),
            reading: "".into(),
            pronunciation: "".into(),
        });
        assert_eq!(builder.entry_count(), 1);
    }

    #[test]
    fn test_dict_lookup_multiple_entries_same_surface() {
        let mut builder = DictBuilder::new();
        // Two entries with same surface but different POS
        builder.add_entry(DictEntry {
            surface: "金".into(),
            left_id: 1,
            right_id: 1,
            cost: 3000,
            pos: "名詞,一般,*,*".into(),
            base_form: "金".into(),
            reading: "キン".into(),
            pronunciation: "キン".into(),
        });
        builder.add_entry(DictEntry {
            surface: "金".into(),
            left_id: 2,
            right_id: 2,
            cost: 3500,
            pos: "名詞,固有名詞,人名,姓".into(),
            base_form: "金".into(),
            reading: "カネ".into(),
            pronunciation: "カネ".into(),
        });

        let dict = builder.build();
        let results = dict.lookup("金曜日".as_bytes());
        assert!(!results.is_empty());
        // Should find both entries for "金"
        let entries_for_gold = &results[0].1;
        assert_eq!(entries_for_gold.len(), 2);
    }

    #[test]
    fn test_dict_lookup_no_match() {
        let mut builder = DictBuilder::new();
        builder.add_entry(DictEntry {
            surface: "東京".into(),
            left_id: 1,
            right_id: 1,
            cost: 3000,
            pos: "名詞,固有名詞,地域,一般".into(),
            base_form: "東京".into(),
            reading: "".into(),
            pronunciation: "".into(),
        });
        let dict = builder.build();
        let results = dict.lookup("大阪".as_bytes());
        assert!(results.is_empty());
    }

    #[test]
    fn test_dict_builder_build_with_default_matrix() {
        let mut builder = DictBuilder::new();
        builder.add_entry(DictEntry {
            surface: "テスト".into(),
            left_id: 0,
            right_id: 0,
            cost: 1000,
            pos: "名詞,一般,*,*".into(),
            base_form: "テスト".into(),
            reading: "テスト".into(),
            pronunciation: "テスト".into(),
        });

        // Build without setting matrix - should use default
        let dict = builder.build();
        assert_eq!(dict.matrix.left_size, 1);
        assert_eq!(dict.matrix.right_size, 1);
    }

    #[test]
    fn test_dict_builder_set_matrix() {
        let mut builder = DictBuilder::new();
        builder.set_matrix(ConnectionMatrix {
            left_size: 5,
            right_size: 5,
            costs: vec![0; 25],
        });
        let dict = builder.build();
        assert_eq!(dict.matrix.left_size, 5);
        assert_eq!(dict.matrix.right_size, 5);
    }

    #[test]
    fn test_parse_range_single_value() {
        let result = DictBuilder::parse_range("0x3040");
        assert_eq!(result, Some((0x3040, 0x3040)));
    }

    #[test]
    fn test_parse_range_range() {
        let result = DictBuilder::parse_range("0x3040..0x309F");
        assert_eq!(result, Some((0x3040, 0x309F)));
    }

    #[test]
    fn test_parse_range_invalid() {
        let result = DictBuilder::parse_range("invalid");
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_to_utf8_valid_utf8() {
        let utf8_bytes = "こんにちは".as_bytes();
        let result = DictBuilder::decode_to_utf8(utf8_bytes);
        assert_eq!(result, "こんにちは");
    }

    #[test]
    fn test_decode_to_utf8_euc_jp() {
        // "日本語" in EUC-JP
        let euc_jp_bytes: &[u8] = &[0xC6, 0xFC, 0xCB, 0xDC, 0xB8, 0xEC];
        let result = DictBuilder::decode_to_utf8(euc_jp_bytes);
        assert_eq!(result, "日本語");
    }

    #[test]
    fn test_dict_entry_clone() {
        let entry = DictEntry {
            surface: "テスト".into(),
            left_id: 1,
            right_id: 2,
            cost: 3000,
            pos: "名詞,一般,*,*".into(),
            base_form: "テスト".into(),
            reading: "テスト".into(),
            pronunciation: "テスト".into(),
        };
        let cloned = entry.clone();
        assert_eq!(&*cloned.surface, "テスト");
        assert_eq!(cloned.left_id, 1);
    }
}
