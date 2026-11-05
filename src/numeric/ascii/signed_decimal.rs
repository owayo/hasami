//! 等号で明示された負小数の、標準の既知prefixを含む経路だけを合成する。
//! 裸の既知語・独自素性・分離符号の旧経路は変更しない。

use super::{AsciiPlan, Token, integer_readings, integer_value, parse, readings};
use std::{ops::Range, sync::Arc};

const LABELS: &[&str] = &[
    "名前",
    "名称",
    "作品名",
    "題名",
    "書名",
    "曲名",
    "タイトル",
    "姓",
    "氏名",
    "人名",
    "名字",
    "苗字",
    "識別子",
    "型番",
    "型式",
    "コード",
    "番号",
    "電話番号",
    "郵便番号",
    "バージョン",
    "ラベル",
    "変数名",
    "数列",
    "キー",
    "ID",
    "id",
    "ＩＤ",
    "ｉｄ",
];

fn named(prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches([' ', '\t', '　', '、', ',', ':', '：']);
    ["", "は", "が", "も", "を"].iter().any(|tail| {
        prefix.strip_suffix(tail).is_some_and(|head| {
            let head = head.trim_end_matches([' ', '\t', '　']);
            LABELS.iter().any(|label| head.ends_with(label))
        })
    })
}

fn tail_allowed(tail: &str) -> bool {
    let tail = tail.trim_start_matches([' ', '\t', '　']);
    if tail.chars().next().is_some_and(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９' | '_' | '/' | '／' | ':' | '：' | '+' | '＋' | '-' | '－' | '−' | '=' | '＝')
    }) || ["さん", "様", "君", "氏", "ちゃん"]
        .iter()
        .any(|suffix| tail.starts_with(suffix))
    {
        return false;
    }
    !["", "という", "と呼ぶ", "と呼ばれる", "と名乗る", "は", "が"]
        .iter()
        .any(|frame| {
            tail.strip_prefix(frame)
                .is_some_and(|tail| LABELS.iter().any(|label| tail.starts_with(label)))
        })
}

pub(super) fn ranges(input: &str, offset: usize, negative: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut allowed = Vec::new();
    let mut quotes = None;
    for range in negative {
        let start = range.start - offset;
        let end = range.end - offset;
        let prefix = input[..start].trim_end_matches([' ', '\t', '　']);
        let Some(label) = prefix.strip_suffix('=') else {
            continue;
        };
        let label = label.trim_end_matches([' ', '\t', '　']);
        if named(label)
            || !super::super::month::eleven_prefix_allowed(label)
            || !tail_allowed(&input[end..])
        {
            continue;
        }
        let ranges = quotes.get_or_insert_with(|| crate::lattice::quotation_ranges(input));
        let quote = ranges.partition_point(|r| r.end <= start);
        if ranges.get(quote).is_some_and(|r| r.start < end) {
            continue;
        }
        allowed.push(range.clone());
    }
    allowed
}

fn prefix(token: &Token) -> bool {
    let Some(raw) = token.surface.strip_prefix('-') else {
        return false;
    };
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let Some(value) = integer_value(raw) else {
        return false;
    };
    let (reading, pronunciation) = integer_readings(value);
    token.is_known
        && token.pos.as_ref() == "名詞,固有名詞,一般,*"
        && token.is_uninflected()
        && token.base_form.as_ref() == format!("−{raw}")
        && token.reading.as_ref() == format!("マイナス{reading}")
        && token.pronunciation.as_ref() == format!("マイナス{pronunciation}")
}

fn piece(token: &Token) -> bool {
    if !token.is_uninflected() {
        return false;
    }
    if token.surface.as_ref() == "." {
        return matches!(token.pos.as_ref(), "記号,一般,*,*" | "記号,句点,*,*")
            && matches!(token.base_form.as_ref(), "." | "．")
            && (super::super::has_reading(token, &[".", "．"])
                || super::unspecified_negative_symbol(token, "."));
    }
    token.pos.as_ref() == "名詞,数,*,*"
        && token.base_form == token.surface
        && token.surface.bytes().all(|b| b.is_ascii_digit())
        && super::standard_readings(&token.surface, &token.reading, &token.pronunciation)
}

fn compose(input: &str, offset: usize, tokens: &[Token], range: &Range<usize>) -> Option<Token> {
    let first = tokens.first()?;
    if first.start != range.start
        || first.end >= range.end
        || !prefix(first)
        || tokens.last()?.end != range.end
        || !tokens.windows(2).all(|w| w[0].end == w[1].start)
        || !tokens[1..].iter().all(piece)
        || !tokens.iter().all(|token| {
            token
                .start
                .checked_sub(offset)
                .zip(token.end.checked_sub(offset))
                .and_then(|(a, b)| input.get(a..b))
                == Some(token.surface.as_ref())
        })
    {
        return None;
    }
    let raw = input.get(range.start - offset..range.end - offset)?;
    let magnitude = raw.strip_prefix('-')?;
    let number = parse(magnitude).filter(|n| n.dot.is_some())?;
    let (reading, pronunciation) = readings(magnitude, number);
    let surface: Arc<str> = Arc::from(raw);
    Some(Token {
        surface: Arc::clone(&surface),
        base_form: surface,
        start: range.start,
        end: range.end,
        pos: Arc::from("名詞,数,*,*"),
        conj_type: Arc::from(""),
        conj_form: Arc::from(""),
        reading: Arc::from(format!("マイナス{reading}")),
        pronunciation: Arc::from(format!("マイナス{pronunciation}")),
        word_cost: tokens
            .iter()
            .map(|t| i64::from(t.word_cost))
            .sum::<i64>()
            .clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
        is_known: false,
    })
}

