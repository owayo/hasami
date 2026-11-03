//! 明示的な数学・設定の短い構文だけで、単独の「値」を「あたい」へ補正する。
//!
//! 漢字ペディアの「値（あたい）」は文字・式が表す数を数学の語義とする。
//! https://www.kanjipedia.jp/kotoba/0004675800
//! 価格の「ね」も正当な読みなので、一般的な取得・比較・計算だけでは変更しない。
//! https://www.kanjipedia.jp/kotoba/0004676100
//!
//! 最良パスの品詞・原形・コスト・位置を保ち、通常の「ネ／ネ」だけを補正する。
//! 複合語、固有名詞、独自読み、保護範囲、空白を挟む構文は補正しない。
//! 引用符は語義や固有名詞の根拠にしない。構文の外側の引用境界は認めるが、
//! 内部の引用符や空白を取り除いて離れた語を構文へ結び直さない。

use super::Token;
use std::{ops::Range, sync::Arc};

/// 一度の解析の元入力と、その入力に対応する絶対バイト位置。
/// 全文解析・文別解析では元全文とorigin=0、独立Workspaceではその一入力とoffset。
/// 引用状態や解析をまたぐ状態は持たない。
#[derive(Default)]
pub(crate) struct ContextualReadingContext<'a> {
    input: &'a str,
    origin: usize,
    spans: Option<&'a mut Vec<crate::ContextualReadingSpan>>,
}

impl<'a> ContextualReadingContext<'a> {
    pub(crate) fn new(
        input: &'a str,
        origin: usize,
        spans: Option<&'a mut Vec<crate::ContextualReadingSpan>>,
    ) -> Self {
        Self {
            input,
            origin,
            spans,
        }
    }

    pub(super) fn source(&self) -> (&'a str, usize) {
        (self.input, self.origin)
    }

    pub(super) fn record(&mut self, token: &Token) {
        if let Some(spans) = self.spans.as_deref_mut() {
            let span = crate::ContextualReadingSpan {
                start: token.start,
                end: token.end,
                pronunciation: token.pronunciation.to_string(),
            };
            spans.push(span);
        }
    }

    pub(super) fn identifier_boundary(&self, absolute_start: usize) -> bool {
        let Some(relative_start) = absolute_start.checked_sub(self.origin) else {
            return false;
        };
        let Some(prefix) = self.input.get(..relative_start) else {
            return false;
        };
        let fold = |c: char| {
            if ('！'..='～').contains(&c) {
                char::from_u32(c as u32 - 0xfee0).unwrap()
            } else {
                c
            }
        };
        // 直結するID区切りの列と、その左の一文字だけを見る。
        // 和文句点・空白・引用を越えず、入力先頭までの走査もしない。
        let mut separator = false;
        for c in prefix.chars().rev().map(fold) {
            if matches!(c, '-' | '.' | '/' | ':' | '_') {
                separator = true;
            } else {
                return separator && c.is_ascii_alphanumeric();
            }
        }
        false
    }
}

fn fixed(t: &Token, surface: &str, pos: &str, reading: &str, pronunciation: &str) -> bool {
    t.is_known
        && t.surface.as_ref() == surface
        && t.base_form.as_ref() == surface
        && t.pos.as_ref() == pos
        && t.reading.as_ref() == reading
        && t.pronunciation.as_ref() == pronunciation
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
}

fn ordinary_value(t: &Token) -> bool {
    fixed(t, "値", "名詞,一般,*,*", "ネ", "ネ")
}

fn left_boundary(tokens: &[Token], start: usize) -> bool {
    start == 0
        || (!tokens[start - 1].pos.starts_with("名詞,")
            && !tokens[start - 1].pos.starts_with("接頭詞,")
            && !tokens[start - 1]
                .surface
                .chars()
                .any(|c| c.is_ascii_alphanumeric() || c == '_')
            && (!matches!(tokens[start - 1].surface.as_ref(), "「" | "『" | "\"" | "'")
                || tokens[start - 1].end == tokens[start].start))
}

fn allowed_right(next: Option<&Token>) -> bool {
    // ASCIIカンマは読み注記の境界からも生じるので、右境界の認定に使わない。
    // 元の和文読点は認定し、正規化前に選択した発音を引き継げるようにする。
    next.is_none_or(|t| {
        (t.is_known && t.pos.starts_with("助詞,"))
            || (t.is_known && t.base_form.as_ref() == "だ" && t.pos.starts_with("助動詞,"))
            || (t.pos.starts_with("記号,")
                && matches!(
                    t.surface.as_ref(),
                    "。" | "、" | "." | "）" | ")" | "」" | "』" | "\"" | "'"
                ))
    })
}

fn suru_past(verb: &Token, past: &Token) -> bool {
    verb.is_known
        && verb.surface.as_ref() == "し"
        && verb.base_form.as_ref() == "する"
        && verb.pos.as_ref() == "動詞,自立,*,*"
        && verb.conj_type.as_ref() == "サ変・スル"
        && verb.conj_form.as_ref() == "連用形"
        && verb.reading.as_ref() == "シ"
        && verb.pronunciation.as_ref() == "シ"
        && past.is_known
        && past.surface.as_ref() == "た"
        && past.base_form.as_ref() == "た"
        && past.pos.as_ref() == "助動詞,*,*,*"
        && past.conj_type.as_ref() == "特殊・タ"
        && past.conj_form.as_ref() == "基本形"
        && past.reading.as_ref() == "タ"
        && past.pronunciation.as_ref() == "タ"
}

