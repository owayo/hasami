//! 近接する既知語の品詞と原形で意味が絞れる構文で、同形語の素性を選び直す。

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
const GENITIVE: &str = "助詞,連体化,*,*";
const COUNTRY: &str = "名詞,固有名詞,地域,国";
const DEPENDENT_NOUN: &str = "名詞,非自立,一般,*";

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
const PERSON: &[Candidate] = &[("方", "方", DEPENDENT_NOUN, "カタ")];
const TEN_MINUTES: &[Candidate] = &[
    ("十", "十", NUMBER, "ジュウ"),
    ("分", "分", COUNTER, "フン"),
];
const RICE: &[Candidate] = &[("米", "米", NOUN, "コメ")];
const LATER: &[Candidate] = &[("後", "後", "名詞,副詞可能,*,*", "アト")];
const RIM: &[Candidate] = &[("縁", "縁", NOUN, "フチ")];
const DEEP_SEA: &[Candidate] = &[("深海", "深海", NOUN, "シンカイ")];
const OEI_ERA: Candidate = ("応永", "応永", "名詞,固有名詞,一般,*", "オウエイ");
const SPECIES: Candidate = ("本種", "本種", NOUN, "ホンシュ");
const STYLE: &[Candidate] = &[("様式", "様式", NOUN, "ヨウシキ")];
const DURATION: &[Candidate] = &[("間", "間", "名詞,副詞可能,*,*", "アイダ")];
const DIRECTIONAL_HE_TO: &[Candidate] = &[
    ("へ", "へ", CASE_PARTICLE, "ヘ"),
    ("と", "と", "助詞,格助詞,引用,*", "ト"),
];

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

fn species_frame(tokens: &[Token], i: usize) -> Option<Range<usize>> {
    let [_, subject, rest @ ..] = &tokens[i..] else {
        return None;
    };
    if !subject_marker(subject) {
        return None;
    }
    // 「冬に」「島で」のように、生態を述べる動作の前に時・場所を置くことがある。
    let predicate = match rest {
        [setting, particle, tail @ ..]
            if setting.is_known_base_form()
                && setting.pos.starts_with("名詞,")
                && fixed(particle, &["に", "で"], CASE_PARTICLE) =>
        {
            tail
        }
        _ => rest,
    };
    let [activity, suru, ..] = predicate else {
        return None;
    };
    (fixed(activity, &["繁殖", "生息", "越冬", "産卵", "分布"], SAHEN) && verb(suru, &["する"]))
        .then_some(i..i + 4 + rest.len() - predicate.len())
}

fn architecture_frame(tokens: &[Token], i: usize) -> Option<Range<usize>> {
    let previous = i.checked_sub(1)?;
    if !fixed(
        &tokens[previous],
        &["パラディオ"],
        "名詞,固有名詞,人名,一般",
    ) {
        return None;
    }
    let [_, shiki, particle, building, ..] = &tokens[i..] else {
        return None;
    };
    if !fixed(shiki, &["式"], "名詞,接尾,一般,*")
        || shiki.reading.as_ref() != "シキ"
        || shiki.pronunciation.as_ref() != "シキ"
    {
        return None;
    }
    let architecture = fixed(particle, &["の"], GENITIVE)
        && fixed(building, &["建物", "建築", "建築物", "邸宅"], NOUN);
    let construction = fixed(particle, &["で"], CASE_PARTICLE) && verb(building, &["建てる"]);
    (architecture || construction).then_some(previous..i + 4)
}

// 引用された地名と「経路を＋移動の述語」があるときだけ、列挙の「へ／と」と区別する。
fn directional_frame(tokens: &[Token], i: usize) -> Option<Range<usize>> {
    let [.., open, destination, close, comma] = &tokens[..i] else {
        return None;
    };
    let [_, to, path, wo, movement, following @ ..] = &tokens[i..] else {
        return None;
    };
    if !matches!(
        (open.surface.as_ref(), close.surface.as_ref()),
        ("「", "」") | ("『", "』")
    ) || !tokens[i - 4..i]
        .windows(2)
        .all(|pair| pair[0].end == pair[1].start)
        || !destination.is_known_base_form()
        || !destination.pos.starts_with("名詞,固有名詞,地域,")
        || !fixed(comma, &[",", "、"], "記号,読点,*,*")
        || !fixed(to, &["と"], "助詞,並立助詞,*,*")
        || to.reading.as_ref() != "ト"
        || to.pronunciation.as_ref() != "ト"
        || !path.is_known_base_form()
        || !path.pos.starts_with("名詞,")
        || !fixed(wo, &["を"], CASE_PARTICLE)
    {
        return None;
    }
    if verb(
        movement,
        &["渡る", "進む", "走る", "通る", "抜ける", "越える"],
    ) {
        return Some(i - 1..i + 5);
    }
    (fixed(movement, &["横断", "縦断", "北上", "南下"], SAHEN)
        && following.first().is_some_and(|aux| verb(aux, &["する"])))
    .then_some(i - 1..i + 6)
}

