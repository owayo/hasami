//! 明示された段位の構文で、同表層の実辞書候補を選ぶ。
//!
//! 姓として正当な語を削除せず、段位の一般名詞候補が一意に存在するときだけ
//! その候補の素性へ切り替える。段位側では最良パスの区切りを保つ。
//! 明示された姓ラベルでは、連続する数詞と「段」を実辞書の姓候補へ結合する。
//! 読みや品詞は組み立てず、裸の語、識別子、明示保護には適用しない。

use super::{ContextualReadingContext, Token};
use crate::{DictEntry, DictError, Dictionary};
use std::{ops::Range, sync::Arc};

fn rank_surface(surface: &str) -> bool {
    let Some(number) = surface.strip_suffix('段') else {
        return false;
    };
    let digit = |c| {
        matches!(
            c,
            '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九'
        )
    };
    let chars: Vec<_> = number.chars().collect();
    match chars.as_slice() {
        ['初'] | ['十'] => true,
        [one] => digit(*one),
        ['十', one] => digit(*one),
        [ten, '十'] => digit(*ten) && *ten != '一',
        [ten, '十', one] => digit(*ten) && *ten != '一' && digit(*one),
        _ => false,
    }
}

fn no_conjugation(value: &str) -> bool {
    value.is_empty() || value == "*"
}

fn kana(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| matches!(c, 'ァ'..='ヺ' | 'ー'))
}

fn same_selected(entry: &DictEntry, token: &Token) -> bool {
    entry.surface == token.surface
        && entry.pos == token.pos
        && entry.base_form == token.base_form
        && entry.reading == token.reading
        && entry.pronunciation == token.pronunciation
        && entry.cost == token.word_cost
        && no_conjugation(&entry.conj_type)
        && no_conjugation(&entry.conj_form)
}

fn candidate(dict: &Dictionary, token: &Token) -> Result<Option<DictEntry>, DictError> {
    let mut selected_exists = false;
    let mut ordinary = None;
    for (end, entries) in dict.lookup(&token.surface)? {
        if end != token.surface.len() {
            continue;
        }
        for entry in entries {
            selected_exists |= same_selected(&entry, token);
            if entry.surface == token.surface
                && entry.base_form == token.surface
                && entry.pos.as_ref() == "名詞,一般,*,*"
                && no_conjugation(&entry.conj_type)
                && no_conjugation(&entry.conj_form)
                && kana(&entry.reading)
                && kana(&entry.pronunciation)
            {
                if ordinary.is_some() {
                    return Ok(None);
                }
                ordinary = Some(entry);
            }
        }
    }
    Ok(selected_exists.then_some(ordinary).flatten())
}

fn at(tokens: &[Token], start: usize) -> Option<&Token> {
    tokens
        .get(tokens.partition_point(|t| t.start < start))
        .filter(|t| t.start == start)
}

fn word<'a>(tokens: &'a [Token], start: usize, surface: &str, pos: &str) -> Option<&'a Token> {
    at(tokens, start).filter(|t| {
        t.is_known
            && t.surface.as_ref() == surface
            && t.base_form == t.surface
            && t.pos.starts_with(pos)
            && t.conj_type.is_empty()
            && t.conj_form.is_empty()
    })
}

fn exact(input: &str, origin: usize, token: &Token) -> bool {
    token
        .start
        .checked_sub(origin)
        .zip(token.end.checked_sub(origin))
        .and_then(|(start, end)| input.get(start..end))
        == Some(token.surface.as_ref())
}

fn identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c,
        'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９' | '_' | '-' | '－' | '−' | '+' | '＋'
        | '/' | '／' | ':' | '：' | '.' | '．' | '〇' | '零' | '一' | '二' | '三'
        | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百' | '千' | '万' | '億')
}

fn wrapped(input: &str, origin: usize, token: &Token) -> Option<Range<usize>> {
    let mut start = token.start.checked_sub(origin)?;
    let mut end = token.end.checked_sub(origin)?;
    if input.get(start..end)? != token.surface.as_ref() {
        return None;
    }
    // 外側で直結する対応引用だけを認め、内部の文字や空白は取り除かない。
    for _ in 0..4 {
        let Some(left) = input[..start].chars().next_back() else {
            break;
        };
        let Some(right) = input[end..].chars().next() else {
            break;
        };
        if !matches!(
            (left, right),
            ('「', '」') | ('『', '』') | ('“', '”') | ('"', '"')
        ) {
            break;
        }
        start -= left.len_utf8();
        end += right.len_utf8();
    }
    if input[..start]
        .chars()
        .next_back()
        .is_some_and(identifier_char)
        || input[end..].chars().next().is_some_and(identifier_char)
    {
        return None;
    }
    Some(origin + start..origin + end)
}

