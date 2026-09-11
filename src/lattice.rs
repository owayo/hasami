//! ラティス構築 + Viterbi デコーディング（最適化版）

use crate::char_class::{ALL_CHAR_TYPES, CharType, type_index};
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

/// アルファベット1文字のカタカナ読みを返す
fn alpha_to_kana(c: char) -> Option<&'static str> {
    match c.to_ascii_uppercase() {
        'A' => Some("エー"),
        'B' => Some("ビー"),
        'C' => Some("シー"),
        'D' => Some("ディー"),
        'E' => Some("イー"),
        'F' => Some("エフ"),
        'G' => Some("ジー"),
        'H' => Some("エイチ"),
        'I' => Some("アイ"),
        'J' => Some("ジェー"),
        'K' => Some("ケー"),
        'L' => Some("エル"),
        'M' => Some("エム"),
        'N' => Some("エヌ"),
        'O' => Some("オー"),
        'P' => Some("ピー"),
        'Q' => Some("キュー"),
        'R' => Some("アール"),
        'S' => Some("エス"),
        'T' => Some("ティー"),
        'U' => Some("ユー"),
        'V' => Some("ブイ"),
        'W' => Some("ダブリュー"),
        'X' => Some("エックス"),
        'Y' => Some("ワイ"),
        'Z' => Some("ゼット"),
        _ => None,
    }
}

/// 数字の後に来る単一アルファベットの単位読みを返す
///
/// 単位記号は大文字・小文字で別の量を表す（T=テスラ / t=トン, M=メガ / m=メートル）ため、
/// 大文字小文字を区別して引く。
fn unit_reading_char(c: char) -> Option<&'static str> {
    match c {
        'W' => Some("ワット"),
        'A' => Some("アンペア"),
        'V' => Some("ボルト"),
        'J' => Some("ジュール"),
        'N' => Some("ニュートン"),
        'T' => Some("テスラ"),
        'F' => Some("ファラド"),
        'H' => Some("ヘンリー"),
        'K' => Some("ケルビン"),
        'm' => Some("メートル"),
        'g' => Some("グラム"),
        't' => Some("トン"),
        'L' | 'l' => Some("リットル"),
        _ => None,
    }
}

/// 数字の後に来る複数文字の単位読みを返す
fn unit_reading_multi(s: &str) -> Option<&'static str> {
    match s {
        "Hz" => Some("ヘルツ"),
        "kHz" => Some("キロヘルツ"),
        "MHz" => Some("メガヘルツ"),
        "GHz" => Some("ギガヘルツ"),
        "km" => Some("キロメートル"),
        "cm" => Some("センチメートル"),
        "mm" => Some("ミリメートル"),
        "nm" => Some("ナノメートル"),
        "kg" => Some("キログラム"),
        "mg" => Some("ミリグラム"),
        "kW" => Some("キロワット"),
        "MW" => Some("メガワット"),
        "mA" => Some("ミリアンペア"),
        "kV" => Some("キロボルト"),
        "dB" => Some("デシベル"),
        "Pa" => Some("パスカル"),
        "hPa" => Some("ヘクトパスカル"),
        "kPa" => Some("キロパスカル"),
        "MPa" => Some("メガパスカル"),
        "KB" => Some("キロバイト"),
        "MB" => Some("メガバイト"),
        "GB" => Some("ギガバイト"),
        "TB" => Some("テラバイト"),
        "Wh" => Some("ワットアワー"),
        "kWh" => Some("キロワットアワー"),
        "Ah" => Some("アンペアアワー"),
        "mAh" => Some("ミリアンペアアワー"),
        "cc" => Some("シーシー"),
        "hp" => Some("馬力"),
        "rpm" => Some("アールピーエム"),
        "fps" => Some("エフピーエス"),
        "bps" => Some("ビーピーエス"),
        "Mbps" => Some("メガビーピーエス"),
        "Gbps" => Some("ギガビーピーエス"),
        _ => None,
    }
}

/// 数字の後に来るアルファベットの単位読みを返す
fn unit_reading(surface: &str) -> Option<&'static str> {
    let mut chars = surface.chars();
    if let Some(c) = chars.next() {
        if chars.next().is_none() {
            return unit_reading_char(c);
        }
    }
    unit_reading_multi(surface)
}

