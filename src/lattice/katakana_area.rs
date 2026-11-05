//! 完全な面積構文の中で、検証できるカタカナ別表記を数量として合成する。
//! 元の固有名詞の辞書候補は残し、引用・識別子・独自素性・未知の全体語を保持する。

use super::{ContextualReadingContext, Token};
use crate::char_class::{CharType, type_index};
use crate::{DictError, Dictionary};
use std::{ops::Range, sync::Arc};

fn fixed(
    t: &Token,
    surface: &str,
    base: &str,
    pos: &str,
    reading: &str,
    pronunciation: &str,
) -> bool {
    t.is_known
        && t.surface.as_ref() == surface
        && t.base_form.as_ref() == base
        && t.pos.as_ref() == pos
        && t.reading.as_ref() == reading
        && t.pronunciation.as_ref() == pronunciation
        && t.is_uninflected()
}

fn japanese(c: char, dict: &Dictionary) -> bool {
    if c.is_ascii_alphanumeric() || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９') {
        return false;
    }
    let class = dict.char_type_index(c) as usize;
    [CharType::Hiragana, CharType::Katakana, CharType::Kanji]
        .into_iter()
        .any(|kind| type_index(kind) == class)
}

fn left_boundary(
    tokens: &[Token],
    index: usize,
    context: &ContextualReadingContext<'_>,
    dict: &Dictionary,
) -> bool {
    let (input, origin) = context.source();
    let Some(prefix) = tokens[index]
        .start
        .checked_sub(origin)
        .and_then(|start| input.get(..start))
    else {
        return false;
    };
    let prefix = prefix.trim_end_matches([' ', '\t', '　']);
    let named = prefix.trim_end_matches([' ', '\t', '　', '、', ',']);
    if ["", "は", "が", "も"].iter().any(|tail| {
        named.strip_suffix(tail).is_some_and(|head| {
            [
                "名前",
                "名称",
                "作品名",
                "題名",
                "書名",
                "曲名",
                "タイトル",
                "識別子",
                "型番",
                "コード",
                "ID",
            ]
            .iter()
            .any(|label| head.ends_with(label))
        })
    }) || context.identifier_boundary(tokens[index].start)
    {
        return false;
    }
    let mut previous = prefix.chars().rev();
    match previous.next() {
        // ASCII句点の左が和文または区切りの場合だけ認める。A.面積のようなIDは保つ。
        Some('.') => previous.next().is_none_or(|c| {
            japanese(c, dict)
                || c.is_whitespace()
                || crate::sentence::is_sentence_ender(c)
                || crate::sentence::is_closing_bracket(c)
        }),
        Some(c) => matches!(c, '。' | '、' | '！' | '？' | '!' | '?' | '\r' | '\n'),
        None => true,
    }
}

fn right_boundary(rest: &str, dict: &Dictionary) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        // TTS前処理のASCII句点も認めるが、識別子の接尾へ広げない。
        Some('.') => chars.next().is_none_or(|c| {
            japanese(c, dict)
                || c.is_whitespace()
                || matches!(
                    c,
                    '.' | '。' | '！' | '？' | '!' | '?' | '」' | '』' | '）' | ')' | '"'
                )
        }),
        Some(c) => matches!(c, '。' | '、' | '！' | '？' | '!' | '?' | '\r' | '\n'),
        None => true,
    }
}

