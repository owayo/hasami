//! 辞書の構築（`build` feature）: MeCab 形式 CSV・matrix.def・char.def・unk.def の読み込み、repair、書き出し

use super::sentence_like::{self, SentenceLikeReason};
use super::{ConnectionMatrix, DictEntry, UnkEntry};
use crate::analyzer::Analyzer;
use crate::char_class::{CharClass, CharClassifier};
use crate::hsd::writer::{self, DictSource};
use crate::hsd::{DictError, Dictionary, Meta, WriteOptions, WriteStats};
use crate::lattice::Token;
use std::collections::HashMap;
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

/// MeCab 形式 CSV の 1 エントリの列数
const MECAB_CSV_COLUMNS: usize = 13;

/// MeCab 形式 CSV の行がコメントかどうか
///
/// `#` で始まり、エントリの列数に満たない行をコメントとみなす。NEologd には `#` で始まる
/// ハッシュタグの語（13 列そろったエントリ）があるので、先頭の文字だけでは判定しない。
fn is_comment_record(record: &csv::StringRecord) -> bool {
    record.len() < MECAB_CSV_COLUMNS && record.get(0).is_some_and(|f| f.starts_with('#'))
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

/// 削除リストのうち、どのエントリにも一致しなかった行を記録する件数
const UNMATCHED_SAMPLE: usize = 10;

/// 文脈 ID の範囲外エラーに載せるエントリの件数
const INVALID_CONTEXT_ID_SAMPLE: usize = 5;

/// 品詞 `pos` が `prefix` の要素列で始まるかどうか判定する
///
/// `,` で区切った要素単位の前方一致をとる。文字列としての前方一致ではないので、
/// `名詞,固有名詞,人` は `名詞,固有名詞,人名,姓` に一致しない。`prefix` が空なら常に真。
fn pos_has_prefix<S: AsRef<str>>(pos: &str, prefix: &[S]) -> bool {
    let mut parts = pos.split(',');
    prefix.iter().all(|p| parts.next() == Some(p.as_ref()))
}

/// 固有名詞の降格で、語末に来てよい接尾辞（IPAdic で「名詞,接尾」の語）
///
/// NEologd の「名詞,固有名詞,一般」には、IPAdic で「一般名詞 + 接尾辞」に分かれる語が 3.7 万あるが、
/// 接尾辞の品詞だけでは一般語と固有名詞を分けられない。「名詞,接尾,一般」にも「〜線」（路線名）
/// 「〜法」（法律名）「〜院」（寺院名）「〜会」（団体名）「〜社」「〜賞」のように固有名詞を作る語が多い。
/// そこで一般名詞を作る接尾辞だけを表層形で挙げる。
///
/// 選び方: SudachiDict で同じ表層形が普通名詞か固有名詞かを参照して候補を絞り、推奨辞書で降格される
/// 表層形を接尾辞ごとに全件か 50 件の標本で目で見て、固有名詞が混ざらないものを残した。
/// 例外として「〜力」（185 語中、力士名「北勝力」・社名「格力」など 4% 前後）と「〜度」（85 語中、
/// 人名「公孫度」など 2〜3%）は入れた。誤って降格しても読みは変わらず、「説得力」「影響力」
/// 「満足度」「理解度」のような抽象語を拾える方が大きい。
/// 外したもの: 「〜論」（「国富論」「資本論」など著作名が 1 割近い）、「〜型」（「吹雪型」「秋月型」
/// など艦級名が 1 割）、「〜系」（「ナスルーラ系」など競走馬の父系名が 1 割強）、「〜書」（「唐書」
/// 「梁書」）、「〜式」（「公文式」「ねじ式」）。「〜機」「〜車」「〜家」「〜士」「〜師」「〜虫」は
/// 境界を確かめていないので入れていない
const COMMON_NOUN_SUFFIXES: [&str; 25] = [
    "的", "化", "性", "者", "物", "学", "力", "率", "感", "度", "費", "料", "権", "症", "制", "体",
    "器", "業", "剤", "員", "官", "数", "罪", "病", "術",
];

/// 固有名詞の降格で、語の途中に来てよい接尾辞（「心理/的/安全/性」の「的」）
const INNER_COMMON_NOUN_SUFFIX: &str = "的";

/// 固有名詞の降格で、ログに出す降格例の件数
const DEMOTION_SAMPLE: usize = 20;

/// 一般語の固有名詞を降格する先の品詞
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DemotedPos {
    /// 名詞,一般（「成果物」「安全性」「担当者」）
    General,
    /// 名詞,サ変接続（語末が「化」。「可視化」「言語化」）
    Sahen,
    /// 名詞,形容動詞語幹（語末が「的」。「多角的」「包括的」）
    AdjectivalStem,
}

impl DemotedPos {
    const ALL: [DemotedPos; 3] = [Self::General, Self::Sahen, Self::AdjectivalStem];

    /// 品詞の文字列（IPAdic のエントリと同じ 4 要素）
    fn pos(self) -> &'static str {
        match self {
            Self::General => "名詞,一般,*,*",
            Self::Sahen => "名詞,サ変接続,*,*",
            Self::AdjectivalStem => "名詞,形容動詞語幹,*,*",
        }
    }

    /// 語末の接尾辞から降格先を決める
    fn for_suffix(suffix: &str) -> Self {
        match suffix {
            "化" => Self::Sahen,
            "的" => Self::AdjectivalStem,
            _ => Self::General,
        }
    }
}

/// 一般名詞の語基（接尾辞の前に来てよい品詞）かどうか
fn is_common_noun_stem(pos: &str) -> bool {
    pos_has_prefix(pos, &["名詞", "一般"])
        || pos_has_prefix(pos, &["名詞", "サ変接続"])
        || pos_has_prefix(pos, &["名詞", "形容動詞語幹"])
}

/// 参照辞書（IPAdic 単体）の解析結果から、固有名詞の降格先を決める
///
/// 次をすべて満たすときだけ降格する（満たさなければ None）。
/// 1. 2 語以上に分かれ、すべて既知語で、表層形を過不足なく覆う（英字などの未知語を含む語は
///    「AACTA賞」のような名称が多いので除く）
/// 2. 最後の語が「名詞,接尾」で、表層形が [`COMMON_NOUN_SUFFIXES`] にある
/// 3. それより前は一般名詞・サ変接続・形容動詞語幹の連続。途中の接尾辞は「的」だけを許し、
///    前後に語基を置く（「心理/的/安全/性」は通し、接尾辞が続く列は通さない）
///
/// Returns: 降格先の品詞と語末の接尾辞
fn demotion_target(surface: &str, tokens: &[Token]) -> Option<(DemotedPos, &'static str)> {
    let (last, body) = tokens.split_last()?;
    if body.is_empty() || tokens.iter().any(|t| !t.is_known) {
        return None;
    }
    let mut rest = surface;
    for t in tokens {
        rest = rest.strip_prefix(&*t.surface)?;
    }
    if !rest.is_empty() || !pos_has_prefix(&last.pos, &["名詞", "接尾"]) {
        return None;
    }
    let suffix = COMMON_NOUN_SUFFIXES
        .iter()
        .copied()
        .find(|s| *s == &*last.surface)?;
    // 直前の語が語基か（語頭では偽。途中の「的」の前後と、最後の接尾辞の前に語基を求める）
    let mut after_stem = false;
    for t in body {
        if is_common_noun_stem(&t.pos) {
            after_stem = true;
        } else if after_stem
            && &*t.surface == INNER_COMMON_NOUN_SUFFIX
            && pos_has_prefix(&t.pos, &["名詞", "接尾"])
        {
            after_stem = false;
        } else {
            return None;
        }
    }
    after_stem.then(|| (DemotedPos::for_suffix(suffix), suffix))
}

