//! ラティス構築 + Viterbi デコーディング
//!
//! 前分割したチャンクごとに次の順で解析する。
//!
//! 1. 文字ごとの前処理（`ChunkChars`）: バイト位置・trie の文字符号・文字種・同じ文字種が続く長さを
//!    1 回だけ求める。辞書引きは開始位置ごとに数文字先まで辿るので、復号と表の引きを繰り返さない
//! 2. ラティスの構築と Viterbi を同時に行う: 開始位置（文字位置）の昇順に、trie の共通接頭辞検索で
//!    見つけたエントリと未知語をノードにする。位置 i に来たときには i で終わるノードが揃っているので、
//!    ノードを作るときに最良の前ノードをその場で決め、終了位置ごとの列（`Lattice::ends`）に
//!    累積コストごと入れる。i で終わるノードが無い位置（BOS から辿り着けない位置）は飛ばす
//! 3. EOS の最良の前ノードから辿って最良パスを求め、トークンを作る
//!
//! 同点のときは先に追加した前ノードが勝つ（`<` の先勝ち）。ノードは、開始位置の昇順、trie が報告する順
//! （短い語から）、群の中のエントリの順、未知語は既知語の後（並び全体、続けて短い接頭辞から）に追加する。
//!
//! 未知語の候補は MeCab と同じ（char.def の group・length、`UnkGrouping::for_each_len`）だが、カタカナの
//! 並び全体の候補には 2 つの規則を足す（外来語の複合語は区切らずに書くので、単語コストが一律の並び全体の
//! 未知語が、既知語 2 語以上の複合語に勝ってしまう）。
//!
//! - 並び全体の候補（3 字以上）は、3 字以上の既知語で隙間なく覆えるなら作らない（[`RunCover`]）。
//!   「オススメアプリ」を 1 つの未知語にせず「オススメ / アプリ」にする。2 字以下の語は数えないので、
//!   辞書にない人名などは断片（ドミ / ニク）に割れず 1 語（ドミニク）のまま
//! - 最良パスに残ったカタカナの未知語（3 字以上）と同じ表層の語が辞書にあれば、その語の素性で出す
//!   （[`dictionary_word_for_unknown`]）。単語コストが高い既知語（IPAdic の「キャンプ」16437）は未知語に
//!   負けるが、分け方は MeCab と同じまま品詞・読みを辞書から取る