fn rewrite(
    tokens: &[Token],
    index: usize,
    dict: &Dictionary,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) -> Result<Option<(Token, Token)>, DictError> {
    let Some(frame) = tokens.get(index..index + 6) else {
        return Ok(None);
    };
    if !fixed(
        &frame[0],
        "面積",
        "面積",
        "名詞,一般,*,*",
        "メンセキ",
        "メンセキ",
    ) || !(fixed(&frame[1], "は", "は", "助詞,係助詞,*,*", "ハ", "ワ")
        || fixed(&frame[1], "が", "が", "助詞,格助詞,一般,*", "ガ", "ガ")
        || fixed(&frame[1], "も", "も", "助詞,係助詞,*,*", "モ", "モ"))
        || !fixed(
            &frame[3],
            "ヘイホー",
            "Hey Ho",
            "名詞,固有名詞,一般,*",
            "ヘイホー",
            "ヘイホー",
        )
        || !fixed(
            &frame[4],
            "メートル",
            "メートル",
            "名詞,接尾,助数詞,*",
            "メートル",
            "メートル",
        )
        || !frame[5].is_known
        || frame[5].surface.as_ref() != "です"
        || frame[5].base_form.as_ref() != "です"
        || frame[5].pos.as_ref() != "助動詞,*,*,*"
        || frame[5].reading.as_ref() != "デス"
        || frame[5].pronunciation.as_ref() != "デス"
        || frame[5].conj_type.as_ref() != "特殊・デス"
        || frame[5].conj_form.as_ref() != "基本形"
        || !frame.windows(2).all(|w| w[0].end == w[1].start)
        || !left_boundary(tokens, index, context, dict)
    {
        return Ok(None);
    }
    let Some((reading, pronunciation)) = crate::numeric::katakana_alias_readings(&frame[2]) else {
        return Ok(None);
    };
    let (input, origin) = context.source();
    if !frame.iter().all(|t| {
        t.start
            .checked_sub(origin)
            .zip(t.end.checked_sub(origin))
            .and_then(|(start, end)| input.get(start..end))
            == Some(t.surface.as_ref())
    }) {
        return Ok(None);
    }
    let end = frame[5].end - origin;
    if !right_boundary(&input[end..], dict) {
        return Ok(None);
    }
    let range = frame[0].start..frame[5].end;
    let protected_index = protected.partition_point(|r| r.end <= range.start);
    if protected
        .get(protected_index)
        .is_some_and(|r| r.start < range.end)
        || !context.unquoted(&range)
    {
        return Ok(None);
    }
    let quantity = &input[frame[2].start - origin..frame[4].end - origin];
    if dict
        .lookup(quantity)?
        .iter()
        .any(|(end, rows)| *end == quantity.len() && !rows.is_empty())
    {
        return Ok(None);
    }
    let mut number = frame[2].clone();
    number.pos = Arc::from("名詞,数,*,*");
    number.reading = Arc::from(reading);
    number.pronunciation = Arc::from(pronunciation);
    number.is_known = false;
    let unit = Token {
        surface: Arc::from(&input[frame[3].start - origin..frame[4].end - origin]),
        start: frame[3].start,
        end: frame[4].end,
        pos: Arc::from("名詞,接尾,助数詞,*"),
        conj_type: Arc::from(""),
        conj_form: Arc::from(""),
        base_form: Arc::from("平方メートル"),
        reading: Arc::from("ヘイホウメートル"),
        pronunciation: Arc::from("ヘイホーメートル"),
        word_cost: (i64::from(frame[3].word_cost) + i64::from(frame[4].word_cost))
            .clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
        is_known: false,
    };
    Ok(Some((number, unit)))
}

pub(super) fn apply(
    out: &mut Vec<Token>,
    first: usize,
    dict: &Dictionary,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) -> Result<(), DictError> {
    let (mut read, mut write) = (first, first);
    while read < out.len() {
        if out[read].surface.as_ref() == "面積"
            && let Some((number, unit)) = rewrite(out, read, dict, protected, context)?
        {
            for _ in 0..2 {
                if write != read {
                    out.swap(write, read);
                }
                write += 1;
                read += 1;
            }
            out[write] = number;
            out[write + 1] = unit;
            read += 3;
            write += 2;
        } else {
            if write != read {
                out.swap(write, read);
            }
            write += 1;
            read += 1;
        }
    }
    out.truncate(write);
    Ok(())
}
