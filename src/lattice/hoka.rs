//! 人を列挙する「外」を、空間の外側と区別して「ホカ」と読む。
//!
//! 「外（ほか）」の「それ以外」の語義: https://www.kanjipedia.jp/kotoba/0000848400
//! 人名（敬称付きも含む）か確認済みの役職と、既知の整数＋名・人が連続するときだけ
//! 標準の接尾語ガイ／ガイを補正する。品詞・原形・コスト・位置は維持する。
//! 一般名詞を人と推測せず、別読み、未知語、空白、引用、識別子は保持する。

use super::{ContextualReadingContext, Token};
use crate::anchors::Anchors;
use std::{ops::Range, sync::Arc};

/// 書き換えるのは単独の「外」なので、チャンクに「外」が無ければ何もしない。
pub(super) fn may_apply(anchors: Anchors) -> bool {
    anchors.intersects(Anchors::HOKA)
}

fn fixed(t: &Token, surface: &str, pos: &str, reading: &str, pronunciation: &str) -> bool {
    t.is_known_base_form()
        && t.surface.as_ref() == surface
        && t.pos.as_ref() == pos
        && t.reading.as_ref() == reading
        && t.pronunciation.as_ref() == pronunciation
}

fn person(t: &Token) -> bool {
    t.is_known_base_form()
        && matches!(
            t.pos.as_ref(),
            "名詞,固有名詞,人名,姓" | "名詞,固有名詞,人名,名" | "名詞,固有名詞,人名,一般"
        )
        && !t.reading.is_empty()
        && !t.pronunciation.is_empty()
}

fn title(t: &Token) -> bool {
    [
        ("課長", "カチョウ", "カチョー"),
        ("部長", "ブチョウ", "ブチョー"),
        ("係長", "カカリチョウ", "カカリチョー"),
        ("社長", "シャチョウ", "シャチョー"),
    ]
    .iter()
    .any(|&(s, r, p)| fixed(t, s, "名詞,一般,*,*", r, p))
}

fn head_start(tokens: &[Token], outside: usize) -> Option<usize> {
    let mut i = outside.checked_sub(1)?;
    if [
        ("さん", "サン"),
        ("氏", "シ"),
        ("様", "サマ"),
        ("さま", "サマ"),
        ("君", "クン"),
        ("くん", "クン"),
    ]
    .iter()
    .any(|&(s, r)| fixed(&tokens[i], s, "名詞,接尾,人名,*", r, r))
    {
        i = i.checked_sub(1)?;
    }
    if title(&tokens[i]) {
        return Some(i);
    }
    if !person(&tokens[i]) {
        return None;
    }
    while i > 0 && person(&tokens[i - 1]) && tokens[i - 1].end == tokens[i].start {
        i -= 1;
    }
    Some(i)
}

fn count_end(tokens: &[Token], start: usize) -> Option<usize> {
    let mut end = start;
    let mut positive = false;
    while let Some(t) = tokens
        .get(end)
        .filter(|t| crate::numeric::standard_integer_token(t))
    {
        positive |= t
            .surface
            .chars()
            .any(|c| !matches!(c, '0' | '０' | '〇' | '零'));
        end += 1;
    }
    if end == start || !positive {
        return None;
    }
    let counter = tokens.get(end)?;
    (fixed(counter, "名", "名詞,接尾,助数詞,*", "メイ", "メイ")
        || fixed(counter, "人", "名詞,接尾,助数詞,*", "ニン", "ニン"))
    .then_some(end)
}

fn left_boundary(tokens: &[Token], start: usize, input: &str, origin: usize) -> bool {
    let Some(prefix) = tokens[start]
        .start
        .checked_sub(origin)
        .and_then(|i| input.get(..i))
    else {
        return false;
    };
    if prefix
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || matches!(c, '。' | '、' | '！' | '？' | '!' | '?'))
    {
        return true;
    }
    start > 0
        && tokens[start - 1].end == tokens[start].start
        && tokens[start - 1].is_known
        && tokens[start - 1].pos.starts_with("助詞,")
}

fn right_boundary(tokens: &[Token], end: usize, input: &str, origin: usize) -> bool {
    let Some(suffix) = tokens[end]
        .end
        .checked_sub(origin)
        .and_then(|i| input.get(i..))
    else {
        return false;
    };
    if suffix
        .chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || matches!(c, '。' | '、' | '！' | '？' | '!' | '?'))
    {
        return true;
    }
    tokens.get(end + 1).is_some_and(|t| {
        t.start == tokens[end].end
            && t.is_known_base_form()
            && matches!(t.surface.as_ref(), "が" | "を" | "は" | "も" | "と" | "に")
            && (t.pos.starts_with("助詞,格助詞,") || t.pos.starts_with("助詞,係助詞,"))
    })
}

