//! ラティス構築 + Viterbi デコーディング（最適化版）

use crate::char_class::{CharType, type_index};
use crate::hsd::{DictError, Dictionary};
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
    /// 品詞情報（品詞と細分類 3 つをカンマでつないだもの。無い細分類は `*`）
    pub pos: Arc<str>,
    /// 活用型（MeCab 形式 CSV の 9 列目。「五段・ラ行」など）。活用しない語・未知語は空文字列
    pub conj_type: Arc<str>,
    /// 活用形（MeCab 形式 CSV の 10 列目。「連用形」など）。活用しない語・未知語は空文字列
    pub conj_form: Arc<str>,
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

/// 小数点の直後では促音化しない語。(表層形, 正しい読み)
///
/// 節番号「4.1節」は「四点一節」に正規化されるが、「一節(イッセツ)」が 1 語として
/// 辞書にあるため「ヨンテンイッセツ」と読まれる。小数点以下の「一」と助数詞の「節」は
/// 別の語なので促音化しない。
/// 「4.1章」「4.1項」は「一」+「章」に分かれるのでこの問題は起きず、
/// 「4.1部」の「一部(イチブ)」は促音化しないのでそのままでよい。
const DECIMAL_COUNTER_OVERRIDES: &[(&str, &str)] = &[("一節", "イチセツ")];

/// 小数点の直後かどうか。「四」「点」「一節」のような並びを見分ける。
fn is_after_decimal_point(tokens: &[Token], i: usize) -> bool {
    i >= 2 && &*tokens[i - 1].surface == "点" && is_number_pos(&tokens[i - 2].pos)
}

