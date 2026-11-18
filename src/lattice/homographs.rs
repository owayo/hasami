//! 近接する既知語の品詞と原形で意味が絞れる構文で、実辞書の候補を選び直す。

use super::{ContextualReadingContext, Token};
use crate::anchors::Anchors;
use crate::{DictError, Dictionary};
use std::{ops::Range, sync::Arc};

pub(super) fn may_apply(anchors: Anchors) -> bool {
    anchors.intersects(Anchors::HOMOGRAPH)
}

const SURNAME: &str = "名詞,固有名詞,人名,姓";
const NUMBER: &str = "名詞,数,*,*";
const COUNTER: &str = "名詞,接尾,助数詞,*";
const NOUN: &str = "名詞,一般,*,*";
const SAHEN: &str = "名詞,サ変接続,*,*";
const VERB: &str = "動詞,自立,*,*";
const CASE_PARTICLE: &str = "助詞,格助詞,一般,*";

type Candidate = (&'static str, &'static str, &'static str, &'static str);

// 表層形・原形・品詞・読み。発音や単語コストは選んだ辞書エントリの値を使う。
const RICE_VOLUME: &[Candidate] = &[("数", "数", NUMBER, "スウ"), ("合", "合", COUNTER, "ゴウ")];
const DOUBLE: &[Candidate] = &[("二", "二", NUMBER, "ニ"), ("重", "重", COUNTER, "ジュウ")];
const STONE_QUESTION: &[Candidate] = &[
    ("石", "石", NOUN, "イシ"),
    ("か", "か", "助詞,副助詞／並立助詞／終助詞,*,*", "カ"),
];
const VENUS: &[Candidate] = &[("金星", "金星", "名詞,固有名詞,一般,*", "キンセイ")];
const YANGTZE: &[Candidate] = &[("長江", "長江", "名詞,固有名詞,地域,一般", "チョウコウ")];
const LONG: &[Candidate] = &[("長けれ", "長い", "形容詞,自立,*,*", "ナガケレ")];

fn fixed(token: &Token, surfaces: &[&str], pos: &str) -> bool {
    token.is_known_base_form()
        && surfaces.contains(&token.surface.as_ref())
        && token.pos.as_ref() == pos
}

fn subject_marker(token: &Token) -> bool {
    fixed(token, &["が"], CASE_PARTICLE) || fixed(token, &["は"], "助詞,係助詞,*,*")
}

fn verb(token: &Token, bases: &[&str]) -> bool {
    token.is_known && bases.contains(&token.base_form.as_ref()) && token.pos.as_ref() == VERB
}

fn correction(tokens: &[Token], i: usize) -> Option<(Range<usize>, &'static [Candidate])> {
    let [token, following @ ..] = &tokens[i..] else {
        return None;
    };
    if !token.is_known
        || !matches!(
            token.surface.as_ref(),
            "数合" | "二重" | "石か" | "金星" | "長江" | "長けれ"
        )
    {
        return None;
    }
    let signature = (
        token.surface.as_ref(),
        token.base_form.as_ref(),
        token.pos.as_ref(),
        token.reading.as_ref(),
        token.pronunciation.as_ref(),
    );
    match (signature, following) {
        (("数合", "数合", SURNAME, "スゴウ", "スゴー"), [no, rice, ..])
            if fixed(no, &["の"], "助詞,連体化,*,*")
                && fixed(rice, &["米", "白米", "玄米"], NOUN) =>
        {
            Some((i..i + 3, RICE_VOLUME))
        }
        (("二重", "二重", SURNAME, "フタエ", "フタエ"), [ni, check, suru, ..])
            if fixed(ni, &["に"], CASE_PARTICLE)
                && fixed(check, &["確認", "検査", "照合"], SAHEN)
                && verb(suru, &["する"]) =>
        {
            Some((i..i + 4, DOUBLE))
        }
        (("石か", "石化", SAHEN, "セキカ", "セキカ"), [to, think, ..])
            if fixed(to, &["と"], "助詞,格助詞,引用,*")
                && verb(think, &["思う", "考える", "尋ねる"]) =>
        {
            Some((i..i + 3, STONE_QUESTION))
        }
        (
            ("金星", "金星", SURNAME, "カナボシ", "カナボシ")
            | ("金星", "金星", NOUN, "キンボシ", "キンボシ"),
            [subject, sky, ni, seen, ..],
        ) if subject_marker(subject)
            && fixed(sky, &["空", "夜空", "夕空"], NOUN)
            && fixed(ni, &["に"], CASE_PARTICLE)
            && verb(seen, &["見える", "輝く", "光る"]) =>
        {
            Some((i..i + 5, VENUS))
        }
        (("長江", "長江", SURNAME, "ナガエ", "ナガエ"), [subject, china, wo, flow, ..])
            if subject_marker(subject)
                && fixed(china, &["中国"], "名詞,固有名詞,地域,国")
                && fixed(wo, &["を"], CASE_PARTICLE)
                && verb(flow, &["流れる"]) =>
        {
            Some((i..i + 5, YANGTZE))
        }
        (("長けれ", "長ける", VERB, "タケレ", "タケレ"), [ba, ..])
            if token.conj_type.as_ref() == "一段"
                && token.conj_form.as_ref() == "仮定形"
                && fixed(ba, &["ば"], "助詞,接続助詞,*,*") =>
        {
            let start = i.checked_sub(2)?;
            let [duration, subject] = &tokens[start..i] else {
                return None;
            };
            (fixed(duration, &["待ち時間", "時間", "期間", "距離"], NOUN)
                && subject_marker(subject))
            .then_some((start..i + 2, LONG))
        }
        _ => None,
    }
}