type Correction = (Range<usize>, &'static [Candidate], usize);

fn correction(tokens: &[Token], i: usize) -> Option<Correction> {
    let [token, following @ ..] = &tokens[i..] else {
        return None;
    };
    if !token.is_known
        || !matches!(
            token.surface.as_ref(),
            "数合"
                | "二重"
                | "石か"
                | "金星"
                | "長江"
                | "長けれ"
                | "方"
                | "十分"
                | "米"
                | "後"
                | "縁"
                | "深海"
                | "へ"
                | "応永"
                | "本種"
                | "様"
                | "間"
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
            if fixed(no, &["の"], GENITIVE) && fixed(rice, &["米", "白米", "玄米"], NOUN) =>
        {
            Some((i..i + 3, RICE_VOLUME, 1))
        }
        (("二重", "二重", SURNAME, "フタエ", "フタエ"), [ni, check, suru, ..])
            if fixed(ni, &["に"], CASE_PARTICLE)
                && fixed(check, &["確認", "検査", "照合"], SAHEN)
                && verb(suru, &["する"]) =>
        {
            Some((i..i + 4, DOUBLE, 1))
        }
        (("石か", "石化", SAHEN, "セキカ", "セキカ"), [to, think, ..])
            if fixed(to, &["と"], "助詞,格助詞,引用,*")
                && verb(think, &["思う", "考える", "尋ねる"]) =>
        {
            Some((i..i + 3, STONE_QUESTION, 1))
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
            Some((i..i + 5, VENUS, 1))
        }
        (("長江", "長江", SURNAME, "ナガエ", "ナガエ"), [subject, china, wo, flow, ..])
            if subject_marker(subject)
                && fixed(china, &["中国"], COUNTRY)
                && fixed(wo, &["を"], CASE_PARTICLE)
                && verb(flow, &["流れる"]) =>
        {
            Some((i..i + 5, YANGTZE, 1))
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
            .then_some((start..i + 2, LONG, 1))
        }
        (("方", "方", DEPENDENT_NOUN, "ホウ", "ホー"), [copula, ..])
            if copula.is_known
                && copula.surface.as_ref() == "です"
                && copula.conj_type.as_ref() == "特殊・デス" =>
        {
            let [.., action, past] = &tokens[..i] else {
                return None;
            };
            (verb(action, &["治める", "率いる"])
                && past.is_known
                && past.surface.as_ref() == "た"
                && past.conj_type.as_ref() == "特殊・タ")
                .then_some((i - 2..i + 2, PERSON, 1))
        }
        (
            ("十分", "十分", "名詞,形容動詞語幹,*,*", "ジュウブン", "ジューブン"),
            [approximation, ..],
        ) if fixed(approximation, &["程度"], "名詞,接尾,一般,*") => {
            let [.., event, subject] = &tokens[..i] else {
                return None;
            };
            ((fixed(event, &["移動", "休憩"], SAHEN)
                || fixed(event, &["所要時間", "待ち時間"], NOUN))
                && subject_marker(subject))
            .then_some((i - 2..i + 2, TEN_MINUTES, 1))
        }
        (("米", "米", COUNTRY, "ベイ", "ベイ"), [no, price, ..])
            if fixed(no, &["の"], GENITIVE) && fixed(price, &["値上がり", "値下がり"], SAHEN) =>
        {
            let adjacent_country = tokens[..i].last().is_some_and(|previous| {
                previous.end == token.start && previous.pos.as_ref() == COUNTRY
            });
            (!adjacent_country).then_some((i..i + 3, RICE, 1))
        }
        (("後", "後", "名詞,接尾,副詞可能,*", "ゴ", "ゴ"), [ni, postpone, te, ..])
            if fixed(ni, &["に"], CASE_PARTICLE)
                && verb(postpone, &["する"])
                && fixed(te, &["て"], "助詞,接続助詞,*,*") =>
        {
            let [.., event, topic] = &tokens[..i] else {
                return None;
            };
            (fixed(event, &["電話", "連絡", "相談"], SAHEN)
                && fixed(topic, &["は"], "助詞,係助詞,*,*"))
            .then_some((i - 2..i + 4, LATER, 1))
        }
        (("縁", "縁", NOUN, "エン", "エン"), [ga, chip, ..])
            if fixed(ga, &["が"], CASE_PARTICLE) && verb(chip, &["欠ける"]) =>
        {
            let [.., vessel, no] = &tokens[..i] else {
                return None;
            };
            (fixed(vessel, &["カップ", "皿", "器", "茶碗"], NOUN) && fixed(no, &["の"], GENITIVE))
                .then_some((i - 2..i + 3, RIM, 1))
        }
        (("深海", "深海", SURNAME, "フカウミ", "フカウミ"), [net, ..])
            if net.is_known
                && net.is_uninflected()
                && matches!(net.surface.as_ref(), "底引き網" | "底びき網")
                && net.base_form.as_ref() == "底引き網"
                && net.pos.as_ref() == NOUN
                && net.reading.as_ref() == "ソコビキアミ" =>
        {
            Some((i..i + 2, DEEP_SEA, 1))
        }
        (("へ", "へ", NOUN, "ヘ", "ヘ" | "エ"), _) if token.is_uninflected() => {
            directional_frame(tokens, i).map(|frame| (frame, DIRECTIONAL_HE_TO, 2))
        }
        (("応永", "応永", SURNAME, "オオエ", "オオエ"), [period, rest @ ..])
            if token.is_uninflected()
                && period.is_known_base_form()
                && matches!(period.pos.as_ref(), NOUN | COUNTER | "名詞,副詞可能,*,*")
                && matches!(
                    (period.surface.as_ref(), period.reading.as_ref()),
                    ("年間", "ネンカン") | ("元年", "ガンネン")
                )
                && !rest.first().is_some_and(|next| {
                    period.end == next.start
                        && (next.pos.starts_with("名詞,") || next.pos.starts_with("接頭詞,"))
                }) =>
        {
            Some((i..i + 2, &[OEI_ERA], 1))
        }
        (("本種", "本種", SURNAME, "モトダネ", "モトダネ"), _) if token.is_uninflected() => {
            species_frame(tokens, i).map(|frame| (frame, &[SPECIES][..], 1))
        }
        (("様", "様", "名詞,接尾,人名,*", "サマ", "サマ"), _) if token.is_uninflected() => {
            architecture_frame(tokens, i).map(|frame| (frame, STYLE, 2))
        }
        (("間", "間", NOUN, "マ", "マ"), [comma, ..])
            if token.is_uninflected() && fixed(comma, &["、", ","], "記号,読点,*,*") =>
        {
            let previous = i.checked_sub(1)?;
            let predicate = &tokens[previous];
            (predicate.conj_form.as_ref() == "基本形"
                && verb(predicate, &["戦う", "話す", "待つ", "眠る", "働く", "休む"]))
            .then_some((previous..i + 2, DURATION, 1))
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
                && (surface != "へ" || entry.pronunciation.as_ref() == "エ")
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
        let candidate = candidate_token(dict, spec, start)?;
        let Some(candidate) = candidate.or_else(|| {
            // 同形の姓だけが登録されていても、文脈で語義が確定する場合は素性を補う。
            // 応永の読みは裾野市史の年号一覧による。
            // https://www.city.susono.shizuoka.jp/static/shishi/shishi0801-owari.pdf
            let pronunciation = match spec {
                OEI_ERA => "オーエイ",
                SPECIES => "ホンシュ",
                _ => return None,
            };
            Some(Token {
                pos: Arc::from(spec.2),
                reading: Arc::from(spec.3),
                pronunciation: Arc::from(pronunciation),
                ..token.clone()
            })
        }) else {
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
        let Some((frame, specs, source_len)) = correction(&out[first..], index - first) else {
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
        let Some(mut replacements) = replacements(dict, specs, &out[index])? else {
            continue;
        };
        if out[index].surface.as_ref() == "十分" {
            crate::numeric::apply_counter_readings(&mut replacements);
        }
        for replacement in &replacements {
            context.record(replacement);
        }
        i = index + replacements.len();
        out.splice(index..index + source_len, replacements);
    }
    Ok(())
}