fn name_context(input: &str, origin: usize, range: &Range<usize>, tokens: &[Token]) -> bool {
    let left = input[..range.start - origin].trim_end_matches([' ', '\t', '　']);
    let right = &input[range.end - origin..];
    [
        "姓は",
        "姓が",
        "名字は",
        "名字が",
        "苗字は",
        "名前は",
        "氏名は",
        "人名は",
        "姓の",
        "名字の",
        "苗字の",
        "名前の",
        "氏名の",
        "人名の",
    ]
    .iter()
    .any(|label| left.ends_with(label))
        || left.ends_with("と読む")
        || left.ends_with("姓が同じ")
        || [
            "さん",
            "氏",
            "様",
            "君",
            "くん",
            "ちゃん",
            "という姓",
            "という名字",
            "という苗字",
            "という名前",
        ]
        .iter()
        .any(|suffix| right.starts_with(suffix))
        || at(tokens, range.end)
            .is_some_and(|t| t.is_known && t.pos.starts_with("名詞,固有名詞,人名,名"))
}

fn following(tokens: &[Token], end: usize, input: &str, origin: usize) -> bool {
    if let Some(t) = at(tokens, end) {
        return t.is_known
            && (t.pos.starts_with("助詞,")
                || t.pos.starts_with("助動詞,")
                || (t.pos.starts_with("記号,")
                    && matches!(
                        t.surface.as_ref(),
                        "。" | "、" | "." | "!" | "?" | "！" | "？" | "」" | "』" | "\""
                    )));
    }
    input
        .get(end - origin..)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['\n', '\r']))
}

fn label_frame(
    tokens: &[Token],
    input: &str,
    origin: usize,
    range: &Range<usize>,
) -> Option<Range<usize>> {
    let left = &input[..range.start - origin];
    for particle in ["は", "が"] {
        if !left.ends_with(&format!("段位{particle}")) {
            continue;
        }
        let start = range.start.checked_sub("段位".len() + particle.len())?;
        let label = word(tokens, start, "段位", "名詞,一般,")?;
        let case = word(tokens, label.end, particle, "助詞,")?;
        if case.end != range.start {
            return None;
        }
        if let Some(copula) = at(tokens, range.end)
            && copula.is_known
            && copula.pos.starts_with("助動詞,")
            && matches!(copula.surface.as_ref(), "です" | "だ")
            && following(tokens, copula.end, input, origin)
        {
            return Some(start..copula.end);
        }
        if input[range.end - origin..].starts_with(['。', '\n', '\r'])
            || range.end - origin == input.len()
        {
            return Some(start..range.end);
        }
    }
    let phrase = word(tokens, range.end, "という", "助詞,")?;
    let label = word(tokens, phrase.end, "段位", "名詞,一般,")?;
    if input[label.end - origin..].starts_with("ではな")
        || input[label.end - origin..].starts_with("でな")
    {
        return None;
    }
    following(tokens, label.end, input, origin).then_some(range.start..label.end)
}

fn predicate_frame(
    tokens: &[Token],
    input: &str,
    origin: usize,
    range: &Range<usize>,
) -> Option<Range<usize>> {
    let case = at(tokens, range.end)?;
    if !case.is_known || !case.pos.starts_with("助詞,格助詞,") {
        return None;
    }
    let verb = if case.surface.as_ref() == "を" {
        let verb = at(tokens, case.end)?;
        (verb.is_known && verb.base_form.as_ref() == "目指す" && verb.pos.starts_with("動詞,自立,"))
            .then_some(verb)?
    } else if case.surface.as_ref() == "に" {
        let promotion = word(tokens, case.end, "昇段", "名詞,サ変接続,")?;
        let verb = at(tokens, promotion.end)?;
        (verb.is_known && verb.base_form.as_ref() == "する" && verb.pos.starts_with("動詞,自立,"))
            .then_some(verb)?
    } else {
        return None;
    };
    following(tokens, verb.end, input, origin).then_some(range.start..verb.end)
}

fn person_frame(
    tokens: &[Token],
    input: &str,
    origin: usize,
    range: &Range<usize>,
) -> Option<Range<usize>> {
    let i = tokens.partition_point(|t| t.start < range.start);
    let person = tokens.get(i.checked_sub(1)?)?;
    if !person.is_known
        || person.end != range.start
        || !person.pos.starts_with("名詞,固有名詞,人名,")
        || !following(tokens, range.end, input, origin)
    {
        return None;
    }
    if person.pos.as_ref() == "名詞,固有名詞,人名,一般" {
        return Some(person.start..range.end);
    }
    if person.pos.as_ref() == "名詞,固有名詞,人名,名"
        && let Some(family) = tokens.get(i.checked_sub(2)?)
        && family.is_known
        && family.end == person.start
        && family.pos.as_ref() == "名詞,固有名詞,人名,姓"
    {
        return Some(family.start..range.end);
    }
    let prefix = &input[..person.start - origin];
    let label_end = if prefix.ends_with("棋士の") {
        person.start - "の".len()
    } else {
        person.start
    };
    let start = label_end.checked_sub("棋士".len())?;
    let label = word(tokens, start, "棋士", "名詞,一般,")?;
    if label.end != label_end {
        return None;
    }
    if label_end != person.start && word(tokens, label_end, "の", "助詞,")?.end != person.start {
        return None;
    }
    Some(start..range.end)
}