fn candidate_token(
    dict: &Dictionary,
    (surface, base, pos, reading): Candidate,
    start: usize,
) -> Result<Option<Token>, DictError> {
    let entry = dict
        .lookup(surface)?
        .into_iter()
        .filter(|(end, _)| *end == surface.len())
        .flat_map(|(_, entries)| entries)
        .filter(|entry| {
            entry.base_form.as_ref() == base
                && entry.pos.as_ref() == pos
                && entry.reading.as_ref() == reading
                && !entry.pronunciation.is_empty()
                && if base == surface {
                    matches!(entry.conj_type.as_ref(), "" | "*")
                        && matches!(entry.conj_form.as_ref(), "" | "*")
                } else {
                    entry.conj_type.as_ref() == "形容詞・アウオ段"
                        && entry.conj_form.as_ref() == "仮定形"
                }
        })
        .min_by_key(|entry| entry.cost);
    let empty_marker = |value: Arc<str>| {
        if value.as_ref() == "*" {
            Arc::from("")
        } else {
            value
        }
    };
    Ok(entry.map(|entry| Token {
        start,
        end: start + surface.len(),
        surface: entry.surface,
        pos: entry.pos,
        conj_type: empty_marker(entry.conj_type),
        conj_form: empty_marker(entry.conj_form),
        base_form: entry.base_form,
        reading: entry.reading,
        pronunciation: entry.pronunciation,
        word_cost: entry.cost,
        is_known: true,
    }))
}

fn replacements(
    dict: &Dictionary,
    specs: &[Candidate],
    token: &Token,
) -> Result<Option<Vec<Token>>, DictError> {
    let mut result = Vec::with_capacity(specs.len());
    let mut start = token.start;
    for &spec in specs {
        let Some(candidate) = candidate_token(dict, spec, start)? else {
            return Ok(None);
        };
        start = candidate.end;
        result.push(candidate);
    }
    Ok(Some(result))
}

pub(super) fn apply(
    out: &mut Vec<Token>,
    first: usize,
    dict: &Dictionary,
    protected: &[Range<usize>],
    context: &mut ContextualReadingContext<'_>,
) -> Result<(), DictError> {
    let mut i = first;
    while i < out.len() {
        let index = i;
        i += 1;
        let Some((frame, specs)) = correction(&out[first..], index - first) else {
            continue;
        };
        let words = &out[first + frame.start..first + frame.end];
        let range = words[0].start..words[words.len() - 1].end;
        if !words.windows(2).all(|pair| pair[0].end == pair[1].start)
            || context.identifier_boundary(range.start)
            || protected
                .iter()
                .any(|r| r.start < range.end && range.start < r.end)
            || !context.unquoted(&range)
        {
            continue;
        }
        let Some(replacements) = replacements(dict, specs, &out[index])? else {
            continue;
        };
        for replacement in &replacements {
            context.record(replacement);
        }
        i = index + replacements.len();
        out.splice(index..index + 1, replacements);
    }
    Ok(())
}
