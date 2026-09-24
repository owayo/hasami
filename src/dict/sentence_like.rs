//! 文や句を 1 語にした名詞の判定（`hasami repair --drop-sentence-like-nouns`）
//!
//! NEologd は曲名・作品名・キャッチフレーズとして文や句そのもの（「どうでしょう」「作りました」「好きだ。」）を
//! 固有名詞 1 語で登録し、表記ゆれの seed は漢字語をかなで書いた語（「ありません」= 有馬線、「回ろう」= 回廊、
//! 「および」= お呼び）を機械的に作る。これらが文中の動詞・助動詞・助詞の並びに勝つと、文末の品詞や句点の位置が
//! 崩れる。
//!
//! 判定には、表層形を参照辞書（IPAdic 単体）で解析した語の列を使う。IPAdic はひらがなの名前やかな書きの漢語を
//! 機能語のでたらめな並びに分ける（「ひなた」→ ひな/た、「けんしょう」→ けんしょ/う）ので、語の列が文法に合う
//! （活用形と付属語の付き方が合う）ときだけ文や句とみなす。規則の選び方は README の「文や句の名詞の削除」。

use crate::lattice::Token;
use crate::sentence::is_sentence_ender;

/// 文や句を 1 語にした名詞とみなした理由
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SentenceLikeReason {
    /// 文末記号で終わり、ほかに文字の無い語（「…。」「？？？」）
    SymbolsWithEnder,
    /// 助詞・助動詞・非自立名詞だけが文法どおりに並ぶ語（「なのか」「にも」「ことも」）
    FunctionWords,
    /// 感動詞 1 語（「こんにちは。」「おはよう。」）
    Interjection,
    /// 表記ゆれで、副詞・接続詞・連体詞か終止形・命令形の用言 1 語（「および」「きっと」「うまい」）
    VariantWord,
    /// 述語（終止形・命令形の用言、助動詞、終助詞）で終わる文（「どうでしょう」「作りました」「好きだ。」）。
    /// 文末記号で終わる文は内容語が 1 つ以下のものだけ（「やはり俺の青春ラブコメはまちがっている。」は残す）
    Predicate,
    /// 活用語 + 助詞で終わる句（「じゃなくて」「しながら」「世界を敵に回しても」）
    ConjugatedParticle,
    /// 内容語 1 つ + 助詞の句（「一緒に」「あなたに」「人として」）
    NounParticle,
}

impl SentenceLikeReason {
    /// すべての理由（統計に出す順）
    pub const ALL: [SentenceLikeReason; 7] = [
        Self::Predicate,
        Self::ConjugatedParticle,
        Self::NounParticle,
        Self::FunctionWords,
        Self::Interjection,
        Self::VariantWord,
        Self::SymbolsWithEnder,
    ];

    /// ログに出す名前
    pub fn name(self) -> &'static str {
        match self {
            Self::SymbolsWithEnder => "symbols-with-ender",
            Self::FunctionWords => "function-words",
            Self::Interjection => "interjection",
            Self::VariantWord => "variant-word",
            Self::Predicate => "predicate",
            Self::ConjugatedParticle => "conjugated-particle",
            Self::NounParticle => "noun-particle",
        }
    }
}

/// 表層形 1 つの判定。エントリの種類ごとに結果が違う
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Verdict {
    /// 固有名詞（人名・地域を除く。原形が表層形と同じ語）。曲名・作品名として文や句が登録される
    pub proper: Option<SentenceLikeReason>,
    /// 人名・地域（名詞,固有名詞,人名 / 名詞,固有名詞,地域）。ひらがなの名前・地名が機能語や
    /// 活用形のでたらめな並びに分かれやすい（「ゆうか」→ ゆう/か、「やよい」→ や/よい）
    pub name: Option<SentenceLikeReason>,
    /// 固有名詞でない名詞（原形が表層形と同じ語）。SudachiDict の「早死に」「本だな」「雨がえる」のような
    /// 送り仮名・かな書きを含む普通名詞が、助詞や活用形の並びに分かれる
    pub common: Option<SentenceLikeReason>,
    /// 表記ゆれ（原形が表層形と違う語）
    pub variant: Option<SentenceLikeReason>,
}

/// エントリの種類（判定の範囲が違う）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryKind {
    /// 固有名詞（人名・地域を除く）: 文や句のすべての判定
    Proper,
    /// 人名・地域: 機能語だけの並びと、文末記号で終わる語だけ
    Name,
    /// 固有名詞でない名詞: 機能語だけの並びと感動詞だけ
    Common,
    /// 表記ゆれ: 名詞を含まない並びと、1 語の副詞・接続詞・連体詞・用言
    Variant,
}

/// 判定に使う、解析結果の 1 語の素性
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Morph<'a> {
    pub surface: &'a str,
    pub pos1: &'a str,
    pub pos2: &'a str,
    pub conj_type: &'a str,
    pub conj_form: &'a str,
    pub base: &'a str,
    pub known: bool,
}

impl<'a> Morph<'a> {
    pub(crate) fn from_token(t: &'a Token) -> Self {
        let mut pos = t.pos.split(',');
        Morph {
            surface: &t.surface,
            pos1: pos.next().unwrap_or(""),
            pos2: pos.next().unwrap_or(""),
            conj_type: &t.conj_type,
            conj_form: &t.conj_form,
            base: &t.base_form,
            known: t.is_known,
        }
    }

    fn is(&self, pos1: &str, pos2: &str) -> bool {
        self.pos1 == pos1 && self.pos2 == pos2
    }

    /// 動詞・形容詞・助動詞（活用する語）
    fn is_conjugating(&self) -> bool {
        matches!(self.pos1, "動詞" | "形容詞" | "助動詞")
    }

    /// 終止形・命令形
    fn in_final_form(&self) -> bool {
        FINAL_FORMS.contains(&self.conj_form)
    }

    fn renyou(&self) -> bool {
        self.conj_form.starts_with("連用")
    }

    fn mizen(&self) -> bool {
        self.conj_form.starts_with("未然")
    }

    /// 終助詞（「か」は IPAdic で 副助詞／並立助詞／終助詞）
    fn is_final_particle(&self) -> bool {
        self.pos1 == "助詞"
            && matches!(self.pos2, "終助詞" | "副助詞／並立助詞／終助詞")
            && FINAL_PARTICLES.contains(&self.surface)
    }

    /// ひらがなだけの普通名詞・固有名詞。ひらがなの名前の断片になりやすいので、だ・です・格助詞の付き先や
    /// する の前の語として当てにしない（「なみだ」→ なみ/だ、「おいも」→ おい/も）
    fn is_weak_noun(&self) -> bool {
        self.pos1 == "名詞"
            && !matches!(
                self.pos2,
                "代名詞"
                    | "非自立"
                    | "形容動詞語幹"
                    | "サ変接続"
                    | "副詞可能"
                    | "数"
                    | "ナイ形容詞語幹"
            )
            && is_hiragana_word(self.surface)
    }

    /// 付属語（前の語に付く語）
    fn is_dependent(&self) -> bool {
        matches!(self.pos1, "助動詞" | "助詞")
            || (matches!(self.pos1, "動詞" | "形容詞") && matches!(self.pos2, "非自立" | "接尾"))
            || self.is("名詞", "非自立")
    }