/// トークンが数字的かどうかを判定する。
///
/// `.` と `,` を許すのは「1,000」「2.5」を 1 トークンで受けるためだが、区切り記号
/// だけのトークンまで数字と見なしてはいけない。Style-Bert-VITS2 は読点を `,`、句点を
/// `.` に正規化して渡すので、それを数字と判定すると直後の英字に単位読みが付き、
/// 「,Aさん」が「アンペアさん」、「.Lが」が「リットルが」になる。
fn is_number_like(token: &Token) -> bool {
    if token.pos.starts_with("名詞,数") {
        return true;
    }
    let is_digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    token.surface.chars().any(is_digit)
        && token
            .surface
            .chars()
            .all(|c| is_digit(c) || c == '.' || c == ',')
}

/// 英字トークンについて、辞書の読みをそのまま使ってよいか判定する。
///
/// 辞書には NASA→ナサ, GIF→ジフ のように綴り読みでは表せない語があるため、原則として
/// 辞書の読みを尊重する。ただし次の場合は辞書を信用せず、呼び出し側の文脈ルール
/// （数字直後の単位読み / スペルアウト）に委ねる。
///
/// - 表層形が 1〜2 文字: 単独の英字に付いた単位読み・略称読み（A→アンペア, G→ギガ,
///   cs→クレディスイス 等）は、日本語文中ではほぼ確実に誤読になる。日本語文に現れる
///   1〜2 文字の英字は略語（AI, PC, VP 等）が大半で、綴り読みの方が当たる。
/// - 読み・発音がカタカナでない: 表層形がそのまま入っている辞書エントリ
///   （Siemens→siemens 等）で、そのままでは音素化できず読みが消える。
fn should_trust_dict_reading(token: &Token) -> bool {
    if !token.is_known {
        return false;
    }
    if token.surface.chars().count() <= 2 {
        return false;
    }
    if !token.reading.is_empty() && !is_katakana_str(&token.reading) {
        return false;
    }
    if !token.pronunciation.is_empty() && !is_katakana_str(&token.pronunciation) {
        return false;
    }
    !token.reading.is_empty() || !token.pronunciation.is_empty()
}

/// 文字列が全てカタカナ（長音記号を含む U+30A0〜U+30FF）かどうか判定する
fn is_katakana_str(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| ('\u{30A0}'..='\u{30FF}').contains(&c))
}

/// Viterbi が選んだ品詞を根拠に補正できる辞書の読み。
/// (表層形, 品詞の接頭, 誤った読み, 正しい読み)
///
/// 「他」は名詞なら「ホカ」（他ならぬ、他で、他は、他より）、接頭詞なら「タ」
/// （他部門、他部署、他業種）。IPAdic の名詞エントリは「タ」なので名詞のときだけ直す。
/// 複合語「他人」「他社」「他者」は 1 トークンなので影響しない。
/// 助詞との組み合わせを列挙する方式では「他ならぬ」や文末の「他」を取りこぼす
const POS_READING_OVERRIDES: &[(&str, &str, &str, &str)] = &[("他", "名詞,", "タ", "ホカ")];

/// 品詞を根拠に辞書の読みを補正する。補正したら true を返す。
fn apply_pos_reading_override(token: &mut Token) -> bool {
    for &(surface, pos_prefix, incorrect, corrected) in POS_READING_OVERRIDES {
        if &*token.surface == surface
            && token.pos.starts_with(pos_prefix)
            && (&*token.reading == incorrect || &*token.pronunciation == incorrect)
        {
            let corrected: Arc<str> = Arc::from(corrected);
            token.reading = Arc::clone(&corrected);
            token.pronunciation = corrected;
            return true;
        }
    }
    false
}

/// 「数」の読みを後続のトークンで切り替える。補正したら true を返す。
///
/// 「数」は品詞が同じでも読みが 2 通りある。「数十人」「数百」のように数詞が続けば
/// 接頭辞の「スウ」、「目玉の数」「数が多い」「数を数える」のように続かなければ
/// 独立した名詞の「カズ」。IPAdic の 名詞,一般 エントリは「スウ」なので、
/// 後続を見て「カズ」に倒す。
///
/// 「数分」「数万」のように助数詞・数詞と結び付く「数」は 名詞,数 に割り当てられる
/// ので、ここには来ない。それでも後続の判定に助数詞を含めるのは、「数十人」の
/// 「数」が 名詞,一般 になるように、品詞の割り当てが揺れるため。
fn apply_kazu_reading_override(token: &mut Token, next_is_number: bool) -> bool {
    if next_is_number
        || &*token.surface != "数"
        || !token.pos.starts_with("名詞,一般,")
        || &*token.reading != "スウ"
    {
        return false;
    }
    let corrected: Arc<str> = Arc::from("カズ");
    token.reading = Arc::clone(&corrected);
    token.pronunciation = corrected;
    true
}