use crate::char_class::{ALL_CHAR_TYPES, CharType, type_index};
use crate::hsd::reader::DictView;
use crate::hsd::trie::Trie;
use crate::hsd::{DictError, Dictionary};
use std::sync::Arc;

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
        // 前後のトークンを見る補正は、対象の語のときだけ前後を調べる（どのトークンでも調べると、
        // 次のトークンの品詞の文字列比較が全トークンに掛かる）
        if &*tokens[i].surface == "数" {
            let next_is_number = tokens.get(i + 1).is_some_and(|t| is_number_pos(&t.pos));
            if apply_kazu_reading_override(&mut tokens[i], next_is_number) {
                continue;
            }
        }
        if DECIMAL_COUNTER_OVERRIDES
            .iter()
            .any(|&(surface, _)| &*tokens[i].surface == surface)
        {
            let after_decimal_point = is_after_decimal_point(tokens, i);
            if apply_decimal_counter_override(&mut tokens[i], after_decimal_point) {
                continue;
            }
        }
        // 非 ASCII の文字は ASCII の英字のバイトを含まないので、バイトで調べても文字で調べるのと同じ
        if !tokens[i].surface.bytes().all(|b| b.is_ascii_alphabetic()) {
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

/// ラティスのノード（終了位置ごとの列に、追加した順に入れる）
///
/// 追加するときに最良の前ノードを決めるので、左文脈 ID は持たない。終了位置は入っている列で分かる。
#[derive(Clone, Copy)]
struct Node {
    /// BOS からこのノードの終わりまでの最小の累積コスト
    cost: i64,
    /// 辞書エントリの番号。BOUNDARY_ID = BOS、`UNK_FLAG | 文字種の番号` = 未知語
    entry: u32,
    /// 最良の前ノードの、開始位置で終わるノードの列（`ends[start]`）の中での番号
    prev: u32,
    /// 開始の文字位置
    start: u32,
    right_id: u16,
    word_cost: i16,
}

const BOUNDARY_ID: u32 = u32::MAX;
const UNK_FLAG: u32 = 0x8000_0000;

/// カタカナの文字種の番号
const KATAKANA: usize = type_index(CharType::Katakana);

/// カタカナの並び全体の候補の規則を当てる長さの下限。並びを覆う既知語もこの字数以上のものだけを数える
///
/// 2 字の語まで数えると、辞書にない人名が断片に割れる（ドミ / ニク、ゲイ / リー）。2 字のカタカナ語は
/// 普通名詞にも断片が多く（メン、ジャ、エリ）、品詞では分けられない。
const COMPOUND_MIN_CHARS: usize = 3;

impl Node {
    const BOS: Node = Node {
        cost: 0,
        entry: BOUNDARY_ID,
        prev: 0,
        start: 0,
        right_id: 0,
        word_cost: 0,
    };

    #[inline]
    fn is_boundary(&self) -> bool {
        self.entry == BOUNDARY_ID
    }

    /// 未知語なら文字種の番号
    #[inline]
    fn unknown_type(&self) -> Option<usize> {
        (self.entry & UNK_FLAG != 0).then_some((self.entry & !UNK_FLAG) as usize)
    }
}

/// 再利用可能なラティスワークスペース
pub struct LatticeWorkspace {
    /// 解析中のチャンクの文字ごとの情報
    chars: ChunkChars,
    lattice: Lattice,
    /// トークンを作る（既知語のキャッシュと、トークンに入れる文字列の解析器ごとの写し）
    tokens: TokenBuilder,
    /// カタカナの並び全体の候補を作るかの表
    cover: RunCover,
    /// 位置ごとに辞書を引いた (語の終わり, 群) の作業用の列
    hits: Vec<(u32, u32)>,
}

/// カタカナの並びで、各位置から並びの終わりまでの未知語の候補（並び全体）を作らないかの表
///
/// 位置 p の候補 [p, e)（e は並びの終わり）は、[`COMPOUND_MIN_CHARS`] 字以上の既知語を隙間なく並べて
/// 覆えるなら作らない。最初の語は p より前から始まってよく（「音ゲー / アプリ」の「音ゲー」は並びの左の
/// 境界をまたぐ）、最後の語は e より後で終わってよい（「スパッと」は「スパッ」の並びの右の境界をまたぐ）。
/// 字数は語の全体で数える。[p, e) と同じ表層の語 1 語だけでは覆えたことにしない（単語コストが高い語は
/// 未知語に負けるので、候補を消すと断片に割れる。候補は残し、出力で辞書の素性にする）。
///
/// 並び（3 字以上）に入った位置で、並びの各位置の辞書引きを先に済ませて表を作る。引いた結果は、並びの
/// 中の位置でラティスにノードを足すときにそのまま使う（同じ位置を 2 度引かない）。並びより前から始まる語は、
/// ラティスにあるノード（辿り着ける位置から作ったもの）から拾う。
#[derive(Default)]
struct RunCover {
    /// 表を作った範囲 [start, end)。end が 0 なら表が無い
    start: usize,
    end: usize,
    /// `suppress[p - start]`: 位置 p の並び全体の候補を作らない
    suppress: Vec<bool>,
    /// 並びの各位置から引いた (語の終わり, 群) を位置の順に並べたもの
    hits: Vec<(u32, u32)>,
    /// `hits[hit_starts[p - start]..hit_starts[p - start + 1]]` が位置 p から引いたもの
    hit_starts: Vec<u32>,
    /// 作業用: `tiles[q - start]` = [q, end) を q から始まる語で覆える（`tiles[end - start]` は true）
    tiles: Vec<bool>,
}

impl RunCover {
    /// チャンクの解析を始めるときに呼ぶ（位置はチャンクごとの文字の番号なので、前の表は使えない）
    fn reset(&mut self) {
        self.end = 0;
        shrink_retained(&mut self.suppress, 0);
        shrink_retained(&mut self.hits, 0);
        shrink_retained(&mut self.hit_starts, 0);
        shrink_retained(&mut self.tiles, 0);
    }

    /// 位置 i の表があるか
    #[inline]
    fn contains(&self, i: usize) -> bool {
        (self.start..self.end).contains(&i)
    }

    /// 位置 i から引いた (語の終わり, 群)。表の外なら None
    #[inline]
    fn hits_at(&self, i: usize) -> Option<&[(u32, u32)]> {
        self.contains(i).then(|| {
            let k = i - self.start;
            &self.hits[self.hit_starts[k] as usize..self.hit_starts[k + 1] as usize]
        })
    }

    /// 位置 i から並びの終わりまでの候補を作らないか（表の外なら false）
    #[inline]
    fn suppresses(&self, i: usize) -> bool {
        self.contains(i) && self.suppress[i - self.start]
    }

    /// 並び [s, e) の表を作る。位置 s でノードを足す前に呼ぶ
    ///
    /// `rest[k]` は位置 s + 1 + k で終わるノードの列（位置 s より前から作ったもの）。`known_reach` は
    /// 位置 s より前から始まる既知語の終わりの最大
    #[allow(clippy::too_many_arguments)]
    fn build(
        &mut self,
        s: usize,
        e: usize,
        input: &str,
        chars: &ChunkChars,
        trie: &Trie<'_>,
        rest: &[Vec<Node>],
        known_reach: usize,
    ) -> Result<(), DictError> {
        let len = e - s;
        self.start = s;
        self.end = e;
        let hits = &mut self.hits;
        hits.clear();
        self.hit_starts.clear();
        for j in s..e {
            self.hit_starts.push(hits.len() as u32);
            trie.common_prefix_search_codes(input, &chars.codes, &chars.offsets, j, |q, g| {
                hits.push((q as u32, g))
            })?;
        }
        self.hit_starts.push(hits.len() as u32);
        let starts = &self.hit_starts;
        // 位置 j から始まる、COMPOUND_MIN_CHARS 字以上の語の終わり
        let long_words = |j: usize| {
            hits[starts[j - s] as usize..starts[j - s + 1] as usize]
                .iter()
                .map(|&(q, _)| q as usize)
                .filter(move |&q| q - j >= COMPOUND_MIN_CHARS)
        };
        // [q, e) を q から始まる語で覆えるか（右から決める）
        let tiles = &mut self.tiles;
        tiles.clear();
        tiles.resize(len + 1, false);
        tiles[len] = true;
        for j in (s..e).rev() {
            tiles[j - s] = long_words(j).any(|q| q >= e || tiles[q - s]);
        }
        // 並び全体をまたぐ既知語（並びより前から始まり、並びの後で終わる）があれば、どの位置も覆える
        let suppress = &mut self.suppress;
        suppress.clear();
        suppress.resize(len, known_reach > e);
        for j in s..e {
            for q in long_words(j) {
                if q < e && !tiles[q - s] {
                    continue;
                }
                // j から始まる並べ方（[j, e) と同じ表層の 1 語だけは除く）
                if q != e {
                    suppress[j - s] = true;
                }
                // j < p < q の位置 p は、左の境界をまたぐ語で覆える
                suppress[j + 1 - s..q.min(e) - s].fill(true);
            }
        }
        // 並びより前から始まり、並びの中か終わりで終わる語
        for q in s + 1..=e {
            if q < e && !tiles[q - s] {
                continue;
            }
            let crosses = rest[q - s - 1].iter().any(|n| {
                let start = n.start as usize;
                start < s && q - start >= COMPOUND_MIN_CHARS && n.unknown_type().is_none()
            });
            if crosses {
                suppress[..q - s].fill(true);
            }
        }
        Ok(())
    }
}

/// 解析するチャンクの文字ごとの情報（チャンクごとに 1 回だけ作る）
///
/// 辞書引きは開始位置ごとに数文字先まで辿るので、文字の復号と符号・文字種の引きを先に済ませておく。
/// 位置はすべて文字の番号（0 始まり）。
#[derive(Default)]
struct ChunkChars {
    /// 各文字のバイト位置。末尾に入力の長さを加えた「文字数 + 1」要素
    offsets: Vec<u32>,
    /// 各文字の trie の符号（辞書のキーに現れない文字は 0）
    codes: Vec<u16>,
    /// 各文字の文字種（`type_index` の値）
    types: Vec<u8>,
    /// その文字から同じ文字種が続く文字数（1 以上）
    runs: Vec<u32>,
}

impl ChunkChars {
    fn fill(&mut self, input: &str, dict: &Dictionary, trie: &Trie<'_>) {
        // 長いチャンクのあとも大きな確保を持ち続けない（文字数は入力のバイト数以下）
        let len = input.len();
        shrink_retained(&mut self.offsets, len + 1);
        shrink_retained(&mut self.codes, len);
        shrink_retained(&mut self.types, len);
        shrink_retained(&mut self.runs, len);
        self.offsets.clear();
        self.codes.clear();
        self.types.clear();
        for (pos, c) in input.char_indices() {
            self.offsets.push(pos as u32);
            // 符号は u16 の表から引くので u16 に収まる
            self.codes.push(trie.char_code(c) as u16);
            self.types.push(dict.char_type_index(c));
        }
        self.offsets.push(input.len() as u32);
        let n = self.types.len();
        self.runs.clear();
        self.runs.resize(n, 1);
        for i in (0..n.saturating_sub(1)).rev() {
            if self.types[i] == self.types[i + 1] {
                self.runs[i] = self.runs[i + 1] + 1;
            }
        }
    }

    fn len(&self) -> usize {
        self.codes.len()
    }
}

/// ラティス
struct Lattice {
    /// `ends[pos]` = 文字位置 pos で終わるノード（追加した順）。`ends[0]` は BOS だけ。
    /// `used` 以降の列は空（容量だけ持つ）
    ends: Vec<Vec<Node>>,
    /// 直前のチャンクで使った列の数
    used: usize,
    /// 最良パス（終了位置, `ends` の中での番号）の再利用バッファ
    path: Vec<(u32, u32)>,
}

/// チャンクが短くなっても容量ごと残しておく、終了位置ごとの列の数。これより長いチャンクの列は
/// 次のチャンクで捨てる（まれな長い入力のあとに大きな確保を持ち続けない）
const ENDS_RETAINED: usize = 1024;

/// 文字ごと・パスの作業用の配列で、次のチャンクに要らなければ手放す容量の下限（要素数）
const BUFFER_RETAINED: usize = 1 << 16;

/// `v` を空にし、`needed` 要素と [`BUFFER_RETAINED`] の大きい方を超える容量を手放す
fn shrink_retained<T>(v: &mut Vec<T>, needed: usize) {
    v.clear();
    let keep = needed.max(BUFFER_RETAINED);
    if v.capacity() > keep {
        v.shrink_to(keep);
    }
}

/// トークンを作る（最良パスのノードから `Token` を組み立てる）
///
/// - 既知語のキャッシュ: 辞書エントリの番号で引く直接マップ方式。同じ語を何度も解析するときに、
///   素性レコードの復号と文字列の確保を省く（v3 は辞書の全文字列をキャッシュしていたが、v4 はよく出る
///   語だけを解析器ごとに持つ）。文脈による読みの補正はキャッシュから作った `Token` に対して行い、
///   キャッシュには戻さない
/// - 品詞・活用型・活用形・未知語の品詞・空文字列は、辞書の表の `Arc` を複製せず、解析器ごとに同じ内容の
///   `Arc` を作って使う（初めて使うときに作る）。辞書の `Arc` をそのまま複製すると、複数のスレッドの
///   解析器が同じ参照カウントをトークンごとに更新し合い、キャッシュラインの取り合いで並列に解析しても
///   速くならない
///
/// 解析器（`Analyzer::clone`）ごとに独立しているのでロックは要らない。違う辞書で解析したら作り直す。
struct TokenBuilder {
    /// 写しとキャッシュを作った辞書の番号
    dict_id: u64,
    empty: Arc<str>,
    pos: Vec<Option<Arc<str>>>,
    conj_types: Vec<Option<Arc<str>>>,
    conj_forms: Vec<Option<Arc<str>>>,
    /// 文字種ごとの未知語の品詞
    unk_pos: Vec<Option<Arc<str>>>,
    /// 既知語のトークンのキャッシュ（エントリ番号, トークン）
    slots: Vec<Option<(u32, Token)>>,
    /// 素性レコードの読み・発音を復号するときの再利用バッファ
    scratch: String,
}

/// キャッシュのスロット数
const TOKEN_CACHE_SLOTS: usize = 1024;
/// これより長い表層形の語はキャッシュしない（まれな長い固有名詞でメモリを使わない）
const TOKEN_CACHE_MAX_SURFACE: usize = 64;

/// 解析器ごとの写しを引く。無ければ `s` から作る（空文字列は `empty` を使う）
#[inline]
fn local_str(slot: &mut Option<Arc<str>>, empty: &Arc<str>, s: &str) -> Arc<str> {
    Arc::clone(slot.get_or_insert_with(|| {
        if s.is_empty() {
            Arc::clone(empty)
        } else {
            Arc::from(s)
        }
    }))
}

impl TokenBuilder {
    fn new() -> Self {
        TokenBuilder {
            dict_id: u64::MAX,
            empty: Arc::from(""),
            pos: Vec::new(),
            conj_types: Vec::new(),
            conj_forms: Vec::new(),
            unk_pos: Vec::new(),
            slots: Vec::new(),
            scratch: String::with_capacity(64),
        }
    }

    /// `dict` で解析する準備（前と違う辞書なら写しとキャッシュを捨てる）
    fn prepare(&mut self, dict: &Dictionary) {
        if self.dict_id == dict.id() {
            return;
        }
        self.dict_id = dict.id();
        let reset = |v: &mut Vec<Option<Arc<str>>>, len: usize| {
            v.clear();
            v.resize(len, None);
        };
        reset(&mut self.pos, dict.pos_count());
        reset(&mut self.conj_types, dict.conj_type_count());
        reset(&mut self.conj_forms, dict.conj_form_count());
        reset(&mut self.unk_pos, ALL_CHAR_TYPES.len());
        self.slots.clear();
        self.slots.resize(TOKEN_CACHE_SLOTS, None);
    }

    /// 既知語のトークン（表層形は入力の部分文字列から作る）。[`TokenBuilder::prepare`] の後に呼ぶ
    fn known(
        &mut self,
        dict: &Dictionary,
        entry_id: u32,
        surface: &str,
        start: usize,
        end: usize,
        word_cost: i16,
    ) -> Result<Token, DictError> {
        let slot = entry_id as usize % TOKEN_CACHE_SLOTS;
        if let Some((cached_id, token)) = &self.slots[slot] {
            if *cached_id == entry_id {
                let mut token = token.clone();
                token.start = start;
                token.end = end;
                token.word_cost = word_cost;
                return Ok(token);
            }
        }
        let f = dict.feature(entry_id as usize)?;
        let scratch = &mut self.scratch;
        let surface: Arc<str> = Arc::from(surface);
        let reading: Arc<str> = if f.reading.is_empty() {
            Arc::clone(&self.empty)
        } else {
            f.reading.to_arc(scratch)
        };
        let pronunciation = match f.pronunciation {
            None => Arc::clone(&reading),
            Some(p) if p.is_empty() => Arc::clone(&self.empty),
            Some(p) => p.to_arc(scratch),
        };
        let base_form = match f.base_form {
            None => Arc::clone(&surface),
            Some(b) => b.to_arc(scratch),
        };
        let empty = &self.empty;
        let token = Token {
            pos: local_str(
                &mut self.pos[f.pos_id as usize],
                empty,
                dict.pos_name(f.pos_id),
            ),
            conj_type: local_str(
                &mut self.conj_types[f.conj_type_id as usize],
                empty,
                dict.token_conj_type(f.conj_type_id),
            ),
            conj_form: local_str(
                &mut self.conj_forms[f.conj_form_id as usize],
                empty,
                dict.token_conj_form(f.conj_form_id),
            ),
            surface,
            start,
            end,
            base_form,
            reading,
            pronunciation,
            word_cost,
            is_known: true,
        };
        if token.surface.len() <= TOKEN_CACHE_MAX_SURFACE {
            self.slots[slot] = Some((entry_id, token.clone()));
        }
        Ok(token)
    }

    /// 未知語のトークン（品詞は文字種ごとのテンプレート、原形は表層形、読みは空）。
    /// [`TokenBuilder::prepare`] の後に呼ぶ
    fn unknown(
        &mut self,
        dict: &Dictionary,
        type_idx: usize,
        surface: &str,
        start: usize,
        end: usize,
        word_cost: i16,
    ) -> Token {
        let surface: Arc<str> = Arc::from(surface);
        let empty = &self.empty;
        Token {
            pos: local_str(
                &mut self.unk_pos[type_idx],
                empty,
                &dict.unk_info(type_idx).pos,
            ),
            conj_type: Arc::clone(empty),
            conj_form: Arc::clone(empty),
            base_form: Arc::clone(&surface),
            reading: Arc::clone(empty),
            pronunciation: Arc::clone(empty),
            surface,
            start,
            end,
            word_cost,
            is_known: false,
        }
    }
}

impl Lattice {
    fn new() -> Self {
        Lattice {
            ends: Vec::with_capacity(ENDS_RETAINED),
            used: 0,
            path: Vec::with_capacity(64),
        }
    }

    /// 文字数 `len` のチャンクのために空にして BOS を置く（終了位置は 0..=len）
    ///
    /// 列の確保はチャンクをまたいで使い回す。空にするのは直前のチャンクで使った列だけ
    fn reset(&mut self, len: usize) {
        let positions = len + 1;
        // 残す数を超える列は先に捨てる（長いチャンクの直後に、捨てる列まで空にして回らない）
        self.ends.truncate(positions.max(ENDS_RETAINED));
        let used = self.used.min(self.ends.len());
        for v in &mut self.ends[..used] {
            v.clear();
        }
        if self.ends.len() < positions {
            self.ends.resize_with(positions, Vec::new);
        }
        self.used = positions;
        self.ends[0].push(Node::BOS);
        shrink_retained(&mut self.path, 0);
    }
}

/// `prevs`（ある位置で終わるノード）のうち、左文脈の行 `row` で接続したときに累積コストが最小のもの
///
/// Returns: (累積コスト + 接続コスト, `prevs` の中での番号)。同点なら先に追加したノード（`<` の先勝ち）。
/// `prevs` が空なら (i64::MAX, 0)
#[inline]
fn best_prev(prevs: &[Node], row: &[i16]) -> (i64, u32) {
    let mut best_cost = i64::MAX;
    let mut best_prev = 0;
    for (k, p) in prevs.iter().enumerate() {
        let total = p.cost + row[p.right_id as usize] as i64;
        if total < best_cost {
            best_cost = total;
            best_prev = k as u32;
        }
    }
    (best_cost, best_prev)
}

impl LatticeWorkspace {
    pub fn new() -> Self {
        LatticeWorkspace {
            chars: ChunkChars::default(),
            lattice: Lattice::new(),
            tokens: TokenBuilder::new(),
            cover: RunCover::default(),
            hits: Vec::new(),
        }
    }

    /// ラティスを構築して Viterbi で最良パスを求める
    ///
    /// 辞書の不正な参照（範囲外の群・文脈 ID・素性レコード、壊れた trie）を見つけたら
    /// [`DictError::Corrupt`] を返す。検証済みの辞書（`hasami info --verify`）では起きない。
    pub fn tokenize(&mut self, input: &str, dict: &Dictionary) -> Result<Vec<Token>, DictError> {
        let mut tokens = Vec::new();
        self.tokenize_into(input, dict, 0, &mut tokens)?;
        Ok(tokens)
    }

    /// [`LatticeWorkspace::tokenize`] の結果を `out` の末尾に足す。トークンの位置には `offset` を足す
    ///
    /// 文脈による読みの補正は、足したトークンの中だけで行う（`out` にあった前のトークンは見ない）。
    /// エラーのときは `out` に途中までのトークンが残りうる。
    pub fn tokenize_into(
        &mut self,
        input: &str,
        dict: &Dictionary,
        offset: usize,
        out: &mut Vec<Token>,
    ) -> Result<(), DictError> {
        if input.is_empty() {
            return Ok(());
        }
        let view = dict.view();
        self.chars.fill(input, dict, &view.trie);
        let chars = &self.chars;
        let n = chars.len();
        let lattice = &mut self.lattice;
        lattice.reset(n);
        let cover = &mut self.cover;
        cover.reset();
        let scratch_hits = &mut self.hits;
        // これまでの位置から始まる既知語の終わりの最大（カタカナの並びをまたぐ語があるかに使う）
        let mut known_reach = 0;

        // 各文字位置から辞書引き + 未知語生成。ノードは開始位置の昇順に追加するので、位置 i に来た
        // ときには i で終わるノードがすべて揃っていて、追加するノードの最良の前ノードをその場で決められる
        // （同点は先に追加した前ノードが勝つ）。i で終わるノードが無い位置には BOS から辿り着けないので、
        // そこから始まるノードは作らない
        for i in 0..n {
            let (done, rest) = lattice.ends.split_at_mut(i + 1);
            let prevs = &done[i];
            if prevs.is_empty() {
                continue;
            }
            let type_idx = chars.types[i] as usize;
            let unk = dict.unk_info(type_idx);
            let run = chars.runs[i];
            // カタカナの並び（3 字以上）に入ったら、並びの各位置の辞書引きを先に済ませて表を作る
            if type_idx == KATAKANA
                && unk.grouping.group()
                && run as usize >= COMPOUND_MIN_CHARS
                && !cover.contains(i)
            {
                cover.build(
                    i,
                    i + run as usize,
                    input,
                    chars,
                    &view.trie,
                    rest,
                    known_reach,
                )?;
            }
            // この位置から引いた (語の終わり, 群)。カタカナの並びの中なら表を作るときに引いたもの
            let hits = match cover.hits_at(i) {
                Some(hits) => hits,
                None => {
                    scratch_hits.clear();
                    view.trie.common_prefix_search_codes(
                        input,
                        &chars.codes,
                        &chars.offsets,
                        i,
                        |end, group| scratch_hits.push((end as u32, group)),
                    )?;
                    &scratch_hits[..]
                }
            };
            let has_known = !hits.is_empty();
            // 群 = trie の値が指すエントリから、群の最後の印が立つエントリまで
            for &(end, group) in hits {
                let end = end as usize;
                known_reach = known_reach.max(end);
                // rest[k] は位置 i + 1 + k で終わるノードの列
                let list = &mut rest[end - i - 1];
                let mut id = group as usize;
                loop {
                    let Some(e) = view.entries.get(id) else {
                        return Err(DictError::corrupt(format!(
                            "group at {group} runs past the last entry"
                        )));
                    };
                    let (left_id, right_id) = (e.left_id(), e.right_id);
                    if left_id as usize >= view.num_left || right_id as usize >= view.num_right {
                        return Err(DictError::corrupt(format!(
                            "entry {id} has context IDs outside the matrix"
                        )));
                    }
                    let (cost, prev) = best_prev(prevs, view.matrix_row(left_id));
                    list.push(Node {
                        cost: cost + e.cost as i64,
                        entry: id as u32,
                        prev,
                        start: i as u32,
                        right_id,
                        word_cost: e.cost,
                    });
                    if e.is_last() {
                        break;
                    }
                    id += 1;
                }
            }

            // 未知語処理（文字種ごとの先頭のテンプレートだけを使う）
            if unk.invoke || !has_known {
                // この位置の未知語は left_id が同じなので、最良の前ノードは 1 度だけ求める
                let (cost, prev) = best_prev(prevs, view.matrix_row(unk.left_id));
                let node = Node {
                    cost: cost + unk.cost as i64,
                    entry: UNK_FLAG | type_idx as u32,
                    prev,
                    start: i as u32,
                    right_id: unk.right_id,
                    word_cost: unk.cost,
                };
                // カタカナの並び全体の候補は、3 字以上の既知語で覆えるなら作らない
                let skip_group = type_idx == KATAKANA
                    && unk.grouping.group()
                    && run as usize >= COMPOUND_MIN_CHARS
                    && cover.suppresses(i);
                let mut added = false;
                unk.grouping.for_each_len(run, |len| {
                    // group のときは、並び全体と同じ長さの候補は並び全体の候補だけ
                    if skip_group && len == run {
                        return;
                    }
                    added = true;
                    rest[len as usize - 1].push(node);
                });
                // 候補が無く、この位置から始まる既知語も無いときだけ 1 文字の未知語（MeCab と同じ）
                if !added && !has_known {
                    rest[0].push(node);
                }
            }
        }

        // --- EOS（left_id 0）の最良前ノードからトレースバック ---
        // 位置 n にはいつも辿り着ける（辿り着ける位置からは、既知語・未知語の候補・1 文字の未知語の
        // どれかで必ず先へ進める。ノードは入力の外で終わらない）
        let (_, last) = best_prev(&lattice.ends[n], view.matrix_row(0));
        lattice.path.clear();
        let (mut pos, mut idx) = (n, last as usize);
        loop {
            let node = lattice.ends[pos][idx];
            if node.is_boundary() {
                break;
            }
            lattice.path.push((pos as u32, idx as u32));
            (pos, idx) = (node.start as usize, node.prev as usize);
        }

        // --- トークン生成（表層形は入力の部分文字列から作る） ---
        let first = out.len();
        let path = &lattice.path;
        out.reserve(path.len());
        self.tokens.prepare(dict);
        // path は末尾のノードから並んでいる（k - 1 が次のノード）
        for k in (0..path.len()).rev() {
            let (end_pos, idx) = path[k];
            let node = lattice.ends[end_pos as usize][idx as usize];
            let (node_start, node_end) = (node.start as usize, end_pos as usize);
            let start = chars.offsets[node_start] as usize;
            let end = chars.offsets[node_end] as usize;
            let surface = &input[start..end];
            let (start, end) = (offset + start, offset + end);
            // カタカナの未知語（3 字以上）と同じ表層の語が辞書にあれば、その語の素性で出す
            let dict_word = match node.unknown_type() {
                Some(KATAKANA) if node_end - node_start >= COMPOUND_MIN_CHARS => {
                    // 前の語（BOS を含む）の right_id と、次の語（無ければ EOS の 0）の left_id
                    let prev_right = lattice.ends[node_start][node.prev as usize].right_id;
                    let next_left = match k.checked_sub(1) {
                        Some(k) => {
                            let (pos, idx) = path[k];
                            left_id(&view, dict, &lattice.ends[pos as usize][idx as usize])
                        }
                        None => 0,
                    };
                    dictionary_word_for_unknown(
                        &view, input, chars, node_start, node_end, prev_right, next_left,
                    )?
                }
                _ => None,
            };
            out.push(match (node.unknown_type(), dict_word) {
                (None, _) => {
                    self.tokens
                        .known(dict, node.entry, surface, start, end, node.word_cost)?
                }
                (Some(_), Some(entry)) => {
                    self.tokens
                        .known(dict, entry, surface, start, end, node.word_cost)?
                }
                (Some(type_idx), None) => {
                    self.tokens
                        .unknown(dict, type_idx, surface, start, end, node.word_cost)
                }
            });
        }
        apply_contextual_readings(&mut out[first..]);
        Ok(())
    }
}

/// ノードの左文脈 ID（未知語は文字種のテンプレートのもの）
fn left_id(view: &DictView<'_>, dict: &Dictionary, node: &Node) -> u16 {
    match node.unknown_type() {
        Some(type_idx) => dict.unk_info(type_idx).left_id,
        // エントリはラティスを作るときに範囲を確かめてある
        None => view.entries[node.entry as usize].left_id(),
    }
}

/// カタカナの未知語 [start, end)（文字位置）と同じ表層の辞書の語があれば、その語のエントリのうち
/// 前の語の right_id・次の語の left_id との接続コストと単語コストの和が最小のもの（同点は辞書の順で先）
///
/// 単語コストだけで選ぶと、同じ表層の品詞の違うエントリ（名詞と接尾など）で前後とつながらないものを
/// 選びうる。ラティスの上では未知語のコスト・文脈 ID で最良パスに残ったので、分け方は変えない。
fn dictionary_word_for_unknown(
    view: &DictView<'_>,
    input: &str,
    chars: &ChunkChars,
    start: usize,
    end: usize,
    prev_right: u16,
    next_left: u16,
) -> Result<Option<u32>, DictError> {
    let mut group = None;
    view.trie
        .common_prefix_search_codes(input, &chars.codes, &chars.offsets, start, |q, g| {
            if q == end {
                group = Some(g);
            }
        })?;
    let Some(group) = group else {
        return Ok(None);
    };
    let next_row = view.matrix_row(next_left);
    let mut best: Option<(i64, u32)> = None;
    let mut id = group as usize;
    loop {
        // 未知語の開始位置から辞書を引いたときに、群のエントリと文脈 ID の範囲を確かめてある
        let e = &view.entries[id];
        let score = view.matrix_row(e.left_id())[prev_right as usize] as i64
            + e.cost as i64
            + next_row[e.right_id as usize] as i64;
        if best.is_none_or(|(b, _)| score < b) {
            best = Some((score, id as u32));
        }
        if e.is_last() {
            break;
        }
        id += 1;
    }
    Ok(best.map(|(_, id)| id))
}

impl Default for LatticeWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;
    use crate::dict::{ConnectionMatrix, DictBuilder, DictEntry};

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
        assert_eq!(ws.lattice.ends.capacity(), 1024);
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

    fn describe(tokens: &[Token]) -> Vec<(String, usize, usize, String, i16, bool)> {
        tokens
            .iter()
            .map(|t| {
                (
                    t.surface.to_string(),
                    t.start,
                    t.end,
                    t.pos.to_string(),
                    t.word_cost,
                    t.is_known,
                )
            })
            .collect()
    }

    #[test]
    fn test_tokenize_into_appends_with_offset() {
        let dict = make_test_dict();
        let mut ws = LatticeWorkspace::new();
        let expected = ws.tokenize("東京都に住んでいる", &dict).unwrap();

        let mut out = ws.tokenize("住んでいる", &dict).unwrap();
        let before = describe(&out);
        ws.tokenize_into("東京都に住んでいる", &dict, 100, &mut out)
            .unwrap();
        // 前にあったトークンはそのまま、後ろに位置をずらして足す
        assert_eq!(describe(&out[..before.len()]), before);
        let appended = &out[before.len()..];
        assert_eq!(appended.len(), expected.len());
        for (a, e) in appended.iter().zip(&expected) {
            assert_eq!((a.start, a.end), (e.start + 100, e.end + 100));
            assert_eq!(&*a.surface, &*e.surface);
        }
        // 空の入力は何も足さない
        ws.tokenize_into("", &dict, 5, &mut out).unwrap();
        assert_eq!(out.len(), before.len() + expected.len());
    }

    #[test]
    fn test_workspace_reuse_across_lengths() {
        // 長い入力（ノード列を残す上限を超える長さを含む）と短い入力を交互に解析しても、
        // 新しいワークスペースで解析したときと同じ結果になる
        let dict = make_test_dict();
        let unit = "東京都に住んでいるXYZ。";
        let unit_chars = unit.chars().count();
        let inputs: Vec<String> = [0, 3, 1, 100, 1, ENDS_RETAINED / unit_chars + 2, 2, 50, 0]
            .iter()
            .map(|&n| format!("{}住む", unit.repeat(n)))
            .collect();
        let mut ws = LatticeWorkspace::new();
        for input in inputs.iter().chain(inputs.iter().rev()) {
            let expected = describe(&LatticeWorkspace::new().tokenize(input, &dict).unwrap());
            assert_eq!(describe(&ws.tokenize(input, &dict).unwrap()), expected);
            assert!(ws.lattice.ends.len() > input.chars().count());
        }
        // 長い入力のあとも、残すノード列は上限まで
        ws.tokenize("住む", &dict).unwrap();
        assert!(ws.lattice.ends.len() <= ENDS_RETAINED);
    }

    #[test]
    fn test_token_strings_are_not_shared_between_workspaces() {
        // 品詞・活用・空文字列の Arc は解析器ごとの写し。別の解析器（別のスレッド）と参照カウントを
        // 取り合わない。同じ解析器の中では使い回す
        let dict = make_test_dict();
        let mut a = LatticeWorkspace::new();
        let mut b = LatticeWorkspace::new();
        let ta = a.tokenize("東京都XYZ", &dict).unwrap();
        let tb = b.tokenize("東京都XYZ", &dict).unwrap();
        let ta2 = a.tokenize("東京都XYZ", &dict).unwrap();
        for i in 0..ta.len() {
            assert_eq!(ta[i].pos, tb[i].pos);
            assert!(!Arc::ptr_eq(&ta[i].pos, &tb[i].pos));
            assert!(!Arc::ptr_eq(&ta[i].conj_type, &tb[i].conj_type));
            assert!(Arc::ptr_eq(&ta[i].pos, &ta2[i].pos));
            assert!(Arc::ptr_eq(&ta[i].conj_type, &ta2[i].conj_form));
        }
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

    /// (表層形, 単語コスト, 品詞) の辞書。文脈 ID はすべて 1 で接続コストは 0、未知語はテンプレートの
    /// 既定値（カタカナは 5000、英字は 6000）
    fn cost_dict(words: &[(&str, i16, &str)]) -> Dictionary {
        let mut builder = DictBuilder::new();
        for &(surface, cost, pos) in words {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: 1,
                right_id: 1,
                cost,
                pos: pos.into(),
                base_form: surface.into(),
                reading: surface.into(),
                pronunciation: surface.into(),
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    /// 表層形の列（未知語は末尾に *）
    fn segments(dict: &Dictionary, text: &str) -> Vec<String> {
        LatticeWorkspace::new()
            .tokenize(text, dict)
            .unwrap()
            .iter()
            .map(|t| format!("{}{}", t.surface, if t.is_known { "" } else { "*" }))
            .collect()
    }

    const NOUN: &str = "名詞,一般,*,*";

    #[test]
    fn test_katakana_compound_of_long_words_is_split() {
        // 並び全体の未知語（5000）は既知語 2 語（6000）より安いが、3 字以上の語で覆えるので作らない
        let dict = cost_dict(&[("オススメ", 3000, NOUN), ("アプリ", 3000, NOUN)]);
        assert_eq!(segments(&dict, "オススメアプリ"), ["オススメ", "アプリ"]);
    }

    #[test]
    fn test_katakana_word_over_short_fragments_stays_unknown() {
        // 2 字の語は数えないので、辞書にない人名は断片に割れない
        let dict = cost_dict(&[("ドミ", 3000, NOUN), ("ニク", 3000, NOUN)]);
        assert_eq!(segments(&dict, "ドミニク"), ["ドミニク*"]);
    }

    #[test]
    fn test_katakana_cover_may_cross_the_run_boundaries() {
        // 並びの左の境界をまたぐ語（音ゲー）から覆う。並びの途中から始まる候補（ーアプリ）も作らない
        let dict = cost_dict(&[
            ("音", 1000, NOUN),
            ("音ゲー", 3500, NOUN),
            ("アプリ", 3000, NOUN),
        ]);
        assert_eq!(segments(&dict, "音ゲーアプリ"), ["音ゲー", "アプリ"]);
        // 右の境界をまたぐ語（スパッと）
        let dict = cost_dict(&[("スパッと", 6000, "副詞,一般,*,*"), ("と", 500, NOUN)]);
        assert_eq!(segments(&dict, "スパッと"), ["スパッと"]);
    }

    #[test]
    fn test_katakana_unknown_takes_features_of_the_same_dictionary_word() {
        // 単語コストの高い既知語（IPAdic の「キャンプ」）は未知語に負ける。分け方はそのまま、素性は辞書の語
        let dict = cost_dict(&[("キャンプ", 9000, "名詞,サ変接続,*,*")]);
        let tokens = LatticeWorkspace::new().tokenize("キャンプ", &dict).unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(tokens[0].is_known);
        assert_eq!(&*tokens[0].pos, "名詞,サ変接続,*,*");
        assert_eq!(&*tokens[0].reading, "キャンプ");
        // 単語コストはラティスで使った未知語のもの
        assert_eq!(tokens[0].word_cost, 5000);
        // 2 字の未知語はそのまま
        let dict = cost_dict(&[("ナビ", 9000, NOUN)]);
        assert_eq!(segments(&dict, "ナビ"), ["ナビ*"]);
    }

    #[test]
    fn test_katakana_unknown_picks_the_entry_that_fits_the_context() {
        // 同じ表層のエントリは、前後の語との接続コストと単語コストの和で選ぶ（単語コストだけなら固有名詞）
        let mut builder = DictBuilder::new();
        for (surface, id, cost, pos) in [
            ("キャンプ", 10, 9000, "名詞,サ変接続,*,*"),
            ("キャンプ", 11, 8900, "名詞,固有名詞,一般,*"),
            ("する", 12, 1000, "動詞,自立,*,*"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: id,
                right_id: id,
                cost,
                pos: pos.into(),
                base_form: surface.into(),
                ..Default::default()
            });
        }
        let mut matrix = ConnectionMatrix::zeros(13, 13);
        // 「キャンプ」の右文脈 → 「する」の左文脈 12
        matrix.costs[12 * 13 + 10] = -500;
        matrix.costs[12 * 13 + 11] = 500;
        builder.set_matrix(matrix);
        let dict = builder.build().unwrap();
        let tokens = LatticeWorkspace::new()
            .tokenize("キャンプする", &dict)
            .unwrap();
        let got: Vec<(&str, &str, bool)> = tokens
            .iter()
            .map(|t| (&*t.surface, &*t.pos, t.is_known))
            .collect();
        assert_eq!(
            got,
            [
                ("キャンプ", "名詞,サ変接続,*,*", true),
                ("する", "動詞,自立,*,*", true)
            ]
        );
    }

    #[test]
    fn test_cover_rules_are_only_for_katakana() {
        // 英字の並びは、3 字以上の既知語で覆えても 1 つの未知語のまま（SoftBank を Soft / Bank に割らない）
        let dict = cost_dict(&[("Soft", 3500, NOUN), ("Bank", 3500, NOUN)]);
        assert_eq!(segments(&dict, "SoftBank"), ["SoftBank*"]);
    }

    #[test]
    fn test_long_run_is_one_unknown_word() {
        // MeCab は 25 字を超える並びを 1 つの候補にしないが、hasami は長さによらず 1 語にする
        let dict = make_test_dict();
        let id = "ookhcbgokankfmjafalglpofmolfopek";
        assert_eq!(segments(&dict, id), [format!("{id}*")]);
        let katakana = "カ".repeat(40);
        assert_eq!(segments(&dict, &katakana), [format!("{katakana}*")]);
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