pub(super) fn apply(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first: usize,
    plan: &AsciiPlan,
) {
    let ranges = &plan.known_negative_decimals;
    let first_range = ranges.partition_point(|range| range.end <= offset);
    if ranges
        .get(first_range)
        .is_none_or(|range| range.start >= offset + input.len())
    {
        return;
    }
    let (mut read, mut write) = (first, first);
    for range in &ranges[first_range..] {
        if range.start >= offset + input.len() {
            break;
        }
        if range.start < offset || range.end > offset + input.len() {
            continue;
        }
        while read < out.len() && out[read].end <= range.start {
            if write != read {
                out.swap(write, read);
            }
            read += 1;
            write += 1;
        }
        let mut limit = read;
        while limit < out.len() && out[limit].end <= range.end {
            limit += 1;
        }
        // 助数詞が続く経路は、元の末尾数詞に対する後段の音便をそのまま使う。
        // 半角空白は読み飛ばされ、全角空白は辞書によってトークンとして残る。
        // 選択済みの水平空白だけを飛ばし、助数詞の音便と空白自体を保持する。
        let following = out[limit..].iter().find(|token| {
            !token
                .surface
                .chars()
                .all(|c| matches!(c, ' ' | '\t' | '　'))
        });
        if following.is_some_and(|next| {
            super::super::pos_has(&next.pos, "名詞,接尾,助数詞")
                || super::super::counter(next).is_some()
        }) {
            continue;
        }
        if let Some(number) = compose(input, offset, &out[read..limit], range) {
            out[write] = number;
            write += 1;
            read = limit;
        }
    }
    while read < out.len() {
        if write != read {
            out.swap(write, read);
        }
        read += 1;
        write += 1;
    }
    out.truncate(write);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_tokens(offset: usize) -> Vec<Token> {
        let raw = "-123.045";
        [
            (0, 2, "名詞,固有名詞,一般,*", "−1", "マイナスイチ"),
            (2, 3, "名詞,数,*,*", "2", "ニ"),
            (3, 4, "名詞,数,*,*", "3", "サン"),
            (4, 5, "記号,一般,*,*", ".", "."),
            (5, 6, "名詞,数,*,*", "0", "ゼロ"),
            (6, 7, "名詞,数,*,*", "4", "ヨン"),
            (7, 8, "名詞,数,*,*", "5", "ゴ"),
        ]
        .into_iter()
        .map(|(start, end, pos, base, speech)| Token {
            surface: Arc::from(&raw[start..end]),
            start: start + offset,
            end: end + offset,
            pos: Arc::from(pos),
            conj_type: Arc::from(""),
            conj_form: Arc::from(""),
            base_form: Arc::from(base),
            reading: Arc::from(speech),
            pronunciation: Arc::from(speech),
            word_cost: 0,
            is_known: true,
        })
        .collect()
    }

    #[test]
    fn source_costs_are_added_once_before_saturation_in_either_direction() {
        for (costs, expected) in [
            ([i16::MAX, i16::MAX, i16::MIN, 0, 0, 0, 0], 32766),
            ([i16::MIN, i16::MIN, i16::MAX, 0, 0, 0, 0], -32769),
            ([i16::MAX, i16::MAX, 0, 0, 0, 0, 0], 65534),
        ] {
            let mut tokens = source_tokens(17);
            for (token, cost) in tokens.iter_mut().zip(costs) {
                token.word_cost = cost;
            }
            let number = compose("-123.045", 17, &tokens, &(17..25)).unwrap();
            assert_eq!(
                number.word_cost,
                expected.clamp(i16::MIN.into(), i16::MAX.into()) as i16
            );
            assert_eq!(number.start, 17);
            assert_eq!(number.end, 25);
            assert!(!number.is_known);
        }
    }

    #[test]
    fn full_source_proof_survives_chunk_offsets_without_authorizing_partial_numbers() {
        let raw = "-123.045";
        for (head, tail, accepted) in [
            ("測定値は= \t", "。", true),
            ("名前は=", "。", false),
            ("数列は=", "。", false),
            ("測定値は=", "A", false),
            ("作品名は「前。\n=", "。次」", false),
            ("前。\n=", "。次", true),
        ] {
            let input = format!("{head}{raw}{tail}");
            let offset = 71;
            let position = offset + head.len();
            let plan = AsciiPlan::new(&input, offset);
            let mut out = source_tokens(0);
            let first = out.len();
            let prior = format!("{out:?}");
            out.extend(source_tokens(position));
            let original = format!("{out:?}");
            apply(raw, position, &mut out, first, &plan);
            assert_eq!(format!("{:?}", &out[..first]), prior);
            if accepted {
                assert_eq!(out.len(), first + 1);
                assert_eq!(out[first].surface.as_ref(), raw);
                assert_eq!(
                    (out[first].start, out[first].end),
                    (position, position + raw.len())
                );
                assert!(!out[first].is_known);
                let mut prefix = source_tokens(position);
                prefix.pop();
                let original = format!("{prefix:?}");
                apply(&raw[..raw.len() - 1], position, &mut prefix, 0, &plan);
                assert_eq!(format!("{prefix:?}"), original);
                let mut suffix: Vec<_> = source_tokens(position).into_iter().skip(1).collect();
                let original = format!("{suffix:?}");
                apply(&raw[2..], position + 2, &mut suffix, 0, &plan);
                assert_eq!(format!("{suffix:?}"), original);
            } else {
                assert_eq!(format!("{out:?}"), original, "{input}");
            }
        }
    }
}
