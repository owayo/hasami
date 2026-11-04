//! 明確な観測・記録の構文だけで、既知の「震度7」の読みを補正する。
//! 引用・括弧・名称の候補と独自の素性を保ち、辞書の登録は除去しない。

use super::{ContextualReadingContext, Token};
use std::{ops::Range, sync::Arc};

fn fixed(t: &Token, surface: &str, pos: &str, speech: (&str, &str)) -> bool {
    t.is_known
        && t.surface.as_ref() == surface
        && t.base_form == t.surface
        && t.pos.as_ref() == pos
        && t.reading.as_ref() == speech.0
        && t.pronunciation.as_ref() == speech.1
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
}

fn inflected(
    t: &Token,
    surface: &str,
    base: &str,
    pos: &str,
    speech: &str,
    conjugation: (&str, &str),
) -> bool {
    t.is_known
        && t.surface.as_ref() == surface
        && t.base_form.as_ref() == base
        && t.pos.as_ref() == pos
        && t.reading.as_ref() == speech
        && t.pronunciation.as_ref() == speech
        && t.conj_type.as_ref() == conjugation.0
        && t.conj_form.as_ref() == conjugation.1
}

fn punctuation(t: &Token) -> bool {
    t.is_known
        && t.base_form == t.surface
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
        && matches!(t.surface.as_ref(), "。" | "、" | "！" | "？" | "!" | "?")
        && (t.pos.starts_with("記号,句点,") || t.pos.starts_with("記号,読点,"))
        && (t.reading.is_empty() || t.reading == t.surface)
        && (t.pronunciation.is_empty() || t.pronunciation == t.surface)
}

fn left_boundary(tokens: &[Token], start: usize, input: &str, origin: usize) -> bool {
    let Some(prefix) = tokens[start]
        .start
        .checked_sub(origin)
        .and_then(|relative| input.get(..relative))
    else {
        return false;
    };
    let prefix = prefix.trim_end_matches([' ', '\t', '　']);
    let label_prefix = prefix.trim_end_matches([' ', '\t', '　', '、', ',']);
    if label_prefix
        .strip_suffix('は')
        .or_else(|| label_prefix.strip_suffix('も'))
        .is_some_and(|head| {
            let head = head.trim_end_matches([' ', '\t', '　']);
            [
                "題名",
                "書名",
                "作品名",
                "名称",
                "曲名",
                "タイトル",
                "識別子",
                "型番",
                "型式",
                "コード",
                "ID",
            ]
            .iter()
            .any(|label| head.ends_with(label))
        })
    {
        return false;
    }
    if start > 0 {
        let t = &tokens[start - 1];
        return punctuation(t)
            || fixed(t, "は", "助詞,係助詞,*,*", ("ハ", "ワ"))
            || fixed(t, "も", "助詞,係助詞,*,*", ("モ", "モ"))
            || fixed(t, "で", "助詞,格助詞,一般,*", ("デ", "デ"));
    }
    prefix
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || matches!(c, '。' | '、' | '！' | '？' | '!' | '?'))
}

/// 外側の範囲だけを保持し、未閉鎖の引用は末尾まで保護する。
pub(crate) fn quotation_ranges(input: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut stack = Vec::new();
    let mut start = 0;
    let mut escaped = false;
    let mut previous = None;
    let mut chars = input.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let before = previous;
        previous = Some(c);
        if c == '\\' {
            escaped = !escaped;
            continue;
        }
        let escaped_quote = escaped && !stack.is_empty() && matches!(c, '\'' | '"' | '`');
        escaped = false;
        if escaped_quote {
            continue;
        }
        if c == '\''
            && before.is_some_and(|c: char| c.is_ascii_alphanumeric())
            && chars.peek().is_some_and(|(_, c)| c.is_ascii_alphanumeric())
        {
            continue;
        }
        if stack
            .last()
            .is_some_and(|&close| close == c || close == '〟' && c == '〞')
        {
            stack.pop();
            if stack.is_empty() {
                ranges.push(start..i + c.len_utf8());
            }
            continue;
        }
        let close = if matches!(c, '\'' | '"' | '`') {
            Some(c)
        } else {
            crate::sentence::closing_bracket(c)
        };
        if let Some(close) = close {
            if stack.is_empty() {
                start = i;
            }
            stack.push(close);
        }
    }
    if !stack.is_empty() {
        ranges.push(start..input.len());
    }
    ranges
}

pub(super) fn apply(
    tokens: &mut [Token],
    i: usize,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) -> bool {
    let Some(phrase) = tokens.get(i..i + 5) else {
        return false;
    };
    if !fixed(
        &phrase[0],
        "震度7",
        "名詞,固有名詞,一般,*",
        ("シンドセブン", "シンドセブン"),
    ) || !fixed(&phrase[1], "を", "助詞,格助詞,一般,*", ("ヲ", "ヲ"))
        || !(fixed(
            &phrase[2],
            "観測",
            "名詞,サ変接続,*,*",
            ("カンソク", "カンソク"),
        ) || fixed(
            &phrase[2],
            "記録",
            "名詞,サ変接続,*,*",
            ("キロク", "キロク"),
        ))
        || !inflected(
            &phrase[3],
            "し",
            "する",
            "動詞,自立,*,*",
            "シ",
            ("サ変・スル", "連用形"),
        )
        || !inflected(
            &phrase[4],
            "た",
            "た",
            "助動詞,*,*,*",
            "タ",
            ("特殊・タ", "基本形"),
        )
        || tokens.get(i + 5).is_some_and(|t| !punctuation(t))
    {
        return false;
    }
    let start = if i > 0
        && fixed(
            &tokens[i - 1],
            "最大",
            "名詞,一般,*,*",
            ("サイダイ", "サイダイ"),
        ) {
        i - 1
    } else {
        i
    };
    let phrase = &tokens[start..i + 5];
    let (input, origin) = context.source();
    if !left_boundary(tokens, start, input, origin)
        || context.identifier_boundary(phrase[0].start)
        || !phrase.windows(2).all(|w| w[0].end == w[1].start)
        || !phrase.iter().all(|t| {
            t.start
                .checked_sub(origin)
                .zip(t.end.checked_sub(origin))
                .and_then(|(start, end)| input.get(start..end))
                == Some(t.surface.as_ref())
        })
    {
        return false;
    }
    let range = phrase[0].start..phrase.last().unwrap().end;
    let Some(tail) = range
        .end
        .checked_sub(origin)
        .and_then(|end| input.get(end..))
    else {
        return false;
    };
    if !tail
        .chars()
        .next()
        .is_none_or(|c| matches!(c, '。' | '、' | '！' | '？' | '!' | '?' | '\n' | '\r'))
    {
        return false;
    }
    // 通常のASCII数量処理は既知語の内側の裸の7も保護する。この一桁だけを
    // 完全な測定構文の確認後に認め、識別子など広い範囲は引き続き保護する。
    let digit = tokens[i].end - 1..tokens[i].end;
    if protected
        .iter()
        .any(|r| r.start < range.end && range.start < r.end && *r != digit)
        || !context.unquoted(&range)
    {
        return false;
    }
    let speech: Arc<str> = Arc::from("シンドナナ");
    tokens[i].reading = Arc::clone(&speech);
    tokens[i].pronunciation = speech;
    context.record(&tokens[i]);
    true
}