    /// 内容語（助詞で終わる句の長さを数える単位）
    fn is_content_word(&self) -> bool {
        match self.pos1 {
            "名詞" => !matches!(self.pos2, "接尾" | "非自立"),
            "動詞" | "形容詞" => self.pos2 == "自立",
            "副詞" | "連体詞" | "接続詞" | "感動詞" => true,
            _ => false,
        }
    }
}

/// 文を終えられる活用形
const FINAL_FORMS: [&str; 5] = ["基本形", "命令ｅ", "命令ｒｏ", "命令ｙｏ", "命令ｉ"];

/// 終助詞として認める語（IPAdic の終助詞のうち、方言・文語・片仮名書きを除く）
const FINAL_PARTICLES: [&str; 17] = [
    "か",
    "かい",
    "かしら",
    "さ",
    "ぜ",
    "ぞ",
    "っけ",
    "な",
    "なあ",
    "なぁ",
    "ね",
    "ねえ",
    "ねぇ",
    "の",
    "よ",
    "わ",
    "もん",
];

/// ひらがなだけの語で文末として認める終助詞（「さ」「わ」「ぞ」などは名前の末尾と区別できない）
const HIRAGANA_FINAL_PARTICLES: [&str; 6] = ["か", "かしら", "な", "ね", "よ", "の"];

/// 終助詞の後ろに続けてよい終助詞の組（「かな」「よね」「のか」）
const FINAL_PARTICLE_PAIRS: [(&str, &str); 10] = [
    ("か", "な"),
    ("か", "ね"),
    ("よ", "ね"),
    ("よ", "な"),
    ("わ", "よ"),
    ("わ", "ね"),
    ("の", "よ"),
    ("の", "ね"),
    ("の", "か"),
    ("ぞ", "よ"),
];

/// 「て」の後ろにだけ付く補助動詞・補助形容詞（原形）
const SUBSIDIARY_AFTER_TE: [&str; 16] = [
    "いる",
    "おる",
    "ある",
    "いく",
    "くる",
    "みる",
    "おく",
    "しまう",
    "あげる",
    "くれる",
    "もらう",
    "やる",
    "いただく",
    "くださる",
    "ほしい",
    "いい",
];

/// 助動詞の種類（付き先の活用形で分ける）。文語・方言の助動詞（る・り・じ・まい・や など）は認めない
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Aux {
    /// た（音便の後ろの だ を含む）: 連用形に付く
    Past,
    /// ます: 連用形に付く
    Polite,
    /// たい: 連用形に付く
    Desire,
    /// ない・ぬ・ん: 未然形に付く
    Negative,
    /// う・よう: 未然形に付く
    Volitional,
    /// だ・です・らしい: 名詞などに付く
    Copula,
}

fn aux_kind(m: &Morph) -> Option<Aux> {
    if m.pos1 != "助動詞" {
        return None;
    }
    Some(match (m.conj_type, m.base) {
        ("特殊・タ", _) => Aux::Past,
        ("特殊・マス", "ます") => Aux::Polite,
        ("特殊・タイ", "たい") => Aux::Desire,
        ("特殊・ナイ", "ない") | ("特殊・ヌ", "ぬ") | ("不変化型", "ぬ" | "ん") => {
            Aux::Negative
        }
        ("不変化型", "う" | "よう") => Aux::Volitional,
        ("特殊・ダ", "だ") | ("特殊・デス", "です") | ("形容詞・イ段", "らしい") => {
            Aux::Copula
        }
        _ => return None,
    })
}

/// ひらがな（U+3041〜U+309F）
fn is_hiragana(c: char) -> bool {
    ('\u{3041}'..='\u{309F}').contains(&c)
}

/// ひらがなと長音符だけの語
fn is_hiragana_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| is_hiragana(c) || c == 'ー')
}

/// 漢字（CJK 統合漢字と拡張 A、々）
fn is_kanji(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c) || c == '々'
}

/// 文末の命令形として当てにならない動詞か
///
/// ひらがな 2 字以下の自立動詞の命令形（する・なる を除く）は、名前や名詞の断片と同じ形になる
/// （「幾花にいろ」の いろ、「ジャガーともひろ」の ひろ、「アニーよ銃をとれ」の とれ）。
/// する の命令形「せい」も方言の形なので認めない（「無とうせい」→ 無/とう/せい）。
fn is_doubtful_imperative(m: &Morph) -> bool {
    m.pos1 == "動詞"
        && m.conj_form.starts_with("命令")
        && ((m.pos2 == "自立"
            && is_hiragana_word(m.surface)
            && m.surface.chars().count() <= 2
            && !matches!(m.base, "する" | "なる"))
            || (m.base == "する" && m.conj_form == "命令ｉ"))
}

/// 表層形が判定の対象になるか（ひらがなか文末記号を含む。含まない語は IPAdic でも名詞・未知語になる）
pub(crate) fn is_candidate_surface(surface: &str) -> bool {
    surface
        .chars()
        .any(|c| is_hiragana(c) || is_sentence_ender(c))
}

/// 全角の英数字・記号を半角に畳む（原形との比較で「いいね！」と「いいね!」を同じとみなす）
fn fold_width(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        _ => c,
    }
}

/// 表記ゆれのエントリ（原形が表層形と違う）か
///
/// 字幅の違いと末尾の文末記号の有無は同じとみなす（「いいね！」の原形「いいね!」、NEologd が作品名の句点を
/// 落として作る「モーニング娘」の原形「モーニング娘。」は表記ゆれでない）。
pub(crate) fn is_variant(surface: &str, base_form: &str) -> bool {
    let key = |s: &str| -> Vec<char> {
        s.trim_end_matches(is_sentence_ender)
            .chars()
            .map(fold_width)
            .collect()
    };
    key(surface) != key(base_form)
}

/// 「た」（`te` なら「て」）が付く形か
///
/// 音便のある五段動詞（サ行以外）は連用タ接続（書い/た、読ん/だ、待っ/て）にだけ付き、連用形（書き/て）には
/// 付かない。形容詞は連用タ接続（高かっ/た）・連用テ接続（高く/て）に付く。
fn takes_ta_te(prev: &Morph, te: bool) -> bool {
    match prev.pos1 {
        "動詞" if prev.conj_type.starts_with("五段") && !prev.conj_type.contains("サ行") => {
            prev.conj_form == "連用タ接続"
        }
        "動詞" => matches!(prev.conj_form, "連用形" | "連用タ接続"),
        "形容詞" => {
            prev.conj_form
                == if te {
                    "連用テ接続"
                } else {
                    "連用タ接続"
                }
        }
        "助動詞" => prev.renyou(),
        _ => false,
    }
}

/// 現代語の活用か（文語の下二段・上二段・四段・文語活用の動詞は、名前のでたらめな分割に多いので認めない）
fn is_modern(m: &Morph) -> bool {
    !["下二", "上二", "四段", "文語"]
        .iter()
        .any(|p| m.conj_type.starts_with(p))
}