fn function_symbol(t: &Token) -> bool {
    let expected = match t.surface.as_ref() {
        "f" => "エフ",
        _ => return false,
    };
    t.base_form == t.surface
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
        && t.reading.as_ref() == expected
        && t.pronunciation.as_ref() == expected
        && ((t.is_known && t.pos.as_ref() == "記号,アルファベット,*,*")
            || (!t.is_known && t.pos.as_ref() == "名詞,固有名詞,組織,*"))
}

fn function_predicate(tokens: &[Token], i: usize) -> bool {
    let Some(pair) = tokens.get(i + 1..i + 3) else {
        return false;
    };
    let verb = &pair[1];
    fixed(&pair[0], "を", "助詞,格助詞,一般,*", "ヲ", "ヲ")
        && verb.is_known
        && verb.surface.as_ref() == "求める"
        && verb.base_form.as_ref() == "求める"
        && verb.pos.as_ref() == "動詞,自立,*,*"
        && verb.conj_type.as_ref() == "一段"
        && verb.conj_form.as_ref() == "基本形"
        && verb.reading.as_ref() == "モトメル"
        && verb.pronunciation.as_ref() == "モトメル"
        && tokens[i].end == pair[0].start
        && pair[0].end == verb.start
}

fn parenthesis(t: &Token, surface: &str) -> bool {
    t.surface.as_ref() == surface
        && t.base_form == t.surface
        && t.pos.starts_with("記号,")
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
        && (t.reading.is_empty() || t.reading == t.surface)
        && (t.pronunciation.is_empty() || t.pronunciation == t.surface)
}

fn integer_argument(t: &Token) -> bool {
    t.is_known
        && t.pos.as_ref() == "名詞,数,*,*"
        && t.base_form == t.surface
        && !t.surface.is_empty()
        && t.surface.len() <= 16
        && t.surface.bytes().all(|b| b.is_ascii_digit())
        && (t.surface.len() == 1 || !t.surface.starts_with('0'))
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
}

fn mathematical_head(t: &Token) -> bool {
    let (reading, pronunciation) = match t.surface.as_ref() {
        "関数" => ("カンスウ", "カンスー"),
        "変数" => ("ヘンスウ", "ヘンスー"),
        "定数" => ("テイスウ", "テイスー"),
        "係数" => ("ケイスウ", "ケイスー"),
        "座標" => ("ザヒョウ", "ザヒョー"),
        _ => return false,
    };
    fixed(t, &t.surface, "名詞,一般,*,*", reading, pronunciation)
}

fn frame_start(tokens: &[Token], i: usize) -> Option<usize> {
    if i >= 2
        && fixed(&tokens[i - 1], "の", "助詞,連体化,*,*", "ノ", "ノ")
        && mathematical_head(&tokens[i - 2])
    {
        let mut start = i - 2;
        // 実測した標準の「三／次元」だけを座標の修飾部として含める。
        // それ以外の前接名詞や次元表記へ、名詞境界を一括して緩めない。
        if tokens[start].surface.as_ref() == "座標"
            && start >= 2
            && fixed(&tokens[start - 2], "三", "名詞,数,*,*", "サン", "サン")
            && fixed(
                &tokens[start - 1],
                "次元",
                "名詞,接尾,助数詞,*",
                "ジゲン",
                "ジゲン",
            )
        {
            start -= 2;
        }
        return Some(start);
    }
    if i >= 3
        && suru_past(&tokens[i - 2], &tokens[i - 1])
        && fixed(
            &tokens[i - 3],
            "積分",
            "名詞,サ変接続,*,*",
            "セキブン",
            "セキブン",
        )
    {
        return Some(i - 3);
    }
    if i >= 5
        && suru_past(&tokens[i - 2], &tokens[i - 1])
        && fixed(
            &tokens[i - 3],
            "計算",
            "名詞,サ変接続,*,*",
            "ケイサン",
            "ケイサン",
        )
        && fixed(
            &tokens[i - 4],
            "として",
            "助詞,格助詞,連語,*",
            "トシテ",
            "トシテ",
        )
        && fixed(
            &tokens[i - 5],
            "極限",
            "名詞,一般,*,*",
            "キョクゲン",
            "キョクゲン",
        )
    {
        let mut start = i - 5;
        if start > 0
            && fixed(
                &tokens[start - 1],
                "対称",
                "名詞,一般,*,*",
                "タイショウ",
                "タイショー",
            )
        {
            start -= 1;
        }
        return Some(start);
    }
    if i >= 5
        && fixed(&tokens[i - 1], "の", "助詞,連体化,*,*", "ノ", "ノ")
        && parenthesis(&tokens[i - 2], ")")
        && integer_argument(&tokens[i - 3])
        && parenthesis(&tokens[i - 4], "(")
        && function_symbol(&tokens[i - 5])
        && function_predicate(tokens, i)
    {
        return Some(i - 5);
    }
    // 「最大」だけを数学の見出しにせず、標準の可能表現まで含める。
    if i >= 4 {
        let take = &tokens[i - 4];
        let possible = &tokens[i - 3];
        if take.is_known
            && take.surface.as_ref() == "とり"
            && take.base_form.as_ref() == "とる"
            && take.pos.as_ref() == "動詞,自立,*,*"
            && take.conj_type.as_ref() == "五段・ラ行"
            && take.conj_form.as_ref() == "連用形"
            && take.reading.as_ref() == "トリ"
            && take.pronunciation.as_ref() == "トリ"
            && possible.is_known
            && possible.surface.as_ref() == "うる"
            && possible.base_form.as_ref() == "うる"
            && possible.pos.as_ref() == "動詞,非自立,*,*"
            && possible.conj_type.as_ref() == "一段・得ル"
            && possible.conj_form.as_ref() == "基本形"
            && possible.reading.as_ref() == "ウル"
            && possible.pronunciation.as_ref() == "ウル"
            && fixed(
                &tokens[i - 2],
                "最大",
                "名詞,一般,*,*",
                "サイダイ",
                "サイダイ",
            )
            && fixed(&tokens[i - 1], "の", "助詞,連体化,*,*", "ノ", "ノ")
        {
            return Some(i - 4);
        }
    }
    None
}