/// 参照辞書で表層形を解析するときのスレッド数の上限
const ANALYSIS_THREADS_MAX: usize = 16;

/// 表層形をそれぞれ参照辞書で解析し、語の列から `judge` で求めた値を表層形の順に返す
///
/// repair の判定（固有名詞の降格、文や句の名詞の削除）は推奨辞書の表層形を 100 万単位で解析するので、
/// スレッドに分けて解析する。
fn analyze_surfaces<T, F>(
    reference: &Arc<Dictionary>,
    surfaces: &[&str],
    judge: F,
) -> Result<Vec<T>, DictError>
where
    T: Send,
    F: Fn(&str, &[Token]) -> T + Sync,
{
    if surfaces.is_empty() {
        return Ok(Vec::new());
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(ANALYSIS_THREADS_MAX);
    let chunk = surfaces.len().div_ceil(threads);
    let judge = &judge;
    std::thread::scope(|scope| {
        let workers: Vec<_> = surfaces
            .chunks(chunk)
            .map(|part| {
                let mut analyzer = Analyzer::from_shared(Arc::clone(reference));
                scope.spawn(move || {
                    part.iter()
                        .map(|surface| {
                            let tokens = analyzer.try_tokenize(surface)?;
                            Ok(judge(surface, &tokens))
                        })
                        .collect::<Result<Vec<_>, DictError>>()
                })
            })
            .collect();
        let mut out = Vec::with_capacity(surfaces.len());
        for worker in workers {
            out.extend(worker.join().expect("analysis worker panicked")?);
        }
        Ok(out)
    })
}

/// 降格先の品詞ごとの文脈 ID と件数
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemotedPosStats {
    /// 降格先の品詞（`名詞,一般,*,*` など）
    pub pos: String,
    /// 付け替えた左文脈 ID（参照辞書でこの品詞のエントリが最も多く使う組）
    pub left_id: u16,
    /// 付け替えた右文脈 ID
    pub right_id: u16,
    /// この品詞に降格したエントリ数
    pub entries: usize,
}

/// [`DictBuilder::demote_common_proper_nouns`] の結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DemotionStats {
    /// 判定したエントリ数（品詞が「名詞,固有名詞,一般」のもの）
    pub examined: usize,
    /// 降格したエントリ数
    pub demoted: usize,
    /// 降格先の品詞ごとの内訳（名詞,一般 → 名詞,サ変接続 → 名詞,形容動詞語幹 の順）
    pub by_pos: Vec<DemotedPosStats>,
    /// 語末の接尾辞ごとの降格件数（多い順）
    pub by_suffix: Vec<(String, usize)>,
    /// 降格したエントリの先頭の数件（`表層形 (読み) -> 品詞`）
    pub samples: Vec<String>,
}

/// 文や句の名詞の削除で、ログに出す例の件数（理由ごと）
const SENTENCE_LIKE_SAMPLE: usize = 3;

/// [`DictBuilder::drop_sentence_like_nouns`] の結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SentenceLikeStats {
    /// 判定したエントリ数（表層形にひらがなか文末記号を含む名詞）
    pub examined: usize,
    /// 削除したエントリ数
    pub dropped: usize,
    /// 理由ごとの削除件数（[`SentenceLikeReason::ALL`] の順。0 件の理由も含む）
    pub by_reason: Vec<(SentenceLikeReason, usize)>,
    /// 削除したエントリの例（理由ごとに先頭の数件、理由の順。`表層形 (読み) [品詞]`）
    pub samples: Vec<(SentenceLikeReason, String)>,
}

/// 数と単位の組の名詞の削除で、ログに出す例の件数
const QUANTITY_SAMPLE: usize = 10;

/// [`DictBuilder::drop_quantity_nouns`] の結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuantityStats {
    /// 判定したエントリ数（表層形が数と単位の記号だけの「名詞,固有名詞,一般」）
    pub examined: usize,
    /// 削除したエントリ数
    pub dropped: usize,
    /// 削除したエントリの先頭の数件（`表層形 (読み) [品詞]`）
    pub samples: Vec<String>,
}

/// 数の字（半角・全角の数字）
fn is_digit(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}

/// 表層形が数と単位の記号だけか（`50%` `0.1℃` `1,000㎞` `30°C`）
///
/// 数は数字の並びで、間に小数点・桁区切り（`.` `,` `．` `，`）を挟んでよい。単位の記号は
/// `scripts/prepare_ipadic.py` が IPAdic に 名詞,接尾,助数詞 の語として足す字（[`crate::pos`] と同じ集合）と、
/// 2 字の単位 `°C` `°F`。
fn is_quantity_surface(surface: &str) -> bool {
    let unit_start = surface
        .char_indices()
        .find(|&(_, c)| !(is_digit(c) || matches!(c, '.' | ',' | '．' | '，')))
        .map_or(surface.len(), |(i, _)| i);
    let (number, unit) = surface.split_at(unit_start);
    number.chars().next().is_some_and(is_digit)
        && number.chars().last().is_some_and(is_digit)
        && !unit.is_empty()
        && (unit.chars().all(crate::pos::is_unit_symbol) || matches!(unit, "°C" | "°F"))
}

/// 参照辞書の解析が、数（名詞,数）と単位（名詞,接尾）だけの並びか（`50` + `%`）
///
/// 小数点・桁区切りの記号は数の間に来てよい。IPAdic が単位を接尾辞にしない字なら（配布辞書の IPAdic は
/// 単位の記号をすべて 名詞,接尾,助数詞 にする）、数と単位に分けても意味が無いので落とさない。
fn is_quantity_analysis(_surface: &str, tokens: &[Token]) -> bool {
    let is = |t: &Token, prefix: &[&str]| pos_has_prefix(&t.pos, prefix);
    tokens.iter().any(|t| is(t, &["名詞", "数"]))
        && tokens.last().is_some_and(|t| is(t, &["名詞", "接尾"]))
        && tokens.iter().all(|t| {
            is(t, &["名詞", "数"])
                || is(t, &["名詞", "接尾"])
                || matches!(&*t.surface, "." | "," | "．" | "，")
        })
}

