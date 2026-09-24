//! 品詞の正規化・否定の判定・モーラ数
//!
//! [`Token::coarse_pos`] は、辞書ごとの品詞体系の違いを吸収した粗い品詞 [`CoarsePos`] を返す。
//! IPAdic 系（IPAdic 単体と、NEologd・SudachiDict を IPAdic の品詞に写して足した辞書）と
//! UniDic 系（`scripts/convert-unidic-csv.py` で変換した辞書）の両方を扱い、名詞と用言の区別・
//! 形式名詞・「の」・否定・句読点と括弧が辞書をまたいで同じ値になるようにそろえる。
//!
//! 品詞体系は辞書のメタデータ（`pos_scheme`）ではなく、トークンの品詞の文字列から判定する。
//! 品詞と細分類の組は両体系で重ならない（`名詞,固有名詞` `助詞,格助詞` のように両体系にある組は
//! 意味も同じ）ので、文字列だけで決まる。対応表は配布辞書に現れる IPAdic の 69 品詞と、UniDic
//! 2.1.2 の 52 品詞をすべて網羅している（テストで確かめている）。
//!
//! 体系ごとの対応表（`IPADIC_POS_TABLE` / `UNIDIC_POS_TABLE`）のほかに、次の 4 つで
//! 辞書の間の食い違いをそろえる。
//!
//! - UniDic は形式名詞（こと・わけ・ため）を普通名詞と区別しないので、仮名書きの形式名詞を
//!   表層形で拾う（`UNIDIC_FORMAL_NOUNS`）。IPAdic では 名詞,非自立。
//! - IPAdic は受け身・使役の「れる」「られる」「せる」「させる」を 動詞,接尾 に置く。UniDic と
//!   学校文法では助動詞なので助動詞にする（`IPADIC_SUFFIX_AUX_VERBS`）。
//! - 助動詞の語幹「そう」「よう」「みたい」（降りそうだ・行くようだ・行くみたいだ）は、IPAdic では
//!   名詞、UniDic では名詞・形状詞に分かれる。学校文法に合わせてどちらも助動詞にする
//!   （「雨が降りそう。」を名詞で終わる文と取り違えない）。
//! - 記号は辞書によって同じ字の品詞が違う（半角の `(` `!` は SudachiDict を足した辞書だけが
//!   括弧開・句点で、ほかは未知語の 記号,一般）。細分類の無い記号（`記号,一般` など）と、
//!   表層形が記号だけの未知語は、表層形で句点・読点・括弧を見分ける（`symbol_by_surface`）。
//!
//! そろえていない食い違いもある。どれも辞書の品詞に従う。
//!
//! - 助詞の細分類: 「しか」「さえ」「すら」は IPAdic では係助詞、UniDic では副助詞。並立の「と」
//!   （りんごとみかん）は IPAdic では並立助詞、UniDic では格助詞。文末の「か」は IPAdic では
//!   副助詞／並立助詞／終助詞（`OtherParticle`）、UniDic では終助詞。
//! - 助数詞: 「3回」の「回」は IPAdic では 名詞,接尾,助数詞（`NounSuffix`）、UniDic では
//!   名詞,普通名詞,助数詞可能（`Noun`）。
//! - 漢字書きの形式名詞（事・物・時）: IPAdic は文脈で 名詞,非自立 と 名詞,一般 を選び分けるが、
//!   UniDic では `Noun`。
//! - 表層形が記号だけの既知語（SudachiDict の絵文字の 名詞,一般 など）は辞書の品詞に従う。

use crate::lattice::Token;

/// 辞書の品詞体系によらない粗い品詞
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoarsePos {
    /// 普通名詞・サ変名詞・形容動詞の語幹（UniDic の形状詞）
    Noun,
    /// 固有名詞
    ProperNoun,
    /// 代名詞
    Pronoun,
    /// 数詞
    Numeral,
    /// 名詞の接尾辞（〜的・〜化・〜性、助数詞）
    NounSuffix,
    /// 形式名詞・非自立の名詞（こと・もの・わけ・ため、「行くのが」の「の」）
    ///
    /// UniDic は形式名詞を普通名詞と区別しないので、仮名書きの形式名詞を表層形で拾う
    /// （「うちに帰る」の「うち」のような実質名詞の用法も含む）。
    FormalNoun,
    /// 動詞
    Verb,
    /// 形容詞
    Adjective,
    /// 助動詞（受け身・使役の「れる」「せる」、助動詞の語幹「そう」「よう」「みたい」を含む）
    AuxVerb,
    /// 格助詞（IPAdic の 助詞,連体化 の「の」を含む）
    CaseParticle,
    /// 接続助詞
    ConjunctiveParticle,
    /// 係助詞（は・も）
    BindingParticle,
    /// 終助詞
    FinalParticle,
    /// そのほかの助詞（副助詞・並立助詞など）
    OtherParticle,
    /// 副詞
    Adverb,
    /// 連体詞（この・その）
    Adnominal,
    /// 接続詞
    Conjunction,
    /// 感動詞・フィラー
    Interjection,
    /// 接頭辞
    Prefix,
    /// 句点（。．！？ など文を終える記号）
    Period,
    /// 読点（、，と半角の ,）
    ///
    /// 桁区切りの `,`（1,000）も読点になる（NEologd・SudachiDict を足した辞書は `,` を
    /// 記号,読点 に登録している）。読点で区画に分けるときは、前後が数字かを呼び出し側で見る。
    Comma,
    /// 開き括弧
    OpenBracket,
    /// 閉じ括弧
    CloseBracket,
    /// そのほかの記号・空白
    Symbol,
    /// どの分類にも当たらない品詞（IPAdic の その他,間投 など）
    Other,
}

/// 品詞体系
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PosScheme {
    /// IPAdic 系（IPAdic・NEologd・SudachiDict を IPAdic の品詞に写したもの）
    Ipadic,
    /// UniDic 系（`scripts/convert-unidic-csv.py` で変換したもの）
    Unidic,
}

/// UniDic にだけある品詞の大分類
const UNIDIC_ONLY_POS1: &[&str] = &["代名詞", "形状詞", "接尾辞", "接頭辞", "補助記号", "空白"];

/// UniDic にだけある細分類 1（大分類は IPAdic にもあるもの）
///
/// `名詞,普通名詞` `名詞,数詞` `名詞,助動詞語幹` `動詞,非自立可能` `形容詞,非自立可能`
/// `助詞,準体助詞` `記号,文字`。IPAdic の「助動詞語幹」は細分類 2 にしか現れない。
const UNIDIC_ONLY_POS2: &[&str] = &[
    "普通名詞",
    "数詞",
    "助動詞語幹",
    "非自立可能",
    "準体助詞",
    "文字",
];

impl PosScheme {
    /// 品詞の大分類と細分類 1 から体系を判定する
    ///
    /// UniDic にだけある分類を含めば UniDic、そうでなければ IPAdic とする。両体系にある組
    /// （`名詞,固有名詞` `助詞,格助詞` `動詞,一般` `記号,一般` など）はどちらの対応表でも
    /// 同じ値になるので、IPAdic と判定しても結果は変わらない。
    fn detect(pos1: &str, pos2: &str) -> Self {
        if UNIDIC_ONLY_POS1.contains(&pos1) || UNIDIC_ONLY_POS2.contains(&pos2) {
            PosScheme::Unidic
        } else {
            PosScheme::Ipadic
        }
    }
}