fn complete_frame(tokens: &[Token], frame: &Range<usize>, input: &str, origin: usize) -> bool {
    let start = tokens.partition_point(|t| t.start < frame.start);
    let end = tokens.partition_point(|t| t.start < frame.end);
    let part = &tokens[start..end];
    part.first().is_some_and(|t| t.start == frame.start)
        && part.last().is_some_and(|t| t.end == frame.end)
        && part.iter().all(|t| t.is_known && exact(input, origin, t))
        && part.windows(2).all(|w| w[0].end == w[1].start)
}

fn surname_label_frame(
    tokens: &[Token],
    input: &str,
    origin: usize,
    range: &Range<usize>,
) -> Option<Range<usize>> {
    let left = &input[..range.start - origin];
    for label in ["姓", "名字", "苗字"] {
        for particle in ["は", "が"] {
            if !left.ends_with(&format!("{label}{particle}")) {
                continue;
            }
            let start = range.start.checked_sub(label.len() + particle.len())?;
            let noun = word(tokens, start, label, "名詞,一般,")?;
            let case = word(tokens, noun.end, particle, "助詞,")?;
            if case.end != range.start {
                return None;
            }
            let copula = at(tokens, range.end)?;
            return (copula.is_known
                && copula.pos.starts_with("助動詞,")
                && matches!(copula.surface.as_ref(), "です" | "だ")
                && following(tokens, copula.end, input, origin))
            .then_some(start..copula.end);
        }
    }
    None
}

/// 既選択の標準的な数詞＋段から、一意の実姓候補だけを選ぶ。
/// 一般候補の実読みとも照合し、独自読みの数詞や助数詞を上書きしない。
fn surname_candidate(
    dict: &Dictionary,
    surface: &str,
    pieces: &[Token],
) -> Result<Option<DictEntry>, DictError> {
    let mut ordinary = None;
    let mut surname = None;
    for (end, entries) in dict.lookup(surface)? {
        if end != surface.len() {
            continue;
        }
        for entry in entries {
            if entry.surface.as_ref() != surface
                || entry.base_form.as_ref() != surface
                || !no_conjugation(&entry.conj_type)
                || !no_conjugation(&entry.conj_form)
                || !kana(&entry.reading)
                || !kana(&entry.pronunciation)
            {
                continue;
            }
            let slot = match entry.pos.as_ref() {
                "名詞,一般,*,*" => &mut ordinary,
                "名詞,固有名詞,人名,姓" => &mut surname,
                _ => continue,
            };
            if slot.is_some() {
                return Ok(None);
            }
            *slot = Some(entry);
        }
    }
    let Some(ordinary) = ordinary else {
        return Ok(None);
    };
    let reading: String = pieces.iter().map(|t| t.reading.as_ref()).collect();
    let pronunciation: String = pieces.iter().map(|t| t.pronunciation.as_ref()).collect();
    Ok(
        (ordinary.reading.as_ref() == reading && ordinary.pronunciation.as_ref() == pronunciation)
            .then_some(surname)
            .flatten(),
    )
}