/// 小数点の直後の促音化を戻す。補正したら true を返す。
fn apply_decimal_counter_override(token: &mut Token, after_decimal_point: bool) -> bool {
    if !after_decimal_point {
        return false;
    }
    for &(surface, corrected) in DECIMAL_COUNTER_OVERRIDES {
        if &*token.surface == surface {
            let corrected: Arc<str> = Arc::from(corrected);
            token.reading = Arc::clone(&corrected);
            token.pronunciation = corrected;
            return true;
        }
    }
    false
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
        let after_decimal_point = is_after_decimal_point(tokens, i);
        if apply_decimal_counter_override(&mut tokens[i], after_decimal_point) {
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
    /// 辞書エントリの番号。BOUNDARY_ID = BOS/EOS、UNK_FLAG付き = 未知語
    entry_id: u32,
    left_id: u16,
    right_id: u16,
    word_cost: i16,
    char_type: CharType,
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

/// 再利用可能なラティスワークスペース
pub struct LatticeWorkspace {
    nodes: Vec<LatticeNode>,
    /// Viterbi用: total_cost[node_idx]
    costs: Vec<i64>,
    /// Viterbi用: prev[node_idx]
    prevs: Vec<u32>,
    /// end_nodes[byte_pos] = そのバイト位置で終了するノードインデックスのリスト
    end_nodes: Vec<Vec<u32>>,
    /// 素性レコードの読み・発音を復号するときの再利用バッファ
    decode_scratch: String,
    /// 既知語のトークンの文字列のキャッシュ
    token_cache: TokenCache,
}

/// 既知語のトークンの文字列（表層形・品詞・活用・原形・読み・発音）のキャッシュ
///
/// 辞書エントリの番号で引く直接マップ方式。同じ語を何度も解析するときに、素性レコードの復号と
/// 文字列の確保を省く（v3 は辞書の全文字列をキャッシュしていたが、v4 はよく出る語だけを
/// 解析器ごとに持つ）。解析器（`Analyzer::clone`）ごとに独立しているのでロックは要らない。
/// 文脈による読みの補正はキャッシュから作った `Token` に対して行い、キャッシュには戻さない。
struct TokenCache {
    /// キャッシュを作った辞書の番号。違う辞書で解析したら捨てる
    dict_id: u64,
    slots: Vec<Option<(u32, Token)>>,
}

/// キャッシュのスロット数
const TOKEN_CACHE_SLOTS: usize = 1024;
/// これより長い表層形の語はキャッシュしない（まれな長い固有名詞でメモリを使わない）
const TOKEN_CACHE_MAX_SURFACE: usize = 64;

impl TokenCache {
    fn new() -> Self {
        TokenCache {
            dict_id: u64::MAX,
            slots: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn known_token(
        &mut self,
        dict: &Dictionary,
        entry_id: u32,
        surface: &str,
        start: usize,
        end: usize,
        word_cost: i16,
        scratch: &mut String,
    ) -> Result<Token, DictError> {
        if self.dict_id != dict.id() {
            self.slots.clear();
            self.slots.resize(TOKEN_CACHE_SLOTS, None);
            self.dict_id = dict.id();
        }
        let slot = &mut self.slots[entry_id as usize % TOKEN_CACHE_SLOTS];
        if let Some((cached_id, token)) = slot {
            if *cached_id == entry_id {
                let mut token = token.clone();
                token.start = start;
                token.end = end;
                token.word_cost = word_cost;
                return Ok(token);
            }
        }
        let token = dict.known_token(entry_id, surface, start, end, word_cost, scratch)?;
        if surface.len() <= TOKEN_CACHE_MAX_SURFACE {
            *slot = Some((entry_id, token.clone()));
        }
        Ok(token)
    }
}

impl LatticeWorkspace {
    pub fn new() -> Self {
        LatticeWorkspace {
            nodes: Vec::with_capacity(4096),
            costs: Vec::with_capacity(4096),
            prevs: Vec::with_capacity(4096),
            end_nodes: Vec::with_capacity(1024),
            decode_scratch: String::with_capacity(64),
            token_cache: TokenCache::new(),
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

    /// ラティスを構築して Viterbi で最良パスを求める
    ///
    /// 辞書の不正な参照（範囲外の群・文脈 ID・素性レコード、壊れた trie）を見つけたら
    /// [`DictError::Corrupt`] を返す。検証済みの辞書（`hasami info --verify`）では起きない。
    pub fn tokenize(&mut self, input: &str, dict: &Dictionary) -> Result<Vec<Token>, DictError> {
        if input.is_empty() {
            return Ok(Vec::new());
        }
        let view = dict.view();
        let classifier = dict.classifier();
        let byte_len = input.len();
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
            },
            0,
        );

        // 各文字位置から辞書引き + 未知語生成
        for (byte_pos, c) in input.char_indices() {
            let mut has_known = false;
            let mut corrupt: Option<DictError> = None;

            // 群 = trie の値が指すエントリから、群の最後の印が立つエントリまで
            view.trie
                .common_prefix_search(input, byte_pos, |end, group| {
                    if corrupt.is_some() {
                        return;
                    }
                    let mut id = group as usize;
                    loop {
                        let Some(e) = view.entries.get(id) else {
                            corrupt = Some(DictError::corrupt(format!(
                                "group at {group} runs past the last entry"
                            )));
                            return;
                        };
                        let (left_id, right_id) = (e.left_id(), e.right_id);
                        if left_id as usize >= view.num_left || right_id as usize >= view.num_right
                        {
                            corrupt = Some(DictError::corrupt(format!(
                                "entry {id} has context IDs outside the matrix"
                            )));
                            return;
                        }
                        self.add_node(
                            end,
                            LatticeNode {
                                start: byte_pos as u32,
                                end: end as u32,
                                entry_id: id as u32,
                                left_id,
                                right_id,
                                word_cost: e.cost,
                                char_type: CharType::Default,
                            },
                            i64::MAX,
                        );
                        has_known = true;
                        if e.is_last() {
                            break;
                        }
                        id += 1;
                    }
                })?;
            if let Some(e) = corrupt {
                return Err(e);
            }

            // 未知語処理（文字種ごとの先頭のテンプレートだけを使う）
            let char_type = classifier.classify_char(c);
            let unk = dict.unk_info(type_index(char_type));
            if unk.invoke || !has_known {
                let single_len = c.len_utf8();
                let (left_id, right_id, cost) = (unk.left_id, unk.right_id, unk.cost);
                let unk_node = |end: usize| LatticeNode {
                    start: byte_pos as u32,
                    end: end as u32,
                    entry_id: UNK_FLAG,
                    left_id,
                    right_id,
                    word_cost: cost,
                    char_type,
                };

                // コールバックで直接ノード追加（Vec アロケーション排除）
                let mut added_single = false;
                classifier.group_at_cb(input, byte_pos, |group_len, _| {
                    let end = byte_pos + group_len;
                    if group_len == single_len {
                        added_single = true;
                    }
                    self.add_node(end, unk_node(end), i64::MAX);
                });

                // 1文字未知語がまだなければ追加
                if !added_single {
                    let single_end = byte_pos + single_len;
                    self.add_node(single_end, unk_node(single_end), i64::MAX);
                }
            }
        }

        // --- Viterbi forward pass (separate costs/prevs arrays for cache locality) ---
        for end_pos in 1..=byte_len {
            for ni_idx in 0..self.end_nodes[end_pos].len() {
                let node_idx = self.end_nodes[end_pos][ni_idx] as usize;
                let node = self.nodes[node_idx];
                // 転置した行列の、この語の left_id の行を前の語の right_id で引く
                let row = view.matrix_row(node.left_id);
                let node_word_cost = node.word_cost as i64;

                let mut best_cost = i64::MAX;
                let mut best_prev = NO_PREV;
                for &prev_idx in &self.end_nodes[node.start as usize] {
                    let prev_total = self.costs[prev_idx as usize];
                    if prev_total == i64::MAX {
                        continue;
                    }
                    let conn_cost = row[self.nodes[prev_idx as usize].right_id as usize] as i64;
                    let total = prev_total + conn_cost + node_word_cost;
                    if total < best_cost {
                        best_cost = total;
                        best_prev = prev_idx;
                    }
                }

                self.costs[node_idx] = best_cost;
                self.prevs[node_idx] = best_prev;
            }
        }

        // --- EOS（left_id 0）の最良前ノード決定 ---
        let eos_row = view.matrix_row(0);
        let mut best_cost = i64::MAX;
        let mut best_last = NO_PREV;
        for &prev_idx in &self.end_nodes[byte_len] {
            let prev_total = self.costs[prev_idx as usize];
            if prev_total == i64::MAX {
                continue;
            }
            let conn_cost = eos_row[self.nodes[prev_idx as usize].right_id as usize] as i64;
            let total = prev_total + conn_cost;
            if total < best_cost {
                best_cost = total;
                best_last = prev_idx;
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

        // --- トークン生成（表層形は入力の部分文字列から作る） ---
        let mut tokens: Vec<Token> = Vec::with_capacity(path.len());
        for &idx in &path {
            let node = self.nodes[idx as usize];
            let (start, end) = (node.start as usize, node.end as usize);
            let surface = &input[start..end];
            if node.is_known() {
                tokens.push(self.token_cache.known_token(
                    dict,
                    node.entry_id,
                    surface,
                    start,
                    end,
                    node.word_cost,
                    &mut self.decode_scratch,
                )?);
            } else {
                let surface: Arc<str> = Arc::from(surface);
                tokens.push(Token {
                    start,
                    end,
                    pos: Arc::clone(&dict.unk_info(type_index(node.char_type)).pos),
                    conj_type: Arc::clone(&EMPTY_ARC),
                    conj_form: Arc::clone(&EMPTY_ARC),
                    base_form: Arc::clone(&surface),
                    reading: Arc::clone(&EMPTY_ARC),
                    pronunciation: Arc::clone(&EMPTY_ARC),
                    surface,
                    word_cost: node.word_cost,
                    is_known: false,
                });
            }
        }
        apply_contextual_readings(&mut tokens);
        Ok(tokens)
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
                ..Default::default()
            });
        }

        builder.build().unwrap()
    }

    #[test]
    fn test_lattice_tokenize() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("東京都に住んでいる", &dict).unwrap();

        assert!(!tokens.is_empty());
        let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(reconstructed, "東京都に住んでいる");
    }

    #[test]
    fn test_workspace_reuse() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();

        let t1 = ws.tokenize("東京都", &dict).unwrap();
        let t2 = ws.tokenize("東京都に住んでいる", &dict).unwrap();

        assert!(!t1.is_empty());
        assert!(!t2.is_empty());
    }

    // --- 追加テスト ---

    #[test]
    fn test_empty_input() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("", &dict).unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_single_known_word() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("東京", &dict).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(&*tokens[0].surface, "東京");
        assert!(tokens[0].is_known);
    }

    #[test]
    fn test_unknown_word_only() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        // "大阪" is not in the dictionary
        let tokens = ws.tokenize("大阪", &dict).unwrap();
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
            let tokens = ws.tokenize(input, &dict).unwrap();
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
        let tokens = ws.tokenize("東京都", &dict).unwrap();

        // With zero connection costs, "東京都" (2000) < "東京" (3000) + "都" (5000) = 8000
        let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(surfaces, vec!["東京都"]);
    }

    #[test]
    fn test_word_cost_preserved() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("東京都", &dict).unwrap();
        assert_eq!(tokens[0].word_cost, 2000);
    }

    #[test]
    fn test_pos_preserved() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("に", &dict).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(&*tokens[0].pos, "助詞,格助詞,一般,*");
    }

    #[test]
    fn test_multiple_tokenize_calls() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();

        // Multiple calls should all work correctly
        for _ in 0..10 {
            let tokens = ws.tokenize("東京都", &dict).unwrap();
            assert!(!tokens.is_empty());
            let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
            assert_eq!(reconstructed, "東京都");
        }
    }

    #[test]
    fn test_unknown_word_has_pos() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("XYZ", &dict).unwrap();
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
        let tokens = ws.tokenize(&input, &dict).unwrap();
        let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(reconstructed, input);
    }

    #[test]
    fn test_single_char_unknown() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let tokens = ws.tokenize("X", &dict).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(&*tokens[0].surface, "X");
        assert!(!tokens[0].is_known);
    }

    #[test]
    fn test_mixed_known_unknown_sequence() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        // "東京" is known, "ABC" is unknown, "に" is known
        let tokens = ws.tokenize("東京ABCに", &dict).unwrap();
        let reconstructed: String = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(reconstructed, "東京ABCに");
    }

    fn reading_token(surface: &str, pos: &str, reading: &str) -> Token {
        Token {
            surface: surface.into(),
            start: 0,
            end: surface.len(),
            pos: pos.into(),
            conj_type: "".into(),
            conj_form: "".into(),
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

    #[test]
    fn test_decimal_counter_is_not_geminated() {
        // 「4.1節」は「四点一節」に正規化される。小数点以下の「一」と「節」は
        // 別の語なので「イッセツ」ではなく「イチセツ」
        let mut tokens = vec![
            reading_token("四", "名詞,数,*,*", "ヨン"),
            reading_token("点", "名詞,一般,*,*", "テン"),
            reading_token("一節", "名詞,一般,*,*", "イッセツ"),
        ];
        apply_contextual_readings(&mut tokens);
        assert_eq!(&*tokens[2].reading, "イチセツ");
        assert_eq!(&*tokens[2].pronunciation, "イチセツ");

        // 小数点の直後でない「一節」は促音化したまま（詩の一節、第一節）
        let mut tokens = vec![
            reading_token("詩", "名詞,一般,*,*", "シ"),
            reading_token("の", "助詞,連体化,*,*", "ノ"),
            reading_token("一節", "名詞,一般,*,*", "イッセツ"),
        ];
        apply_contextual_readings(&mut tokens);
        assert_eq!(&*tokens[2].reading, "イッセツ");

        // 「点」の前が数詞でなければ小数点ではない（「要点」「一節」が並ぶ文）
        let mut tokens = vec![
            reading_token("要", "名詞,一般,*,*", "ヨウ"),
            reading_token("点", "名詞,一般,*,*", "テン"),
            reading_token("一節", "名詞,一般,*,*", "イッセツ"),
        ];
        apply_contextual_readings(&mut tokens);
        assert_eq!(&*tokens[2].reading, "イッセツ");
    }
}