/// 数詞または助数詞か。「数」の読みの切り替えに使う。
fn is_number_pos(pos: &str) -> bool {
    pos.starts_with("名詞,数") || pos.starts_with("名詞,接尾,助数詞")
}

/// 辞書の読みを文脈で補正し、読みが欠けているトークンを補完する。
/// - 品詞で読みが決まる語: `POS_READING_OVERRIDES`（「他」の名詞/接頭詞）
/// - 後続で読みが決まる語: 「数」（数詞が続けば「スウ」、続かなければ「カズ」）
/// - アルファベット: 数字トークンの直後 → 単位読み（該当する場合）、なければアルファベット読み
/// - 繰り返し記号「々」: 直前のトークンの読み（そこだけ無音になるのを防ぐ）
/// - 仮名のみの語: 表層形をカタカナ化した読み（未知語で読みが空になるケースの救済）
fn apply_contextual_readings(tokens: &mut [Token]) {
    for i in 0..tokens.len() {
        if tokens[i].surface.is_empty() {
            continue;
        }
        if apply_pos_reading_override(&mut tokens[i]) {
            continue;
        }
        let next_is_number = tokens.get(i + 1).is_some_and(|t| is_number_pos(&t.pos));
        if apply_kazu_reading_override(&mut tokens[i], next_is_number) {
            continue;
        }
        if !tokens[i].surface.chars().all(|c| c.is_ascii_alphabetic()) {
            // 「々」は記号として辞書に入っていて読みを持たない。「日々」「人々」のように
            // 辞書にある語なら 1 トークンになるが、「前々項」のような語は
            // 「前」「々」「項」に割れ、「々」がそこだけ無音になる。
            // 繰り返し記号なので直前の読みを繰り返す（連濁までは再現できない）
            if &*tokens[i].surface == "々" && tokens[i].pronunciation.is_empty() && i > 0 {
                let reading = Arc::clone(&tokens[i - 1].reading);
                let pronunciation = Arc::clone(&tokens[i - 1].pronunciation);
                if !pronunciation.is_empty() {
                    tokens[i].reading = reading;
                    tokens[i].pronunciation = pronunciation;
                    continue;
                }
            }
            fill_kana_reading(&mut tokens[i]);
            continue;
        }

        if should_trust_dict_reading(&tokens[i]) {
            continue;
        }

        let preceded_by_number = i > 0 && is_number_like(&tokens[i - 1]);

        if preceded_by_number {
            if let Some(reading) = unit_reading(&tokens[i].surface) {
                let arc: Arc<str> = Arc::from(reading);
                tokens[i].reading = Arc::clone(&arc);
                tokens[i].pronunciation = arc;
                continue;
            }
        }

        // アルファベット読み（スペルアウト）
        let kana = if tokens[i].surface.len() == 1 {
            let c = tokens[i].surface.chars().next().unwrap();
            alpha_to_kana(c).map(Arc::from)
        } else {
            alphabet_reading(&tokens[i].surface)
        };
        if let Some(kana) = kana {
            tokens[i].reading = Arc::clone(&kana);
            tokens[i].pronunciation = kana;
        }
    }
}

/// 仮名のみからなる表層形をカタカナの読みに変換する。
/// ひらがなはカタカナへ写像し、カタカナと長音記号はそのまま使う。
/// 仮名以外の文字が含まれる場合は None を返す。
fn kana_reading(surface: &str) -> Option<Arc<str>> {
    if surface.is_empty() {
        return None;
    }
    let mut reading = String::with_capacity(surface.len());
    for c in surface.chars() {
        let kana = match c {
            // ひらがな (U+3041〜U+3096) → カタカナ (U+30A1〜U+30F6)
            'ぁ'..='ゖ' => char::from_u32(c as u32 + 0x60)?,
            // カタカナ (U+30A1〜U+30F6) と長音記号はそのまま
            'ァ'..='ヶ' | 'ー' => c,
            _ => return None,
        };
        reading.push(kana);
    }
    Some(Arc::from(reading.as_str()))
}