pub(super) fn apply(
    tokens: &mut [Token],
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) {
    let (input, origin) = context.source();
    for i in 1..tokens.len() {
        if !fixed(&tokens[i], "外", "名詞,接尾,一般,*", "ガイ", "ガイ") {
            continue;
        }
        let Some(start) = head_start(tokens, i) else {
            continue;
        };
        let Some(end) = count_end(tokens, i + 1) else {
            continue;
        };
        let phrase = &tokens[start..=end];
        if !phrase.windows(2).all(|w| w[0].end == w[1].start)
            || !phrase.iter().all(|t| {
                t.start
                    .checked_sub(origin)
                    .zip(t.end.checked_sub(origin))
                    .and_then(|(a, b)| input.get(a..b))
                    == Some(t.surface.as_ref())
            })
            || !left_boundary(tokens, start, input, origin)
            || !right_boundary(tokens, end, input, origin)
        {
            continue;
        }
        let frame = tokens[start].start..tokens[end].end;
        if context.identifier_boundary(frame.start)
            || protected
                .iter()
                .any(|r| r.start < frame.end && frame.start < r.end)
            || !context.unquoted(&frame)
        {
            continue;
        }
        tokens[i].reading = Arc::from("ホカ");
        tokens[i].pronunciation = Arc::clone(&tokens[i].reading);
        context.record(&tokens[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrase(offset: usize) -> (String, Vec<Token>) {
        let mut position = offset;
        let mut tokens = Vec::new();
        for (s, p, r, pn) in [
            ("田中", "名詞,固有名詞,人名,姓", "タナカ", "タナカ"),
            ("さん", "名詞,接尾,人名,*", "サン", "サン"),
            ("外", "名詞,接尾,一般,*", "ガイ", "ガイ"),
            ("二", "名詞,数,*,*", "ニ", "ニ"),
            ("名", "名詞,接尾,助数詞,*", "メイ", "メイ"),
        ] {
            tokens.push(Token {
                surface: s.into(),
                start: position,
                end: position + s.len(),
                pos: p.into(),
                conj_type: "".into(),
                conj_form: "".into(),
                base_form: s.into(),
                reading: r.into(),
                pronunciation: pn.into(),
                word_cost: 123,
                is_known: true,
            });
            position += s.len();
        }
        ("田中さん外二名".into(), tokens)
    }

    #[test]
    fn only_the_outside_readings_change_at_a_nonzero_origin() {
        let (input, mut tokens) = phrase(17);
        let original = tokens.clone();
        let mut spans = Vec::new();
        let mut context = ContextualReadingContext::new(&input, 17, Some(&mut spans));
        apply(&mut tokens, &[], &mut context);
        assert_eq!(&*tokens[2].pronunciation, "ホカ");
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end), (29, 32));
        tokens[2].reading = original[2].reading.clone();
        tokens[2].pronunciation = original[2].pronunciation.clone();
        assert_eq!(format!("{tokens:?}"), format!("{original:?}"));
    }

    #[test]
    fn unknown_custom_inflected_or_inconsistent_evidence_is_preserved() {
        for i in 0..5 {
            for field in [
                "known",
                "pos",
                "base",
                "reading",
                "pronunciation",
                "conj_type",
                "conj_form",
                "start",
                "end",
            ] {
                let (input, mut tokens) = phrase(0);
                let t = &mut tokens[i];
                match field {
                    "known" => t.is_known = false,
                    "pos" => t.pos = "名詞,一般,*,*".into(),
                    "base" => t.base_form = "別語".into(),
                    "reading" => t.reading = if i == 0 { "" } else { "ベツ" }.into(),
                    "pronunciation" => t.pronunciation = if i == 0 { "" } else { "ベツ" }.into(),
                    "conj_type" => t.conj_type = "特殊".into(),
                    "conj_form" => t.conj_form = "基本形".into(),
                    "start" => t.start += 1,
                    "end" => t.end += 1,
                    _ => unreachable!(),
                }
                let original = format!("{tokens:?}");
                let mut spans = Vec::new();
                apply(
                    &mut tokens,
                    &[],
                    &mut ContextualReadingContext::new(&input, 0, Some(&mut spans)),
                );
                assert_eq!(format!("{tokens:?}"), original, "{i}: {field}");
                assert!(spans.is_empty(), "{i}: {field}");
            }
        }
    }

    #[test]
    fn every_intersecting_protected_byte_preserves_the_phrase() {
        let (input, original) = phrase(7);
        for byte in 7..7 + input.len() {
            let mut tokens = original.clone();
            let range = byte..byte + 1;
            apply(
                &mut tokens,
                std::slice::from_ref(&range),
                &mut ContextualReadingContext::new(&input, 7, None),
            );
            assert_eq!(format!("{tokens:?}"), format!("{original:?}"));
        }
    }

    #[test]
    fn gating_matches_the_ungated_path_for_the_actual_correction() {
        let (input, original) = phrase(0);
        let run = || {
            let mut tokens = original.clone();
            if may_apply(Anchors::scan(&input).gated()) {
                apply(
                    &mut tokens,
                    &[],
                    &mut ContextualReadingContext::new(&input, 0, None),
                );
            }
            tokens
        };
        let gated = run();
        let reference = crate::anchors::ungated(run);
        assert_eq!(&*gated[2].pronunciation, "ホカ");
        assert_eq!(format!("{gated:?}"), format!("{reference:?}"));
        assert!(!may_apply(Anchors::of('内')));
    }
}