/// 参照辞書に同じ表層形・品詞・原形のエントリがあるか
///
/// 文や句の名詞の削除は、参照辞書（IPAdic）の語の切り方を正とするので、参照辞書自身の語は落とさない。
/// 読みは比べない（repair の発音の修復が、IPAdic の小数点「．」(名詞,数) のような仮名でない読みを空にする）。
fn reference_has_entry(reference: &Dictionary, e: &DictEntry) -> Result<bool, DictError> {
    Ok(reference
        .lookup(&e.surface)?
        .into_iter()
        .any(|(end, entries)| {
            end == e.surface.len()
                && entries
                    .iter()
                    .any(|r| r.pos == e.pos && r.base_form == e.base_form)
        }))
}

/// 参照辞書にある接尾辞の読み（固有名詞のエントリの読みを除く）
///
/// 降格するエントリの読みは、語末の接尾辞のこの読みのどれかで終わらなければならない
/// （「力」ならリョク・リキ・チカラ）。「こだま学(コダママナブ)」「かわら力(カワラツトム)」のような
/// 人名や、「鉄道員(ポッポヤ)」「漂泊者(アウトロー)」のような作品名は接尾辞を字どおりに読まないので、
/// これで除ける。固有名詞かどうかを決める条件ではなく候補を絞る条件で、「目力(メヂカラ)」
/// 「兄者(アニジャ)」のように連濁などで読みが変わる一般語も降格しない。
fn common_suffix_readings(
    reference: &Dictionary,
) -> Result<HashMap<&'static str, Vec<Arc<str>>>, DictError> {
    let mut out = HashMap::new();
    for suffix in COMMON_NOUN_SUFFIXES {
        let mut readings: Vec<Arc<str>> = Vec::new();
        for (end, entries) in reference.lookup(suffix)? {
            if end != suffix.len() {
                continue;
            }
            for e in entries {
                if !e.reading.is_empty()
                    && !pos_has_prefix(&e.pos, &["名詞", "固有名詞"])
                    && !readings.contains(&e.reading)
                {
                    readings.push(e.reading);
                }
            }
        }
        out.insert(suffix, readings);
    }
    Ok(out)
}

/// 参照辞書で降格先の品詞のエントリが最も多く使う (left_id, right_id) の組を求める
///
/// 左右を別々に数えると実在しない組を作りうるので、組で数える。同数なら ID の小さい組を採る。
fn demoted_context_ids(
    reference: &Dictionary,
) -> Result<HashMap<DemotedPos, (u16, u16)>, DictError> {
    let mut counts: HashMap<(DemotedPos, u16, u16), usize> = HashMap::new();
    reference.for_each_entry(|e| {
        if let Some(&target) = DemotedPos::ALL.iter().find(|p| *e.pos == *p.pos()) {
            *counts.entry((target, e.left_id, e.right_id)).or_default() += 1;
        }
        Ok(())
    })?;
    let mut best: HashMap<DemotedPos, (usize, u16, u16)> = HashMap::new();
    for ((target, left, right), n) in counts {
        let slot = best.entry(target).or_insert((n, left, right));
        if (n, std::cmp::Reverse((left, right))) > (slot.0, std::cmp::Reverse((slot.1, slot.2))) {
            *slot = (n, left, right);
        }
    }
    DemotedPos::ALL
        .iter()
        .map(|&target| {
            best.get(&target)
                .map(|&(_, left, right)| (target, (left, right)))
                .ok_or_else(|| {
                    DictError::invalid(format!(
                        "the reference dictionary has no `{}` entries to take context IDs from; \
                         pass an IPAdic dictionary",
                        target.pos()
                    ))
                })
        })
        .collect()
}

/// [`DictBuilder::drop_entries_from_csv`] で CSV を適用した結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemovalStats {
    /// 削除条件として読んだ行数（コメント行・空行を除く）
    pub rows: usize,
    /// 削除したエントリ数
    pub dropped: usize,
    /// どのエントリにも一致しなかった行数
    pub unmatched_rows: usize,
    /// 一致しなかった行の先頭の数件（`行番号: 行の内容`）
    pub unmatched_samples: Vec<String>,
}

/// 削除リスト CSV の 1 行
struct RemovalRule {
    line_no: u64,
    text: String,
    /// 品詞の接頭辞（`,` で区切った要素）。空なら品詞を問わない
    pos_prefix: Vec<String>,
}

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

/// EUC-JP の変換表で写し先が分かれる字: (WHATWG・CP932 側, JIS・iconv 側)
const EUC_JP_AMBIGUOUS: [(char, char); 7] = [
    ('\u{2015}', '\u{2014}'), // ― → — (0xA1BD)
    ('\u{FF5E}', '\u{301C}'), // ～ → 〜 (0xA1C1)
    ('\u{2225}', '\u{2016}'), // ∥ → ‖ (0xA1C2)
    ('\u{FF0D}', '\u{2212}'), // － → − (0xA1DD)
    ('\u{FFE0}', '\u{00A2}'), // ￠ → ¢ (0xA1F1)
    ('\u{FFE1}', '\u{00A3}'), // ￡ → £ (0xA1F2)
    ('\u{FFE2}', '\u{00AC}'), // ￢ → ¬ (0xA2CC)
];

/// encoding_rs が EUC-JP から写した字を JIS の対応表の字にする
fn euc_jp_jis_side(c: char) -> char {
    EUC_JP_AMBIGUOUS
        .iter()
        .find(|(whatwg, _)| *whatwg == c)
        .map_or(c, |&(_, jis)| jis)
}