fn configuration_head(t: &Token) -> bool {
    // IPAdicの未知語と、NEologd由来の正規化見出しを実測した組だけ認める。
    // 固有名詞の品詞一般を技術語扱いせず、別の原形・読み・人名等は除外する。
    t.surface.as_ref() == "コンフィグ"
        && t.reading.as_ref() == "コンフィグ"
        && t.pronunciation.as_ref() == "コンフィグ"
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
        && ((!t.is_known && t.base_form == t.surface && t.pos.as_ref() == "名詞,一般,*,*")
            || (t.is_known
                && t.base_form.as_ref() == "config"
                && t.pos.as_ref() == "名詞,固有名詞,一般,*"))
}

fn standard_suru(verb: &Token) -> bool {
    verb.is_known
        && verb.base_form.as_ref() == "する"
        && verb.pos.as_ref() == "動詞,自立,*,*"
        && verb.conj_type.as_ref() == "サ変・スル"
        && match (verb.surface.as_ref(), verb.conj_form.as_ref()) {
            ("する", "基本形") => {
                verb.reading.as_ref() == "スル" && verb.pronunciation.as_ref() == "スル"
            }
            ("し", "連用形" | "未然形") => {
                verb.reading.as_ref() == "シ" && verb.pronunciation.as_ref() == "シ"
            }
            _ => false,
        }
}

fn configuration_frame(tokens: &[Token], i: usize) -> Option<(usize, usize)> {
    if i < 2 || i + 3 >= tokens.len() {
        return None;
    }
    (configuration_head(&tokens[i - 2])
        && fixed(&tokens[i - 1], "の", "助詞,連体化,*,*", "ノ", "ノ")
        && fixed(&tokens[i + 1], "を", "助詞,格助詞,一般,*", "ヲ", "ヲ")
        && fixed(
            &tokens[i + 2],
            "設定",
            "名詞,サ変接続,*,*",
            "セッテイ",
            "セッテイ",
        )
        && standard_suru(&tokens[i + 3]))
    .then_some((i - 2, i + 3))
}

fn difference_frame(tokens: &[Token], i: usize) -> Option<(usize, usize)> {
    if i < 2 || i + 3 >= tokens.len() {
        return None;
    }
    (fixed(&tokens[i - 2], "差分", "名詞,一般,*,*", "サブン", "サブン")
        && fixed(&tokens[i - 1], "の", "助詞,連体化,*,*", "ノ", "ノ")
        && fixed(&tokens[i + 1], "を", "助詞,格助詞,一般,*", "ヲ", "ヲ")
        && fixed(
            &tokens[i + 2],
            "算出",
            "名詞,サ変接続,*,*",
            "サンシュツ",
            "サンシュツ",
        )
        && standard_suru(&tokens[i + 3]))
    .then_some((i - 2, i + 3))
}

fn expression_return_frame(tokens: &[Token], i: usize) -> Option<(usize, usize)> {
    if i < 3 || i + 2 >= tokens.len() {
        return None;
    }
    let verb = &tokens[i + 2];
    (fixed(&tokens[i - 3], "式", "名詞,一般,*,*", "シキ", "シキ")
        && fixed(&tokens[i - 2], "自体", "名詞,一般,*,*", "ジタイ", "ジタイ")
        && fixed(&tokens[i - 1], "が", "助詞,格助詞,一般,*", "ガ", "ガ")
        && fixed(&tokens[i + 1], "を", "助詞,格助詞,一般,*", "ヲ", "ヲ")
        && verb.is_known
        && verb.surface.as_ref() == "返す"
        && verb.base_form.as_ref() == "返す"
        && verb.pos.as_ref() == "動詞,自立,*,*"
        && verb.conj_type.as_ref() == "五段・サ行"
        && verb.conj_form.as_ref() == "基本形"
        && verb.reading.as_ref() == "カエス"
        && verb.pronunciation.as_ref() == "カエス")
        .then_some((i - 3, i + 2))
}

fn frequent_value_frame(tokens: &[Token], i: usize) -> Option<(usize, usize)> {
    if i < 4 || i + 1 >= tokens.len() {
        return None;
    }
    let high = &tokens[i - 1];
    let copula = &tokens[i + 1];
    // 価格の「高い値」ではなく、最上級の頻度と標準の述語まで認定する。
    (fixed(
        &tokens[i - 4],
        "最も",
        "副詞,一般,*,*",
        "モットモ",
        "モットモ",
    ) && fixed(&tokens[i - 3], "頻度", "名詞,一般,*,*", "ヒンド", "ヒンド")
        && fixed(&tokens[i - 2], "が", "助詞,格助詞,一般,*", "ガ", "ガ")
        && high.is_known
        && high.surface.as_ref() == "高い"
        && high.base_form.as_ref() == "高い"
        && high.pos.as_ref() == "形容詞,自立,*,*"
        && high.conj_type.as_ref() == "形容詞・アウオ段"
        && high.conj_form.as_ref() == "基本形"
        && high.reading.as_ref() == "タカイ"
        && high.pronunciation.as_ref() == "タカイ"
        && copula.is_known
        && copula.surface.as_ref() == "です"
        && copula.base_form.as_ref() == "です"
        && copula.pos.as_ref() == "助動詞,*,*,*"
        && copula.conj_type.as_ref() == "特殊・デス"
        && copula.conj_form.as_ref() == "基本形"
        && copula.reading.as_ref() == "デス"
        && copula.pronunciation.as_ref() == "デス")
        .then_some((i - 4, i + 1))
}