/// `cur` が直前の語 `prev` に付けるか（活用形・品詞で決める）
fn attaches(prev: &Morph, cur: &Morph) -> bool {
    if !prev.known || !cur.known {
        return false;
    }
    if let Some(aux) = aux_kind(cur) {
        return match aux {
            // た は連用形・音便形に、だ は撥音便・イ音便（読ん/だ、泳い/だ）にだけ付く
            // （「赤めだか」→ 赤め/だ/か、「こんた」のような音便と合わない並びを除く）
            Aux::Past => {
                takes_ta_te(prev, false)
                    && if cur.surface.starts_with('だ') {
                        prev.conj_form == "連用タ接続"
                            && (prev.surface.ends_with('ん') || prev.surface.ends_with('い'))
                    } else {
                        !prev.surface.ends_with('ん')
                    }
            }
            Aux::Polite | Aux::Desire => {
                matches!(prev.pos1, "動詞" | "助動詞") && prev.conj_form == "連用形"
            }
            // ない・ぬ・ん は未然形に（「曲ろ/ん」の未然ウ接続には付かない）
            Aux::Negative => {
                matches!(prev.pos1, "動詞" | "助動詞")
                    && matches!(prev.conj_form, "未然形" | "未然ヌ接続" | "未然特殊")
            }
            // う は未然ウ接続（書こ/う）か助動詞の未然形（でしょ/う、だろ/う）に、よう は未然形にも付く
            Aux::Volitional => {
                matches!(prev.pos1, "動詞" | "助動詞")
                    && (prev.conj_form == "未然ウ接続"
                        || (prev.conj_form == "未然形"
                            && (prev.pos1 == "助動詞" || cur.base == "よう")))
            }
            Aux::Copula => match prev.pos1 {
                "名詞" => !prev.is_weak_noun(),
                "副詞" => true,
                "動詞" | "形容詞" | "助動詞" => {
                    prev.in_final_form() || prev.conj_form == "体言接続"
                }
                // 「からだ」「だけだ」「かだ」「私もだ」。係助詞の後ろは も・こそ だけで、格助詞 が・を・の と
                // 終助詞の後ろには来ない（「はだ」→ は/だ は かな書きの語の断片）
                "助詞" => match prev.pos2 {
                    "副助詞" | "副助詞／並立助詞／終助詞" => true,
                    "格助詞" => !matches!(prev.surface, "が" | "を" | "の"),
                    "係助詞" => matches!(prev.surface, "も" | "こそ"),
                    _ => false,
                },
                _ => false,
            },
        };
    }
    if cur.pos1 == "助動詞" {
        return false; // 文語・方言の助動詞
    }
    if cur.pos1 == "助詞" {
        if matches!(cur.pos2, "終助詞" | "副助詞／並立助詞／終助詞") {
            if !cur.is_final_particle() {
                return false;
            }
            return match prev.pos1 {
                "動詞" | "形容詞" | "助動詞" => prev.in_final_form(),
                "助詞" => FINAL_PARTICLE_PAIRS.contains(&(prev.surface, cur.surface)),
                _ => prev.is("名詞", "非自立"),
            };
        }
        if cur.pos2 == "接続助詞" {
            return match cur.surface {
                "て" | "で" | "ても" | "でも" => {
                    // 撥音便の後ろは「で」、促音便の後ろは「て」
                    takes_ta_te(prev, true)
                        && !(prev.surface.ends_with('ん') && cur.surface.starts_with('て'))
                        && !(prev.surface.ends_with('っ') && cur.surface.starts_with('で'))
                }
                "ながら" | "つつ" => {
                    matches!(prev.pos1, "動詞" | "助動詞") && prev.conj_form == "連用形"
                }
                "ば" => prev.is_conjugating() && prev.conj_form.starts_with("仮定"),
                "のに" | "ので" => {
                    prev.is_conjugating() && (prev.in_final_form() || prev.conj_form == "体言接続")
                }
                _ => prev.is_conjugating() && prev.in_final_form(),
            };
        }
        if cur.pos2 == "連体化" {
            return false; // 「の」で句は終わらず、述語の前にも来ない
        }
        // 格助詞・係助詞・副助詞・並立助詞・副詞化
        return match prev.pos1 {
            "名詞" => !prev.is_weak_noun(),
            "副詞" => true,
            "助詞" => particle_follows(prev, cur),
            // だ・です の後ろは引用の「と」と並立助詞・副助詞（「そうだと」「だの」「だなんて」）、連用形「で」の
            // 後ろは係助詞・副助詞（「では」「でも」）だけ（「だより」→ だ/より、「だも」は かな書きの語の断片）
            "助動詞" if aux_kind(prev) == Some(Aux::Copula) && prev.base != "らしい" => {
                match prev.conj_form {
                    "基本形" => {
                        (cur.pos2 == "格助詞" && cur.surface == "と")
                            || matches!(cur.pos2, "並立助詞" | "副助詞")
                    }
                    "連用形" => matches!(cur.pos2, "係助詞" | "副助詞"),
                    _ => false,
                }
            }
            "動詞" | "形容詞" | "助動詞" => {
                prev.in_final_form() || prev.renyou() || prev.conj_form == "体言接続"
            }
            _ => false,
        };
    }
    if matches!(cur.pos1, "動詞" | "形容詞") && cur.pos2 == "非自立" {
        // 補助動詞・補助形容詞（て/いる、て/みる、て/いく、て/ほしい）は「て」の後ろ、複合動詞の後ろの要素
        // （連用形 + すぎる・あう・出す）は連用形の後ろに付く（「飛び/い/た」「ぶれ/いこ/う」を除く）
        let after_te = prev.is("助詞", "接続助詞") && matches!(prev.surface, "て" | "で");
        return after_te
            || (!SUBSIDIARY_AFTER_TE.contains(&cur.base) && prev.pos1 == "動詞" && prev.renyou());
    }
    if cur.is("動詞", "接尾") {
        return prev.pos1 == "動詞" && prev.mizen();
    }
    if cur.is("名詞", "非自立") {
        return prev.is_conjugating() && (prev.in_final_form() || prev.conj_form == "体言接続");
    }
    false
}

/// 格助詞・係助詞・副助詞・並立助詞 `cur` が、直前の助詞 `prev` に続けられるか
///
/// 係助詞の後ろは「誰もが」「今こそは」「さえも」だけ、格助詞の後ろは係助詞・副助詞（「には」「をも」「にだけ」）と
/// 「へと」「AとBとで」「これからが」だけで、が・の の後ろには続かない。終助詞の後ろは引用の「と」（「かなと思う」）
/// だけで、連体化の後ろには来ない（「はは」→ は/は、「はに」→ は/に、「ものみ」→ も/のみ、「がも」→ が/も、
/// 「鶏もも」→ 鶏/も/も は かな書きの語や名前の断片）。
fn particle_follows(prev: &Morph, cur: &Morph) -> bool {
    match prev.pos2 {
        "係助詞" => matches!(
            (prev.surface, cur.surface),
            ("も" | "こそ", "が") | ("こそ", "は") | ("さえ", "も")
        ),
        "格助詞" => {
            let takes = match cur.pos2 {
                "係助詞" => !(prev.surface == "を" && cur.surface == "は"),
                "副助詞" => true,
                _ => matches!(
                    (prev.surface, cur.surface),
                    ("へ", "と") | ("と", "で") | ("から", "が")
                ),
            };
            takes && !matches!(prev.surface, "が" | "の")
        }
        "終助詞" => matches!(cur.surface, "と" | "って"),
        "並立助詞" => cur.pos2 != "並立助詞",
        "副助詞" | "副助詞／並立助詞／終助詞" | "接続助詞" | "副詞化" => true,
        _ => false,
    }
}