/// 辞書ビルダー: MeCab形式のCSVから辞書を構築
pub struct DictBuilder {
    entries: Vec<DictEntry>,
    matrix: Option<ConnectionMatrix>,
    char_classifier: CharClassifier,
    unk_entries: HashMap<String, Vec<UnkEntry>>,
    /// 読み込んだ辞書のメタデータ（repair・merge で引き継ぐ）
    meta: Option<Meta>,
    /// 品詞・活用型・活用形の Arc を共有するための表（種類が少なく、エントリごとに複製するとメモリを食う）
    interned: HashMap<Box<str>, Arc<str>>,
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
            meta: None,
            interned: HashMap::new(),
        }
    }

    /// 品詞・活用型・活用形の文字列を共有の Arc にする
    fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(arc) = self.interned.get(s) {
            return Arc::clone(arc);
        }
        let arc: Arc<str> = Arc::from(s);
        self.interned.insert(s.into(), Arc::clone(&arc));
        arc
    }

    /// 既存の .hsd 辞書を取り込む（merge・repair 用）
    ///
    /// エントリ、接続行列（matrix.def なしで作った辞書のゼロ行列は取り込まない）、
    /// 文字種定義、未知語テンプレート、メタデータを取り込む。支配エントリを除いた最終辞書は
    /// 消した候補を復元できないので取り込めない（除去前の中間辞書か上流の CSV から作り直す）。
    pub fn load_hsd<P: AsRef<Path>>(&mut self, path: P) -> Result<(), DictError> {
        let path = path.as_ref();
        let dict = Dictionary::load(path)?;
        if dict.is_pruned() {
            return Err(DictError::invalid(format!(
                "{} is a final dictionary with dominated entries pruned; \
                 repair or merge the intermediate dictionary (built without --prune-dominated) \
                 or rebuild it from the source CSV",
                path.display()
            )));
        }
        let before = self.entries.len();
        self.entries.reserve(dict.entry_count());
        dict.for_each_entry(|entry| {
            self.entries.push(entry);
            Ok(())
        })?;

        if self.matrix.is_none() {
            self.matrix = dict.connection_matrix();
        }
        for (class, templates) in dict.unk_entries() {
            self.unk_entries.entry(class).or_insert(templates);
        }
        self.char_classifier = dict.char_classifier();
        if self.meta.is_none() {
            self.meta = Some(dict.meta().clone());
        }

        eprintln!(
            "Imported {} entries from existing dictionary",
            self.entries.len() - before
        );
        Ok(())
    }

    /// 取り込んだ辞書のメタデータ（`load_hsd` していなければ None）
    pub fn meta(&self) -> Option<&Meta> {
        self.meta.as_ref()
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
            let best = trusted
                .entry((&entry.surface, &entry.reading))
                .or_insert(&entry.pronunciation);
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
            } else if entry.pos.starts_with("記号") {
                // 記号（「、」「。」「「」等）は読み・発音に記号そのものを持つ（MeCab・OpenJTalk と同じ）。
                // 空にすると、句読点を発音の並びで見分ける読み上げの前処理が句読点を見失う
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
    /// 汎用のフィルタ（[`Self::drop_conflicting_ortho_variants`] 等）では拾えない個別の
    /// 誤読エントリや、外国人名のように判定を別に済ませた語の集合を落とすために使う。
    ///
    /// CSV は `表層形,読み[,品詞]` で、4 列目以降は無視する。3 列目に品詞を書くと、
    /// その品詞で始まるエントリだけを削除する。品詞は `"名詞,固有名詞,人名"` のように
    /// 引用符で 1 つの列にまとめ、`,` で区切った要素単位で前方一致をとる
    /// （`名詞,固有名詞,人` は `名詞,固有名詞,人名,姓` に一致しない）。3 列目が無いか空なら
    /// 品詞を問わない。`#` で始まる行と空行は読み飛ばす。
    ///
    /// Returns: 読んだ行数・削除したエントリ数・どのエントリにも一致しなかった行
    pub fn drop_entries_from_csv<P: AsRef<Path>>(&mut self, path: P) -> io::Result<RemovalStats> {
        let path = path.as_ref();
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);

        // 表層形 → 読み → その組に対応する行の添字
        let mut rules: Vec<RemovalRule> = Vec::new();
        let mut index: HashMap<String, HashMap<String, Vec<usize>>> = HashMap::new();
        // 行番号を正確に出すため 1 行ずつ読む (CSV リーダーの行位置は空行やコメント行を数えない)。
        // 削除リストは小さく、1 つの値が行をまたぐこともない
        for (idx, line) in content.lines().enumerate() {
            let line_no = (idx + 1) as u64;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .trim(csv::Trim::All)
                .from_reader(line.as_bytes());
            let record = match rdr.records().next() {
                Some(Ok(record)) => record,
                Some(Err(e)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{}:{}: CSV parse error: {}", path.display(), line_no, e),
                    ));
                }
                None => continue,
            };
            if record.len() < 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{}:{}: expected `surface,reading[,pos]`: {}",
                        path.display(),
                        line_no,
                        &record[0]
                    ),
                ));
            }
            let pos = record.get(2).unwrap_or("");
            let text = if pos.is_empty() {
                format!("{},{}", &record[0], &record[1])
            } else {
                format!("{},{},\"{}\"", &record[0], &record[1], pos)
            };
            let pos_prefix = if pos.is_empty() {
                Vec::new()
            } else {
                pos.split(',').map(|p| p.trim().to_string()).collect()
            };
            index
                .entry(record[0].to_string())
                .or_default()
                .entry(record[1].to_string())
                .or_default()
                .push(rules.len());
            rules.push(RemovalRule {
                line_no,
                text,
                pos_prefix,
            });
        }

        let mut matched = vec![false; rules.len()];
        let before = self.entries.len();
        self.entries.retain(|e| {
            let Some(ids) = index.get(&*e.surface).and_then(|m| m.get(&*e.reading)) else {
                return true;
            };
            let mut keep = true;
            for &i in ids {
                if pos_has_prefix(&e.pos, &rules[i].pos_prefix) {
                    matched[i] = true;
                    keep = false;
                }
            }
            keep
        });

        let mut stats = RemovalStats {
            rows: rules.len(),
            dropped: before - self.entries.len(),
            ..RemovalStats::default()
        };
        for (rule, _) in rules.iter().zip(&matched).filter(|(_, m)| !**m) {
            stats.unmatched_rows += 1;
            if stats.unmatched_samples.len() < UNMATCHED_SAMPLE {
                stats
                    .unmatched_samples
                    .push(format!("{}: {}", rule.line_no, rule.text));
            }
        }
        Ok(stats)
    }

    /// 一般語に付いた「名詞,固有名詞,一般」を一般名詞に降格する
    ///
    /// NEologd は「成果物」「多角的」「可視化」「安全性」「担当者」のような一般語を
    /// 「名詞,固有名詞,一般」で登録している。固有名詞を具体性の手掛かりに数える処理
    /// （文章の Linter 等）では、抽象的な文が具体的に見えてしまう。
    ///
    /// `reference`（IPAdic 単体の辞書）で表層形を解析し、「一般名詞・サ変接続・形容動詞語幹の
    /// 連続 + 一般名詞を作る接尾辞」に分かれ、読みが接尾辞の読みで終わるエントリを、`名詞,一般`
    /// （語末が「化」なら `名詞,サ変接続`、「的」なら `名詞,形容動詞語幹`）に変える。判定の条件は
    /// `demotion_target` と `common_suffix_readings`、接尾辞の一覧は `COMMON_NOUN_SUFFIXES` を参照。
    /// 人名・地域・組織（固有名詞の他の下位分類）は対象にしない。
    ///
    /// 品詞に合わせて文脈 ID も付け替える（参照辞書でその品詞のエントリが最も多く使う組）。
    /// コスト・原形・読み・発音は変えない。文脈 ID を持ち込むので、参照辞書はこの辞書と同じ
    /// 接続行列を持っていなければならない（違えばエラー）。
    pub fn demote_common_proper_nouns(
        &mut self,
        reference: &Arc<Dictionary>,
    ) -> Result<DemotionStats, DictError> {
        match (&self.matrix, reference.connection_matrix()) {
            (None, _) => {}
            (Some(ours), Some(theirs))
                if ours.num_left == theirs.num_left
                    && ours.num_right == theirs.num_right
                    && ours.costs == theirs.costs => {}
            _ => {
                return Err(DictError::invalid(
                    "the reference dictionary for --demote-common-proper-nouns has a different \
                     connection matrix, so its context IDs mean something else in this dictionary; \
                     pass the IPAdic dictionary this one was built from",
                ));
            }
        }
        let context_ids = demoted_context_ids(reference)?;

        // 表層形ごとに 1 回だけ解析する（同じ表層形のエントリが複数ある）
        let mut candidates: Vec<usize> = Vec::new();
        let mut surface_index: HashMap<&str, usize> = HashMap::new();
        let mut surfaces: Vec<&str> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if pos_has_prefix(&e.pos, &["名詞", "固有名詞", "一般"]) {
                candidates.push(i);
                surface_index.entry(&e.surface).or_insert_with(|| {
                    surfaces.push(&e.surface);
                    surfaces.len() - 1
                });
            }
        }
        let targets = analyze_surfaces(reference, &surfaces, demotion_target)?;
        let suffix_readings = common_suffix_readings(reference)?;
        let decisions: Vec<(usize, DemotedPos, &str)> = candidates
            .iter()
            .filter_map(|&i| {
                let e = &self.entries[i];
                let (target, suffix) = targets[surface_index[&*e.surface]]?;
                suffix_readings[suffix]
                    .iter()
                    .any(|r| e.reading.ends_with(&**r))
                    .then_some((i, target, suffix))
            })
            .collect();
        drop(surface_index);
        drop(surfaces);

        let mut stats = DemotionStats {
            examined: candidates.len(),
            demoted: decisions.len(),
            ..DemotionStats::default()
        };
        let mut per_pos: HashMap<DemotedPos, usize> = HashMap::new();
        let mut per_suffix: HashMap<&str, usize> = HashMap::new();
        for &(i, target, suffix) in &decisions {
            *per_pos.entry(target).or_default() += 1;
            *per_suffix.entry(suffix).or_default() += 1;
            if stats.samples.len() < DEMOTION_SAMPLE {
                let e = &self.entries[i];
                stats
                    .samples
                    .push(format!("{} ({}) -> {}", e.surface, e.reading, target.pos()));
            }
        }
        for target in DemotedPos::ALL {
            let (left_id, right_id) = context_ids[&target];
            stats.by_pos.push(DemotedPosStats {
                pos: target.pos().to_string(),
                left_id,
                right_id,
                entries: per_pos.get(&target).copied().unwrap_or(0),
            });
        }
        stats.by_suffix = per_suffix
            .into_iter()
            .map(|(s, n)| (s.to_string(), n))
            .collect();
        stats
            .by_suffix
            .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let pos: HashMap<DemotedPos, Arc<str>> = DemotedPos::ALL
            .iter()
            .map(|&target| (target, self.intern(target.pos())))
            .collect();
        for (i, target, _) in decisions {
            let (left_id, right_id) = context_ids[&target];
            let e = &mut self.entries[i];
            e.pos = Arc::clone(&pos[&target]);
            e.left_id = left_id;
            e.right_id = right_id;
        }
        Ok(stats)
    }

    /// 文や句を 1 語にした名詞を取り除く
    ///
    /// NEologd は曲名・作品名などとして文や句そのもの（「どうでしょう」「作りました」「好きだ。」）を
    /// 固有名詞 1 語で登録し、表記ゆれの seed は漢字語をかなで書いた語（「ありません」= 有馬線、
    /// 「回ろう」= 回廊、「および」= お呼び）を作る。これらが文中の動詞・助動詞・助詞の並びに勝つと、
    /// 文末の品詞や句点の位置が崩れる。
    ///
    /// 表層形にひらがなか文末記号を含む名詞を `reference`（IPAdic 単体の辞書）で解析し、文法に合う文や句
    /// （述語や助詞で終わる並び、機能語だけの並び、感動詞 1 語、記号 + 文末記号 など）になるエントリを
    /// 落とす。判定の規則は [`SentenceLikeReason`] と `sentence_like` モジュールを参照。
    /// `reference` 自身が持つエントリ（同じ表層形・品詞・原形）は落とさない。文脈 ID を持ち込まない
    /// ので、`reference` の接続行列はこの辞書と違ってもよい。
    pub fn drop_sentence_like_nouns(
        &mut self,
        reference: &Arc<Dictionary>,
    ) -> Result<SentenceLikeStats, DictError> {
        // 表層形ごとに 1 回だけ解析する（同じ表層形のエントリが複数ある）
        let mut candidates: Vec<usize> = Vec::new();
        let mut surface_index: HashMap<&str, usize> = HashMap::new();
        let mut surfaces: Vec<&str> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if pos_has_prefix(&e.pos, &["名詞"]) && sentence_like::is_candidate_surface(&e.surface)
            {
                candidates.push(i);
                surface_index.entry(&e.surface).or_insert_with(|| {
                    surfaces.push(&e.surface);
                    surfaces.len() - 1
                });
            }
        }
        let verdicts = analyze_surfaces(reference, &surfaces, sentence_like::judge)?;

        let mut stats = SentenceLikeStats {
            examined: candidates.len(),
            ..SentenceLikeStats::default()
        };
        let mut per_reason: HashMap<SentenceLikeReason, usize> = HashMap::new();
        let mut doomed = vec![false; self.entries.len()];
        for &i in &candidates {
            let e = &self.entries[i];
            let verdict = &verdicts[surface_index[&*e.surface]];
            let Some(reason) = verdict.for_entry(&e.pos, &e.surface, &e.base_form) else {
                continue;
            };
            if reference_has_entry(reference, e)? {
                continue;
            }
            doomed[i] = true;
            let n = per_reason.entry(reason).or_default();
            *n += 1;
            if *n <= SENTENCE_LIKE_SAMPLE {
                stats
                    .samples
                    .push((reason, format!("{} ({}) [{}]", e.surface, e.reading, e.pos)));
            }
        }
        drop(surface_index);
        drop(surfaces);

        stats.dropped = per_reason.values().sum();
        stats.by_reason = SentenceLikeReason::ALL
            .iter()
            .map(|&r| (r, per_reason.get(&r).copied().unwrap_or(0)))
            .collect();
        stats
            .samples
            .sort_by_key(|(r, _)| SentenceLikeReason::ALL.iter().position(|a| a == r));
        let mut doomed = doomed.into_iter();
        self.entries.retain(|_| !doomed.next().unwrap_or(false));
        Ok(stats)
    }

    /// 数と単位の記号だけの表層形を 1 語にした固有名詞を取り除く
    ///
    /// NEologd は「50%」「0.1℃」「30℃」（原形「30度」）のような数と単位の組を「名詞,固有名詞,一般」で
    /// 登録している。これが勝つと単位が数から分かれず、単位を 名詞,接尾,助数詞（[`crate::CoarsePos`] の
    /// NounSuffix）として扱う処理が効かない。
    ///
    /// 表層形が数（小数点・桁区切りを含む）と単位の記号だけの「名詞,固有名詞,一般」のエントリを
    /// `reference`（IPAdic 単体の辞書）で解析し、数（名詞,数）と単位（名詞,接尾）だけの並びに分かれるものを
    /// 落とす。人名・組織（「100%ORANGE」「4℃」のように名前として登録された語）は対象にしない。
    /// `reference` 自身が持つエントリは落とさない。文脈 ID を持ち込まないので、接続行列は問わない。
    pub fn drop_quantity_nouns(
        &mut self,
        reference: &Arc<Dictionary>,
    ) -> Result<QuantityStats, DictError> {
        let mut candidates: Vec<usize> = Vec::new();
        let mut surface_index: HashMap<&str, usize> = HashMap::new();
        let mut surfaces: Vec<&str> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if pos_has_prefix(&e.pos, &["名詞", "固有名詞", "一般"])
                && is_quantity_surface(&e.surface)
            {
                candidates.push(i);
                surface_index.entry(&e.surface).or_insert_with(|| {
                    surfaces.push(&e.surface);
                    surfaces.len() - 1
                });
            }
        }
        let quantities = analyze_surfaces(reference, &surfaces, is_quantity_analysis)?;

        let mut stats = QuantityStats {
            examined: candidates.len(),
            ..QuantityStats::default()
        };
        let mut doomed = vec![false; self.entries.len()];
        for &i in &candidates {
            let e = &self.entries[i];
            if !quantities[surface_index[&*e.surface]] || reference_has_entry(reference, e)? {
                continue;
            }
            doomed[i] = true;
            stats.dropped += 1;
            if stats.samples.len() < QUANTITY_SAMPLE {
                stats
                    .samples
                    .push(format!("{} ({}) [{}]", e.surface, e.reading, e.pos));
            }
        }
        drop(surface_index);
        drop(surfaces);
        let mut doomed = doomed.into_iter();
        self.entries.retain(|_| !doomed.next().unwrap_or(false));
        Ok(stats)
    }

    /// 接続行列の範囲外の文脈 ID を持つエントリを取り除く
    ///
    /// 範囲外の ID は解析時に接続コストが引けず 0 として扱われるため、そのエントリは
    /// 前後の語との接続で不当に有利になる。配布していた統合辞書には、IPAdic の文脈 ID に
    /// 写し替えられず SudachiDict の文脈 ID のまま残った重複エントリがこの形で含まれていた。
    ///
    /// 接続行列が読み込まれていなければ何もしない。
    ///
    /// Returns: 削除されたエントリ数
    pub fn drop_invalid_context_ids(&mut self) -> usize {
        let Some(matrix) = &self.matrix else {
            return 0;
        };
        let doomed: Vec<bool> = self
            .entries
            .iter()
            .map(|e| !matrix.contains_ids(e.left_id, e.right_id))
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
                    "  drop: {} ({}) left_id={} right_id={} [{}]",
                    entry.surface, entry.reading, entry.left_id, entry.right_id, entry.pos
                );
            }
            let mut doomed = doomed.into_iter();
            self.entries.retain(|_| !doomed.next().unwrap_or(false));
        }
        removed
    }

    /// 全エントリと未知語テンプレートの文脈 ID が接続行列の範囲内か検査する
    ///
    /// 範囲外の ID は解析時に接続コスト 0 として扱われ、誤った解析結果を黙って生む。
    /// 辞書を書き出す前にここで止める。接続行列が読み込まれていなければ検査しない。
    pub fn check_context_ids(&self) -> io::Result<()> {
        let Some(matrix) = &self.matrix else {
            return Ok(());
        };
        let mut invalid = 0usize;
        let mut samples: Vec<String> = Vec::new();
        for e in &self.entries {
            if !matrix.contains_ids(e.left_id, e.right_id) {
                invalid += 1;
                if samples.len() < INVALID_CONTEXT_ID_SAMPLE {
                    samples.push(format!(
                        "{} [{}] left_id={} right_id={}",
                        e.surface, e.pos, e.left_id, e.right_id
                    ));
                }
            }
        }
        let mut classes: Vec<&String> = self.unk_entries.keys().collect();
        classes.sort();
        for class in classes {
            for t in &self.unk_entries[class] {
                if !matrix.contains_ids(t.left_id, t.right_id) {
                    invalid += 1;
                    if samples.len() < INVALID_CONTEXT_ID_SAMPLE {
                        samples.push(format!(
                            "unknown-word template {} [{}] left_id={} right_id={}",
                            t.char_class, t.pos, t.left_id, t.right_id
                        ));
                    }
                }
            }
        }
        if invalid == 0 {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} entries have context IDs outside the connection matrix \
                 (left_id < {}, right_id < {}):\n  {}\n\
                 Remove them with `hasami repair --drop-invalid-context-ids`.",
                invalid,
                matrix.num_left,
                matrix.num_right,
                samples.join("\n  ")
            ),
        ))
    }

    /// 接続行列を直接設定
    pub fn set_matrix(&mut self, matrix: ConnectionMatrix) {
        self.matrix = Some(matrix);
    }

    /// CharClassifier を直接設定
    pub fn set_char_classifier(&mut self, classifier: CharClassifier) {
        self.char_classifier = classifier;
    }

    /// 未知語テンプレート（文字種名 → テンプレート）を直接設定
    pub fn set_unk_entries(&mut self, unk_entries: HashMap<String, Vec<UnkEntry>>) {
        self.unk_entries = unk_entries;
    }

    /// MeCab形式のCSVファイルからエントリを追加
    /// 形式: surface,left_id,right_id,cost,pos1,pos2,pos3,pos4,conj_type,conj_form,base_form,reading,pronunciation
    ///
    /// 接続行列を読み込み済みなら、文脈 ID が行列の範囲内かも行ごとに検査する。
    /// 範囲外の行はファイル名と行番号付きのエラーにする（黙って取り込むと、解析時に
    /// 接続コスト 0 として扱われ誤った解析結果を生む）。
    pub fn add_csv<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();

        // ファイルをバイト列として読み込み、エンコーディングを検出
        let raw_bytes = std::fs::read(path)?;
        let content = Self::decode_to_utf8(&raw_bytes);

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(content.as_bytes());

        // エラーに出す行番号。CSV リーダーの行位置は読み飛ばした空行を数えないので、
        // レコードの開始位置までの改行を数えて実際の行番号を求める
        let bytes = content.as_bytes();
        let (mut line_no, mut scanned) = (1usize, 0usize);
        for result in rdr.records() {
            let record = result.map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: CSV parse error: {}", path.display(), e),
                )
            })?;
            // 位置は読み飛ばした空行の先頭を指すことがあるので、改行の後ろまで進める
            let mut start = record
                .position()
                .map_or(scanned, |p| (p.byte() as usize).min(bytes.len()));
            while start < bytes.len() && matches!(bytes[start], b'\n' | b'\r') {
                start += 1;
            }
            if start > scanned {
                line_no += bytes[scanned..start]
                    .iter()
                    .filter(|&&b| b == b'\n')
                    .count();
                scanned = start;
            }
            if is_comment_record(&record) {
                continue;
            }
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
            if let Some(matrix) = &self.matrix {
                if !matrix.contains_ids(left_id, right_id) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "{}:{}: context ID outside the connection matrix: \
                             left_id={} (< {}), right_id={} (< {})",
                            path.display(),
                            line_no,
                            left_id,
                            matrix.num_left,
                            right_id,
                            matrix.num_right
                        ),
                    ));
                }
            }

            // 品詞情報を結合
            let pos_parts: Vec<&str> = (4..record.len().min(8))
                .filter_map(|i| record.get(i))
                .collect();
            let pos = self.intern(&pos_parts.join(","));
            // 活用型・活用形は空か列が無ければ `*`
            let conj = |i: usize| record.get(i).filter(|s| !s.is_empty()).unwrap_or("*");
            let conj_type = self.intern(conj(8));
            let conj_form = self.intern(conj(9));

            let base_form: Arc<str> = match record.get(10) {
                Some(b) if b != &record[0] => b.into(),
                _ => Arc::clone(&surface),
            };
            let reading: Arc<str> = record.get(11).unwrap_or("").into();
            let pronunciation: Arc<str> = match record.get(12) {
                Some(p) if p != &*reading => p.into(),
                Some(_) => Arc::clone(&reading),
                None => "".into(),
            };

            self.entries.push(DictEntry {
                surface,
                left_id,
                right_id,
                cost,
                pos,
                conj_type,
                conj_form,
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
    ///
    /// 1 行目は「前の語の right_id の数」「次の語の left_id の数」、以降の行は
    /// 「前の語の right_id」「次の語の left_id」「コスト」（MeCab の matrix.def と同じ）。
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
        let num_right: u32 = parse_field(path, 1, "right_id count", parts[0])?;
        let num_left: u32 = parse_field(path, 1, "left_id count", parts[1])?;
        if num_right > 1 << 16 || num_left > 1 << 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: matrix size {num_right}x{num_left} is too large",
                    path.display()
                ),
            ));
        }
        let mut matrix = ConnectionMatrix::zeros(num_left, num_right);

        for (line_no, line) in lines.enumerate().map(|(i, l)| (i + 2, l)) {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let right_id: u32 = parse_field(path, line_no, "right_id", parts[0])?;
            let left_id: u32 = parse_field(path, line_no, "left_id", parts[1])?;
            let cost: i16 = parse_field(path, line_no, "cost", parts[2])?;
            if right_id >= num_right || left_id >= num_left {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{}:{}: matrix id out of range: right_id={} (< {}), left_id={} (< {})",
                        path.display(),
                        line_no,
                        right_id,
                        num_right,
                        left_id,
                        num_left
                    ),
                ));
            }
            matrix.costs[left_id as usize * num_right as usize + right_id as usize] = cost;
        }

        eprintln!(
            "Loaded matrix: {}x{} ({} entries)",
            num_right,
            num_left,
            matrix.costs.len()
        );
        self.matrix = Some(matrix);

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
    ///
    /// EUC-JP のうち変換表によって写し先が分かれる 7 字（ダッシュ・波ダッシュ・マイナス等）は、
    /// JIS の対応表（iconv・MeCab と同じ）の字にそろえる。encoding_rs（WHATWG）は Windows（CP932）と
    /// 同じ字に写すが、それでは MeCab の出力と文字列が食い違い、JIS 側の字で書かれた文章
    /// （「〜」U+301C、「—」U+2014 等）にも当たらない。
    pub(crate) fn decode_to_utf8(bytes: &[u8]) -> String {
        // まずUTF-8として試す
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
        // EUC-JPとしてデコード
        let (cow, _, _) = encoding_rs::EUC_JP.decode(bytes);
        cow.chars().map(euc_jp_jis_side).collect()
    }

    fn source(&self) -> DictSource<'_> {
        DictSource {
            entries: &self.entries,
            matrix: self.matrix.as_ref(),
            classifier: &self.char_classifier,
            unk_entries: &self.unk_entries,
        }
    }

    /// 書き出しの設定（取り込んだ辞書があればそのメタデータを引き継ぐ）
    pub fn write_options(&self) -> WriteOptions {
        WriteOptions {
            meta: self
                .meta
                .clone()
                .unwrap_or_else(|| WriteOptions::default().meta),
            prune_dominated: false,
        }
    }

    /// メモリ上に辞書を作る（テスト・Python の `DictBuilder` 用）
    pub fn build(self) -> Result<Dictionary, DictError> {
        let opts = self.write_options();
        self.build_with(&opts)
    }

    /// 設定を指定してメモリ上に辞書を作る
    pub fn build_with(&self, opts: &WriteOptions) -> Result<Dictionary, DictError> {
        let sections = writer::build_sections(&self.source(), opts, |_, _| {})?;
        let (words, len) = sections.to_aligned_buffer();
        Dictionary::from_owned(words, len)
    }

    /// .hsd ファイルに書き出す
    ///
    /// 同じディレクトリの一時ファイルに書いてから rename で差し替えるので、書き出しに失敗しても
    /// 元のファイルは壊れない。`progress(確定したキー数, キーの総数)` は trie の構築中に呼ばれる。
    pub fn write_hsd<P: AsRef<Path>>(
        &self,
        path: P,
        opts: &WriteOptions,
        progress: impl FnMut(usize, usize),
    ) -> Result<WriteStats, DictError> {
        let mut sections = writer::build_sections(&self.source(), opts, progress)?;
        sections.write_file(path.as_ref())?;
        Ok(sections.stats.clone())
    }
}