pub(super) fn apply(
    tokens: &mut [Token],
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) {
    for i in 0..tokens.len() {
        if !ordinary_value(&tokens[i]) {
            continue;
        }
        let (start, end) = if let Some(start) = frame_start(tokens, i) {
            if !allowed_right(tokens.get(i + 1))
                || tokens.get(i + 1).is_some_and(|t| {
                    matches!(t.surface.as_ref(), "」" | "』" | "\"" | "'")
                        && tokens[i].end != t.start
                })
            {
                continue;
            }
            (start, i)
        } else if let Some(frame) = configuration_frame(tokens, i) {
            frame
        } else if let Some(frame) = difference_frame(tokens, i) {
            frame
        } else if let Some(frame) = expression_return_frame(tokens, i) {
            frame
        } else if let Some(frame) = frequent_value_frame(tokens, i) {
            frame
        } else {
            continue;
        };
        let phrase = &tokens[start..=end];
        // 助数詞に続かない単独整数も数量補正では保護される。f(2)の引数と
        // 完全一致する範囲だけは許すが、引数そのものの全フィールドは変えない。
        // 文字を含むIDや、引数を越える保護範囲は従来どおり除外する。
        let argument = (i - start == 5 && function_symbol(&tokens[start]))
            .then(|| tokens[start + 2].start..tokens[start + 2].end);
        if !left_boundary(tokens, start)
            || context.identifier_boundary(phrase[0].start)
            || !phrase.windows(2).all(|w| w[0].end == w[1].start)
            || protected.iter().any(|r| {
                r.start < phrase.last().unwrap().end
                    && phrase[0].start < r.end
                    && argument.as_ref() != Some(r)
            })
        {
            continue;
        }
        let reading: Arc<str> = Arc::from("アタイ");
        tokens[i].reading = Arc::clone(&reading);
        tokens[i].pronunciation = reading;
        context.record(&tokens[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(tokens: &mut [Token], protected: &[Range<usize>]) {
        super::apply(tokens, protected, &mut ContextualReadingContext::default());
    }

    fn tokens(words: &[&str]) -> Vec<Token> {
        let mut offset = 0;
        words
            .iter()
            .map(|&surface| {
                let (pos, base, r, p, kind, form) = match surface {
                    "値" => ("名詞,一般,*,*", surface, "ネ", "ネ", "", ""),
                    "コンフィグ" => (
                        "名詞,固有名詞,一般,*",
                        "config",
                        "コンフィグ",
                        "コンフィグ",
                        "",
                        "",
                    ),
                    "設定" => ("名詞,サ変接続,*,*", surface, "セッテイ", "セッテイ", "", ""),
                    "式" => ("名詞,一般,*,*", surface, "シキ", "シキ", "", ""),
                    "自体" => ("名詞,一般,*,*", surface, "ジタイ", "ジタイ", "", ""),
                    "が" => ("助詞,格助詞,一般,*", surface, "ガ", "ガ", "", ""),
                    "返す" => (
                        "動詞,自立,*,*",
                        surface,
                        "カエス",
                        "カエス",
                        "五段・サ行",
                        "基本形",
                    ),
                    "とり" => (
                        "動詞,自立,*,*",
                        "とる",
                        "トリ",
                        "トリ",
                        "五段・ラ行",
                        "連用形",
                    ),
                    "うる" => (
                        "動詞,非自立,*,*",
                        surface,
                        "ウル",
                        "ウル",
                        "一段・得ル",
                        "基本形",
                    ),
                    "最大" => ("名詞,一般,*,*", surface, "サイダイ", "サイダイ", "", ""),
                    "最も" => ("副詞,一般,*,*", surface, "モットモ", "モットモ", "", ""),
                    "頻度" => ("名詞,一般,*,*", surface, "ヒンド", "ヒンド", "", ""),
                    "高い" => (
                        "形容詞,自立,*,*",
                        surface,
                        "タカイ",
                        "タカイ",
                        "形容詞・アウオ段",
                        "基本形",
                    ),
                    "です" => (
                        "助動詞,*,*,*",
                        surface,
                        "デス",
                        "デス",
                        "特殊・デス",
                        "基本形",
                    ),
                    "する" => (
                        "動詞,自立,*,*",
                        surface,
                        "スル",
                        "スル",
                        "サ変・スル",
                        "基本形",
                    ),
                    "関数" => ("名詞,一般,*,*", surface, "カンスウ", "カンスー", "", ""),
                    "変数" => ("名詞,一般,*,*", surface, "ヘンスウ", "ヘンスー", "", ""),
                    "定数" => ("名詞,一般,*,*", surface, "テイスウ", "テイスー", "", ""),
                    "係数" => ("名詞,一般,*,*", surface, "ケイスウ", "ケイスー", "", ""),
                    "座標" => ("名詞,一般,*,*", surface, "ザヒョウ", "ザヒョー", "", ""),
                    "差分" => ("名詞,一般,*,*", surface, "サブン", "サブン", "", ""),
                    "算出" => (
                        "名詞,サ変接続,*,*",
                        surface,
                        "サンシュツ",
                        "サンシュツ",
                        "",
                        "",
                    ),
                    "三" => ("名詞,数,*,*", surface, "サン", "サン", "", ""),
                    "次元" => ("名詞,接尾,助数詞,*", surface, "ジゲン", "ジゲン", "", ""),
                    "_" | "-" | "." | "/" | ":" | "＿" | "－" | "．" | "／" | "：" | "," => {
                        ("記号,一般,*,*", surface, "", "", "", "")
                    }
                    "の" => ("助詞,連体化,*,*", surface, "ノ", "ノ", "", ""),
                    "を" => ("助詞,格助詞,一般,*", surface, "ヲ", "ヲ", "", ""),
                    "し" => ("動詞,自立,*,*", "する", "シ", "シ", "サ変・スル", "連用形"),
                    "求める" => (
                        "動詞,自立,*,*",
                        surface,
                        "モトメル",
                        "モトメル",
                        "一段",
                        "基本形",
                    ),
                    "た" => ("助動詞,*,*,*", surface, "タ", "タ", "特殊・タ", "基本形"),
                    "積分" => ("名詞,サ変接続,*,*", surface, "セキブン", "セキブン", "", ""),
                    "計算" => ("名詞,サ変接続,*,*", surface, "ケイサン", "ケイサン", "", ""),
                    "極限" => ("名詞,一般,*,*", surface, "キョクゲン", "キョクゲン", "", ""),
                    "対称" => ("名詞,一般,*,*", surface, "タイショウ", "タイショー", "", ""),
                    "として" => ("助詞,格助詞,連語,*", surface, "トシテ", "トシテ", "", ""),
                    "f" => ("記号,アルファベット,*,*", surface, "エフ", "エフ", "", ""),
                    "(" | ")" | "「" | "」" | "『" | "』" | "\"" | "'" => {
                        ("記号,一般,*,*", surface, surface, surface, "", "")
                    }
                    "2" => ("名詞,数,*,*", surface, "ニ", "ニ", "", ""),
                    _ => ("名詞,一般,*,*", surface, surface, surface, "", ""),
                };
                let token = Token {
                    surface: Arc::from(surface),
                    start: offset,
                    end: offset + surface.len(),
                    pos: Arc::from(pos),
                    conj_type: Arc::from(kind),
                    conj_form: Arc::from(form),
                    base_form: Arc::from(base),
                    reading: Arc::from(r),
                    pronunciation: Arc::from(p),
                    word_cost: 1234,
                    is_known: true,
                };
                offset = token.end;
                token
            })
            .collect()
    }

    fn assert_other_fields_equal(before: &Token, after: &Token) {
        assert_eq!(before.surface, after.surface);
        assert_eq!(before.start, after.start);
        assert_eq!(before.end, after.end);
        assert_eq!(before.pos, after.pos);
        assert_eq!(before.conj_type, after.conj_type);
        assert_eq!(before.conj_form, after.conj_form);
        assert_eq!(before.base_form, after.base_form);
        assert_eq!(before.word_cost, after.word_cost);
        assert_eq!(before.is_known, after.is_known);
    }

    fn strong_frame_words() -> [Vec<&'static str>; 3] {
        [
            vec!["式", "自体", "が", "値", "を", "返す"],
            vec!["とり", "うる", "最大", "の", "値"],
            vec!["最も", "頻度", "が", "高い", "値", "です"],
        ]
    }

    #[test]
    fn strong_value_frames_change_only_target_readings_and_receipts() {
        for frame in strong_frame_words() {
            for quoted in [false, true] {
                let words = if quoted {
                    [vec!["「"], frame.clone(), vec!["」"]].concat()
                } else {
                    frame.clone()
                };
                let input = words.concat();
                let mut ts = tokens(&words);
                let before = ts.clone();
                let mut spans = Vec::new();
                super::apply(
                    &mut ts,
                    &[],
                    &mut ContextualReadingContext::new(&input, 0, Some(&mut spans)),
                );
                let target = ts.iter().find(|t| t.surface.as_ref() == "値").unwrap();
                assert_eq!(target.reading.as_ref(), "アタイ", "{input}");
                assert_eq!(target.pronunciation.as_ref(), "アタイ", "{input}");
                assert_eq!(
                    spans,
                    vec![crate::ContextualReadingSpan {
                        start: target.start,
                        end: target.end,
                        pronunciation: "アタイ".into(),
                    }],
                    "{input}"
                );
                for (old, new) in before.iter().zip(&ts) {
                    assert_other_fields_equal(old, new);
                    if old.surface.as_ref() != "値" {
                        assert_eq!(old.reading, new.reading);
                        assert_eq!(old.pronunciation, new.pronunciation);
                    }
                }
            }
        }
    }

    #[test]
    fn strong_value_frames_require_every_standard_profile_and_protection_check() {
        for words in strong_frame_words() {
            for index in 0..words.len() {
                for mode in 0..6 {
                    let mut ts = tokens(&words);
                    match mode {
                        0 => ts[index].is_known = false,
                        1 => ts[index].pos = Arc::from("名詞,固有名詞,一般,*"),
                        2 => ts[index].base_form = Arc::from("別語"),
                        3 => ts[index].reading = Arc::from("ベツメイ"),
                        4 => ts[index].pronunciation = Arc::from("ベツメイ"),
                        _ => ts[index].conj_form = Arc::from("別の活用形"),
                    }
                    let before = format!("{ts:?}");
                    apply(&mut ts, &[]);
                    assert_eq!(format!("{ts:?}"), before, "{words:?}/{index}/{mode}");
                }
                let mut ts = tokens(&words);
                let range = ts[index].start..ts[index].end;
                let before = format!("{ts:?}");
                apply(&mut ts, &[range]);
                assert_eq!(format!("{ts:?}"), before, "protected {words:?}/{index}");
            }
            for reading in ["アタイ", "チ"] {
                let mut ts = tokens(&words);
                let target = ts.iter_mut().find(|t| t.surface.as_ref() == "値").unwrap();
                target.reading = Arc::from(reading);
                target.pronunciation = Arc::from(reading);
                let before = format!("{ts:?}");
                apply(&mut ts, &[]);
                assert_eq!(format!("{ts:?}"), before);
            }
            for join in 1..words.len() {
                let mut ts = tokens(&words);
                for t in &mut ts[join..] {
                    t.start += "　".len();
                    t.end += "　".len();
                }
                let before = format!("{ts:?}");
                apply(&mut ts, &[]);
                assert_eq!(format!("{ts:?}"), before, "gap {words:?}/{join}");
            }
        }
    }

    #[test]
    fn strong_value_frames_keep_origin_aware_identifier_boundaries() {
        for frame in strong_frame_words() {
            for separator in ["-", ".", "/", ":", "_", "－", "．", "／", "：", "＿"] {
                for origin in [0, "前置き。".len()] {
                    let words = [vec!["SKU", separator], frame.clone()].concat();
                    let input = words.concat();
                    let mut ts = tokens(&words);
                    for t in &mut ts {
                        t.start += origin;
                        t.end += origin;
                    }
                    let before = format!("{ts:?}");
                    let mut spans = Vec::new();
                    super::apply(
                        &mut ts,
                        &[],
                        &mut ContextualReadingContext::new(&input, origin, Some(&mut spans)),
                    );
                    assert_eq!(format!("{ts:?}"), before, "{input}/{origin}");
                    assert!(spans.is_empty());
                }
            }
        }
    }

    #[test]
    fn strong_value_frames_do_not_expand_prices_annotations_or_verb_scope() {
        for words in [
            vec!["魚", "の", "値", "を", "返す"],
            vec!["最も", "高い", "値", "です"],
            vec!["頻度", "が", "高い", "値", "です"],
            vec!["値", "を", "返す"],
            vec!["式", "自体", "が", "値", "(", "ね", ")", "を", "返す"],
            vec!["とり", "うる", "最大", "の", "値", "(", "ね", ")"],
            vec!["最も", "頻度", "が", "高い", "値", "(", "ね", ")", "です"],
            vec!["とり", "うる", "最大", "の", "値", ","],
            vec!["「", "式", "」", "自体", "が", "値", "を", "返す"],
            vec!["とり", "うる", "「", "最大", "」", "の", "値"],
            vec!["最も", "頻度", "が", "「", "高い", "」", "値", "です"],
            vec!["最も", "頻度", "が", "高い", "値", "を"],
        ] {
            let mut ts = tokens(&words);
            let before = format!("{ts:?}");
            apply(&mut ts, &[]);
            assert_eq!(format!("{ts:?}"), before, "{words:?}");
        }
    }

    #[test]
    fn receipts_are_bijective_with_actual_reading_changes_including_quotes() {
        let mut ts = tokens(&["「", "関数", "の", "値", "」", "関数", "の", "値", "を"]);
        let before = ts.clone();
        let mut spans = Vec::new();
        super::apply(
            &mut ts,
            &[],
            &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
        );
        assert_eq!(spans.len(), 2);
        let mut changed = Vec::new();
        for (old, new) in before.iter().zip(&ts) {
            assert_other_fields_equal(old, new);
            if old.reading != new.reading || old.pronunciation != new.pronunciation {
                assert_eq!(old.reading.as_ref(), "ネ");
                assert_eq!(old.pronunciation.as_ref(), "ネ");
                assert_eq!(new.reading.as_ref(), "アタイ");
                assert_eq!(new.pronunciation.as_ref(), "アタイ");
                changed.push((new.start, new.end));
            }
        }
        assert_eq!(
            changed,
            spans.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn explicit_math_frames_change_only_both_readings() {
        for words in [
            vec!["関数", "の", "値", "を"],
            vec!["f", "(", "2", ")", "の", "値", "を", "求める"],
            vec!["積分", "し", "た", "値", "を"],
            vec!["極限", "として", "計算", "し", "た", "値", "を"],
            vec!["対称", "極限", "として", "計算", "し", "た", "値", "を"],
            vec!["「", "関数", "の", "値", "」"],
            vec!["『", "積分", "し", "た", "値", "』"],
            vec!["\"", "f", "(", "2", ")", "の", "値", "を", "求める", "\""],
        ] {
            let mut ts = tokens(&words);
            let before = ts.clone();
            apply(&mut ts, &[]);
            for (old, new) in before.iter().zip(&ts) {
                assert_other_fields_equal(old, new);
                if new.surface.as_ref() == "値" {
                    assert_eq!(new.reading.as_ref(), "アタイ");
                    assert_eq!(new.pronunciation.as_ref(), "アタイ");
                } else {
                    assert_eq!(old.reading, new.reading);
                    assert_eq!(old.pronunciation, new.pronunciation);
                }
            }
        }
    }

    #[test]
    fn configuration_profiles_require_the_head_relation_and_setting_predicate() {
        for unknown in [false, true] {
            for verb in ["する", "し"] {
                let mut ts = tokens(&["「", "コンフィグ", "の", "値", "を", "設定", verb, "」"]);
                if unknown {
                    ts[1].is_known = false;
                    ts[1].base_form = Arc::from("コンフィグ");
                    ts[1].pos = Arc::from("名詞,一般,*,*");
                }
                let before = ts.clone();
                let mut spans = Vec::new();
                super::apply(
                    &mut ts,
                    &[],
                    &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
                );
                assert_eq!(spans.len(), 1);
                assert_eq!(ts[3].pronunciation.as_ref(), "アタイ");
                for (old, new) in before.iter().zip(&ts) {
                    assert_other_fields_equal(old, new);
                    if new.surface.as_ref() != "値" {
                        assert_eq!(old.reading, new.reading);
                        assert_eq!(old.pronunciation, new.pronunciation);
                    }
                }
            }
        }
        for mode in 0..10 {
            let mut ts = tokens(&["コンフィグ", "の", "値", "を", "設定", "する"]);
            match mode {
                0 => ts[0].pos = Arc::from("名詞,固有名詞,人名,名"),
                1 => ts[0].pos = Arc::from("名詞,固有名詞,組織,*"),
                2 => ts[0].base_form = Arc::from("別名"),
                3 => ts[0].reading = Arc::from("ベツメイ"),
                4 => ts[0].is_known = false,
                5 => ts[1].pos = Arc::from("名詞,一般,*,*"),
                6 => ts[4].is_known = false,
                7 => ts[5].base_form = Arc::from("読む"),
                8 => ts[5].conj_form = Arc::from("仮定形"),
                _ => {
                    for t in &mut ts[4..] {
                        t.start += 1;
                        t.end += 1;
                    }
                }
            }
            let before = format!("{ts:?}");
            let mut spans = Vec::new();
            super::apply(
                &mut ts,
                &[],
                &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
            );
            assert!(spans.is_empty(), "mode {mode}");
            assert_eq!(format!("{ts:?}"), before);
        }
        let mut ts = tokens(&["コンフィグ", "の", "値", "を", "設定", "する"]);
        let protected = ts[4].start..ts[5].end;
        let before = format!("{ts:?}");
        apply(&mut ts, &[protected]);
        assert_eq!(format!("{ts:?}"), before);
    }

    #[test]
    fn mathematical_heads_require_standard_known_common_nouns() {
        for head in ["変数", "定数", "係数"] {
            for quoted in [false, true] {
                let words = if quoted {
                    vec!["「", head, "の", "値", "」"]
                } else {
                    vec![head, "の", "値", "を"]
                };
                let mut ts = tokens(&words);
                let before = ts.clone();
                let mut spans = Vec::new();
                super::apply(
                    &mut ts,
                    &[],
                    &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
                );
                assert_eq!(spans.len(), 1, "{head}");
                for (old, new) in before.iter().zip(&ts) {
                    assert_other_fields_equal(old, new);
                    if old.surface.as_ref() != "値" {
                        assert_eq!(old.reading, new.reading);
                        assert_eq!(old.pronunciation, new.pronunciation);
                    }
                }
            }
            for mode in 0..5 {
                let mut ts = tokens(&[head, "の", "値", "を"]);
                match mode {
                    0 => ts[0].is_known = false,
                    1 => ts[0].pos = Arc::from("名詞,固有名詞,一般,*"),
                    2 => ts[0].base_form = Arc::from("別名"),
                    3 => ts[0].reading = Arc::from("ベツメイ"),
                    _ => ts[0].pronunciation = Arc::from("ベツメイ"),
                }
                let before = format!("{ts:?}");
                let mut spans = Vec::new();
                super::apply(
                    &mut ts,
                    &[],
                    &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
                );
                assert!(spans.is_empty(), "{head} mode {mode}");
                assert_eq!(format!("{ts:?}"), before);
            }
        }
    }

    #[test]
    fn underscore_identifier_prefix_is_not_a_frame_boundary() {
        for head in ["関数", "変数", "定数", "係数"] {
            let mut ts = tokens(&["SKU", "_", head, "の", "値", "を"]);
            let before = format!("{ts:?}");
            let mut spans = Vec::new();
            super::apply(
                &mut ts,
                &[],
                &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
            );
            assert!(spans.is_empty(), "{head}");
            assert_eq!(format!("{ts:?}"), before);
        }
        let mut ts = tokens(&["SKU", "_", "コンフィグ", "の", "値", "を", "設定", "する"]);
        let before = format!("{ts:?}");
        let mut spans = Vec::new();
        super::apply(
            &mut ts,
            &[],
            &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
        );
        assert!(spans.is_empty());
        assert_eq!(format!("{ts:?}"), before);
    }

    #[test]
    fn coordinate_and_difference_frames_change_only_the_value() {
        for words in [
            vec!["座標", "の", "値", "を"],
            vec!["「", "三", "次元", "座標", "の", "値", "」"],
            vec!["差分", "の", "値", "を", "算出", "する"],
            vec!["『", "差分", "の", "値", "を", "算出", "し", "た", "』"],
        ] {
            let mut ts = tokens(&words);
            let before = ts.clone();
            let mut spans = Vec::new();
            super::apply(
                &mut ts,
                &[],
                &mut ContextualReadingContext::new("", 0, Some(&mut spans)),
            );
            assert_eq!(spans.len(), 1, "{words:?}");
            for (old, new) in before.iter().zip(&ts) {
                assert_other_fields_equal(old, new);
                if new.surface.as_ref() == "値" {
                    assert_eq!(new.reading.as_ref(), "アタイ");
                    assert_eq!(new.pronunciation.as_ref(), "アタイ");
                } else {
                    assert_eq!(old.reading, new.reading);
                    assert_eq!(old.pronunciation, new.pronunciation);
                }
            }
        }
    }

    #[test]
    fn quantity_head_profiles_modifiers_and_predicates_are_bounded() {
        for words in [
            vec!["二", "次元", "座標", "の", "値", "を"],
            vec!["3", "次元", "座標", "の", "値", "を"],
            vec!["製品", "三", "次元", "座標", "の", "値", "を"],
            vec!["SKU", "_", "座標", "の", "値", "を"],
            vec!["SKU", "_", "三", "次元", "座標", "の", "値", "を"],
            vec!["差分", "の", "値", "を"],
            vec!["差分", "の", "値", "を", "計算", "する"],
            vec!["魚", "の", "値", "を", "算出", "する"],
            vec!["SKU", "_", "差分", "の", "値", "を", "算出", "する"],
            vec!["「", "三", "次元", "座標", "」", "の", "値", "を"],
        ] {
            let mut ts = tokens(&words);
            let before = format!("{ts:?}");
            apply(&mut ts, &[]);
            assert_eq!(format!("{ts:?}"), before, "{words:?}");
        }
        for index in 0..3 {
            for mode in 0..3 {
                let mut ts = tokens(&["三", "次元", "座標", "の", "値", "を"]);
                match mode {
                    0 => ts[index].is_known = false,
                    1 => ts[index].pos = Arc::from("名詞,固有名詞,一般,*"),
                    _ => ts[index].pronunciation = Arc::from("ベツメイ"),
                }
                let before = format!("{ts:?}");
                apply(&mut ts, &[]);
                assert_eq!(format!("{ts:?}"), before);
            }
        }
        for index in [0, 4, 5] {
            let mut ts = tokens(&["差分", "の", "値", "を", "算出", "する"]);
            ts[index].base_form = Arc::from("別語");
            let before = format!("{ts:?}");
            apply(&mut ts, &[]);
            assert_eq!(format!("{ts:?}"), before);
        }
        let mut ts = tokens(&["三", "次元", "座標", "の", "値", "を"]);
        let protected = ts[0].start..ts[1].end;
        let before = format!("{ts:?}");
        apply(&mut ts, &[protected]);
        assert_eq!(format!("{ts:?}"), before);
    }

    #[test]
    fn ipadic_unknown_function_symbol_and_brackets_are_not_names() {
        let mut ts = tokens(&["f", "(", "2", ")", "の", "値", "を", "求める"]);
        ts[0].is_known = false;
        ts[0].pos = Arc::from("名詞,固有名詞,組織,*");
        for i in [1, 3] {
            ts[i].is_known = false;
            ts[i].reading = Arc::from("");
            ts[i].pronunciation = Arc::from("");
        }
        let protected_argument = ts[2].start..ts[2].end;
        let argument_before = format!("{:?}", ts[2]);
        apply(&mut ts, &[protected_argument]);
        assert_eq!(ts[5].reading.as_ref(), "アタイ");
        assert_eq!(format!("{:?}", ts[2]), argument_before);
    }

    #[test]
    fn prices_generic_verbs_compounds_and_internal_quotes_are_unchanged() {
        for words in [
            vec!["魚", "の", "値", "を"],
            vec!["計算", "し", "た", "値", "を"],
            vec!["得", "られ", "た", "値", "を"],
            vec!["現在", "の", "値", "を"],
            vec!["戻り値", "を"],
            vec!["戻り", "値", "を"],
            vec!["「", "関数", "」", "の", "値", "を"],
            vec!["関数", "の", "値", "さん"],
            vec!["製品", "f", "(", "2", ")", "の", "値", "を"],
            vec!["f2", "の", "値", "を"],
            vec!["f", "(", "2", ")", "の", "値", "を"],
        ] {
            let mut ts = tokens(&words);
            let before = format!("{ts:?}");
            apply(&mut ts, &[]);
            assert_eq!(format!("{ts:?}"), before, "{words:?}");
        }
    }

    #[test]
    fn names_unknown_target_and_explicit_readings_are_unchanged() {
        for (pos, base, reading, pronunciation, known) in [
            ("名詞,固有名詞,人名,名", "値", "ネ", "ネ", true),
            ("名詞,一般,*,*", "値", "アタイ", "アタイ", true),
            ("名詞,一般,*,*", "値", "チ", "チ", true),
            ("名詞,一般,*,*", "値", "ネ", "アタイ", true),
            ("名詞,一般,*,*", "別語", "ネ", "ネ", true),
            ("名詞,一般,*,*", "値", "ネ", "ネ", false),
        ] {
            let mut ts = tokens(&["関数", "の", "値", "を"]);
            ts[2].pos = Arc::from(pos);
            ts[2].base_form = Arc::from(base);
            ts[2].reading = Arc::from(reading);
            ts[2].pronunciation = Arc::from(pronunciation);
            ts[2].is_known = known;
            let before = format!("{ts:?}");
            apply(&mut ts, &[]);
            assert_eq!(format!("{ts:?}"), before);
        }
    }

    #[test]
    fn nested_wrappers_do_not_change_the_meaning_of_math() {
        let mut ts = tokens(&[
            "「", "『", "関数", "の", "値", "』", "」", "関数", "の", "値", "を",
        ]);
        let before = ts.clone();
        apply(&mut ts, &[]);
        assert_eq!(ts[4].reading.as_ref(), "アタイ");
        assert_eq!(ts[9].reading.as_ref(), "アタイ");
        for (old, new) in before.iter().zip(&ts) {
            assert_other_fields_equal(old, new);
        }
    }

    #[test]
    fn gaps_wrong_inflection_and_protected_frames_do_not_match() {
        let mut ts = tokens(&["積分", "し", "た", "値", "を"]);
        for t in &mut ts[1..] {
            t.start += 1;
            t.end += 1;
        }
        let before = format!("{ts:?}");
        apply(&mut ts, &[]);
        assert_eq!(format!("{ts:?}"), before);

        let mut ts = tokens(&["f", "(", "2", ")", "の", "値", "を", "求める"]);
        let wider_than_argument = ts[1].start..ts[2].end;
        let before = format!("{ts:?}");
        apply(&mut ts, &[wider_than_argument]);
        assert_eq!(format!("{ts:?}"), before);

        let mut ts = tokens(&["積分", "し", "た", "値", "を"]);
        ts[1].base_form = Arc::from("別語");
        let before = format!("{ts:?}");
        apply(&mut ts, &[]);
        assert_eq!(format!("{ts:?}"), before);

        let mut ts = tokens(&["関数", "の", "値", "を"]);
        let range = ts[0].start..ts[2].end;
        let before = format!("{ts:?}");
        apply(&mut ts, &[range]);
        assert_eq!(format!("{ts:?}"), before);
    }
}