/// 述語の頭（付属語の前の内容語）`morphs[head]` の左側が文として成り立つか
fn head_is_anchored(morphs: &[Morph], head: usize) -> bool {
    let h = &morphs[head];
    if !h.known || !is_modern(h) {
        return false;
    }
    if head == 0 {
        return true;
    }
    let prev = &morphs[head - 1];
    match h.pos1 {
        "動詞" | "形容詞" => match prev.pos1 {
            // 付き先のある助詞（「平成/ば/しる」のような付き先の無い助詞を除く）
            // 主語を表す「の」は名詞を修飾する節（終止形 = 連体形）の中だけで、命令形の前には来ない
            // （「砂のしろ」→ 砂/の/しろ）
            "助詞" => {
                prev.pos2 != "連体化"
                    && !(prev.surface == "の" && h.conj_form.starts_with("命令"))
                    && head >= 2
                    && attaches(&morphs[head - 2], prev)
            }
            "副詞" | "接続詞" | "連体詞" | "記号" => true,
            // 名詞 + する・できる（「スルー/する」「結婚/し/ない」）
            // 名詞 + する・できる は、サ変接続の名詞か、IPAdic が 名詞,一般 にする片仮名・英字の語に限る
            // （「やつしろ」→ やつ/しろ、「地球する」を除く）。口語の「しょ」（しょう）も除く
            // （「土しょうが」→ 土/しょ/う/が）
            "名詞" => {
                (prev.pos2 == "サ変接続"
                    || (prev.pos2 == "一般"
                        && !prev.surface.chars().any(|c| is_hiragana(c) || is_kanji(c))))
                    && matches!(h.base, "する" | "できる")
                    && h.surface != "しょ"
            }
            _ => false,
        },
        "名詞" | "副詞" | "助詞" | "連体詞" | "接続詞" => true,
        _ => false,
    }
}

/// ひらがなだけの語で、名前・かな書きの漢語と取り違えにくい文末か（`head` は述語の頭）
///
/// 裸の用言（「あまがえる」→ あま/が/える）、未然形 + う（「けんしょう」→ けんしょ/う）、ん（「はこん」）、
/// 名詞 + か（「ゆか」→ ゆ/か）は、名前・漢語のでたらめな分割と区別できないので文末とみなさない。
fn is_stable_hiragana_ending(morphs: &[Morph], head: usize) -> bool {
    let n = morphs.len();
    if head + 1 >= n {
        return false;
    }
    let last = &morphs[n - 1];
    let prev = &morphs[n - 2];
    if last.is_final_particle() {
        return HIRAGANA_FINAL_PARTICLES.contains(&last.surface)
            && match prev.pos1 {
                "動詞" | "形容詞" | "助動詞" => prev.conj_form == "基本形",
                "助詞" => true,
                _ => prev.is("名詞", "非自立"),
            };
    }
    match aux_kind(last) {
        // ません
        Some(Aux::Negative) if last.base == "ん" => aux_kind(prev) == Some(Aux::Polite),
        Some(Aux::Negative) => last.base == "ない",
        // 音便の後ろか、ました・なかった・かった・だった・でした
        Some(Aux::Past) => prev.conj_form == "連用タ接続" || prev.pos1 == "助動詞",
        // でしょう・だろう・ましょう、て + 非自立動詞の意志形（やってみよう）
        Some(Aux::Volitional) => {
            matches!(aux_kind(prev), Some(Aux::Copula | Aux::Polite)) || prev.is("動詞", "非自立")
        }
        // のだ・んだ
        Some(Aux::Copula) if last.base == "だ" => prev.is("名詞", "非自立"),
        Some(Aux::Copula | Aux::Polite | Aux::Desire) => true,
        None => false,
    }
}

/// 助詞・助動詞・非自立名詞だけが文法どおりに並ぶ語か（「なのか」「にも」「ことも」「ですか」）
fn is_function_chain(morphs: &[Morph]) -> bool {
    if morphs.len() < 2
        || !morphs.iter().all(|m| {
            m.known
                && is_hiragana_word(m.surface)
                && (matches!(m.pos1, "助詞" | "助動詞") || m.is("名詞", "非自立"))
        })
    {
        return false;
    }
    let first = &morphs[0];
    let starts_well = (first.pos1 == "助詞"
        && matches!(first.pos2, "格助詞" | "係助詞" | "副助詞"))
        || aux_kind(first) == Some(Aux::Copula)
        || (first.is("名詞", "非自立") && matches!(first.surface, "こと" | "の" | "もの" | "ん"));
    starts_well
        && morphs.windows(2).all(|w| {
            if w[1].pos1 == "名詞" {
                // 「な/の」の「の」（だ の体言接続 + 非自立名詞）
                aux_kind(&w[0]) == Some(Aux::Copula) && w[0].conj_form == "体言接続"
            } else {
                attaches(&w[0], &w[1])
            }
        })
        && morphs[morphs.len() - 1].pos2 != "連体化"
}

/// 語の列が文や句として終わるときの種類
fn phrase_ending(
    morphs: &[Morph],
    allow_particle_end: bool,
    hiragana_only: bool,
) -> Option<SentenceLikeReason> {
    let last = morphs.last()?;
    if !last.known {
        return None;
    }
    let ends_with_predicate = last.is_final_particle()
        || match last.pos1 {
            "動詞" => last.in_final_form() && is_modern(last) && !is_doubtful_imperative(last),
            "形容詞" => last.conj_form == "基本形",
            "助動詞" => last.conj_form == "基本形" && aux_kind(last).is_some(),
            _ => false,
        };
    let ends_with_particle =
        !ends_with_predicate && allow_particle_end && last.pos1 == "助詞" && last.pos2 != "連体化";
    if !ends_with_predicate && !ends_with_particle {
        return None;
    }
    // 末尾から付属語をたどり、それぞれが前の語に付けるかを確かめる
    let mut head = morphs.len() - 1;
    while head > 0 && morphs[head].is_dependent() {
        if !attaches(&morphs[head - 1], &morphs[head]) {
            return None;
        }
        head -= 1;
    }
    // 付属語で始まる並び（「よふかしのうた」→ よ/ふか/し/…）は名前のでたらめな分割とみなす
    // （付属語だけの並びは is_function_chain で判定済み）
    if morphs[0].is_dependent() || !head_is_anchored(morphs, head) {
        return None;
    }
    if ends_with_predicate {
        return (!hiragana_only || is_stable_hiragana_ending(morphs, head))
            .then_some(SentenceLikeReason::Predicate);
    }
    // 助詞で終わる: 末尾の助詞の連なりの前の語（付き先）で分ける
    let host_index = morphs.iter().rposition(|m| m.pos1 != "助詞")?;
    let host = &morphs[host_index];
    if host.is_conjugating() {
        if hiragana_only
            && !(last.is("助詞", "接続助詞") && matches!(last.surface, "て" | "で" | "ながら"))
        {
            return None;
        }
        return Some(SentenceLikeReason::ConjugatedParticle);
    }
    if hiragana_only && !(host.is("名詞", "代名詞") || host.is("名詞", "非自立")) {
        return None;
    }
    // 内容語が 2 つ以上ある句（「ティファニーで朝食を」「身も心も」）は作品名として残す
    (morphs.iter().filter(|m| m.is_content_word()).count() <= 1)
        .then_some(SentenceLikeReason::NounParticle)
}