/// 全エントリを MeCab 形式の CSV（13 列）で書き出す
///
/// 列は `表層形,左文脈ID,右文脈ID,コスト,品詞1..4,活用型,活用形,原形,読み,発音`。
/// 品詞が 4 要素に満たないときは `*` で埋め、5 要素以上なら 4 列目に残りをまとめる
/// （`DictBuilder::add_csv` で読み戻すと同じ品詞文字列になる）。
///
/// Returns: 書き出したエントリ数
pub fn write_lexicon_csv<W: std::io::Write>(dict: &Dictionary, writer: W) -> io::Result<usize> {
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_writer(writer);
    let mut count = 0;
    let mut io_error: Option<io::Error> = None;
    let result = dict.for_each_entry(|e| {
        let mut parts = e.pos.splitn(4, ',');
        let pos_cols: [&str; 4] = std::array::from_fn(|_| parts.next().unwrap_or("*"));
        let (left_id, right_id, cost) = (
            e.left_id.to_string(),
            e.right_id.to_string(),
            e.cost.to_string(),
        );
        wtr.write_record([
            &*e.surface,
            &left_id,
            &right_id,
            &cost,
            pos_cols[0],
            pos_cols[1],
            pos_cols[2],
            pos_cols[3],
            &e.conj_type,
            &e.conj_form,
            &e.base_form,
            &e.reading,
            &e.pronunciation,
        ])
        .map_err(|err| {
            // 書き込み先の io エラーは種類 (BrokenPipe 等) を保ったまま返す
            let err = match err.into_kind() {
                csv::ErrorKind::Io(err) => err,
                kind => io::Error::other(format!("{kind:?}")),
            };
            let message = err.to_string();
            io_error = Some(err);
            DictError::Invalid(message)
        })?;
        count += 1;
        Ok(())
    });
    if let Some(e) = io_error {
        return Err(e);
    }
    result?;
    wtr.flush()?;
    Ok(count)
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
            ..Default::default()
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
            ..Default::default()
        });

        let dict = builder.build().unwrap();
        let results = dict.lookup("東京都").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "東京".len());
        assert_eq!(&*results[0].1[0].surface, "東京");
        assert_eq!(&*results[0].1[0].pronunciation, "トーキョー");
        assert_eq!(&*results[0].1[0].conj_type, "*");
    }

    // --- 追加テスト ---

    #[test]
    fn test_connection_matrix_cost() {
        // 次の語の left_id が 3、前の語の right_id が 2。costs[left_id * num_right + right_id]
        let matrix = ConnectionMatrix {
            num_left: 3,
            num_right: 2,
            costs: vec![0, 1, 2, 3, 4, 5],
        };
        assert_eq!(matrix.cost(0, 0), Some(0));
        assert_eq!(matrix.cost(1, 0), Some(1));
        assert_eq!(matrix.cost(0, 1), Some(2));
        assert_eq!(matrix.cost(1, 2), Some(5));
    }

    #[test]
    fn test_connection_matrix_out_of_bounds() {
        let matrix = ConnectionMatrix::zeros(2, 2);
        assert_eq!(matrix.cost(10, 0), None);
        assert_eq!(matrix.cost(0, 10), None);
    }

    #[test]
    fn test_connection_matrix_contains_ids() {
        // left_id が 3 種類、right_id が 2 種類
        let matrix = ConnectionMatrix::zeros(3, 2);
        assert!(matrix.contains_ids(2, 1));
        assert!(!matrix.contains_ids(3, 0));
        assert!(!matrix.contains_ids(0, 2));
    }

    #[test]
    fn test_load_matrix_reads_mecab_orientation() {
        // 1 行目は「right_id の数 left_id の数」、各行は「right_id left_id コスト」。
        // 正方でない行列で向きを取り違えないことを確かめる
        let dir = std::env::temp_dir().join(format!("hasami-matrix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("matrix.def");
        std::fs::write(&path, "2 3\n0 0 1\n0 2 3\n1 0 4\n1 2 6\n").unwrap();
        let mut builder = DictBuilder::new();
        builder.load_matrix(&path).unwrap();
        let m = builder.matrix.as_ref().unwrap();
        assert_eq!((m.num_right, m.num_left), (2, 3));
        assert_eq!(m.cost(0, 0), Some(1));
        assert_eq!(m.cost(0, 2), Some(3));
        assert_eq!(m.cost(1, 0), Some(4));
        assert_eq!(m.cost(1, 2), Some(6));
        assert_eq!(m.cost(1, 1), Some(0));
        std::fs::write(&path, "2 3\n2 0 1\n").unwrap();
        assert!(builder.load_matrix(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_pos_has_prefix_is_element_wise() {
        let prefix = |s: &str| -> Vec<String> { s.split(',').map(String::from).collect() };
        let pos = "名詞,固有名詞,人名,姓";
        assert!(pos_has_prefix::<&str>(pos, &[]));
        assert!(pos_has_prefix(pos, &prefix("名詞,固有名詞,人名")));
        assert!(pos_has_prefix(pos, &prefix("名詞,固有名詞,人名,姓")));
        // 文字列としては前方一致でも、要素の途中で切れていれば一致しない
        assert!(!pos_has_prefix(pos, &prefix("名詞,固有名詞,人")));
        assert!(!pos_has_prefix(pos, &prefix("名詞,固有名詞,人名,姓,*")));
        assert!(!pos_has_prefix(pos, &prefix("名詞,一般")));
    }

    #[test]
    fn test_is_quantity_surface() {
        for surface in ["50%", "0.1℃", "1,000㎞", "30°C", "５０％", "4℃", "12.5‰"] {
            assert!(is_quantity_surface(surface), "{surface}");
        }
        // 単位で終わらない・数で始まらない・単位の記号でない字を含む・数が区切りで終わる
        for surface in ["50", "%50", "100%ORANGE", "50%増", "3D", "50★", "1.%", "℃"] {
            assert!(!is_quantity_surface(surface), "{surface}");
        }
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        });

        let dict = builder.build().unwrap();
        let results = dict.lookup("金曜日").unwrap();
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
            ..Default::default()
        });
        let dict = builder.build().unwrap();
        let results = dict.lookup("大阪").unwrap();
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
            ..Default::default()
        });

        // matrix.def なしでは、使われている文脈 ID を覆うゼロ行列を置く
        let dict = builder.build().unwrap();
        assert_eq!(dict.matrix_dims(), (1, 1));
        assert!(dict.connection_matrix().is_none());
    }

    #[test]
    fn test_dict_builder_set_matrix() {
        let mut builder = DictBuilder::new();
        builder.set_matrix(ConnectionMatrix::zeros(5, 4));
        builder.add_entry(DictEntry {
            surface: "テスト".into(),
            left_id: 4,
            right_id: 3,
            ..Default::default()
        });
        let dict = builder.build().unwrap();
        assert_eq!(dict.matrix_dims(), (5, 4));
        assert!(dict.connection_matrix().is_some());
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
    fn test_decode_to_utf8_euc_jp_uses_jis_side_characters() {
        // ダッシュ・波ダッシュ・‖・マイナス・¢・£・¬ は MeCab（iconv）と同じ字にする
        let bytes: &[u8] = &[
            0xA1, 0xBD, 0xA1, 0xC1, 0xA1, 0xC2, 0xA1, 0xDD, 0xA1, 0xF1, 0xA1, 0xF2, 0xA2, 0xCC,
        ];
        let result = DictBuilder::decode_to_utf8(bytes);
        assert_eq!(
            result,
            "\u{2014}\u{301C}\u{2016}\u{2212}\u{00A2}\u{00A3}\u{00AC}"
        );
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
            ..Default::default()
        };
        let cloned = entry.clone();
        assert_eq!(&*cloned.surface, "テスト");
        assert_eq!(cloned.left_id, 1);
    }
}