/// 読み・発音が空のトークンに、表層形から作ったカタカナ読みを補う。
///
/// 未知語（辞書に無いカタカナ語など）は読みが空のまま返されるため、そのままでは
/// 後段の音声合成で発音が欠落する。表層形が仮名のみなら読みは自明なので補完する。
fn fill_kana_reading(token: &mut Token) {
    if !token.reading.is_empty() && !token.pronunciation.is_empty() {
        return;
    }
    let Some(kana) = kana_reading(&token.surface) else {
        return;
    };
    if token.reading.is_empty() {
        token.reading = Arc::clone(&kana);
    }
    if token.pronunciation.is_empty() {
        token.pronunciation = kana;
    }
}

fn alphabet_reading(surface: &str) -> Option<Arc<str>> {
    if surface.is_empty() || !surface.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut reading = String::new();
    for c in surface.chars() {
        reading.push_str(alpha_to_kana(c)?);
    }
    Some(Arc::from(reading.as_str()))
}

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
    let mut table: Vec<UnkTemplates> = (0..NUM_CHAR_TYPES)
        .map(|_| UnkTemplates {
            templates: Vec::new(),
            invoke: false,
        })
        .collect();

    for &ct in &ALL_CHAR_TYPES {
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
        let mut tokens: Vec<Token> = path
            .iter()
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
            .collect();
        apply_contextual_readings(&mut tokens);
        tokens
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
        let mut tokens: Vec<Token> = path
            .iter()
            .map(|&idx| {
                let node = &self.nodes[idx as usize];
                if node.is_known() {
                    let arcs = dict.entry_arcs(node.entry_id);
                    Token {
                        surface: arcs.surface,
                        start: node.start as usize,
                        end: node.end as usize,
                        pos: arcs.pos,
                        base_form: arcs.base_form,
                        reading: arcs.reading,
                        pronunciation: arcs.pronunciation,
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
            .collect();
        apply_contextual_readings(&mut tokens);
        tokens
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

    // --- 追加テスト ---

    #[test]
    fn test_empty_input() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("", &dict);
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_single_known_word() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("東京", &dict);
        assert_eq!(tokens.len(), 1);
        assert_eq!(&*tokens[0].surface, "東京");
        assert!(tokens[0].is_known);
    }

    #[test]
    fn test_unknown_word_only() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        // "大阪" is not in the dictionary
        let tokens = ws.tokenize("大阪", &dict);
        assert!(!tokens.is_empty());
        // All tokens should be unknown
        for t in &tokens {
            assert!(!t.is_known);
        }
    }

    #[test]
    fn test_token_surface_reconstruction() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let inputs = ["東京都", "東京都に住んでいる", "に", "いる"];
        for input in inputs {
            let tokens = ws.tokenize(input, &dict);
            let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
            assert_eq!(
                reconstructed, input,
                "Surface reconstruction failed for '{}'",
                input
            );
        }
    }

    #[test]
    fn test_viterbi_prefers_lower_cost() {
        // "東京都" (cost 2000) should be preferred over "東京" (3000) + "都" (5000)
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("東京都", &dict);

        // With zero connection costs, "東京都" (2000) < "東京" (3000) + "都" (5000) = 8000
        let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(surfaces, vec!["東京都"]);
    }

    #[test]
    fn test_word_cost_preserved() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("東京都", &dict);
        assert_eq!(tokens[0].word_cost, 2000);
    }

    #[test]
    fn test_pos_preserved() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("に", &dict);
        assert_eq!(tokens.len(), 1);
        assert_eq!(&*tokens[0].pos, "助詞,格助詞,一般,*");
    }

    #[test]
    fn test_multiple_tokenize_calls() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();

        // Multiple calls should all work correctly
        for _ in 0..10 {
            let tokens = ws.tokenize("東京都", &dict);
            assert!(!tokens.is_empty());
            let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
            assert_eq!(reconstructed, "東京都");
        }
    }

    #[test]
    fn test_unknown_word_has_pos() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("XYZ", &dict);
        // Unknown words should still have POS assigned
        for t in &tokens {
            assert!(!t.pos.is_empty(), "Unknown word should have POS");
        }
    }

    #[test]
    fn test_lattice_workspace_default() {
        let ws = LatticeWorkspace::default();
        assert_eq!(ws.nodes.capacity(), 4096);
    }

    #[test]
    fn test_long_input() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        // Repeat known text many times
        let input = "東京都に住んでいる".repeat(50);
        let tokens = ws.tokenize(&input, &dict);
        let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(reconstructed, input);
    }

    #[test]
    fn test_single_char_unknown() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("X", &dict);
        assert_eq!(tokens.len(), 1);
        assert_eq!(&*tokens[0].surface, "X");
        assert!(!tokens[0].is_known);
    }

    #[test]
    fn test_mixed_known_unknown_sequence() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        // "東京" is known, "ABC" is unknown, "に" is known
        let tokens = ws.tokenize("東京ABCに", &dict);
        let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(reconstructed, "東京ABCに");
    }

    fn reading_token(surface: &str, pos: &str, reading: &str) -> Token {
        Token {
            surface: surface.into(),
            start: 0,
            end: surface.len(),
            pos: pos.into(),
            base_form: surface.into(),
            reading: reading.into(),
            pronunciation: reading.into(),
            word_cost: 0,
            is_known: true,
        }
    }

    #[test]
    fn test_hoka_reading_depends_on_selected_pos() {
        let mut tokens = vec![
            // 「他ならぬ」「他で」「他は」の名詞。辞書の読みは「タ」だが「ホカ」が正しい
            reading_token("他", "名詞,一般,*,*", "タ"),
            // 「他部門」「他部署」の接頭詞。こちらは「タ」が正しい
            reading_token("他", "接頭詞,名詞接続,*,*", "タ"),
            // 既に正しい読みを持つ名詞は触らない
            reading_token("他", "名詞,非自立,副詞可能,*", "ホカ"),
        ];

        apply_contextual_readings(&mut tokens);

        assert_eq!(&*tokens[0].reading, "ホカ");
        assert_eq!(&*tokens[0].pronunciation, "ホカ");
        assert_eq!(&*tokens[1].reading, "タ");
        assert_eq!(&*tokens[1].pronunciation, "タ");
        assert_eq!(&*tokens[2].reading, "ホカ");
    }

    #[test]
    fn test_kazu_reading_depends_on_following_token() {
        // 「目玉の数が」「数を数える」のように数詞が続かなければ独立した名詞の「カズ」
        for next in [
            reading_token("が", "助詞,格助詞,一般,*", "ガ"),
            reading_token("を", "助詞,格助詞,一般,*", "ヲ"),
        ] {
            let surface = next.surface.to_string();
            let mut tokens = vec![reading_token("数", "名詞,一般,*,*", "スウ"), next];
            apply_contextual_readings(&mut tokens);
            assert_eq!(&*tokens[0].reading, "カズ", "数{surface}");
            assert_eq!(&*tokens[0].pronunciation, "カズ", "数{surface}");
        }

        // 「数十人」「数百」のように数詞・助数詞が続けば接頭辞の「スウ」
        for next in [
            reading_token("十", "名詞,数,*,*", "ジュウ"),
            reading_token("分", "名詞,接尾,助数詞,*", "フン"),
        ] {
            let surface = next.surface.to_string();
            let mut tokens = vec![reading_token("数", "名詞,一般,*,*", "スウ"), next];
            apply_contextual_readings(&mut tokens);
            assert_eq!(&*tokens[0].reading, "スウ", "数{surface}");
        }

        // 文末の「数」は後続が無いので「カズ」
        let mut tokens = vec![reading_token("数", "名詞,一般,*,*", "スウ")];
        apply_contextual_readings(&mut tokens);
        assert_eq!(&*tokens[0].reading, "カズ");

        // 「数分」のように 名詞,数 が選ばれた「数」は触らない
        let mut tokens = vec![
            reading_token("数", "名詞,数,*,*", "スウ"),
            reading_token("分", "名詞,接尾,助数詞,*", "フン"),
        ];
        apply_contextual_readings(&mut tokens);
        assert_eq!(&*tokens[0].reading, "スウ");
    }
}