/// エントリの種類ごとの判定
fn classify(
    morphs: &[Morph],
    kind: EntryKind,
    ender_final: bool,
    hiragana_only: bool,
) -> Option<SentenceLikeReason> {
    if morphs.is_empty()
        || morphs
            .iter()
            .any(|m| matches!(m.pos1, "フィラー" | "その他"))
    {
        return None;
    }
    if is_function_chain(morphs) {
        return Some(SentenceLikeReason::FunctionWords);
    }
    match kind {
        // 人名・地名はひらがなの名前がでたらめに分かれやすい。文末記号で終わる語だけ判定する
        EntryKind::Name if !ender_final => return None,
        // 普通名詞（SudachiDict の「早死に」「本だな」「雨がえる」）は文や句と区別できない
        EntryKind::Common => {
            return matches!(morphs, [m] if m.known && m.pos1 == "感動詞")
                .then_some(SentenceLikeReason::Interjection);
        }
        _ => {}
    }
    let variant = kind == EntryKind::Variant;
    if let [m] = morphs {
        if !m.known {
            return None;
        }
        if m.pos1 == "感動詞" {
            return Some(SentenceLikeReason::Interjection);
        }
        let predicate = match m.pos1 {
            "副詞" | "接続詞" | "連体詞" => true,
            "動詞" | "形容詞" => m.pos2 == "自立" && m.in_final_form(),
            _ => false,
        };
        return (variant && predicate).then_some(SentenceLikeReason::VariantWord);
    }
    // 表記ゆれは名詞を含まない並びに限る（「ごみだし」→ ごみ/だ/し、「供えもの」→ 供え/も/の を残す）
    if variant && morphs.iter().any(|m| m.pos1 == "名詞") {
        return None;
    }
    let reason = phrase_ending(morphs, !ender_final, hiragana_only)?;
    // 文末記号で終わる文は、内容語（名詞 + する の する を除く）が 2 つ以上なら、文末記号まで名前に含む作品名と
    // みなして残す（「やはり俺の青春ラブコメはまちがっている。」「エースをねらえ！」）。文中の文と同じ形になりやすい
    // 短い文（「好きだ。」「いいね！」「お仕事です!」）だけを落とす。残した語は文分割の例外表にも入る
    let content = morphs
        .iter()
        .filter(|m| m.is_content_word() && !(m.pos1 == "動詞" && m.base == "する"))
        .count();
    (!ender_final || content <= 1).then_some(reason)
}

/// 表層形を参照辞書で解析した語の列 `tokens` から、エントリの種類ごとの判定を求める
pub(crate) fn judge(surface: &str, tokens: &[Token]) -> Verdict {
    let stem = surface.trim_end_matches(is_sentence_ender);
    let ender_final = stem.len() < surface.len();
    if ender_final && !stem.chars().any(char::is_alphanumeric) {
        let reason = Some(SentenceLikeReason::SymbolsWithEnder);
        return Verdict {
            proper: reason,
            name: reason,
            common: reason,
            variant: reason,
        };
    }
    let mut morphs: Vec<Morph> = tokens.iter().map(Morph::from_token).collect();
    if ender_final {
        while morphs
            .last()
            .is_some_and(|m| !m.surface.is_empty() && m.surface.chars().all(is_sentence_ender))
        {
            morphs.pop();
        }
    }
    let hiragana_only = is_hiragana_word(stem);
    let verdict = |kind| classify(&morphs, kind, ender_final, hiragana_only);
    Verdict {
        proper: verdict(EntryKind::Proper),
        name: verdict(EntryKind::Name),
        common: verdict(EntryKind::Common),
        variant: verdict(EntryKind::Variant),
    }
}