/// 明示的な姓ラベルに限り、実姓候補の範囲を一度だけ線形に結合する。
pub(super) fn apply_explicit_names(
    out: &mut Vec<Token>,
    first: usize,
    dict: &Dictionary,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) -> Result<(), DictError> {
    let (input, origin) = context.source();
    let tokens = &out[first..];
    let mut plans = Vec::new();
    for (i, number) in tokens.iter().enumerate() {
        if !number.is_known || number.pos.as_ref() != "名詞,数,*,*" {
            continue;
        }
        let mut end = i;
        while end < (i + 3).min(tokens.len()) {
            let piece = &tokens[end];
            if !piece.is_known
                || piece.pos.as_ref() != "名詞,数,*,*"
                || piece.base_form != piece.surface
                || !piece.conj_type.is_empty()
                || !piece.conj_form.is_empty()
                || !exact(input, origin, piece)
                || end > i && tokens[end - 1].end != piece.start
                || piece.end - number.start > 3 * '一'.len_utf8()
            {
                break;
            }
            end += 1;
        }
        let Some(suffix) = tokens.get(end) else {
            continue;
        };
        if end == i
            || !suffix.is_known
            || suffix.surface.as_ref() != "段"
            || suffix.base_form != suffix.surface
            || suffix.pos.as_ref() != "名詞,接尾,助数詞,*"
            || !suffix.conj_type.is_empty()
            || !suffix.conj_form.is_empty()
            || tokens[end - 1].end != suffix.start
            || !exact(input, origin, suffix)
        {
            continue;
        }
        let start = number.start - origin;
        let finish = suffix.end - origin;
        let surface = &input[start..finish];
        if !rank_surface(surface) {
            continue;
        }
        // 安い局所判定の後でのみ候補を引く。token照合が「百姓」等の部分一致を拒む。
        let left = input[..start].trim_end_matches(['「', '『', '“', '"']);
        if !["姓は", "姓が", "名字は", "名字が", "苗字は", "苗字が"]
            .iter()
            .any(|label| left.ends_with(label))
        {
            continue;
        }
        let Some(entry) = surname_candidate(dict, surface, &tokens[i..=end])? else {
            continue;
        };
        let replacement = Token {
            surface: entry.surface,
            start: number.start,
            end: suffix.end,
            pos: entry.pos,
            conj_type: Arc::from(""),
            conj_form: Arc::from(""),
            base_form: entry.base_form,
            reading: entry.reading,
            pronunciation: entry.pronunciation,
            word_cost: entry.cost,
            is_known: true,
        };
        let Some(range) = wrapped(input, origin, &replacement) else {
            continue;
        };
        let Some(frame) = surname_label_frame(tokens, input, origin, &range) else {
            continue;
        };
        if context.identifier_boundary(range.start)
            || context.identifier_boundary(frame.start)
            || input[..frame.start - origin]
                .chars()
                .next_back()
                .is_some_and(identifier_char)
            || !complete_frame(tokens, &frame, input, origin)
            || protected
                .iter()
                .any(|r| r.start < frame.end && frame.start < r.end)
        {
            continue;
        }
        let pronunciation: String = tokens[i..=end]
            .iter()
            .map(|t| t.pronunciation.as_ref())
            .collect();
        let changed = pronunciation != replacement.pronunciation.as_ref();
        plans.push((i + first, end + first + 1, replacement, changed));
    }
    if plans.is_empty() {
        return Ok(());
    }
    // 先に全候補を検査し、未変更のtoken座標でframeを証明してから結合する。
    let (mut read, mut write) = (first, first);
    for (begin, end, replacement, changed) in plans {
        while read < begin {
            out.swap(write, read);
            read += 1;
            write += 1;
        }
        out[write] = replacement;
        if changed {
            context.record(&out[write]);
        }
        write += 1;
        read = end;
    }
    while read < out.len() {
        out.swap(write, read);
        read += 1;
        write += 1;
    }
    out.truncate(write);
    Ok(())
}

pub(super) fn apply(
    tokens: &mut [Token],
    dict: &Dictionary,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) -> Result<(), DictError> {
    let (input, origin) = context.source();
    for i in 0..tokens.len() {
        let token = &tokens[i];
        if !token.is_known
            || token.pos.as_ref() != "名詞,固有名詞,人名,姓"
            || token.base_form != token.surface
            || !token.conj_type.is_empty()
            || !token.conj_form.is_empty()
            || !rank_surface(&token.surface)
        {
            continue;
        }
        if !tokens[i.saturating_sub(8)..(i + 12).min(tokens.len())]
            .iter()
            .all(|t| exact(input, origin, t))
        {
            continue;
        }
        let Some(range) = wrapped(input, origin, token) else {
            continue;
        };
        if context.identifier_boundary(range.start) || name_context(input, origin, &range, tokens) {
            continue;
        }
        let Some(frame) = label_frame(tokens, input, origin, &range)
            .or_else(|| predicate_frame(tokens, input, origin, &range))
            .or_else(|| person_frame(tokens, input, origin, &range))
        else {
            continue;
        };
        if context.identifier_boundary(frame.start)
            || input[..frame.start - origin]
                .chars()
                .next_back()
                .is_some_and(identifier_char)
            || !complete_frame(tokens, &frame, input, origin)
            || protected
                .iter()
                .any(|r| r.start < frame.end && frame.start < r.end)
        {
            continue;
        }
        let Some(entry) = candidate(dict, token)? else {
            continue;
        };
        let pronunciation_changed = token.pronunciation != entry.pronunciation;
        let replacement = Token {
            surface: Arc::clone(&token.surface),
            start: token.start,
            end: token.end,
            pos: entry.pos,
            conj_type: Arc::from(""),
            conj_form: Arc::from(""),
            base_form: entry.base_form,
            reading: entry.reading,
            pronunciation: entry.pronunciation,
            word_cost: entry.cost,
            is_known: true,
        };
        tokens[i] = replacement;
        if pronunciation_changed {
            context.record(&tokens[i]);
        }
    }
    Ok(())
}

#[cfg(all(test, feature = "build"))]
mod tests;