/// 品詞の対応表の 1 行: (大分類, 細分類 1, 細分類 2, 粗い品詞)。空文字列はどの値にも一致する
type PosRule = (&'static str, &'static str, &'static str, CoarsePos);

/// IPAdic 系の品詞の対応表（上から順に照合し、最初に一致した行を使う）
const IPADIC_POS_TABLE: &[PosRule] = &[
    ("名詞", "固有名詞", "", CoarsePos::ProperNoun),
    ("名詞", "代名詞", "", CoarsePos::Pronoun),
    ("名詞", "数", "", CoarsePos::Numeral),
    // 助動詞の語幹。「雨だそうだ」「降りそうだ」の「そう」、「行くようだ」の「よう」。
    // UniDic の 名詞,助動詞語幹・形状詞,助動詞語幹 に当たる
    ("名詞", "特殊", "助動詞語幹", CoarsePos::AuxVerb),
    ("名詞", "接尾", "助動詞語幹", CoarsePos::AuxVerb),
    ("名詞", "非自立", "助動詞語幹", CoarsePos::AuxVerb),
    // 形容動詞語幹の「みたい」は表の前に原形で助動詞にする（「こんなふうに」の「ふう」は形式名詞）
    ("名詞", "非自立", "", CoarsePos::FormalNoun),
    ("名詞", "接尾", "", CoarsePos::NounSuffix),
    // 一般・サ変接続・形容動詞語幹・副詞可能・ナイ形容詞語幹・引用文字列（いわく）・
    // 接続詞的（対・兼）・動詞非自立的（ご覧・頂戴。UniDic では普通名詞）
    ("名詞", "", "", CoarsePos::Noun),
    // 受け身・使役の 動詞,接尾 は表の前に原形で助動詞にする（IPADIC_SUFFIX_AUX_VERBS）
    ("動詞", "", "", CoarsePos::Verb),
    ("形容詞", "", "", CoarsePos::Adjective),
    ("助動詞", "", "", CoarsePos::AuxVerb),
    ("助詞", "格助詞", "", CoarsePos::CaseParticle),
    // 「AのB」の「の」
    ("助詞", "連体化", "", CoarsePos::CaseParticle),
    // 「ゆっくりと」の「と」。UniDic では格助詞
    ("助詞", "副詞化", "", CoarsePos::CaseParticle),
    ("助詞", "接続助詞", "", CoarsePos::ConjunctiveParticle),
    ("助詞", "係助詞", "", CoarsePos::BindingParticle),
    ("助詞", "終助詞", "", CoarsePos::FinalParticle),
    // 副助詞・並立助詞・副助詞／並立助詞／終助詞（か）・特殊（かな）
    ("助詞", "", "", CoarsePos::OtherParticle),
    ("副詞", "", "", CoarsePos::Adverb),
    ("連体詞", "", "", CoarsePos::Adnominal),
    ("接続詞", "", "", CoarsePos::Conjunction),
    ("感動詞", "", "", CoarsePos::Interjection),
    ("フィラー", "", "", CoarsePos::Interjection),
    ("接頭詞", "", "", CoarsePos::Prefix),
    ("記号", "句点", "", CoarsePos::Period),
    ("記号", "読点", "", CoarsePos::Comma),
    ("記号", "括弧開", "", CoarsePos::OpenBracket),
    ("記号", "括弧閉", "", CoarsePos::CloseBracket),
    // 一般・空白・アルファベット（表層形で句点・読点・括弧を見分け直す）
    ("記号", "", "", CoarsePos::Symbol),
];

/// UniDic 系の品詞の対応表（上から順に照合し、最初に一致した行を使う）
///
/// 普通名詞のうち形式名詞（こと・わけ・ため）は品詞で見分けられないので、表の前に表層形で
/// 判定する（[`UNIDIC_FORMAL_NOUNS`]）。
const UNIDIC_POS_TABLE: &[PosRule] = &[
    ("名詞", "固有名詞", "", CoarsePos::ProperNoun),
    ("名詞", "数詞", "", CoarsePos::Numeral),
    // 「雨だそうだ」の「そう」。IPAdic の 名詞,特殊,助動詞語幹 に当たる
    ("名詞", "助動詞語幹", "", CoarsePos::AuxVerb),
    // 普通名詞（一般・サ変可能・形状詞可能・副詞可能・助数詞可能）
    ("名詞", "", "", CoarsePos::Noun),
    ("代名詞", "", "", CoarsePos::Pronoun),
    // 「降りそうだ」「行くようだ」「行くみたいだ」の「そう」「よう」「みたい」。
    // IPAdic の 名詞,接尾,助動詞語幹・名詞,非自立,助動詞語幹・名詞,非自立,形容動詞語幹 に当たる
    ("形状詞", "助動詞語幹", "", CoarsePos::AuxVerb),
    // 形容動詞の語幹（静か・好き）とタリ活用（堂々）。IPAdic の 名詞,形容動詞語幹 に当たる
    ("形状詞", "", "", CoarsePos::Noun),
    ("接尾辞", "名詞的", "", CoarsePos::NounSuffix),
    // 「多角的」の「的」。IPAdic の 名詞,接尾,形容動詞語幹 に当たる
    ("接尾辞", "形状詞的", "", CoarsePos::NounSuffix),
    // 「寒がる」の「がる」。IPAdic の 動詞,接尾 に当たる
    ("接尾辞", "動詞的", "", CoarsePos::Verb),
    // 「子供っぽい」の「っぽい」。IPAdic の 形容詞,接尾 に当たる
    ("接尾辞", "形容詞的", "", CoarsePos::Adjective),
    ("接頭辞", "", "", CoarsePos::Prefix),
    ("動詞", "", "", CoarsePos::Verb),
    ("形容詞", "", "", CoarsePos::Adjective),
    ("助動詞", "", "", CoarsePos::AuxVerb),
    ("助詞", "格助詞", "", CoarsePos::CaseParticle),
    ("助詞", "接続助詞", "", CoarsePos::ConjunctiveParticle),
    ("助詞", "係助詞", "", CoarsePos::BindingParticle),
    ("助詞", "終助詞", "", CoarsePos::FinalParticle),
    // 「行くのが」「行くんだ」の「の」「ん」。IPAdic の 名詞,非自立 に当たる
    ("助詞", "準体助詞", "", CoarsePos::FormalNoun),
    // 副助詞
    ("助詞", "", "", CoarsePos::OtherParticle),
    ("副詞", "", "", CoarsePos::Adverb),
    ("連体詞", "", "", CoarsePos::Adnominal),
    ("接続詞", "", "", CoarsePos::Conjunction),
    // 一般・フィラー
    ("感動詞", "", "", CoarsePos::Interjection),
    ("補助記号", "句点", "", CoarsePos::Period),
    ("補助記号", "読点", "", CoarsePos::Comma),
    ("補助記号", "括弧開", "", CoarsePos::OpenBracket),
    ("補助記号", "括弧閉", "", CoarsePos::CloseBracket),
    // 一般・ＡＡ（表層形で句点・読点・括弧を見分け直す）
    ("補助記号", "", "", CoarsePos::Symbol),
    // 一般・文字
    ("記号", "", "", CoarsePos::Symbol),
    ("空白", "", "", CoarsePos::Symbol),
];

/// IPAdic が 動詞,接尾 に置く受け身・使役の助動詞（原形）
///
/// UniDic と学校文法では助動詞。「示される」の「れる」を動詞と数えると、述語の動詞を
/// 取り違える。同じ 動詞,接尾 でも「寒がる」の「がる」は UniDic でも 接尾辞,動詞的 なので動詞のまま。
const IPADIC_SUFFIX_AUX_VERBS: &[&str] =
    &["れる", "られる", "せる", "させる", "しめる", "す", "さす"];

/// IPAdic が 名詞,非自立,形容動詞語幹 に置く助動詞の語幹（原形）
///
/// 「行くみたいだ」の「みたい」。UniDic では 形状詞,助動詞語幹 で、学校文法では助動詞「みたいだ」。
/// 同じ品詞の「こんなふうに」の「ふう」は形式名詞のまま。
const IPADIC_AUX_VERB_STEMS: &[&str] = &["みたい"];

/// UniDic で 名詞,普通名詞 になる形式名詞（表層形）
///
/// IPAdic では 名詞,非自立 に分かれる語。UniDic は形式名詞を普通名詞と区別しないので表層形で
/// 拾う（語彙素は「積り」のように版で表記が揺れるので使わない）。漢字書きは実質名詞のことが多い
/// もの（事・物・時・所・方）を除き、ほぼ形式名詞にしか使わない「筈」「為」「儘」だけを入れる。
const UNIDIC_FORMAL_NOUNS: &[&str] = &[
    "こと",
    "もの",
    "もん",
    "わけ",
    "はず",
    "つもり",
    "ところ",
    "とき",
    "ため",
    "うち",
    "まま",
    "ほう",
    "とおり",
    "たび",
    "くせ",
    "せい",
    "うえ",
    "かぎり",
    "ついで",
    "ふり",
    "すえ",
    "とたん",
    "あまり",
    "なか",
    "ころ",
    "ゆえ",
    "ふう",
    "筈",
    "為",
    "儘",
];

/// 否定の助動詞の原形
///
/// IPAdic 系は「ない」（なかっ・なく・なけれ も原形は ない）、「無い」、「ぬ」（ず・ざる・ね も
/// 原形は ぬ）、「ん」（言えません）、関西方言の「へん」「ひん」。UniDic は語彙素で、
/// 「ぬ」「ん」「ず」はどれも「ず」になる。打消しの推量・意志の「まい」「じ」は、打消しに
/// 推量・意志が重なった別の助動詞なので入れない。
const NEGATIVE_AUX_VERBS: &[&str] = &["ない", "無い", "ぬ", "ん", "ず", "へん", "ひん"];

/// 否定の形容詞の原形（「お金がない」「問題は無い」。UniDic の語彙素は「無い」）
const NEGATIVE_ADJECTIVES: &[&str] = &["ない", "無い"];

/// 対応表を上から照合する。どの行にも当たらなければ `Other`
fn lookup(table: &[PosRule], pos1: &str, pos2: &str, pos3: &str) -> CoarsePos {
    table
        .iter()
        .find(|&&(p1, p2, p3, _)| {
            p1 == pos1 && (p2.is_empty() || p2 == pos2) && (p3.is_empty() || p3 == pos3)
        })
        .map_or(CoarsePos::Other, |&(_, _, _, coarse)| coarse)
}

/// IPAdic 系の品詞から粗い品詞を決める
fn ipadic_coarse_pos(pos1: &str, pos2: &str, pos3: &str, base_form: &str) -> CoarsePos {
    let aux_verb = match (pos1, pos2) {
        ("動詞", "接尾") => IPADIC_SUFFIX_AUX_VERBS.contains(&base_form),
        ("名詞", "非自立") => IPADIC_AUX_VERB_STEMS.contains(&base_form),
        _ => false,
    };
    if aux_verb {
        return CoarsePos::AuxVerb;
    }
    lookup(IPADIC_POS_TABLE, pos1, pos2, pos3)
}

/// UniDic 系の品詞から粗い品詞を決める
fn unidic_coarse_pos(pos1: &str, pos2: &str, pos3: &str, surface: &str) -> CoarsePos {
    if pos1 == "名詞" && pos2 == "普通名詞" && UNIDIC_FORMAL_NOUNS.contains(&surface) {
        return CoarsePos::FormalNoun;
    }
    lookup(UNIDIC_POS_TABLE, pos1, pos2, pos3)
}

/// 表層形が記号だけか（文字・数字を含まない）
fn is_symbol_only(surface: &str) -> bool {
    !surface.is_empty() && surface.chars().all(|c| !c.is_alphanumeric())
}

/// 記号の粗い品詞を表層形で決める
///
/// 同じ字でも辞書によって品詞が違う。半角の `(` `!` は SudachiDict を足した辞書では括弧開・句点、
/// IPAdic・NEologd では未知語の 記号,一般。全角の `！` は IPAdic 系では 記号,一般、UniDic では
/// 補助記号,句点。細分類の無い記号はここで見分け直して、辞書をまたいで同じ値にする。
/// 全部の字が句点（または読点・開き括弧・閉じ括弧）のときだけその分類にし、混ざれば `Symbol`。
fn symbol_by_surface(surface: &str) -> CoarsePos {
    // 数に付く単位の記号は、全角の「％」（IPAdic の 名詞,接尾,助数詞）と同じく名詞の接尾辞にする
    if !surface.is_empty() && surface.chars().all(is_unit_symbol) {
        return CoarsePos::NounSuffix;
    }
    let mut chars = surface.chars();
    let Some(first) = chars.next() else {
        return CoarsePos::Symbol;
    };
    let kind = punctuation_kind(first);
    if chars.all(|c| punctuation_kind(c) == kind) {
        kind
    } else {
        CoarsePos::Symbol
    }
}

/// 数に付く単位の記号か（`%` `％` `‰` `℃` `℉` `°` と、CJK 互換文字の単位）
///
/// 配布辞書は `scripts/prepare_ipadic.py` がこれらを 名詞,接尾,助数詞 の語として足すので品詞からも決まるが、
/// 語を持たない辞書（UniDic 系、利用者が作った辞書）でも同じ値にするために表層形で見分ける。CJK 互換文字は
/// カタカナの組文字（U+3300..U+3357）のうち建物・ギリシャ文字の名前でないものと、ラテン文字の組文字の単位
/// （午前・午後・株式会社・対数など単位でない字と、読みの分かれる ㏏ ㏿ ㍲ を除く）。字の集合は
/// `prepare_ipadic.py` の `UNIT_SYMBOLS`・`KATAKANA_SQUARES`・`LATIN_UNIT_SQUARES` と同じ（全角の `％` は IPAdic にもとからある）。
/// 辞書の修復（`hasami repair --drop-quantity-nouns`）も、数と単位の記号だけの表層形を見分けるのに使う。
pub(crate) fn is_unit_symbol(c: char) -> bool {
    match c {
        '%' | '％' | '‰' | '℃' | '℉' | '°' => true,
        '\u{3300}'..='\u{3357}' => !matches!(
            c,
            '㌀' | '㌁' | '㌏' | '㌞' | '㌪' | '㌱' | '㌼' | '㍁' | '㍇'
        ),
        '\u{3371}' | '\u{3373}'..='\u{3374}' | '\u{3376}'..='\u{337A}' => true,
        '\u{3380}'..='\u{33DF}' => !matches!(
            c,
            '㏂' | '㏇' | '㏍' | '㏏' | '㏑' | '㏒' | '㏗' | '㏘' | '㏚'
        ),
        _ => false,
    }
}

/// 1 字の記号の分類（句点・読点・括弧でなければ `Symbol`）
///
/// 句点と括弧の字は文分割（[`crate::sentence`]）の文末記号・括弧をそのまま使う（字の集合を 1 か所で持つ）。
/// ASCII の `.` は小数点・略語と区別できないので句点に入れない（辞書が 記号,句点 にしていればそれに従う）。
fn punctuation_kind(c: char) -> CoarsePos {
    if crate::sentence::is_sentence_ender(c) {
        return CoarsePos::Period;
    }
    if let Some((_, open)) = crate::sentence::bracket(c) {
        return if open {
            CoarsePos::OpenBracket
        } else {
            CoarsePos::CloseBracket
        };
    }
    match c {
        '、' | '，' | '､' | ',' => CoarsePos::Comma,
        _ => CoarsePos::Symbol,
    }
}

/// ひらがなをカタカナに寄せる（ほかの字はそのまま）
fn to_katakana(c: char) -> char {
    match c {
        // ぁ〜ゖ と ゝゞ はカタカナと 0x60 ずれて並ぶ
        'ぁ'..='ゖ' | 'ゝ' | 'ゞ' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
        _ => c,
    }
}

/// 直前の仮名と合わせて 1 モーラになる小書き文字（拗音・外来音）
fn is_combining_small_kana(c: char) -> bool {
    matches!(
        c,
        'ャ' | 'ュ' | 'ョ' | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ヮ'
    )
}

/// 仮名のモーラ数を数える（カタカナ以外の字は数えない。ひらがなはカタカナとして数える）
fn count_morae(s: &str) -> usize {
    let mut count = 0;
    // 直前の字が、小書き文字と合わせて 1 モーラになる仮名か
    let mut after_onset = false;
    for c in s.chars().map(to_katakana) {
        if is_combining_small_kana(c) {
            // 先頭・促音・長音・小書き文字の直後など、合わせる相手がなければ単独で 1 モーラ
            if !after_onset {
                count += 1;
            }
            after_onset = false;
        } else if matches!(c, 'ァ'..='ヺ' | 'ー' | 'ヽ' | 'ヾ') {
            count += 1;
            after_onset = !matches!(c, 'ッ' | 'ー');
        } else {
            after_onset = false;
        }
    }
    count
}

impl Token {
    /// 辞書の品詞体系によらない粗い品詞
    ///
    /// IPAdic 系と UniDic 系のどちらの辞書でも、同じ語が同じ値になるようにそろえる
    /// （そろえている範囲と、残る食い違いは [`crate::pos`] の説明を参照）。
    ///
    /// - 「の」は IPAdic の 助詞,連体化 と 助詞,格助詞、UniDic の 助詞,格助詞 のどれでも
    ///   [`CoarsePos::CaseParticle`]。「行くのが」の「の」（IPAdic の 名詞,非自立、UniDic の
    ///   助詞,準体助詞）は [`CoarsePos::FormalNoun`]。
    /// - 記号は句点・読点・開き括弧・閉じ括弧を区別する。辞書が細分類を付けていない記号と、
    ///   表層形が記号だけ（文字・数字を含まない）の未知語は、表層形で見分ける（IPAdic の
    ///   unk.def は記号の未知語を 名詞,サ変接続 にするので、未知語は品詞を見ない）。
    pub fn coarse_pos(&self) -> CoarsePos {
        if !self.is_known && is_symbol_only(&self.surface) {
            return symbol_by_surface(&self.surface);
        }
        let mut fields = self.pos.split(',');
        let pos1 = fields.next().unwrap_or("");
        let pos2 = fields.next().unwrap_or("");
        let pos3 = fields.next().unwrap_or("");
        let coarse = match PosScheme::detect(pos1, pos2) {
            PosScheme::Ipadic => ipadic_coarse_pos(pos1, pos2, pos3, &self.base_form),
            PosScheme::Unidic => unidic_coarse_pos(pos1, pos2, pos3, &self.surface),
        };
        if coarse == CoarsePos::Symbol {
            symbol_by_surface(&self.surface)
        } else {
            coarse
        }
    }

    /// 否定の形態素か（助動詞「ない」「ぬ」「ん」「ず」、形容詞「ない」）
    ///
    /// 原形で判定する。IPAdic 系では「言えません」の「ん」の原形が `ん`、「知らぬ」の「ぬ」と
    /// 「行かず」の「ず」が `ぬ`、「なかった」の「なかっ」が `ない`。漢字書きの「無い」と、
    /// 関西方言の「へん」「ひん」（行かへん）も否定にする。UniDic では「ぬ」「ん」「ず」の
    /// 語彙素が `ず`、形容詞「ない」の語彙素が `無い`。
    ///
    /// 辞書が 1 語にまとめた語（NEologd の形容詞「できない」）の中の否定は数えない。
    pub fn is_negation(&self) -> bool {
        let base_form = &*self.base_form;
        match self.pos.split(',').next().unwrap_or("") {
            "助動詞" => NEGATIVE_AUX_VERBS.contains(&base_form),
            "形容詞" => NEGATIVE_ADJECTIVES.contains(&base_form),
            _ => false,
        }
    }

    /// モーラ数
    ///
    /// 発音から数え、発音に仮名が 1 字も無ければ（空、または表層形のままの壊れたエントリ）
    /// 読みから数える。拗音・外来音の小書き文字（ャュョァィゥェォヮ）は直前の仮名と合わせて
    /// 1 モーラ、促音「ッ」・撥音「ン」・長音「ー」はそれぞれ 1 モーラ。ひらがなはカタカナとして
    /// 数え、仮名以外の字（記号・英字・漢字・中黒）は数えない。読みの無いトークン（未知語の
    /// 漢字・数字の列、記号）は 0。
    pub fn mora_count(&self) -> usize {
        match count_morae(&self.pronunciation) {
            0 => count_morae(&self.reading),
            n => n,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(surface: &str, pos: &str, base_form: &str) -> Token {
        Token {
            surface: surface.into(),
            start: 0,
            end: surface.len(),
            pos: pos.into(),
            conj_type: "".into(),
            conj_form: "".into(),
            base_form: base_form.into(),
            reading: "".into(),
            pronunciation: "".into(),
            word_cost: 0,
            is_known: true,
        }
    }

    fn unknown(surface: &str, pos: &str) -> Token {
        Token {
            is_known: false,
            ..token(surface, pos, surface)
        }
    }

    fn spoken(reading: &str, pronunciation: &str) -> Token {
        Token {
            reading: reading.into(),
            pronunciation: pronunciation.into(),
            ..token("語", "名詞,一般,*,*", "語")
        }
    }

    /// 配布辞書（ipadic / ipadic-neologd / ipadic-neologd-sudachi）に現れる 69 の品詞
    const IPADIC_INVENTORY: &[(&str, CoarsePos)] = &[
        ("その他,間投,*,*", CoarsePos::Other),
        ("フィラー,*,*,*", CoarsePos::Interjection),
        ("副詞,一般,*,*", CoarsePos::Adverb),
        ("副詞,助詞類接続,*,*", CoarsePos::Adverb),
        ("助動詞,*,*,*", CoarsePos::AuxVerb),
        ("助詞,並立助詞,*,*", CoarsePos::OtherParticle),
        ("助詞,係助詞,*,*", CoarsePos::BindingParticle),
        ("助詞,副助詞,*,*", CoarsePos::OtherParticle),
        (
            "助詞,副助詞／並立助詞／終助詞,*,*",
            CoarsePos::OtherParticle,
        ),
        ("助詞,副詞化,*,*", CoarsePos::CaseParticle),
        ("助詞,接続助詞,*,*", CoarsePos::ConjunctiveParticle),
        ("助詞,格助詞,一般,*", CoarsePos::CaseParticle),
        ("助詞,格助詞,引用,*", CoarsePos::CaseParticle),
        ("助詞,格助詞,連語,*", CoarsePos::CaseParticle),
        ("助詞,特殊,*,*", CoarsePos::OtherParticle),
        ("助詞,終助詞,*,*", CoarsePos::FinalParticle),
        ("助詞,連体化,*,*", CoarsePos::CaseParticle),
        ("動詞,接尾,*,*", CoarsePos::Verb),
        ("動詞,自立,*,*", CoarsePos::Verb),
        ("動詞,非自立,*,*", CoarsePos::Verb),
        ("名詞,サ変接続,*,*", CoarsePos::Noun),
        ("名詞,ナイ形容詞語幹,*,*", CoarsePos::Noun),
        ("名詞,一般,*,*", CoarsePos::Noun),
        ("名詞,代名詞,一般,*", CoarsePos::Pronoun),
        ("名詞,代名詞,縮約,*", CoarsePos::Pronoun),
        ("名詞,副詞可能,*,*", CoarsePos::Noun),
        ("名詞,動詞非自立的,*,*", CoarsePos::Noun),
        ("名詞,固有名詞,一般,*", CoarsePos::ProperNoun),
        ("名詞,固有名詞,人名,一般", CoarsePos::ProperNoun),
        ("名詞,固有名詞,人名,名", CoarsePos::ProperNoun),
        ("名詞,固有名詞,人名,姓", CoarsePos::ProperNoun),
        ("名詞,固有名詞,地域,一般", CoarsePos::ProperNoun),
        ("名詞,固有名詞,地域,国", CoarsePos::ProperNoun),
        ("名詞,固有名詞,組織,*", CoarsePos::ProperNoun),
        ("名詞,引用文字列,*,*", CoarsePos::Noun),
        ("名詞,形容動詞語幹,*,*", CoarsePos::Noun),
        ("名詞,接尾,サ変接続,*", CoarsePos::NounSuffix),
        ("名詞,接尾,一般,*", CoarsePos::NounSuffix),
        ("名詞,接尾,人名,*", CoarsePos::NounSuffix),
        ("名詞,接尾,副詞可能,*", CoarsePos::NounSuffix),
        ("名詞,接尾,助動詞語幹,*", CoarsePos::AuxVerb),
        ("名詞,接尾,助数詞,*", CoarsePos::NounSuffix),
        ("名詞,接尾,地域,*", CoarsePos::NounSuffix),
        ("名詞,接尾,形容動詞語幹,*", CoarsePos::NounSuffix),
        ("名詞,接尾,特殊,*", CoarsePos::NounSuffix),
        ("名詞,接続詞的,*,*", CoarsePos::Noun),
        ("名詞,数,*,*", CoarsePos::Numeral),
        ("名詞,特殊,助動詞語幹,*", CoarsePos::AuxVerb),
        ("名詞,非自立,一般,*", CoarsePos::FormalNoun),
        ("名詞,非自立,副詞可能,*", CoarsePos::FormalNoun),
        ("名詞,非自立,助動詞語幹,*", CoarsePos::AuxVerb),
        ("名詞,非自立,形容動詞語幹,*", CoarsePos::FormalNoun),
        ("形容詞,接尾,*,*", CoarsePos::Adjective),
        ("形容詞,自立,*,*", CoarsePos::Adjective),
        ("形容詞,非自立,*,*", CoarsePos::Adjective),
        ("感動詞,*,*,*", CoarsePos::Interjection),
        ("接続詞,*,*,*", CoarsePos::Conjunction),
        ("接頭詞,動詞接続,*,*", CoarsePos::Prefix),
        ("接頭詞,名詞接続,*,*", CoarsePos::Prefix),
        ("接頭詞,形容詞接続,*,*", CoarsePos::Prefix),
        ("接頭詞,数接続,*,*", CoarsePos::Prefix),
        ("記号,アルファベット,*,*", CoarsePos::Symbol),
        ("記号,一般,*,*", CoarsePos::Symbol),
        ("記号,句点,*,*", CoarsePos::Period),
        ("記号,括弧閉,*,*", CoarsePos::CloseBracket),
        ("記号,括弧開,*,*", CoarsePos::OpenBracket),
        ("記号,空白,*,*", CoarsePos::Symbol),
        ("記号,読点,*,*", CoarsePos::Comma),
        ("連体詞,*,*,*", CoarsePos::Adnominal),
    ];

    /// UniDic（2.1.2 の left-id.def）の 52 の品詞
    const UNIDIC_INVENTORY: &[(&str, CoarsePos)] = &[
        ("代名詞,*,*,*", CoarsePos::Pronoun),
        ("副詞,*,*,*", CoarsePos::Adverb),
        ("助動詞,*,*,*", CoarsePos::AuxVerb),
        ("助詞,係助詞,*,*", CoarsePos::BindingParticle),
        ("助詞,副助詞,*,*", CoarsePos::OtherParticle),
        ("助詞,接続助詞,*,*", CoarsePos::ConjunctiveParticle),
        ("助詞,格助詞,*,*", CoarsePos::CaseParticle),
        ("助詞,準体助詞,*,*", CoarsePos::FormalNoun),
        ("助詞,終助詞,*,*", CoarsePos::FinalParticle),
        ("動詞,一般,*,*", CoarsePos::Verb),
        ("動詞,非自立可能,*,*", CoarsePos::Verb),
        ("名詞,助動詞語幹,*,*", CoarsePos::AuxVerb),
        ("名詞,固有名詞,一般,*", CoarsePos::ProperNoun),
        ("名詞,固有名詞,人名,一般", CoarsePos::ProperNoun),
        ("名詞,固有名詞,人名,名", CoarsePos::ProperNoun),
        ("名詞,固有名詞,人名,姓", CoarsePos::ProperNoun),
        ("名詞,固有名詞,地名,一般", CoarsePos::ProperNoun),
        ("名詞,固有名詞,地名,国", CoarsePos::ProperNoun),
        ("名詞,数詞,*,*", CoarsePos::Numeral),
        ("名詞,普通名詞,サ変可能,*", CoarsePos::Noun),
        ("名詞,普通名詞,サ変形状詞可能,*", CoarsePos::Noun),
        ("名詞,普通名詞,一般,*", CoarsePos::Noun),
        ("名詞,普通名詞,副詞可能,*", CoarsePos::Noun),
        ("名詞,普通名詞,助数詞可能,*", CoarsePos::Noun),
        ("名詞,普通名詞,形状詞可能,*", CoarsePos::Noun),
        ("形容詞,一般,*,*", CoarsePos::Adjective),
        ("形容詞,非自立可能,*,*", CoarsePos::Adjective),
        ("形状詞,タリ,*,*", CoarsePos::Noun),
        ("形状詞,一般,*,*", CoarsePos::Noun),
        ("形状詞,助動詞語幹,*,*", CoarsePos::AuxVerb),
        ("感動詞,フィラー,*,*", CoarsePos::Interjection),
        ("感動詞,一般,*,*", CoarsePos::Interjection),
        ("接尾辞,動詞的,*,*", CoarsePos::Verb),
        ("接尾辞,名詞的,サ変可能,*", CoarsePos::NounSuffix),
        ("接尾辞,名詞的,一般,*", CoarsePos::NounSuffix),
        ("接尾辞,名詞的,副詞可能,*", CoarsePos::NounSuffix),
        ("接尾辞,名詞的,助数詞,*", CoarsePos::NounSuffix),
        ("接尾辞,形容詞的,*,*", CoarsePos::Adjective),
        ("接尾辞,形状詞的,*,*", CoarsePos::NounSuffix),
        ("接続詞,*,*,*", CoarsePos::Conjunction),
        ("接頭辞,*,*,*", CoarsePos::Prefix),
        ("空白,*,*,*", CoarsePos::Symbol),
        ("補助記号,一般,*,*", CoarsePos::Symbol),
        ("補助記号,句点,*,*", CoarsePos::Period),
        ("補助記号,括弧閉,*,*", CoarsePos::CloseBracket),
        ("補助記号,括弧開,*,*", CoarsePos::OpenBracket),
        ("補助記号,読点,*,*", CoarsePos::Comma),
        ("補助記号,ＡＡ,一般,*", CoarsePos::Symbol),
        ("補助記号,ＡＡ,顔文字,*", CoarsePos::Symbol),
        ("記号,一般,*,*", CoarsePos::Symbol),
        ("記号,文字,*,*", CoarsePos::Symbol),
        ("連体詞,*,*,*", CoarsePos::Adnominal),
    ];

    fn split_pos(pos: &str) -> (&str, &str, &str) {
        let mut fields = pos.split(',');
        (
            fields.next().unwrap_or(""),
            fields.next().unwrap_or(""),
            fields.next().unwrap_or(""),
        )
    }

    #[test]
    fn test_every_ipadic_pos_is_mapped() {
        for &(pos, expected) in IPADIC_INVENTORY {
            let (pos1, pos2, _) = split_pos(pos);
            assert_eq!(PosScheme::detect(pos1, pos2), PosScheme::Ipadic, "{pos}");
            assert_eq!(token("語", pos, "語").coarse_pos(), expected, "{pos}");
        }
    }

    #[test]
    fn test_every_unidic_pos_is_mapped() {
        for &(pos, expected) in UNIDIC_INVENTORY {
            assert_eq!(token("語", pos, "語").coarse_pos(), expected, "{pos}");
        }
    }

    #[test]
    fn test_pos_shared_by_both_schemes_maps_to_the_same_value() {
        // UniDic の品詞のうち IPAdic と判定されるもの（両体系にある組）は、IPAdic の表でも
        // UniDic の表でも同じ値になる。だから体系の判定を IPAdic に倒してよい
        let mut shared = 0;
        for &(pos, _) in UNIDIC_INVENTORY {
            let (pos1, pos2, pos3) = split_pos(pos);
            if PosScheme::detect(pos1, pos2) == PosScheme::Ipadic {
                shared += 1;
                assert_eq!(
                    ipadic_coarse_pos(pos1, pos2, pos3, "語"),
                    unidic_coarse_pos(pos1, pos2, pos3, "語"),
                    "{pos}"
                );
            }
        }
        assert!(shared > 0);
    }

    #[test]
    fn test_no_is_a_case_particle_in_both_schemes() {
        // 「AのB」の IPAdic の連体化・格助詞、UniDic の格助詞
        for pos in ["助詞,連体化,*,*", "助詞,格助詞,一般,*", "助詞,格助詞,*,*"] {
            assert_eq!(
                token("の", pos, "の").coarse_pos(),
                CoarsePos::CaseParticle,
                "{pos}"
            );
        }
        // 「行くのが」の「の」は形式名詞
        for pos in ["名詞,非自立,一般,*", "助詞,準体助詞,*,*"] {
            assert_eq!(
                token("の", pos, "の").coarse_pos(),
                CoarsePos::FormalNoun,
                "{pos}"
            );
        }
        // 「行くの？」の「の」は終助詞
        assert_eq!(
            token("の", "助詞,終助詞,*,*", "の").coarse_pos(),
            CoarsePos::FinalParticle
        );
    }

    #[test]
    fn test_passive_and_causative_suffixes_are_aux_verbs() {
        for base_form in ["れる", "られる", "せる", "させる", "しめる", "す", "さす"]
        {
            assert_eq!(
                token("れ", "動詞,接尾,*,*", base_form).coarse_pos(),
                CoarsePos::AuxVerb,
                "{base_form}"
            );
        }
        // 自立の動詞「する」の活用形「さ」「し」は動詞のまま
        assert_eq!(
            token("さ", "動詞,自立,*,*", "する").coarse_pos(),
            CoarsePos::Verb
        );
        // 「寒がる」の「がる」は動詞のまま（UniDic の 接尾辞,動詞的 と同じ）
        assert_eq!(
            token("がる", "動詞,接尾,*,*", "がる").coarse_pos(),
            CoarsePos::Verb
        );
        // UniDic の受け身は助動詞
        assert_eq!(
            token("れる", "助動詞,*,*,*", "れる").coarse_pos(),
            CoarsePos::AuxVerb
        );
    }

    #[test]
    fn test_auxiliary_verb_stems_are_aux_verbs_in_both_schemes() {
        for (surface, pos) in [
            // IPAdic: 伝聞・様態の「そう」、「よう」、形容動詞語幹の「みたい」
            ("そう", "名詞,特殊,助動詞語幹,*"),
            ("そう", "名詞,接尾,助動詞語幹,*"),
            ("よう", "名詞,非自立,助動詞語幹,*"),
            ("みたい", "名詞,非自立,形容動詞語幹,*"),
            // UniDic
            ("そう", "名詞,助動詞語幹,*,*"),
            ("そう", "形状詞,助動詞語幹,*,*"),
            ("よう", "形状詞,助動詞語幹,*,*"),
            ("みたい", "形状詞,助動詞語幹,*,*"),
        ] {
            assert_eq!(
                token(surface, pos, surface).coarse_pos(),
                CoarsePos::AuxVerb,
                "{surface} {pos}"
            );
        }
        // 「こんなふうに」の「ふう」は形式名詞（IPAdic は「みたい」と同じ品詞、UniDic は普通名詞）
        for pos in ["名詞,非自立,形容動詞語幹,*", "名詞,普通名詞,形状詞可能,*"]
        {
            assert_eq!(
                token("ふう", pos, "ふう").coarse_pos(),
                CoarsePos::FormalNoun,
                "{pos}"
            );
        }
    }

    #[test]
    fn test_unidic_formal_nouns_are_found_by_surface() {
        for (surface, pos) in [
            ("こと", "名詞,普通名詞,一般,*"),
            ("もの", "名詞,普通名詞,サ変可能,*"),
            ("わけ", "名詞,普通名詞,一般,*"),
            ("ため", "名詞,普通名詞,副詞可能,*"),
            ("とおり", "名詞,普通名詞,助数詞可能,*"),
        ] {
            assert_eq!(
                token(surface, pos, surface).coarse_pos(),
                CoarsePos::FormalNoun,
                "{surface}"
            );
        }
        // 漢字書きの「事」と、形式名詞でない普通名詞
        assert_eq!(
            token("事", "名詞,普通名詞,一般,*", "事").coarse_pos(),
            CoarsePos::Noun
        );
        assert_eq!(
            token("部屋", "名詞,普通名詞,一般,*", "部屋").coarse_pos(),
            CoarsePos::Noun
        );
        // IPAdic の 名詞,一般 の「こと」は表層形で見分けない（辞書が非自立と分けている）
        assert_eq!(
            token("こと", "名詞,一般,*,*", "こと").coarse_pos(),
            CoarsePos::Noun
        );
    }

    #[test]
    fn test_symbol_subcategories_follow_the_dictionary() {
        for (surface, pos, expected) in [
            ("。", "記号,句点,*,*", CoarsePos::Period),
            ("、", "記号,読点,*,*", CoarsePos::Comma),
            ("「", "記号,括弧開,*,*", CoarsePos::OpenBracket),
            ("」", "記号,括弧閉,*,*", CoarsePos::CloseBracket),
            ("。", "補助記号,句点,*,*", CoarsePos::Period),
            ("、", "補助記号,読点,*,*", CoarsePos::Comma),
            ("「", "補助記号,括弧開,*,*", CoarsePos::OpenBracket),
            ("」", "補助記号,括弧閉,*,*", CoarsePos::CloseBracket),
            // ASCII の `.` は辞書が句点にしていれば句点（SudachiDict を足した辞書）
            (".", "記号,句点,*,*", CoarsePos::Period),
            ("・", "記号,一般,*,*", CoarsePos::Symbol),
            ("…", "補助記号,一般,*,*", CoarsePos::Symbol),
            ("　", "記号,空白,*,*", CoarsePos::Symbol),
            ("　", "空白,*,*,*", CoarsePos::Symbol),
        ] {
            assert_eq!(
                token(surface, pos, surface).coarse_pos(),
                expected,
                "{surface} {pos}"
            );
        }
    }

    #[test]
    fn test_generic_symbols_are_classified_by_surface() {
        // IPAdic 系の 記号,一般 と UniDic の 補助記号,一般・記号,一般 の句点・読点・括弧
        for (surface, expected) in [
            ("！", CoarsePos::Period),
            ("？", CoarsePos::Period),
            ("!?", CoarsePos::Period),
            ("‼", CoarsePos::Period),
            ("､", CoarsePos::Comma),
            (",", CoarsePos::Comma),
            ("(", CoarsePos::OpenBracket),
            ("｢", CoarsePos::OpenBracket),
            ("〝", CoarsePos::OpenBracket),
            (")", CoarsePos::CloseBracket),
            ("｣", CoarsePos::CloseBracket),
            ("〞", CoarsePos::CloseBracket),
            // 句点と括弧が混ざった記号や、句読点でない記号はそのまま
            ("()", CoarsePos::Symbol),
            (".", CoarsePos::Symbol),
            ("…", CoarsePos::Symbol),
        ] {
            for pos in ["記号,一般,*,*", "補助記号,一般,*,*", "記号,文字,*,*"] {
                assert_eq!(
                    token(surface, pos, surface).coarse_pos(),
                    expected,
                    "{surface} {pos}"
                );
            }
        }
    }

    #[test]
    fn test_unknown_symbols_are_symbols_whatever_the_pos() {
        // IPAdic の unk.def は記号の未知語を 名詞,サ変接続 にする
        for (surface, expected) in [
            ("＠", CoarsePos::Symbol),
            ("※", CoarsePos::Symbol),
            ("!", CoarsePos::Period),
            ("(", CoarsePos::OpenBracket),
            (")", CoarsePos::CloseBracket),
            (",", CoarsePos::Comma),
        ] {
            assert_eq!(
                unknown(surface, "名詞,サ変接続,*,*").coarse_pos(),
                expected,
                "{surface}"
            );
        }
        // 文字・数字を含む未知語は品詞に従う
        assert_eq!(
            unknown("ｶﾀｶﾅ", "名詞,一般,*,*").coarse_pos(),
            CoarsePos::Noun
        );
        assert_eq!(
            unknown("2026", "名詞,数,*,*").coarse_pos(),
            CoarsePos::Numeral
        );
        assert_eq!(
            unknown("C++", "名詞,固有名詞,組織,*").coarse_pos(),
            CoarsePos::ProperNoun
        );
        // 辞書にある語は表層形が記号だけでも辞書の品詞に従う（SudachiDict の絵文字の名詞など）
        assert_eq!(
            token("＠", "名詞,サ変接続,*,*", "＠").coarse_pos(),
            CoarsePos::Noun
        );
    }

    #[test]
    fn test_unrecognized_pos_is_other() {
        for pos in ["", "*,*,*,*", "未知の品詞,*,*,*"] {
            assert_eq!(
                token("語", pos, "語").coarse_pos(),
                CoarsePos::Other,
                "{pos:?}"
            );
        }
        // 細分類の欠けた品詞でも大分類で決まる
        assert_eq!(token("語", "名詞", "語").coarse_pos(), CoarsePos::Noun);
    }

    #[test]
    fn test_negation_by_base_form() {
        for (surface, pos, base_form) in [
            // IPAdic 系
            ("ない", "助動詞,*,*,*", "ない"),
            ("なかっ", "助動詞,*,*,*", "ない"),
            ("ん", "助動詞,*,*,*", "ん"),
            ("ぬ", "助動詞,*,*,*", "ぬ"),
            ("ず", "助動詞,*,*,*", "ぬ"),
            ("ざる", "助動詞,*,*,*", "ぬ"),
            ("無けれ", "助動詞,*,*,*", "無い"),
            ("へん", "助動詞,*,*,*", "へん"),
            ("なかっ", "形容詞,自立,*,*", "ない"),
            ("無い", "形容詞,自立,*,*", "無い"),
            // UniDic
            ("ん", "助動詞,*,*,*", "ず"),
            ("ぬ", "助動詞,*,*,*", "ず"),
            ("ない", "助動詞,*,*,*", "ない"),
            ("ない", "形容詞,非自立可能,*,*", "無い"),
            ("なかれ", "形容詞,非自立可能,*,*", "無い"),
        ] {
            assert!(
                token(surface, pos, base_form).is_negation(),
                "{surface} {pos} {base_form}"
            );
        }
        for (surface, pos, base_form) in [
            ("ませ", "助動詞,*,*,*", "ます"),
            ("まい", "助動詞,*,*,*", "まい"),
            ("少ない", "形容詞,自立,*,*", "少ない"),
            // 否定を含んで 1 語になった語（NEologd の「できない」）は数えない
            ("できない", "形容詞,非自立,*,*", "できない"),
            // 「ない」と同じ字の別の語
            ("ない", "動詞,自立,*,*", "なう"),
            ("ん", "名詞,非自立,一般,*", "ん"),
            ("ん", "助詞,準体助詞,*,*", "の"),
            ("ず", "動詞,自立,*,*", "ずる"),
        ] {
            assert!(
                !token(surface, pos, base_form).is_negation(),
                "{surface} {pos} {base_form}"
            );
        }
    }

    #[test]
    fn test_mora_count_of_katakana() {
        for (pronunciation, expected) in [
            ("ア", 1),
            ("キャ", 1),
            ("キョウ", 2),
            ("トーキョー", 4),
            ("ガッコウ", 4),
            ("シンブン", 4),
            ("ファイル", 3),
            ("ヴァイオリン", 5),
            ("ティー", 2),
            ("クヮ", 1),
            ("ヶ", 1),
            ("ッ", 1),
            // 合わせる仮名が無い小書き文字は単独で 1 モーラ
            ("ァ", 1),
            ("ッァ", 2),
            ("アァァ", 2),
        ] {
            assert_eq!(
                spoken("", pronunciation).mora_count(),
                expected,
                "{pronunciation}"
            );
        }
    }

    #[test]
    fn test_mora_count_ignores_non_kana_and_accepts_hiragana() {
        assert_eq!(spoken("", "きょう").mora_count(), 2);
        assert_eq!(spoken("", "がっこう").mora_count(), 4);
        assert_eq!(spoken("", "ジョン・スミス").mora_count(), 5);
        assert_eq!(spoken("", "。").mora_count(), 0);
        assert_eq!(spoken("", "").mora_count(), 0);
    }

    #[test]
    fn test_mora_count_falls_back_to_reading() {
        // 発音が空なら読み
        assert_eq!(spoken("キョウ", "").mora_count(), 2);
        // 発音が表層形のまま（仮名が無い）なら読み
        assert_eq!(spoken("ホウホウ", "方法").mora_count(), 4);
        // 発音があれば発音（助詞「は」の発音は「ワ」）
        assert_eq!(spoken("トウキョウ", "トーキョー").mora_count(), 4);
        assert_eq!(spoken("ハ", "ワ").mora_count(), 1);
    }

    /// 配布辞書のパス。`dict/` は Git LFS で管理している
    fn distributed_dicts() -> Vec<(&'static str, crate::Analyzer)> {
        ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]
            .into_iter()
            .map(|name| {
                let path = format!("{}/dict/{name}.hsd", env!("CARGO_MANIFEST_DIR"));
                let analyzer = crate::Analyzer::load(&path)
                    .unwrap_or_else(|e| panic!("{path}: {e}（Git LFS の辞書を取得したか確認）"));
                (name, analyzer)
            })
            .collect()
    }

    fn describe(tokens: &[Token]) -> String {
        tokens
            .iter()
            .map(|t| {
                format!(
                    "{}({:?}{}, {}モーラ)",
                    t.surface,
                    t.coarse_pos(),
                    if t.is_negation() { ", 否定" } else { "" },
                    t.mora_count()
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    #[ignore = "配布辞書（dict/*.hsd）を使う。cargo test -- --ignored で実行"]
    fn test_acceptance_with_distributed_dictionaries() {
        for (name, mut analyzer) in distributed_dicts() {
            // 「の」は 助詞,連体化 でも 助詞,格助詞 でも格助詞
            for text in [
                "AのBのC",
                "運用コストの削減の実現",
                "けいおん!の話",
                "モーニング娘。のライブ",
            ] {
                let tokens = analyzer.tokenize(text);
                println!("{name}: {text} → {}", describe(&tokens));
                let no: Vec<&Token> = tokens.iter().filter(|t| &*t.surface == "の").collect();
                assert!(!no.is_empty(), "{name}: {text}");
                for t in no {
                    assert_eq!(
                        t.coarse_pos(),
                        CoarsePos::CaseParticle,
                        "{name}: {text}: {}",
                        t.pos
                    );
                }
            }
            for text in [
                "言えません",
                "知らぬ",
                "行かない",
                "行かなかった",
                "行かず",
                "お金がなかった",
            ] {
                let tokens = analyzer.tokenize(text);
                println!("{name}: {text} → {}", describe(&tokens));
                assert!(tokens.iter().any(Token::is_negation), "{name}: {text}");
            }
        }
    }

    #[test]
    #[ignore = "配布辞書（dict/*.hsd）を使う。cargo test -- --ignored で実行"]
    fn test_distributed_dictionaries_agree_on_coarse_pos() {
        // 3 つの辞書で記号の品詞が違う字（半角の `(` `!` `,`、全角の `！`）も、同じ粗い品詞になる
        let texts = [
            "注意(重要)!本当?すごい！1,000円。",
            "することができる。",
            "示されるものだ。",
            "雨が降りそうだ。",
        ];
        let mut expected: Vec<Vec<(String, CoarsePos)>> = Vec::new();
        for (name, mut analyzer) in distributed_dicts() {
            for (i, text) in texts.iter().enumerate() {
                let tokens = analyzer.tokenize(text);
                println!("{name}: {text} → {}", describe(&tokens));
                let got: Vec<(String, CoarsePos)> = tokens
                    .iter()
                    .map(|t| (t.surface.to_string(), t.coarse_pos()))
                    .collect();
                match expected.get(i) {
                    Some(first) => assert_eq!(&got, first, "{name}: {text}"),
                    None => expected.push(got),
                }
            }
        }
        let has = |i: usize, surface: &str, coarse: CoarsePos| {
            expected[i].contains(&(surface.to_string(), coarse))
        };
        // 記号は表層形で句点・読点・括弧に分かれる
        assert!(has(0, "(", CoarsePos::OpenBracket));
        assert!(has(0, "!", CoarsePos::Period));
        assert!(has(0, "！", CoarsePos::Period));
        assert!(has(0, ",", CoarsePos::Comma));
        // 「こと」は形式名詞、受け身の「れる」と助動詞の語幹「そう」は助動詞
        assert!(has(1, "こと", CoarsePos::FormalNoun));
        assert!(has(2, "れる", CoarsePos::AuxVerb));
        assert!(has(3, "そう", CoarsePos::AuxVerb));
    }

    #[test]
    fn test_unit_symbols_are_noun_suffixes() {
        // 単位の記号は、辞書の語（名詞,接尾,助数詞）でも、記号の語・未知語（IPAdic の 記号,一般、UniDic の
        // 補助記号,一般）でも名詞の接尾辞
        for surface in [
            "%", "％", "‰", "℃", "℉", "°", "㎏", "㎞", "㌢", "㍍", "㎡", "㏄", "㍱",
        ] {
            for t in [
                token(surface, "名詞,接尾,助数詞,*", surface),
                token(surface, "記号,一般,*,*", surface),
                token(surface, "補助記号,一般,*,*", surface),
                unknown(surface, "記号,一般,*,*"),
                unknown(surface, "名詞,サ変接続,*,*"),
            ] {
                assert_eq!(t.coarse_pos(), CoarsePos::NounSuffix, "{surface} {}", t.pos);
            }
        }
        // 単位でない記号・組文字はそのまま
        for surface in ["＄", "′", "㍻", "㍿", "㏂", "㌀", "※"] {
            assert_eq!(
                unknown(surface, "記号,一般,*,*").coarse_pos(),
                CoarsePos::Symbol,
                "{surface}"
            );
        }
        // 句点・括弧と混ざれば記号
        assert_eq!(
            unknown("%。", "記号,一般,*,*").coarse_pos(),
            CoarsePos::Symbol
        );
    }
}