impl Verdict {
    /// エントリの品詞と原形に合う判定
    pub(crate) fn for_entry(
        &self,
        pos: &str,
        surface: &str,
        base_form: &str,
    ) -> Option<SentenceLikeReason> {
        if is_variant(surface, base_form) {
            self.variant
        } else if pos.starts_with("名詞,固有名詞,人名,") || pos.starts_with("名詞,固有名詞,地域,")
        {
            self.name
        } else if pos.starts_with("名詞,固有名詞,") {
            self.proper
        } else {
            self.common
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `表層/品詞1,品詞2/活用型/活用形/原形` を空白で区切った列から語の列を作る。活用しない語は
    /// 活用型以降を省き、原形を省くと表層形になる。未知語は表層の前に `?` を付ける
    fn morphs(spec: &str) -> Vec<Morph<'_>> {
        spec.split(' ')
            .map(|w| {
                let (known, w) = match w.strip_prefix('?') {
                    Some(rest) => (false, rest),
                    None => (true, w),
                };
                let f: Vec<&str> = w.split('/').collect();
                let (pos1, pos2) = f[1].split_once(',').unwrap_or((f[1], "*"));
                Morph {
                    surface: f[0],
                    pos1,
                    pos2,
                    conj_type: f.get(2).copied().unwrap_or(""),
                    conj_form: f.get(3).copied().unwrap_or(""),
                    base: f.get(4).copied().unwrap_or(f[0]),
                    known,
                }
            })
            .collect()
    }

    /// 人名でも表記ゆれでもない語として、文末記号を含まない表層形を判定する
    fn classify_as(kind: EntryKind, spec: &str) -> Option<SentenceLikeReason> {
        let m = morphs(spec);
        let hira = m.iter().all(|m| is_hiragana_word(m.surface));
        classify(&m, kind, false, hira)
    }

    fn canonical(spec: &str) -> Option<SentenceLikeReason> {
        classify_as(EntryKind::Proper, spec)
    }

    fn variant(spec: &str) -> Option<SentenceLikeReason> {
        classify_as(EntryKind::Variant, spec)
    }

    use SentenceLikeReason::*;

    #[test]
    fn test_predicate_endings_from_issue() {
        // 「どうでしょう」「作りました」「よろしくお願いします」「個人の感想です」「できるかな」「スルーする」
        assert_eq!(
            canonical(
                "どう/副詞,助詞類接続 でしょ/助動詞,*/特殊・デス/未然形/です う/助動詞,*/不変化型/基本形"
            ),
            Some(Predicate)
        );
        assert_eq!(
            canonical(
                "作り/動詞,自立/五段・ラ行/連用形/作る まし/助動詞,*/特殊・マス/連用形/ます た/助動詞,*/特殊・タ/基本形"
            ),
            Some(Predicate)
        );
        assert_eq!(
            canonical(
                "よろしく/副詞,一般 お願い/名詞,サ変接続 し/動詞,自立/サ変・スル/連用形/する ます/助動詞,*/特殊・マス/基本形"
            ),
            Some(Predicate)
        );
        assert_eq!(
            canonical(
                "個人/名詞,一般 の/助詞,連体化 感想/名詞,一般 です/助動詞,*/特殊・デス/基本形"
            ),
            Some(Predicate)
        );
        assert_eq!(
            canonical(
                "できる/動詞,自立/一段/基本形 か/助詞,副助詞／並立助詞／終助詞 な/助詞,終助詞"
            ),
            Some(Predicate)
        );
        assert_eq!(
            canonical("スルー/名詞,一般 する/動詞,自立/サ変・スル/基本形"),
            Some(Predicate)
        );
        assert_eq!(
            canonical("い/動詞,自立/一段/未然形/いる ない/助動詞,*/特殊・ナイ/基本形"),
            Some(Predicate)
        );
        assert_eq!(
            canonical("辻褄/名詞,一般 を/助詞,格助詞 合わせる/動詞,自立/一段/基本形"),
            Some(Predicate)
        );
    }

    #[test]
    fn test_particle_endings() {
        assert_eq!(
            canonical("一緒/名詞,サ変接続 に/助詞,格助詞"),
            Some(NounParticle)
        );
        assert_eq!(
            canonical("あなた/名詞,代名詞 に/助詞,格助詞"),
            Some(NounParticle)
        );
        assert_eq!(
            canonical(
                "じゃ/接続詞 なく/形容詞,自立/形容詞・アウオ段/連用テ接続/ない て/助詞,接続助詞"
            ),
            Some(ConjugatedParticle)
        );
        // 内容語が 2 つある助詞で終わる句は作品名として残す
        assert_eq!(
            canonical(
                "ティファニー/名詞,固有名詞 で/助詞,格助詞 朝食/名詞,サ変接続 を/助詞,格助詞"
            ),
            None
        );
        // 連体化の「の」では終わらない
        assert_eq!(
            canonical("櫻井/名詞,固有名詞 ゆり/名詞,固有名詞 の/助詞,連体化"),
            None
        );
    }

    #[test]
    fn test_names_with_broken_analysis_are_kept() {
        // 名詞 + た（「瀬戸ひなた」→ ひな/た）: た は連用形にしか付かない
        assert_eq!(
            canonical("瀬戸/名詞,固有名詞 ひな/名詞,一般 た/助動詞,*/特殊・タ/基本形"),
            None
        );
        // 名詞 + 形容詞（「柊あおい」）: 述語の頭の前に助詞が無い
        assert_eq!(
            canonical("柊/名詞,一般 あおい/形容詞,自立/形容詞・アウオ段/基本形"),
            None
        );
        // 付き先の無い助詞（「平成ばしる」→ 平成/ば/しる）
        assert_eq!(
            canonical("平成/名詞,固有名詞 ば/助詞,接続助詞 しる/動詞,自立/五段・ラ行/基本形"),
            None
        );
        // 終助詞の組でない（「東かがわ」→ 東/か/が/わ）
        assert_eq!(
            canonical(
                "東/名詞,一般 か/助詞,副助詞／並立助詞／終助詞 が/助詞,格助詞 わ/助詞,終助詞"
            ),
            None
        );
        // 文語の助動詞（「澤北るな」→ る/な）
        assert_eq!(
            canonical("澤/名詞,固有名詞 北/名詞,一般 る/助動詞,*/文語・ル/基本形 な/助詞,終助詞"),
            None
        );
        // フィラーを含む（「こやけじま」→ …/ま）
        assert_eq!(
            canonical(
                "こ/名詞,一般 やけ/動詞,自立/一段/連用形 じ/助動詞,*/不変化型/基本形 ま/フィラー"
            ),
            None
        );
    }

    #[test]
    fn test_hiragana_only_needs_stable_ending() {
        // ひらがなだけでも崩れにくい文末なら落とす
        assert_eq!(
            canonical(
                "し/動詞,自立/サ変・スル/連用形/する ませ/助動詞,*/特殊・マス/未然形/ます ん/助動詞,*/不変化型/基本形"
            ),
            Some(Predicate)
        );
        // 未然形 + う（「けんしょう」）、ん（「ゆにこん」）、裸の動詞（「あまがえる」）は名前・漢語と区別できない
        assert_eq!(
            variant(
                "けんしょ/動詞,自立/五段・サ行/未然ウ接続/けんしょす う/助動詞,*/不変化型/基本形"
            ),
            None
        );
        assert_eq!(
            canonical(
                "ゆ/名詞,一般 に/助詞,格助詞 こ/動詞,自立/カ変・来ル/未然形/くる ん/助動詞,*/不変化型/基本形"
            ),
            None
        );
        assert_eq!(
            canonical("あま/名詞,一般 が/助詞,格助詞 える/動詞,自立/一段/基本形"),
            None
        );
        // て + 非自立動詞の意志形は崩れにくい文末（「やってみよう」）
        assert_eq!(
            canonical(
                "やっ/動詞,自立/五段・ラ行/連用タ接続/やる て/助詞,接続助詞 みよ/動詞,非自立/一段/未然ウ接続/みる う/助動詞,*/不変化型/基本形"
            ),
            Some(Predicate)
        );
        // ひらがなの名詞に だ は付けない（「なみだ」→ なみ/だ）
        assert_eq!(
            canonical(
                "オバケ/名詞,一般 の/助詞,連体化 なみ/名詞,固有名詞 だ/助動詞,*/特殊・ダ/基本形"
            ),
            None
        );
    }

    #[test]
    fn test_function_chains() {
        assert_eq!(
            canonical(
                "な/助動詞,*/特殊・ダ/体言接続/だ の/名詞,非自立 か/助詞,副助詞／並立助詞／終助詞"
            ),
            Some(FunctionWords)
        );
        assert_eq!(
            canonical("に/助詞,格助詞 も/助詞,係助詞"),
            Some(FunctionWords)
        );
        assert_eq!(
            canonical("こと/名詞,非自立 も/助詞,係助詞"),
            Some(FunctionWords)
        );
        // 終助詞で始まる並び（「ぜのん」）や連体化の「の」の後ろに続く並び（「ことのは」）は名前として残す
        assert_eq!(
            canonical("ぜ/助詞,終助詞 の/助詞,連体化 ん/名詞,非自立"),
            None
        );
        assert_eq!(
            canonical("こと/名詞,非自立 の/助詞,連体化 は/助詞,係助詞"),
            None
        );
        assert_eq!(
            canonical("で/助動詞,*/特殊・ダ/連用形/だ は/助詞,係助詞"),
            Some(FunctionWords)
        );
        assert_eq!(
            canonical("だ/助動詞,*/特殊・ダ/基本形 ね/助詞,終助詞"),
            Some(FunctionWords)
        );
        assert_eq!(
            canonical("だけ/助詞,副助詞 だ/助動詞,*/特殊・ダ/基本形"),
            Some(FunctionWords)
        );
        // 文法に合わない助詞の並びは、かな書きの語・名前の断片として残す（SudachiDict の「はは」= 母、
        // 「だより」= 便り、「はだ」= 肌、「ものみ」= 物見、「しもだ」は人名）
        for spec in [
            "は/助詞,係助詞 は/助詞,係助詞",
            "は/助詞,係助詞 に/助詞,格助詞",
            "は/助詞,係助詞 だ/助動詞,*/特殊・ダ/基本形",
            "も/助詞,係助詞 のみ/助詞,副助詞",
            "が/助詞,格助詞 も/助詞,係助詞",
            "だ/助動詞,*/特殊・ダ/基本形 より/助詞,格助詞",
            "だ/助動詞,*/特殊・ダ/基本形 も/助詞,係助詞",
            "よ/助詞,終助詞 だ/助動詞,*/特殊・ダ/基本形",
        ] {
            assert_eq!(canonical(spec), None, "{spec}");
        }
        // 係助詞・格助詞の後ろに続けてよい組
        assert_eq!(
            canonical("も/助詞,係助詞 が/助詞,格助詞"),
            Some(FunctionWords)
        );
        assert_eq!(
            canonical("へ/助詞,格助詞 と/助詞,格助詞"),
            Some(FunctionWords)
        );
        assert_eq!(
            canonical("今宵/名詞,副詞可能 こそ/助詞,係助詞 は/助詞,係助詞"),
            Some(NounParticle)
        );
        assert_eq!(
            canonical("鶏/名詞,一般 も/助詞,係助詞 も/助詞,係助詞"),
            None
        );
        // 終助詞の後ろの引用の「と」（「働いたら負けかなと思ってる」）
        assert_eq!(
            canonical(
                "そう/副詞,助詞類接続 だ/助動詞,*/特殊・ダ/基本形 ね/助詞,終助詞 と/助詞,格助詞 \
                 言っ/動詞,自立/五段・ワ行促音便/連用タ接続/言う た/助動詞,*/特殊・タ/基本形"
            ),
            Some(Predicate)
        );
        // 機能語だけの並びは人名でも落とす
        let m = morphs("に/助詞,格助詞 も/助詞,係助詞");
        assert_eq!(
            classify(&m, EntryKind::Name, false, true),
            Some(FunctionWords)
        );
    }

    #[test]
    fn test_person_names_are_kept() {
        // 「ゆうか」は「言うか」と同じ形。人名でなければ文とみなすが、人名は残す
        let m =
            morphs("ゆう/動詞,自立/五段・ワ行ウ音便/基本形/ゆう か/助詞,副助詞／並立助詞／終助詞");
        assert_eq!(
            classify(&m, EntryKind::Proper, false, true),
            Some(Predicate)
        );
        assert_eq!(classify(&m, EntryKind::Name, false, true), None);
        let m = morphs("る/助動詞,*/文語・ル/体言接続 ん/助詞,終助詞");
        assert_eq!(classify(&m, EntryKind::Name, false, true), None);
    }

    #[test]
    fn test_variants() {
        // 1 語の副詞・接続詞・用言（「および」=お呼び、「きっと」=キット、「うまい」=熟寝）
        assert_eq!(variant("および/接続詞"), Some(VariantWord));
        assert_eq!(variant("きっと/副詞,一般"), Some(VariantWord));
        assert_eq!(
            variant("うまい/形容詞,自立/形容詞・アウオ段/基本形"),
            Some(VariantWord)
        );
        // 表記ゆれでない語の 1 語は落とさない（名前と同じ形の副詞など）
        assert_eq!(canonical("きっと/副詞,一般"), None);
        // 「回ろう」=回廊、「しながら」=品柄、「いって」=一手
        assert_eq!(
            variant("回ろ/動詞,自立/五段・ラ行/未然ウ接続/回る う/助動詞,*/不変化型/基本形"),
            Some(Predicate)
        );
        assert_eq!(
            variant("し/動詞,自立/サ変・スル/連用形/する ながら/助詞,接続助詞"),
            Some(ConjugatedParticle)
        );
        assert_eq!(
            variant("いっ/動詞,自立/五段・カ行促音便/連用タ接続/いく て/助詞,接続助詞"),
            Some(ConjugatedParticle)
        );
        // 撥音便の後ろの「て」は文法に合わない（「こんて」）
        assert_eq!(
            variant("こん/動詞,自立/五段・マ行/連用タ接続/こむ て/助詞,接続助詞"),
            None
        );
        // 名詞を含む表記ゆれは残す（「ごみだし」=ごみ出し）
        assert_eq!(
            variant("ごみ/名詞,一般 だ/助動詞,*/特殊・ダ/基本形 し/助詞,接続助詞"),
            None
        );
    }

    #[test]
    fn test_conjugation_forms_must_match() {
        // た・て は音便のある五段動詞の連用タ接続に付き、だ は撥音便・イ音便にだけ付く
        assert_eq!(
            canonical(
                "神/名詞,一般 は/助詞,係助詞 死ん/動詞,自立/五段・ナ行/連用タ接続/死ぬ だ/助動詞,*/特殊・タ/基本形/だ"
            ),
            Some(Predicate)
        );
        assert_eq!(
            canonical(
                "赤め/動詞,自立/一段/連用形/赤める だ/助動詞,*/特殊・タ/基本形/だ か/助詞,副助詞／並立助詞／終助詞"
            ),
            None
        );
        assert_eq!(
            variant("書い/動詞,自立/五段・カ行イ音便/連用タ接続/書く て/助詞,接続助詞"),
            Some(ConjugatedParticle)
        );
        assert_eq!(
            variant("かき/動詞,自立/五段・カ行イ音便/連用形/かく て/助詞,接続助詞"),
            None
        );
        // ん は未然形に付き、未然ウ接続には付かない（「曲ろん」）
        assert_eq!(
            variant("曲ろ/動詞,自立/五段・ラ行/未然ウ接続/曲る ん/助動詞,*/不変化型/基本形"),
            None
        );
        // 「いる」は「て」の後ろにだけ付く（「飛びいた」→ 飛び/い/た）
        assert_eq!(
            variant(
                "飛び/動詞,自立/五段・バ行/連用形/飛ぶ い/動詞,非自立/一段/連用形/いる た/助動詞,*/特殊・タ/基本形"
            ),
            None
        );
        // 補助動詞は「て」の後ろに付く（「みていこう」は文、「ぶれいこう」= 無礼講 は ぶれ/いこ/う で残す）
        assert_eq!(
            variant(
                "み/動詞,自立/一段/連用形/みる て/助詞,接続助詞 いこ/動詞,非自立/五段・カ行促音便/未然ウ接続/いく う/助動詞,*/不変化型/基本形"
            ),
            Some(Predicate)
        );
        assert_eq!(
            variant(
                "ぶれ/動詞,自立/一段/連用形/ぶれる いこ/動詞,非自立/五段・カ行促音便/未然ウ接続/いく う/助動詞,*/不変化型/基本形"
            ),
            None
        );
        // 口語の「しょ」は 名詞 + する とみなさない（「土しょうが」）
        assert_eq!(
            canonical(
                "土/名詞,一般 しょ/動詞,自立/サ変・スル/未然ウ接続/する う/助動詞,*/不変化型/基本形 が/助詞,接続助詞"
            ),
            None
        );
        // 名詞 + する はサ変接続の名詞か片仮名の語に限る（「自重しろ」は文、「やつしろ」は名前）
        assert_eq!(
            canonical("自重/名詞,サ変接続 しろ/動詞,自立/サ変・スル/命令ｒｏ/する"),
            Some(Predicate)
        );
        assert_eq!(
            canonical("やつ/名詞,代名詞 しろ/動詞,自立/サ変・スル/命令ｒｏ/する"),
            None
        );
        assert_eq!(
            canonical("地球/名詞,一般 する/動詞,自立/サ変・スル/基本形"),
            None
        );
        // ひらがな 2 字以下の命令形（「幾花にいろ」）は名前の断片とみなす
        assert_eq!(
            canonical("幾/名詞,数 花/名詞,一般 に/助詞,格助詞 いろ/動詞,自立/一段/命令ｒｏ/いる"),
            None
        );
        assert_eq!(
            canonical("翼/名詞,一般 に/助詞,格助詞 なれ/動詞,自立/五段・ラ行/命令ｅ/なる"),
            Some(Predicate)
        );
        // 付属語で始まる並び（「よふかしのうた」）、主語の「の」+ 命令形（「砂のしろ」）、
        // 文語の動詞（「特攻隊に捧ぐ」）は名前のでたらめな分割とみなす
        assert_eq!(
            canonical(
                "よ/助詞,終助詞 ふか/名詞,サ変接続 し/動詞,自立/サ変・スル/未然形/する のう/助動詞,*/特殊・ナイ/連用ゴザイ接続/ない た/助動詞,*/特殊・タ/基本形"
            ),
            None
        );
        assert_eq!(
            canonical("砂/名詞,一般 の/助詞,格助詞 しろ/動詞,自立/サ変・スル/命令ｒｏ/する"),
            None
        );
        assert_eq!(
            canonical("特攻隊/名詞,一般 に/助詞,格助詞 捧ぐ/動詞,自立/下二・ガ行/基本形/捧ぐ"),
            None
        );
    }

    #[test]
    fn test_common_nouns_and_places_are_kept() {
        // SudachiDict の普通名詞（「早死に」「本だな」）は助詞・活用形の並びに分かれても残す
        let honda = "本/名詞,一般 だ/助動詞,*/特殊・ダ/基本形 な/助詞,終助詞";
        assert_eq!(
            classify_as(EntryKind::Common, "早死/名詞,サ変接続 に/助詞,格助詞"),
            None
        );
        assert_eq!(classify_as(EntryKind::Common, honda), None);
        // 同じ並びでも固有名詞なら文とみなす
        assert_eq!(classify_as(EntryKind::Proper, honda), Some(Predicate));
        // 機能語だけの並びと感動詞は普通名詞でも落とす
        assert_eq!(
            classify_as(
                EntryKind::Common,
                "だ/助動詞,*/特殊・ダ/基本形 な/助詞,終助詞"
            ),
            Some(FunctionWords)
        );
        assert_eq!(
            classify_as(EntryKind::Common, "こんにちは/感動詞"),
            Some(Interjection)
        );
        // 地名は人名と同じく、文末記号で終わらなければ残す（「やよい」→ や/よい）
        assert_eq!(
            classify_as(
                EntryKind::Name,
                "や/助詞,並立助詞 よい/形容詞,自立/形容詞・アウオ段/基本形"
            ),
            None
        );
    }

    #[test]
    fn test_verdict_for_entry_picks_kind() {
        let v = Verdict {
            proper: Some(Predicate),
            name: None,
            common: Some(Interjection),
            variant: Some(VariantWord),
        };
        assert_eq!(
            v.for_entry("名詞,固有名詞,一般,*", "作りました", "作りました"),
            Some(Predicate)
        );
        assert_eq!(
            v.for_entry("名詞,固有名詞,組織,*", "いない", "いない"),
            Some(Predicate)
        );
        assert_eq!(
            v.for_entry("名詞,固有名詞,人名,一般", "ゆうか", "ゆうか"),
            None
        );
        assert_eq!(
            v.for_entry("名詞,固有名詞,地域,一般", "やよい", "やよい"),
            None
        );
        assert_eq!(
            v.for_entry("名詞,一般,*,*", "おはよう", "おはよう"),
            Some(Interjection)
        );
        assert_eq!(
            v.for_entry("名詞,一般,*,*", "および", "お呼び"),
            Some(VariantWord)
        );
    }

    #[test]
    fn test_ender_final_sentences_keep_long_titles() {
        // 文末記号の前の文（`judge` が文末記号を除いた語の列）は、内容語が 1 つ以下なら落とす
        let ender = |spec: &str| classify(&morphs(spec), EntryKind::Proper, true, false);
        assert_eq!(
            ender("好き/名詞,形容動詞語幹 だ/助動詞,*/特殊・ダ/基本形"),
            Some(Predicate)
        );
        // 名詞 + する の する は数えない
        assert_eq!(
            ender("結婚/名詞,サ変接続 する/動詞,自立/サ変・スル/基本形"),
            Some(Predicate)
        );
        // 内容語が 2 つ以上なら、文末記号まで名前に含む作品名として残す（「エースをねらえ！」）
        assert_eq!(
            ender(
                "エース/名詞,一般 を/助詞,格助詞 ねらえ/動詞,自立/五段・ワ行促音便/命令ｅ/ねらう"
            ),
            None
        );
        // 文末記号の無い同じ語は落とす
        assert_eq!(
            canonical(
                "エース/名詞,一般 を/助詞,格助詞 ねらえ/動詞,自立/五段・ワ行促音便/命令ｅ/ねらう"
            ),
            Some(Predicate)
        );
    }

    #[test]
    fn test_judge_handles_sentence_enders() {
        fn verdict(surface: &str) -> Verdict {
            judge(surface, &[])
        }
        let symbols = Some(SymbolsWithEnder);
        assert_eq!(
            verdict("…。"),
            Verdict {
                proper: symbols,
                name: symbols,
                common: symbols,
                variant: symbols
            }
        );
        assert_eq!(verdict("？？？").proper, symbols);
        // 文字を含む語は解析結果で決める（語の列が空なら落とさない）
        assert_eq!(verdict("モーニング娘。"), Verdict::default());
    }

    #[test]
    fn test_is_variant_folds_width() {
        assert!(!is_variant("いいね！", "いいね!"));
        assert!(is_variant("ありません", "有馬線"));
        assert!(!is_variant("どうでしょう", "どうでしょう"));
        // 作品名の句点を落とした語は表記ゆれでない（原形「僕たちは世界を変えることができない。」）
        assert!(!is_variant("モーニング娘", "モーニング娘。"));
        assert!(!is_variant(
            "僕たちは世界を変えることができない",
            "僕たちは世界を変えることができない。"
        ));
    }

    #[test]
    fn test_is_candidate_surface() {
        assert!(is_candidate_surface("どうでしょう"));
        assert!(is_candidate_surface("…。"));
        assert!(is_candidate_surface("Yahoo!"));
        assert!(!is_candidate_surface("有馬線"));
        assert!(!is_candidate_surface("モーニング"));
    }
}
