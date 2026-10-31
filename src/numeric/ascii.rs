//! ASCII 数量の構文検査と位取り。識別子の候補全体を先に調べ、途中の数字だけを変換しない。

use crate::lattice::Token;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub(super) struct Number {
    integer: u64,
    dot: Option<usize>,
}

/// 整数16桁、小数部16桁。カンマは整数部の正しい3桁区切りだけを許す。
pub(super) fn parse(raw: &str) -> Option<Number> {
    let (mut integer, mut integer_digits, mut fraction_digits) = (0u64, 0usize, 0usize);
    let (mut group, mut comma, mut dot) = (0usize, false, None);
    for (i, b) in raw.bytes().enumerate() {
        match b {
            b'0'..=b'9' if dot.is_none() => {
                integer_digits += 1;
                group += 1;
                if integer_digits > 16 {
                    return None;
                }
                integer = integer * 10 + u64::from(b - b'0');
            }
            b'0'..=b'9' => {
                fraction_digits += 1;
                if fraction_digits > 16 {
                    return None;
                }
            }
            b',' if dot.is_none() => {
                if group == 0 || (!comma && group > 3) || (comma && group != 3) {
                    return None;
                }
                comma = true;
                group = 0;
            }
            b'.' if dot.is_none() && integer_digits > 0 && (!comma || group == 3) => dot = Some(i),
            _ => return None,
        }
    }
    if integer_digits == 0
        || (comma && group != 3)
        || (dot.is_some() && fraction_digits == 0)
        || (raw.starts_with('0') && integer_digits != 1)
    {
        return None;
    }
    Some(Number { integer, dot })
}

pub(super) fn integer_value(raw: &str) -> Option<u64> {
    let parsed = parse(raw)?;
    parsed.dot.is_none().then_some(parsed.integer)
}

fn kanji(integer: u64) -> String {
    if integer == 0 {
        return "零".into();
    }
    const DIGITS: &[char; 10] = &['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
    let mut out = String::new();
    for (divisor, large) in [
        (1_000_000_000_000, "兆"),
        (100_000_000, "億"),
        (10_000, "万"),
        (1, ""),
    ] {
        let mut group = integer / divisor % 10_000;
        if group == 0 {
            continue;
        }
        for (small, unit) in [(1_000, '千'), (100, '百'), (10, '十')] {
            let n = group / small;
            if n != 0 {
                if n != 1 {
                    out.push(DIGITS[n as usize]);
                }
                out.push(unit);
            }
            group %= small;
        }
        if group != 0 {
            out.push(DIGITS[group as usize]);
        }
        out.push_str(large);
    }
    out
}

pub(super) fn standard_readings(raw: &str, reading: &str, pronunciation: &str) -> bool {
    let Some(integer) = integer_value(raw) else {
        return false;
    };
    super::standard_kanji_readings(&kanji(integer), reading, pronunciation)
}

pub(super) fn readings(raw: &str, number: Number) -> (String, String) {
    let spelling = kanji(number.integer);
    let mut chars = spelling.chars().peekable();
    let (mut reading, mut pronunciation) = (String::new(), String::new());
    let mut previous = None;
    while let Some(c) = chars.next() {
        let kana = super::numeral_glyph_readings(previous, c, chars.peek().copied())[0];
        reading.push_str(kana);
        pronunciation.push_str(match kana {
            "ジュウ" => "ジュー",
            "キュウ" => "キュー",
            "チョウ" => "チョー",
            _ => kana,
        });
        previous = Some(c);
    }
    if let Some(dot) = number.dot {
        reading.push_str("テン");
        pronunciation.push_str("テン");
        for b in raw[dot + 1..].bytes() {
            let kana = [
                "ゼロ",
                "イチ",
                "ニ",
                "サン",
                "ヨン",
                "ゴ",
                "ロク",
                "ナナ",
                "ハチ",
                "キュウ",
            ][(b - b'0') as usize];
            reading.push_str(kana);
            pronunciation.push_str(if kana == "キュウ" {
                "キュー"
            } else {
                kana
            });
        }
    }
    (reading, pronunciation)
}

/// 位を省略した漢数字も、同じ位取りと音便で読む。
pub(super) fn integer_readings(integer: u64) -> (String, String) {
    readings("", Number { integer, dot: None })
}

#[derive(Debug)]
struct Candidate {
    number: Range<usize>,
    envelope: Range<usize>,
    parsed: Option<Number>,
}

#[derive(Default, Debug)]
pub(crate) struct AsciiPlan {
    input_end: usize,
    candidates: Vec<Candidate>,
    month_starts: Vec<usize>,
    kanji_starts: Vec<usize>,
    year_starts: Vec<usize>,

    comma_year_starts: Vec<usize>,
    negative_decimals: Vec<Range<usize>>,
    unit_frames: Vec<super::units::Frame>,
    scaled_bad_ends: Vec<usize>,
}

fn envelope_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_.,:+*/=-%^".contains(&b)
}

/// 数量や識別子の認可とは分け、独立した負小数の符号だけを補うための位置を持つ。
fn negative_decimals(input: &str, offset: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for (start, _) in input.match_indices('-') {
        let prefix = input[..start].trim_end_matches(char::is_whitespace);
        let label = prefix.trim_end_matches(|c: char| {
            c.is_whitespace()
                || matches!(c, '(' | '[' | '（' | '［' | '=' | ':' | '：' | ',' | '，')
        });
        let comma = prefix.ends_with([',', '，']);
        if comma
            && prefix[..prefix.len() - prefix.chars().next_back().unwrap().len_utf8()]
                .trim_end_matches(char::is_whitespace)
                .ends_with([',', '，'])
        {
            continue;
        }
        if label.chars().next_back().is_some_and(|c| {
            c.is_ascii_alphabetic()
                || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '_')
                || (!comma && (super::digit(c).is_some() || super::place(c).is_some()))
        }) || [
            "電話",
            "電話番号",
            "番号",
            "型番",
            "郵便番号",
            "識別子",
            "コード",
            "バージョン",
        ]
        .iter()
        .any(|label_name| label.ends_with(label_name))
        {
            continue;
        }
        if prefix.chars().next_back().is_some_and(|c| {
            !matches!(
                c,
                '(' | '[' | '（' | '［' | '=' | ':' | '：' | ';' | '；' | ',' | '，' | '、' | '。'
            )
        }) {
            continue;
        }
        let number = start + 1;
        let end = number
            + input[number..]
                .bytes()
                .take_while(|b| b.is_ascii_digit() || *b == b'.')
                .count();
        if !parse(&input[number..end]).is_some_and(|n| n.dot.is_some()) {
            continue;
        }
        let rest = &input[end..];
        if rest.chars().next().is_some_and(|c| {
            (c.is_ascii() && envelope_byte(c as u8) && c != ',')
                || super::digit(c).is_some()
                || super::place(c).is_some()
                || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '．' | '－' | '−' | '＋' | '／' | '∕' | '÷' | '×' | '²' | '³')
        }) || rest.starts_with(',')
            && rest[1..].chars().next().is_some_and(|c| !c.is_whitespace())
            || rest
                .trim_start_matches([')', ']', '）', '］'])
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９' | '_'))
        {
            continue;
        }
        ranges.push(offset + start..offset + end);
    }
    ranges
}

impl AsciiPlan {
    /// 既存の識別子保護候補が、指定した全体範囲と一致するか。
    pub(crate) fn identifier_envelope_exact(&self, start: usize, end: usize) -> bool {
        self.candidates
            .iter()
            .any(|candidate| candidate.parsed.is_none() && candidate.envelope == (start..end))
    }

    pub(crate) fn input_end(&self) -> usize {
        self.input_end
    }
    pub(super) fn year_number_starts(&self) -> &[usize] {
        &self.year_starts
    }

    pub(super) fn has_kanji_in(&self, start: usize, end: usize) -> bool {
        let i = self.kanji_starts.partition_point(|&s| s < start);
        self.kanji_starts.get(i).is_some_and(|&s| s < end)
    }

    pub(super) fn kanji_number_start(&self, start: usize) -> bool {
        self.kanji_starts.binary_search(&start).is_ok()
    }

    pub(super) fn comma_year_start(&self, start: usize) -> bool {
        self.comma_year_starts.binary_search(&start).is_ok()
    }

    pub(super) fn has_month_in(&self, start: usize, end: usize) -> bool {
        let i = self.month_starts.partition_point(|&s| s < start);
        self.month_starts.get(i).is_some_and(|&s| s < end)
    }

    pub(super) fn month_number_start(&self, start: usize) -> bool {
        self.month_starts.binary_search(&start).is_ok()
    }

    /// 識別子・小数・単位を含まない、独立した整数候補との完全一致。
    pub(super) fn bare_integer(&self, start: usize, end: usize) -> bool {
        let i = self.candidates.partition_point(|c| c.number.start < start);
        self.candidates.get(i).is_some_and(|c| {
            c.number == (start..end)
                && c.envelope == c.number
                && c.parsed.is_some_and(|n| n.dot.is_none())
        })
    }

    /// 位置は offset を加えた入力全体の位置。空白越しの英字・裸の数字も識別子の前置とみなす。
    pub(crate) fn scaled_unit_end_allowed(&self, end: usize) -> bool {
        self.scaled_bad_ends.binary_search(&end).is_err()
    }

    pub(crate) fn unit_frame_at(&self, start: usize) -> Option<&super::units::Frame> {
        let index = self
            .unit_frames
            .partition_point(|f| f.number.start <= start);
        index
            .checked_sub(1)
            .and_then(|i| self.unit_frames.get(i))
            .filter(|f| start < f.end)
    }

    pub(crate) fn unit_frames_in(&self, start: usize, end: usize) -> &[super::units::Frame] {
        let first = self.unit_frames.partition_point(|f| f.number.start < start);
        let last = self.unit_frames.partition_point(|f| f.end <= end);
        &self.unit_frames[first.min(last)..last]
    }

    pub(crate) fn new(input: &str, offset: usize) -> Self {
        let kanji_starts = super::kanji::number_starts(input, offset);

        let comma_year_starts = super::kanji::comma_year_starts(input, offset, &kanji_starts);
        let mut plan = Self {
            input_end: offset + input.len(),
            month_starts: super::month::number_starts(input, offset),
            kanji_starts,
            year_starts: super::year::number_starts(input, offset, |prefix| {
                !super::kanji::identifier_prefix(prefix)
            }),

            comma_year_starts,
            scaled_bad_ends: if input.contains(['万', '億']) && input.contains(['〇', '零']) {
                input.char_indices().filter_map(|(i,c)| {
                    (c.is_ascii_alphanumeric() || matches!(c, '_' | '^' | '/' | '*' | '+' | '-' | '=' | '²' | '³' | 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９'))
                        .then_some(offset+i)
                }).collect()
            } else {
                Vec::new()
            },
            ..Self::default()
        };
        // 全角の英数字・記号は UTF-8 の EF から始まる。漢字だけの入力では走査しない。
        if !input.bytes().any(|b| b.is_ascii_digit() || b == 0xef) {
            return plan;
        }
        plan.negative_decimals = negative_decimals(input, offset);
        let bytes = input.as_bytes();
        let (mut pos, mut prefix) = (0, false);
        while pos < bytes.len() {
            if envelope_byte(bytes[pos]) {
                let start = pos;
                while pos < bytes.len() && envelope_byte(bytes[pos]) {
                    pos += 1;
                }
                let block = &input[start..pos];
                if let Some(first) = block.bytes().position(|b| b.is_ascii_digit()) {
                    // 単独の先行カンマは文の区切り。A,10 や ,,10 は識別子として守る。
                    let comma_delimiter = first == 1
                        && block.starts_with(',')
                        && !input[..start].ends_with(|c| matches!(c, '０'..='９'));
                    if comma_delimiter {
                        prefix = false;
                    }
                    let end = first
                        + block[first..]
                            .bytes()
                            .take_while(|b| b.is_ascii_digit() || matches!(b, b',' | b'.'))
                            .count();
                    let suffix = &block[end..];
                    let allowed_suffix = suffix.is_empty()
                        || suffix == "%"
                        || suffix.bytes().all(|b| b.is_ascii_alphabetic())
                        || super::units::reading(suffix).is_some();
                    let alias_prefix = !super::units::requires_frame(suffix)
                        || super::month::eleven_prefix_allowed(&input[..start + first]);
                    let parsed = if (first == 0 || comma_delimiter)
                        && !prefix
                        && allowed_suffix
                        && alias_prefix
                    {
                        parse(&block[first..end])
                    } else {
                        None
                    };
                    plan.candidates.push(Candidate {
                        number: offset + start + first..offset + start + end,
                        envelope: offset + start + usize::from(comma_delimiter)..offset + pos,
                        parsed,
                    });
                }
                prefix = block.bytes().any(|b| b.is_ascii_alphanumeric());
            } else {
                let c = input[pos..].chars().next().unwrap();
                if matches!(c, '０'..='９') {
                    let start = pos;
                    // 「-」「+」「_」だけの ASCII block は prefix を立てないが、
                    // 全角数字に直結・空白越しで前置されれば同じ識別子として守る。
                    let left = input[..start]
                        .trim_end_matches(char::is_whitespace)
                        .as_bytes();
                    let comma_delimiter = left.last() == Some(&b',')
                        && (left.len() == 1 || !envelope_byte(left[left.len() - 2]))
                        && !input[..start]
                            .trim_end_matches(char::is_whitespace)
                            .strip_suffix(',')
                            .is_some_and(|prefix| prefix.ends_with(|c| matches!(c, '０'..='９')));
                    let left_marker =
                        !comma_delimiter && left.last().is_some_and(|&b| envelope_byte(b));
                    while pos < bytes.len()
                        && input[pos..].starts_with(|c| matches!(c, '０'..='９'))
                    {
                        pos += 3;
                    }
                    let suffix_identifier = input[pos..].chars().next().is_some_and(|c| {
                        (c.is_ascii() && envelope_byte(c as u8))
                            || super::digit(c).is_some()
                            || super::place(c).is_some()
                            || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '－' | '−' | '‐' | '‑' | '‒' | '–' | '—' | '―' | '．' | '，' | '／' | '∕' | '÷' | '×' | '＋')
                    });
                    if prefix || left_marker || suffix_identifier {
                        let range = offset + start..offset + pos;
                        plan.candidates.push(Candidate {
                            number: range.clone(),
                            envelope: range,
                            parsed: None,
                        });
                    }
                    prefix = true;
                    continue;
                }
                if matches!(
                    c,
                    '－' | '−'
                        | '‐'
                        | '‑'
                        | '‒'
                        | '–'
                        | '—'
                        | '―'
                        | '．'
                        | '，'
                        | '／'
                        | '∕'
                        | '÷'
                        | '×'
                        | '＋'
                ) {
                    prefix = true;
                } else if !c.is_whitespace() && !(prefix && matches!(c, '(' | '[' | '（' | '［'))
                {
                    prefix = matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９' | '〇' | '零' | '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九');
                }
                pos += c.len_utf8();
            }
        }
        plan.unit_frames = plan
            .candidates
            .iter()
            .filter_map(|c| {
                if c.parsed.is_none() || c.envelope.end == c.number.end {
                    return None;
                }
                Some(super::units::Frame {
                    number: c.number.clone(),
                    end: c.envelope.end,
                    reading: super::units::reading(
                        &input[c.number.end - offset..c.envelope.end - offset],
                    )?,
                })
            })
            .collect();
        plan
    }
}

fn number_pos(t: &Token) -> bool {
    super::pos_has(&t.pos, "名詞,数")
}

pub(super) fn quantity(t: &Token, unit: impl Fn(&str) -> bool) -> bool {
    super::pos_has(&t.pos, "名詞,接尾,助数詞")
        || super::counter(t).is_some()
        || (t.surface.as_ref() == "年度" && super::pos_has(&t.pos, "名詞"))
        || (number_pos(t) && matches!(t.surface.as_ref(), "百" | "千" | "万" | "億" | "兆"))
        || unit(&t.surface)
}

pub(super) fn unit_boundary(input: &str, offset: usize, t: &Token) -> bool {
    let Some(end) = t.end.checked_sub(offset) else {
        return false;
    };
    input.get(end..).is_some_and(|rest| rest.chars().next().is_none_or(|c| {
        !c.is_ascii_alphanumeric() && !matches!(c, '_' | '^' | '/' | '*' | '+' | '-' | '=' | '²' | '³' | 'Ａ'..='Ｚ' | 'ａ'..='ｚ' | '０'..='９')
    }))
}

/// 範囲 first.. だけを線形に圧縮する。保護した範囲は後段の数詞補正にも渡す。
pub(crate) fn apply(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first: usize,
    plan: &AsciiPlan,
    cursor: &mut usize,
    unit: impl Fn(&str) -> bool,
) -> Vec<Range<usize>> {
    let mut protected = Vec::new();
    let (mut read, mut write) = (first, first);
    let end = offset + input.len();
    while *cursor < plan.candidates.len() && plan.candidates[*cursor].envelope.end <= offset {
        *cursor += 1;
    }
    while *cursor < plan.candidates.len() {
        let candidate = &plan.candidates[*cursor];
        if candidate.envelope.start >= end {
            break;
        }
        // 候補全体を含むチャンクまで進めず、各チャンクの後段にも同じ保護範囲を渡す。
        if candidate.envelope.end > end {
            protected.push(candidate.envelope.clone());
            break;
        }
        *cursor += 1;
        while read < out.len() && out[read].end <= candidate.number.start {
            out.swap(write, read);
            read += 1;
            write += 1;
        }
        let start = read;
        let mut limit = read;
        while limit < out.len() && out[limit].end <= candidate.number.end {
            limit += 1;
        }
        let eligible = candidate.parsed.is_some()
            && start < limit
            && out[start].start == candidate.number.start
            && out[limit - 1].end == candidate.number.end
            && out[start..limit]
                .iter()
                .all(|t| number_pos(t) || matches!(t.surface.as_ref(), "," | "."))
            && out[start..limit].windows(2).all(|p| p[0].end == p[1].start)
            && (!super::units::requires_frame(
                &input[candidate.number.end - offset..candidate.envelope.end - offset],
            ) || out[start..limit].iter().all(super::units::number_piece))
            && ((candidate.envelope == candidate.number
                && candidate.parsed.is_some_and(|number| number.dot.is_some()))
                || out.get(limit).is_some_and(|t| {
                    t.start == candidate.number.end
                        && quantity(t, &unit)
                        && (candidate.envelope.end == candidate.number.end
                            || t.end == candidate.envelope.end)
                        && (!unit(&t.surface) || unit_boundary(input, offset, t))
                }));
        if !eligible {
            protected.push(candidate.envelope.clone());
            continue;
        }
        let raw = &input[candidate.number.start - offset..candidate.number.end - offset];
        if limit - start == 1 {
            let token = &mut out[start];
            // 非空の既知の読みは尊重する。未知語と欠けたフィールドだけ位取りで補う。
            if !token.is_known || token.reading.is_empty() || token.pronunciation.is_empty() {
                let (reading, pronunciation) = readings(raw, candidate.parsed.unwrap());
                if !token.is_known || token.reading.is_empty() {
                    token.reading = Arc::from(reading.as_str());
                }
                if !token.is_known || token.pronunciation.is_empty() {
                    token.pronunciation = Arc::from(pronunciation.as_str());
                }
            }
            out.swap(write, start);
        } else {
            let (reading, pronunciation) = readings(raw, candidate.parsed.unwrap());
            let cost: i64 = out[start..limit]
                .iter()
                .map(|t| i64::from(t.word_cost))
                .sum();
            let surface: Arc<str> = Arc::from(raw);
            out[write] = Token {
                base_form: Arc::clone(&surface),
                surface,
                start: candidate.number.start,
                end: candidate.number.end,
                pos: Arc::from("名詞,数,*,*"),
                conj_type: Arc::from(""),
                conj_form: Arc::from(""),
                reading: Arc::from(reading),
                pronunciation: Arc::from(pronunciation),
                word_cost: cost.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
                is_known: false,
            };
        }
        read = limit;
        write += 1;
    }
    while read < out.len() {
        out.swap(write, read);
        read += 1;
        write += 1;
    }
    out.truncate(write);
    apply_negative_decimals(input, offset, out, first, plan);
    protected
}

fn negative_body_metadata(token: &Token) -> bool {
    if !token.conj_type.is_empty() || !token.conj_form.is_empty() {
        return false;
    }
    if token.surface.as_ref() == "." {
        return matches!(token.pos.as_ref(), "記号,一般,*,*" | "記号,句点,*,*")
            && matches!(token.base_form.as_ref(), "." | "．");
    }
    integer_value(&token.surface).is_some_and(|value| {
        super::numeral_value(std::iter::once(token.base_form.as_ref())) == Some(value)
    }) || standard_readings(&token.base_form, &token.reading, &token.pronunciation)
}

fn unspecified_negative_symbol(token: &Token, literal: &str) -> bool {
    !token.is_known
        && token.surface.as_ref() == literal
        && token.end.checked_sub(token.start) == Some(1)
        && token.pos.as_ref() == "記号,一般,*,*"
        && token.base_form == token.surface
        && token.conj_type.is_empty()
        && token.conj_form.is_empty()
        && token.reading.is_empty()
        && token.pronunciation.is_empty()
}

fn apply_negative_decimals(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first_token: usize,
    plan: &AsciiPlan,
) {
    let end = offset + input.len();
    let first = plan.negative_decimals.partition_point(|r| r.end <= offset);
    if plan
        .negative_decimals
        .get(first)
        .is_none_or(|range| range.start >= end)
    {
        return;
    }
    let (mut read, mut write) = (first_token, first_token);
    for range in &plan.negative_decimals[first..] {
        if range.start >= end {
            break;
        }
        if range.start < offset || range.end > end {
            continue;
        }
        while read < out.len() && out[read].end <= range.start {
            out.swap(write, read);
            read += 1;
            write += 1;
        }
        let sign = read;
        let Some(token) = out.get(sign) else { continue };
        if token.start != range.start
            || token.end != range.start + 1
            || token.surface.as_ref() != "-"
            || !super::pos_has(&token.pos, "記号,一般")
            || !(super::has_reading(token, &["-", "－", "−"])
                || unspecified_negative_symbol(token, "-"))
        {
            continue;
        }
        let mut limit = sign + 1;
        while limit < out.len() && out[limit].end <= range.end {
            limit += 1;
        }
        let body = &out[sign + 1..limit];
        if body.is_empty()
            || body[0].start != token.end
            || body.last().is_none_or(|t| t.end != range.end)
            || !body.windows(2).all(|p| p[0].end == p[1].start)
            || !body.iter().all(|t| {
                t.start
                    .checked_sub(offset)
                    .zip(t.end.checked_sub(offset))
                    .and_then(|(start, end)| input.get(start..end))
                    == Some(t.surface.as_ref())
                    && ((number_pos(t)
                        && standard_readings(&t.surface, &t.reading, &t.pronunciation))
                        || (t.surface.as_ref() == "."
                            && super::pos_has(&t.pos, "記号")
                            && (super::has_reading(t, &[".", "．"])
                                || unspecified_negative_symbol(t, "."))))
            })
        {
            continue;
        }
        let raw = &input[range.start + 1 - offset..range.end - offset];
        let magnitude = if body.len() > 1
            && body.iter().all(negative_body_metadata)
            && let Some(number) = parse(raw).filter(|number| number.dot.is_some())
        {
            let (reading, pronunciation) = readings(raw, number);
            let cost: i64 = body.iter().map(|token| i64::from(token.word_cost)).sum();
            let surface: Arc<str> = Arc::from(raw);
            Some(Token {
                surface: Arc::clone(&surface),
                base_form: surface,
                start: range.start + 1,
                end: range.end,
                pos: Arc::from("名詞,数,*,*"),
                conj_type: Arc::from(""),
                conj_form: Arc::from(""),
                reading: Arc::from(reading),
                pronunciation: Arc::from(pronunciation),
                word_cost: cost.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
                is_known: false,
            })
        } else {
            None
        };
        super::set_readings(&mut out[sign], "マイナス", "マイナス");
        out.swap(write, sign);
        write += 1;
        if let Some(magnitude) = magnitude {
            out[write] = magnitude;
            write += 1;
        } else {
            for i in sign + 1..limit {
                out.swap(write, i);
                write += 1;
            }
        }
        read = limit;
    }
    while read < out.len() {
        out.swap(write, read);
        read += 1;
        write += 1;
    }
    out.truncate(write);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity_grammar_limits_and_number_notation() {
        for raw in [
            "0",
            "10",
            "2031",
            "1,000",
            "2.5",
            "0.5",
            "1234567890123456",
            "1234567890123456.1234567890123456",
        ] {
            assert!(parse(raw).is_some(), "{raw}");
        }
        for raw in [
            "",
            "00",
            "01",
            "00.5",
            ".5",
            "5.",
            "1,23",
            "1,,000",
            "1234,000",
            "1.2.3",
            "2.5,000",
            "12345678901234567",
            "0.12345678901234567",
        ] {
            assert!(parse(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn positional_readings_include_places_internal_zeroes_and_euphony() {
        for (raw, r, p) in [
            ("10", "ジュウ", "ジュー"),
            ("2031", "ニセンサンジュウイチ", "ニセンサンジューイチ"),
            ("1,000", "セン", "セン"),
            ("2.5", "ニテンゴ", "ニテンゴ"),
            ("1010", "センジュウ", "センジュー"),
            ("300", "サンビャク", "サンビャク"),
            ("600", "ロッピャク", "ロッピャク"),
            ("8000", "ハッセン", "ハッセン"),
            ("10000", "イチマン", "イチマン"),
            ("100000000", "イチオク", "イチオク"),
            ("1000000000000", "イッチョウ", "イッチョー"),
            ("8000000000000", "ハッチョウ", "ハッチョー"),
            ("10000000000000", "ジュッチョウ", "ジュッチョー"),
            ("0.09", "ゼロテンゼロキュウ", "ゼロテンゼロキュー"),
        ] {
            assert_eq!(
                readings(raw, parse(raw).unwrap()),
                (r.into(), p.into()),
                "{raw}"
            );
        }
    }

    fn input_tokens(text: &str, offset: usize) -> Vec<Token> {
        text.char_indices()
            .map(|(i, c)| {
                let surface = c.to_string();
                Token {
                    surface: surface.clone().into(),
                    start: offset + i,
                    end: offset + i + c.len_utf8(),
                    pos: if c.is_ascii_digit() {
                        "名詞,数,*,*"
                    } else if matches!(c, '円' | '分' | '倍') {
                        "名詞,接尾,助数詞,*"
                    } else {
                        "記号,一般,*,*"
                    }
                    .into(),
                    base_form: surface.into(),
                    conj_type: "".into(),
                    conj_form: "".into(),
                    reading: "".into(),
                    pronunciation: "".into(),
                    word_cost: 100,
                    is_known: true,
                }
            })
            .collect()
    }

    fn signed_input_tokens(text: &str, offset: usize) -> Vec<Token> {
        let mut out = input_tokens(text, offset);
        for t in &mut out {
            let (r, p) = if t.surface.bytes().all(|b| b.is_ascii_digit()) {
                readings(&t.surface, parse(&t.surface).unwrap())
            } else {
                (t.surface.to_string(), t.surface.to_string())
            };
            super::super::set_readings(t, &r, &p);
        }
        out
    }

    #[test]
    fn negative_decimal_signs_and_magnitudes_keep_surrounding_fields_and_protection() {
        for text in [
            "-47.25",
            "=-47.25",
            "値：-47.25",
            "（-47.25）",
            "[ -47.25]",
            "12.5; -47.25",
            "12.5, -47.25",
            "=-0.25",
            "値：-47.25度",
        ] {
            let offset = 91;
            let mut out = signed_input_tokens("前", 0);
            let first = out.len();
            out.extend(signed_input_tokens(text, offset));
            let mut prior = out.clone();
            let mut baseline = AsciiPlan::new(text, offset);
            baseline.negative_decimals.clear();
            let expected_protection =
                apply(text, offset, &mut prior, first, &baseline, &mut 0, |_| {
                    false
                });
            let mut expected = prior.clone();
            let sign = expected
                .iter()
                .position(|t| t.surface.as_ref() == "-")
                .unwrap();
            super::super::set_readings(&mut expected[sign], "マイナス", "マイナス");
            let raw = if text.contains("-0.25") {
                "0.25"
            } else {
                "47.25"
            };
            let start = expected[sign].end;
            let end = start + raw.len();
            let limit = expected.partition_point(|t| t.end <= end);
            let (reading, pronunciation) = if raw == "0.25" {
                ("ゼロテンニゴ", "ゼロテンニゴ")
            } else {
                ("ヨンジュウナナテンニゴ", "ヨンジューナナテンニゴ")
            };
            expected.splice(
                sign + 1..limit,
                [Token {
                    surface: raw.into(),
                    base_form: raw.into(),
                    start,
                    end,
                    pos: "名詞,数,*,*".into(),
                    conj_type: "".into(),
                    conj_form: "".into(),
                    reading: reading.into(),
                    pronunciation: pronunciation.into(),
                    word_cost: (100 * raw.len()) as i16,
                    is_known: false,
                }],
            );
            let plan = AsciiPlan::new(text, offset);
            let protected = apply(text, offset, &mut out, first, &plan, &mut 0, |_| false);
            assert_eq!(format!("{out:?}"), format!("{expected:?}"), "{text}");
            assert_eq!(protected, expected_protection, "{text}");
            for t in &out[first..] {
                assert_eq!(&text[t.start - offset..t.end - offset], t.surface.as_ref());
            }
            let once = format!("{out:?}");
            apply(text, offset, &mut out, first, &plan, &mut 0, |_| false);
            assert_eq!(format!("{out:?}"), once, "{text}");
        }
    }

    #[test]
    fn empty_unknown_negative_symbols_complete_only_admitted_decimal_values() {
        for unknown_sign in [false, true] {
            for unknown_dot in [false, true] {
                for (text, reading, pronunciation) in [
                    ("-0.5", "ゼロテンゴ", "ゼロテンゴ"),
                    ("値：-12.25", "ジュウニテンニゴ", "ジューニテンニゴ"),
                    ("-0.1本", "ゼロテンイチ", "ゼロテンイチ"),
                ] {
                    let offset = 91;
                    let mut out = signed_input_tokens(text, offset);
                    for t in &mut out {
                        if (unknown_sign && t.surface.as_ref() == "-")
                            || (unknown_dot && t.surface.as_ref() == ".")
                        {
                            t.is_known = false;
                            t.reading = "".into();
                            t.pronunciation = "".into();
                        }
                    }
                    let sign = out.iter().position(|t| t.surface.as_ref() == "-").unwrap();
                    let original_sign = out[sign].clone();
                    apply(
                        text,
                        offset,
                        &mut out,
                        0,
                        &AsciiPlan::new(text, offset),
                        &mut 0,
                        |_| false,
                    );
                    let mut expected_sign = original_sign;
                    super::super::set_readings(&mut expected_sign, "マイナス", "マイナス");
                    assert_eq!(format!("{:?}", out[sign]), format!("{expected_sign:?}"));
                    assert_eq!(out[sign + 1].reading.as_ref(), reading, "{text}");
                    assert_eq!(
                        out[sign + 1].pronunciation.as_ref(),
                        pronunciation,
                        "{text}"
                    );
                    let once = format!("{out:?}");
                    apply(
                        text,
                        offset,
                        &mut out,
                        0,
                        &AsciiPlan::new(text, offset),
                        &mut 0,
                        |_| false,
                    );
                    assert_eq!(format!("{out:?}"), once, "{text}");
                }
            }
        }
    }

    #[test]
    fn unknown_negative_symbol_fallback_keeps_explicit_or_nonstandard_metadata() {
        for symbol in [0, 2] {
            for mutation in 0..10 {
                let text = "-0.5";
                let mut out = signed_input_tokens(text, 0);
                out[symbol].is_known = false;
                out[symbol].reading = "".into();
                out[symbol].pronunciation = "".into();
                match mutation {
                    0 => out[symbol].is_known = true,
                    1 => out[symbol].reading = "custom".into(),
                    2 => out[symbol].pronunciation = "custom".into(),
                    3 => out[symbol].base_form = "custom".into(),
                    4 => out[symbol].conj_type = "custom".into(),
                    5 => out[symbol].conj_form = "custom".into(),
                    6 => out[symbol].pos = "記号,句点,*,*".into(),
                    7 => out[symbol].pos = "名詞,一般,*,*".into(),
                    8 => out[symbol].pos = "記号,一般,custom,*".into(),
                    9 => out[symbol].end += 1,
                    _ => unreachable!(),
                }
                let expected = format!("{out:?}");
                apply(
                    text,
                    0,
                    &mut out,
                    0,
                    &AsciiPlan::new(text, 0),
                    &mut 0,
                    |_| false,
                );
                assert_eq!(
                    format!("{out:?}"),
                    expected,
                    "symbol={symbol} mutation={mutation}"
                );
            }
        }
        for text in ["ID:-0.5", "4 - 0.5", "4-0.5", "=-0.5A", "--0.5"] {
            let mut out = signed_input_tokens(text, 0);
            for t in &mut out {
                if matches!(t.surface.as_ref(), "-" | ".") {
                    t.is_known = false;
                    t.reading = "".into();
                    t.pronunciation = "".into();
                }
            }
            let mut expected = out.clone();
            let mut baseline = AsciiPlan::new(text, 0);
            baseline.negative_decimals.clear();
            apply(text, 0, &mut expected, 0, &baseline, &mut 0, |_| false);
            apply(
                text,
                0,
                &mut out,
                0,
                &AsciiPlan::new(text, 0),
                &mut 0,
                |_| false,
            );
            assert_eq!(format!("{out:?}"), format!("{expected:?}"), "{text}");
        }
    }

    #[test]
    fn negative_magnitude_metadata_keeps_legacy_sign_only_and_single_correct_body() {
        for mutation in 0..8 {
            let text = "-0.5";
            let mut out = signed_input_tokens(text, 0);
            match mutation {
                0 => out[1].base_form = "ordinary".into(),
                1 => out[1].base_form = "一".into(),
                2 => out[1].conj_type = "fixture".into(),
                3 => out[3].conj_form = "fixture".into(),
                4 => out[2].base_form = "ordinary".into(),
                5 => out[2].pos = "記号,アルファベット,*,*".into(),
                6 => out[2].conj_type = "fixture".into(),
                7 => out[2].conj_form = "fixture".into(),
                _ => unreachable!(),
            }
            let mut expected = out.clone();
            super::super::set_readings(&mut expected[0], "マイナス", "マイナス");
            apply(
                text,
                0,
                &mut out,
                0,
                &AsciiPlan::new(text, 0),
                &mut 0,
                |_| false,
            );
            assert_eq!(
                format!("{out:?}"),
                format!("{expected:?}"),
                "mutation {mutation}"
            );
        }
        for known in [true, false] {
            let text = "-0.5";
            let mut out = signed_input_tokens(text, 0);
            let mut body = out[1].clone();
            body.surface = "0.5".into();
            body.base_form = "custom metadata".into();
            body.end = 4;
            body.reading = "ゼロテンゴ".into();
            body.pronunciation = "ゼロテンゴ".into();
            body.word_cost = 1234;
            body.is_known = known;
            body.conj_type = "custom".into();
            out.splice(1..4, [body.clone()]);
            apply(
                text,
                0,
                &mut out,
                0,
                &AsciiPlan::new(text, 0),
                &mut 0,
                |_| false,
            );
            assert_eq!(out.len(), 2);
            assert_eq!(format!("{:?}", out[1]), format!("{body:?}"));
            // 単一decimalは旧integer-only判定の対象外。新しい符号認可を足さず保持する。
            assert_eq!(out[0].reading.as_ref(), "-");
        }
    }

    #[test]
    fn negative_magnitude_cost_excludes_sign_and_saturates_once() {
        for bound in [i16::MIN, i16::MAX] {
            let text = "-12.25";
            let mut out = signed_input_tokens(text, 31);
            out[0].word_cost = 137;
            for token in &mut out[1..] {
                token.word_cost = bound;
            }
            let sign = out[0].clone();
            apply(
                text,
                31,
                &mut out,
                0,
                &AsciiPlan::new(text, 31),
                &mut 0,
                |_| false,
            );
            assert_eq!(out.len(), 2);
            assert_eq!(out[1].word_cost, bound);
            assert_eq!(out[0].word_cost, 137);
            assert_eq!((out[1].start, out[1].end), (32, 37));
            let mut restored = out[0].clone();
            restored.reading = sign.reading.clone();
            restored.pronunciation = sign.pronunciation.clone();
            assert_eq!(format!("{restored:?}"), format!("{sign:?}"));
        }
        let text = "-0.5";
        let mut out = signed_input_tokens(text, 0);
        out[1].base_form = "零".into();
        out[2].base_form = "．".into();
        apply(
            text,
            0,
            &mut out,
            0,
            &AsciiPlan::new(text, 0),
            &mut 0,
            |_| false,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].reading.as_ref(), "ゼロテンゴ");
        assert_eq!(out[1].word_cost, 300);
    }

    #[test]
    fn repeated_negative_magnitudes_compact_once_and_keep_known_minus_words() {
        let text = "-0.5; -3.5; -0.5";
        let mut out = signed_input_tokens(text, 0);
        let sign = out.iter().position(|t| t.start == 6).unwrap();
        let mut known = out[sign].clone();
        known.surface = "-3".into();
        known.base_form = "-3".into();
        known.end = 8;
        known.pos = "名詞,数,*,*".into();
        known.reading = "マイナスサン".into();
        known.pronunciation = "マイナスサン".into();
        out.splice(sign..sign + 2, [known.clone()]);
        apply(
            text,
            0,
            &mut out,
            0,
            &AsciiPlan::new(text, 0),
            &mut 0,
            |_| false,
        );
        assert_eq!(
            out.iter().filter(|t| t.surface.as_ref() == "0.5").count(),
            2
        );
        assert_eq!(
            format!(
                "{:?}",
                out.iter().find(|t| t.surface.as_ref() == "-3").unwrap()
            ),
            format!("{known:?}")
        );
        let text = std::iter::repeat_n("-0.5", 2000)
            .collect::<Vec<_>>()
            .join("; ");
        let mut out = signed_input_tokens(&text, 0);
        let plan = AsciiPlan::new(&text, 0);
        apply(&text, 0, &mut out, 0, &plan, &mut 0, |_| false);
        assert_eq!(
            out.iter().filter(|t| t.surface.as_ref() == "0.5").count(),
            2000
        );
        assert_eq!(
            out.iter()
                .filter(|t| t.reading.as_ref() == "マイナス")
                .count(),
            2000
        );
        assert!(
            out.iter()
                .filter(|t| t.surface.as_ref() == "0.5")
                .all(|t| t.word_cost == 300 && t.reading.as_ref() == "ゼロテンゴ")
        );
        let before = format!("{out:?}");
        apply(&text, 0, &mut out, 0, &plan, &mut 0, |_| false);
        assert_eq!(format!("{out:?}"), before);
    }

    #[test]
    fn negative_decimal_signs_do_not_reinterpret_ids_operators_or_dictionary_readings() {
        for text in [
            "A-47.25",
            "ID -47.25",
            "ID:-47.25",
            "ID, -47.25",
            "コード=-47.25",
            "4=-47.25",
            ",,-47.25",
            "4-47.25",
            "4 -47.25",
            "4 - 47.25",
            "--47.25",
            "=-47.25B",
            "=-47.25_1",
            "=-47.25/2",
            "=-47.25e3",
            "=-47.25.1",
            "=-047.25",
            "=-47.",
            "=-.25",
            "=-47.25,3",
            "=-47.25%",
            "ID（-47.25）",
            "ID ( (-47.25)",
            "=-47.25)B",
            "=−47.25",
            "=-47",
            "=-10本",
            "+47.25",
            "-47.25-48",
            "=-47.25²",
            "=-47.25Ａ",
            "=-47.25一",
        ] {
            assert!(
                AsciiPlan::new(text, 0).negative_decimals.is_empty(),
                "{text}"
            );
        }
        for mutation in 0..5 {
            let text = "=-47.25";
            let mut out = signed_input_tokens(text, 0);
            match mutation {
                0 => super::super::set_readings(&mut out[1], "ハイフン", "ハイフン"),
                1 => out[1].pos = "名詞,一般,*,*".into(),
                2 => out[2].reading = "ヨシ".into(),
                3 => out[3].pronunciation = "ナナコ".into(),
                _ => out[4].pos = "名詞,固有名詞,一般,*".into(),
            }
            let before = format!("{out:?}");
            apply(
                text,
                0,
                &mut out,
                0,
                &AsciiPlan::new(text, 0),
                &mut 0,
                |_| false,
            );
            assert_eq!(format!("{out:?}"), before, "mutation={mutation}");
        }
        // 既知の-4が既にマイナスを含むときは、同じ符号を二度読まない。
        let text = "=-47.25";
        let mut out = signed_input_tokens(text, 0);
        let mut known = out[1].clone();
        known.surface = "-4".into();
        known.end += 1;
        super::super::set_readings(&mut known, "マイナスヨン", "マイナスヨン");
        out.splice(1..3, [known]);
        let before = format!("{out:?}");
        apply(
            text,
            0,
            &mut out,
            0,
            &AsciiPlan::new(text, 0),
            &mut 0,
            |_| false,
        );
        assert_eq!(format!("{out:?}"), before);
    }

    #[test]
    fn negative_decimal_signs_require_the_complete_global_span_in_one_chunk() {
        let full = "ID -47.25";
        let plan = AsciiPlan::new(full, 37);
        let text = "-47.25";
        let mut out = signed_input_tokens(text, 40);
        let before = format!("{out:?}");
        apply(text, 40, &mut out, 0, &plan, &mut 0, |_| false);
        assert_eq!(format!("{out:?}"), before);
        let full = "値：-47.25";
        let plan = AsciiPlan::new(full, 37);
        let mut out = signed_input_tokens("前", 0);
        let first = out.len();
        let mut cursor = 0;
        for (offset, part) in [(37, "値：-47."), (47, "25")] {
            let start = out.len();
            out.extend(signed_input_tokens(part, offset));
            apply(part, offset, &mut out, start, &plan, &mut cursor, |_| false);
        }
        assert!(
            out[first..]
                .iter()
                .filter(|t| t.surface.as_ref() == "-")
                .all(|t| t.reading.as_ref() == "-")
        );
    }

    #[test]
    fn compression_covers_the_original_number_and_preserves_existing_output() {
        let text = "1,000円2.5倍";
        let mut out = input_tokens("先", 0);
        let previous = format!("{:?}", out[0]);
        out.extend(input_tokens(text, 91));
        apply(
            text,
            91,
            &mut out,
            1,
            &AsciiPlan::new(text, 91),
            &mut 0,
            |_| false,
        );
        assert_eq!(format!("{:?}", out[0]), previous);
        assert_eq!(out[1].surface.as_ref(), "1,000");
        assert_eq!(out[1].pronunciation.as_ref(), "セン");
        assert_eq!(out[1].word_cost, 500);
        assert!(!out[1].is_known);
        assert_eq!(out[3].surface.as_ref(), "2.5");
        assert_eq!(out[3].pronunciation.as_ref(), "ニテンゴ");
        for t in &out[1..] {
            assert_eq!(&text[t.start - 91..t.end - 91], t.surface.as_ref());
        }
    }

    #[test]
    fn bare_decimals_keep_the_point_but_identifiers_and_integers_stay_unchanged() {
        for (text, expected) in [("9.2", "キューテンニ"), ("2.50", "ニテンゴゼロ")] {
            let mut out = input_tokens(text, 91);
            let protected = apply(
                text,
                91,
                &mut out,
                0,
                &AsciiPlan::new(text, 91),
                &mut 0,
                |_| false,
            );
            assert!(protected.is_empty());
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].surface.as_ref(), text);
            assert_eq!(out[0].pronunciation.as_ref(), expected);
            assert_eq!(out[0].start, 91);
            assert_eq!(out[0].end, 91 + text.len());
            assert_eq!(out[0].word_cost, 100 * text.len() as i16);
        }
        for text in ["2031", "v9.2", "ISO 9.2", "9.2.1", "09.2", "9.", "-9.2"] {
            let mut out = input_tokens(text, 91);
            let before = format!("{out:?}");
            apply(
                text,
                91,
                &mut out,
                0,
                &AsciiPlan::new(text, 91),
                &mut 0,
                |_| false,
            );
            assert_eq!(format!("{out:?}"), before, "{text}");
        }
    }

    #[test]
    fn crossing_envelopes_remain_protected_in_every_intersecting_chunk() {
        for parts in [&["ISO.", "8兆"][..], &["ISO.", ".", "8兆"][..]] {
            let input = parts.concat();
            let offset = 91;
            let plan = AsciiPlan::new(&input, offset);
            let envelope = offset..offset + input.len() - "兆".len();
            let mut cursor = 0;
            let mut position = offset;
            let mut out = input_tokens("先", 0);
            for (i, part) in parts.iter().enumerate() {
                let first = out.len();
                out.extend(input_tokens(part, position));
                let before = format!("{out:?}");
                let protected = apply(part, position, &mut out, first, &plan, &mut cursor, |_| {
                    false
                });
                assert_eq!(protected.as_slice(), std::slice::from_ref(&envelope));
                assert_eq!(format!("{out:?}"), before);
                assert_eq!(cursor, usize::from(i + 1 == parts.len()));
                position += part.len();
            }
        }
    }

    #[test]
    fn identifiers_and_malformed_runs_are_preserved_as_a_whole() {
        for text in [
            "03-1234-5678円",
            "123-4567円",
            "123－4567円",
            "123−4567円",
            "ISO9001円",
            "ISO 9001円",
            "ISO\n9001円",
            "v1.2.3円",
            "192.168.0.1円",
            "1,23円",
            "1,,000円",
            ",,10円",
            ",1,23円",
            ",1,,000円",
            "A,10円",
            "ISO,10円",
            "１,10円",
            "-,10円",
            "+,10円",
            "1.2.3円",
            "1．23円",
            "1，234円",
            "0010分",
            "1e3円",
            "10m2",
            "10ms",
            "123４円",
            "１２34円",
            "−10円",
        ] {
            let mut out = input_tokens(text, 0);
            let before = format!("{out:?}");
            apply(
                text,
                0,
                &mut out,
                0,
                &AsciiPlan::new(text, 0),
                &mut 0,
                |s| s == "m",
            );
            assert_eq!(format!("{out:?}"), before, "{text}");
        }
        let text = format!("ISO{}9001円", ".1".repeat(100_000));
        assert_eq!(AsciiPlan::new(&text, 0).candidates.len(), 1);
    }

    #[test]
    fn single_comma_prefix_delimits_quantities_and_bare_decimals() {
        for text in [
            "前に,10円",
            "一,10円",
            "10円,20円",
            "ISO ,10円",
            "ISO\n,10円",
            "１ ,10円",
            "前に,2.50円",
            "一,9.2",
            "10円,1,000円",
        ] {
            let offset = 91;
            let plan = AsciiPlan::new(text, offset);
            assert!(plan.candidates.iter().all(|c| c.parsed.is_some()), "{text}");
            for c in &plan.candidates {
                assert_eq!(c.number, c.envelope, "{text}");
                assert!(text.as_bytes()[c.number.start - offset].is_ascii_digit());
            }
            let mut out = input_tokens(text, offset);
            let commas: Vec<_> = out
                .iter()
                .filter(|t| t.surface.as_ref() == ",")
                .map(|t| (t.start, format!("{t:?}")))
                .collect();
            let mut cursor = 0;
            let protected = apply(text, offset, &mut out, 0, &plan, &mut cursor, |_| false);
            assert!(protected.is_empty(), "{text}: {protected:?}");
            assert_eq!(cursor, plan.candidates.len());
            for c in &plan.candidates {
                let token = out.iter().find(|t| t.start == c.number.start).unwrap();
                assert_eq!(token.end, c.number.end);
                assert_eq!(token.word_cost, 100 * token.surface.len() as i16);
                assert_eq!(token.pos.as_ref(), "名詞,数,*,*");
                assert!(!token.is_known);
                assert!(token.mora_count() > 0);
                assert_eq!(
                    plan.bare_integer(c.number.start, c.number.end),
                    c.parsed.unwrap().dot.is_none()
                );
            }
            // 数の桁区切りに属するカンマ以外は、その全フィールドを保つ。
            for (start, before) in commas {
                if !plan.candidates.iter().any(|c| c.number.contains(&start)) {
                    let token = out.iter().find(|t| t.start == start).unwrap();
                    assert_eq!(format!("{token:?}"), before);
                }
            }
            for token in &out {
                assert_eq!(
                    &text[token.start - offset..token.end - offset],
                    token.surface.as_ref()
                );
            }
        }
    }

    #[test]
    fn synthetic_cost_is_summed_widely_and_saturated_only_once() {
        for (costs, expected) in [
            ([30000, 30000, -30000], 30000),
            ([32000, 32000, 32000], i16::MAX),
            ([-32000, -32000, -32000], i16::MIN),
        ] {
            let text = "123円";
            let mut out = input_tokens(text, 0);
            for (t, cost) in out.iter_mut().zip(costs) {
                t.word_cost = cost;
            }
            apply(
                text,
                0,
                &mut out,
                0,
                &AsciiPlan::new(text, 0),
                &mut 0,
                |_| false,
            );
            assert_eq!(out[0].word_cost, expected);
        }
    }

    #[test]
    fn fullwidth_identifier_digits_are_protected_but_plain_quantities_are_not() {
        for (text, protected_number) in [
            ("Ａ１つ", true),
            ("A１点", true),
            ("ID１章", true),
            ("ID １つ", true),
            ("ID\n１つ", true),
            ("ID（１つ", true),
            ("-１つ", true),
            ("+１章", true),
            ("_１点", true),
            (".\n１つ", true),
            ("A,１つ", true),
            ("ISO,１つ", true),
            ("１,１つ", true),
            (",,１つ", true),
            ("-,１つ", true),
            ("１Aつ", true),
            ("１_つ", true),
            ("１2つ", true),
            ("1１つ", true),
            ("１．１つ", true),
            ("１つ", false),
            ("価格１つ", false),
            ("(１つ", false),
            ("（１つ", false),
            ("前に,１つ", false),
            ("一,１つ", false),
            ("ISO ,１つ", false),
            ("１ ,１つ", false),
        ] {
            let mut out = input_tokens(text, 0);
            for t in &mut out {
                if matches!(t.surface.as_ref(), "1" | "１" | "2") {
                    t.pos = "名詞,数,*,*".into();
                    let reading = if t.surface.as_ref() == "2" {
                        "ニ"
                    } else {
                        "イチ"
                    };
                    t.reading = reading.into();
                    t.pronunciation = reading.into();
                } else if let Some((reading, pronunciation)) = match t.surface.as_ref() {
                    "つ" => Some(("ツ", "ツ")),
                    "点" => Some(("テン", "テン")),
                    "章" => Some(("ショウ", "ショー")),
                    _ => None,
                } {
                    t.pos = "名詞,接尾,助数詞,*".into();
                    t.reading = reading.into();
                    t.pronunciation = pronunciation.into();
                }
            }
            let before = format!("{out:?}");
            let ranges = apply(
                text,
                0,
                &mut out,
                0,
                &AsciiPlan::new(text, 0),
                &mut 0,
                |_| false,
            );
            super::super::apply_counter_readings_with_protection(&mut out, &ranges);
            if protected_number {
                assert_eq!(format!("{out:?}"), before, "{text}");
            } else {
                assert!(ranges.is_empty(), "{text}: {ranges:?}");
                assert!(
                    out.iter()
                        .any(|t| t.surface.as_ref() == "１" && t.reading.as_ref() == "ヒト"),
                    "{text}: {out:?}"
                );
            }
        }
        let text = "12円、Ａ１つ、34円、１ID";
        let plan = AsciiPlan::new(text, 37);
        assert!(
            plan.candidates
                .windows(2)
                .all(|p| p[0].number.start < p[1].number.start)
        );
        assert!(plan.bare_integer(37, 39));
        let ascii_second = 37 + text.find("34").unwrap();
        assert!(plan.bare_integer(ascii_second, ascii_second + 2));
        let wide = 37 + text.find('１').unwrap();
        assert!(!plan.bare_integer(wide, wide + 3));
    }

    #[test]
    fn fullwidth_protection_and_cursor_survive_a_run_split_across_chunks() {
        let parts = ["ID.", "\n", "１", "２つ"];
        let full = parts.concat();
        let offset = 91;
        let plan = AsciiPlan::new(&full, offset);
        assert_eq!(plan.candidates.len(), 1);
        let envelope = plan.candidates[0].envelope.clone();
        let (mut position, mut cursor) = (offset, 0);
        let mut out = input_tokens("前", 0);
        for (i, part) in parts.iter().enumerate() {
            let first = out.len();
            out.extend(input_tokens(part, position));
            let before = format!("{out:?}");
            let ranges = apply(part, position, &mut out, first, &plan, &mut cursor, |_| {
                false
            });
            assert_eq!(format!("{out:?}"), before);
            if i >= 2 {
                assert_eq!(ranges, vec![envelope.clone()]);
            } else {
                assert!(ranges.is_empty());
            }
            assert_eq!(cursor, usize::from(i == parts.len() - 1));
            position += part.len();
        }
    }
}

#[cfg(all(test, feature = "build"))]
mod analyzer_tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};
    use crate::{Analyzer, Dictionary};

    fn dictionary() -> Dictionary {
        let mut builder = DictBuilder::new();
        for (n, r, p) in [
            ("0", "ゼロ", "ゼロ"),
            ("1", "イチ", "イチ"),
            ("2", "ニ", "ニ"),
            ("3", "サン", "サン"),
            ("4", "ヨン", "ヨン"),
            ("5", "ゴ", "ゴ"),
            ("6", "ロク", "ロク"),
            ("7", "ナナ", "ナナ"),
            ("8", "ハチ", "ハチ"),
            ("9", "キュウ", "キュー"),
            ("一", "イチ", "イチ"),
            ("八", "ハチ", "ハチ"),
            ("十", "ジュウ", "ジュー"),
            ("１", "イチ", "イチ"),
            ("８", "ハチ", "ハチ"),
        ] {
            builder.add_entry(DictEntry {
                surface: n.into(),
                pos: "名詞,数,*,*".into(),
                reading: r.into(),
                pronunciation: p.into(),
                ..Default::default()
            });
        }
        for (surface, pos, r, p, cost) in [
            ("分", "名詞,接尾,助数詞,*", "フン", "フン", 0),
            ("本", "名詞,接尾,助数詞,*", "ホン", "ホン", 0),
            ("円", "名詞,接尾,助数詞,*", "エン", "エン", 0),
            ("倍", "名詞,接尾,助数詞,*", "バイ", "バイ", 0),
            ("人", "名詞,接尾,助数詞,*", "ニン", "ニン", 0),
            ("日", "名詞,接尾,助数詞,*", "ニチ", "ニチ", 0),
            ("つ", "名詞,接尾,助数詞,*", "ツ", "ツ", 0),
            ("点", "名詞,接尾,助数詞,*", "テン", "テン", 0),
            ("章", "名詞,接尾,助数詞,*", "ショウ", "ショー", 0),
            ("年度", "名詞,一般,*,*", "ネンド", "ネンド", 0),
            ("万", "名詞,数,*,*", "マン", "マン", 0),
            ("兆", "名詞,数,*,*", "チョウ", "チョー", 0),
            ("kg", "名詞,一般,*,*", "キログラム", "キログラム", 0),
            ("12", "名詞,数,*,*", "ジュウニ", "ジューニ", -100),
            ("11", "名詞,数,*,*", "テンイチ", "テンイチ", -100),
            (
                "2026年度",
                "名詞,固有名詞,一般,*",
                "ニセンニジュウロクネンド",
                "ニセンニジューロクネンド",
                -100,
            ),
            (
                "2.5倍",
                "名詞,固有名詞,一般,*",
                "ニテンゴバイ",
                "ニテンゴバイ",
                -100,
            ),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                pos: pos.into(),
                reading: r.into(),
                pronunciation: p.into(),
                base_form: format!("base:{surface}").into(),
                conj_type: "fixture-type".into(),
                conj_form: "fixture-form".into(),
                cost,
                ..Default::default()
            });
        }
        builder.build().unwrap()
    }

    fn pronunciation(tokens: &[Token]) -> String {
        tokens.iter().map(|t| t.pronunciation.as_ref()).collect()
    }

    #[test]
    fn analyzer_quantity_readings_cover_integers_fractions_units_and_native_counters() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for (text, expected) in [
            ("10分", "ジュップン"),
            ("2031年度", "ニセンサンジューイチネンド"),
            ("1,000円", "センエン"),
            ("1.5倍", "イチテンゴバイ"),
            ("0.5分", "ゼロテンゴフン"),
            ("1.0分", "イチテンゼロフン"),
            ("2.50分", "ニテンゴゼロフン"),
            ("300本", "サンビャッポン"),
            ("21本", "ニジューイッポン"),
            ("14日", "ジューヨッカ"),
            ("24日", "ニジューヨッカ"),
            ("2人", "フタリ"),
            ("12人", "ジューニニン"),
            ("13日", "ジューサンニチ"),
            ("10万", "ジューマン"),
            ("1兆", "イッチョー"),
            ("8兆", "ハッチョー"),
            ("10兆", "ジュッチョー"),
            ("一兆", "イッチョー"),
            ("八兆", "ハッチョー"),
            ("十兆", "ジュッチョー"),
            ("1000kg", "センキログラム"),
            ("1,000円2.5倍", "センエンニテンゴバイ"),
            ("10万2031円", "ジューマンニセンサンジューイチエン"),
            ("10円2031年度", "ジューエンニセンサンジューイチネンド"),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert_eq!(pronunciation(&out), expected, "{text}");
            for t in &out {
                assert_eq!(&text[t.start..t.end], t.surface.as_ref());
            }
        }
        let out = analyzer.try_tokenize("2031年度").unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].surface.as_ref(), "2031");
        assert!(!out[0].is_known);
        assert_eq!(out[0].base_form.as_ref(), "2031");
        assert_eq!(out[0].pos.as_ref(), "名詞,数,*,*");
        assert!(out[0].conj_type.is_empty() && out[0].conj_form.is_empty());
        assert!(out[0].mora_count() > 0);
    }

    #[test]
    fn known_single_number_and_known_mixed_word_keep_all_dictionary_fields() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for (text, surface, expected) in [
            ("12円", "12", "ジューニ"),
            ("11本", "11", "テンイチ"),
            ("2026年度", "2026年度", "ニセンニジューロクネンド"),
            ("2.5倍", "2.5倍", "ニテンゴバイ"),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            let t = &out[0];
            assert_eq!(t.surface.as_ref(), surface);
            assert_eq!(t.base_form.as_ref(), format!("base:{surface}"));
            assert_eq!(t.conj_type.as_ref(), "fixture-type");
            assert_eq!(t.conj_form.as_ref(), "fixture-form");
            assert_eq!(t.word_cost, -100);
            assert!(t.is_known);
            assert_eq!(t.pronunciation.as_ref(), expected);
        }
    }

    #[test]
    fn analyzer_bare_decimals_keep_readings_and_utf8_spans() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for (text, surface, reading, pronunciation) in [
            ("図9.2", "9.2", "キュウテンニ", "キューテンニ"),
            ("値は2.50", "2.50", "ニテンゴゼロ", "ニテンゴゼロ"),
            ("0.5の値", "0.5", "ゼロテンゴ", "ゼロテンゴ"),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            let token = out.iter().find(|t| t.surface.as_ref() == surface).unwrap();
            assert_eq!(token.reading.as_ref(), reading, "{text}: {out:?}");
            assert_eq!(token.pronunciation.as_ref(), pronunciation, "{text}");
            for token in &out {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
    }

    #[test]
    fn analyzer_negative_decimal_signs_are_local_and_do_not_duplicate_known_minus() {
        fn sign_dictionary(known_minus: bool) -> Dictionary {
            let mut b = DictBuilder::new();
            for (s, pos, r, p) in [
                ("0", "名詞,数,*,*", "ゼロ", "ゼロ"),
                ("1", "名詞,数,*,*", "イチ", "イチ"),
                ("2", "名詞,数,*,*", "ニ", "ニ"),
                ("4", "名詞,数,*,*", "ヨン", "ヨン"),
                ("5", "名詞,数,*,*", "ゴ", "ゴ"),
                ("7", "名詞,数,*,*", "ナナ", "ナナ"),
                ("-", "記号,一般,*,*", "-", "-"),
                ("=", "記号,一般,*,*", "=", "="),
                (".", "記号,句点,*,*", "．", "．"),
                ("：", "記号,一般,*,*", "：", "："),
                (";", "記号,一般,*,*", ";", ";"),
                ("値", "名詞,一般,*,*", "アタイ", "アタイ"),
                ("度", "名詞,接尾,助数詞,*", "ド", "ド"),
                ("A", "名詞,一般,*,*", "エー", "エー"),
                ("ID", "名詞,一般,*,*", "アイディー", "アイディー"),
            ] {
                b.add_entry(DictEntry {
                    surface: s.into(),
                    pos: pos.into(),
                    reading: r.into(),
                    pronunciation: p.into(),
                    base_form: format!("base:{s}").into(),
                    cost: 17,
                    ..Default::default()
                });
            }
            if known_minus {
                b.add_entry(DictEntry {
                    surface: "-4".into(),
                    pos: "名詞,固有名詞,一般,*".into(),
                    base_form: "−4".into(),
                    reading: "マイナスヨン".into(),
                    pronunciation: "マイナスヨン".into(),
                    cost: -1000,
                    ..Default::default()
                });
            }
            b.build().unwrap()
        }
        let mut a = Analyzer::from_dict(sign_dictionary(false));
        for text in ["=-47.25", "値：-47.25", "12.5; -47.25", "=-47.25度"] {
            let out = a.try_tokenize(text).unwrap();
            let sign = out.iter().find(|t| t.surface.as_ref() == "-").unwrap();
            assert_eq!(sign.reading.as_ref(), "マイナス", "{text}");
            assert_eq!(sign.pronunciation.as_ref(), "マイナス", "{text}");
            assert_eq!(sign.pos.as_ref(), "記号,一般,*,*");
            assert_eq!(sign.base_form.as_ref(), "base:-");
            assert_eq!(sign.word_cost, 17);
            assert!(sign.is_known);
            for t in &out {
                assert_eq!(&text[t.start..t.end], t.surface.as_ref());
            }
            let sentences = a
                .try_tokenize_sentences(text, &crate::sentence::SplitOptions::default())
                .unwrap();
            let p: String = sentences
                .iter()
                .flat_map(|(_, v)| v)
                .map(|t| t.pronunciation.as_ref())
                .collect();
            assert_eq!(p, pronunciation(&out));
        }
        for text in ["A-47.25", "ID -47.25", "4-47.25", "=-47.25A", "=-10度"] {
            let out = a.try_tokenize(text).unwrap();
            assert!(
                out.iter()
                    .filter(|t| t.surface.as_ref() == "-")
                    .all(|t| t.pronunciation.as_ref() == "-"),
                "{text}: {out:?}"
            );
        }
        let mut a = Analyzer::from_dict(sign_dictionary(true));
        let out = a.try_tokenize("=-47.25").unwrap();
        assert_eq!(
            out.iter()
                .filter(|t| t.pronunciation.contains("マイナス"))
                .count(),
            1
        );
        let known = out.iter().find(|t| t.surface.as_ref() == "-4").unwrap();
        assert_eq!(known.pronunciation.as_ref(), "マイナスヨン");
        assert_eq!(known.word_cost, -1000);
        assert!(known.is_known);
    }

    #[test]
    fn protected_single_digits_do_not_receive_new_trillion_euphony() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for (text, surface, expected) in [
            ("ISO 1兆", "1", "イチ"),
            ("ISO 8兆", "8", "ハチ"),
            ("ISO\n8兆", "8", "ハチ"),
            ("−1兆", "1", "イチ"),
            ("01兆", "1", "イチ"),
            ("1.2.1兆", "1", "イチ"),
            ("1,01兆", "1", "イチ"),
            ("1８兆", "８", "ハチ"),
            ("1八兆", "八", "ハチ"),
            ("８兆", "８", "ハチ"),
            ("１８兆", "８", "ハチ"),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            let t = out
                .iter()
                .rev()
                .find(|t| t.surface.as_ref() == surface)
                .unwrap();
            assert_eq!(t.reading.as_ref(), expected, "{text}");
            assert_eq!(t.pronunciation.as_ref(), expected, "{text}");
        }
    }

    #[test]
    fn original_input_protects_identifier_prefixes_across_analyzer_chunks_and_sentences() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for separator in [" ", "\t", "\n", "\r\n"] {
            let text = format!("ISO{separator}9001円");
            let out = analyzer.try_tokenize(&text).unwrap();
            assert_eq!(
                out.iter()
                    .filter(|t| t.surface.chars().all(|c| c.is_ascii_digit()))
                    .count(),
                4,
                "{text}"
            );
            let sentences = analyzer
                .try_tokenize_sentences(&text, &crate::sentence::SplitOptions::default())
                .unwrap();
            assert_eq!(
                sentences
                    .iter()
                    .flat_map(|(_, t)| t)
                    .filter(|t| t.surface.chars().all(|c| c.is_ascii_digit()))
                    .count(),
                4
            );
        }
        let out = analyzer.try_tokenize("価格。10分。2.50分").unwrap();
        assert!(
            out.iter()
                .any(|t| t.surface.as_ref() == "10" && t.pronunciation.as_ref() == "ジュッ")
        );
        assert!(out.iter().any(|t| t.surface.as_ref() == "2.50" && t.pronunciation.as_ref() == "ニテンゴゼロ"));
    }

    #[test]
    fn prepared_chunks_keep_identifier_protection_until_the_envelope_ends() {
        let dict = dictionary();
        let mut workspace = crate::lattice::LatticeWorkspace::new();
        for parts in [&["ISO.", "8兆"][..], &["ISO.", ".", "8兆"][..]] {
            let input = parts.concat();
            for offset in [0, 91] {
                let plan = AsciiPlan::new(&input, offset);
                let mut cursor = 0;
                let mut position = offset;
                let mut out = Vec::new();
                let mut cursors = Vec::new();
                for part in parts {
                    let first = out.len();
                    let previous = format!("{out:?}");
                    workspace
                        .tokenize_into_prepared(part, &dict, position, &mut out, &plan, &mut cursor)
                        .unwrap();
                    assert_eq!(format!("{:?}", &out[..first]), previous);
                    cursors.push(cursor);
                    position += part.len();
                }
                let number = out.iter().find(|t| t.surface.as_ref() == "8").unwrap();
                assert_eq!(number.reading.as_ref(), "ハチ", "{parts:?} at {offset}");
                assert_eq!(
                    number.pronunciation.as_ref(),
                    "ハチ",
                    "{parts:?} at {offset}"
                );
                assert!(cursors[..cursors.len() - 1].iter().all(|&c| c == 0));
                assert_eq!(cursor, plan.candidates.len());
                for t in &out {
                    assert_eq!(&input[t.start - offset..t.end - offset], t.surface.as_ref());
                }
            }
        }
        let out = workspace.tokenize("8兆", &dict).unwrap();
        assert_eq!(pronunciation(&out), "ハッチョー");
    }

    #[test]
    fn direct_workspace_quantity_phase_keeps_offset_and_prior_output_outside_its_context() {
        let dict = dictionary();
        let mut workspace = crate::lattice::LatticeWorkspace::new();
        let mut out = workspace.tokenize("ISO", &dict).unwrap();
        let previous = format!("{out:?}");
        let first = out.len();
        workspace
            .tokenize_into("1,000円", &dict, 91, &mut out)
            .unwrap();
        assert_eq!(format!("{:?}", &out[..first]), previous);
        assert_eq!(out[first].surface.as_ref(), "1,000");
        assert_eq!(out[first].pronunciation.as_ref(), "セン");
        assert_eq!((out[first].start, out[first].end), (91, 96));
    }

    #[test]
    fn fullwidth_digits_in_identifiers_do_not_receive_new_counter_readings() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for text in [
            "Ａ１つ",
            "A１点",
            "ID１章",
            "ID\n１つ",
            "ID.\n１章",
            "１Aつ",
            "1１つ",
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            let number = out.iter().find(|t| t.surface.as_ref() == "１").unwrap();
            assert_eq!(number.reading.as_ref(), "イチ", "{text}: {out:?}");
            let sentences = analyzer
                .try_tokenize_sentences(text, &crate::sentence::SplitOptions::default())
                .unwrap();
            let number = sentences
                .iter()
                .flat_map(|(_, tokens)| tokens)
                .find(|t| t.surface.as_ref() == "１")
                .unwrap();
            assert_eq!(number.reading.as_ref(), "イチ", "sentences: {text}");
        }
        let out = analyzer.try_tokenize("１つ").unwrap();
        assert_eq!(pronunciation(&out), "ヒトツ");
    }

    #[test]
    fn comma_delimited_quantities_keep_euphony_and_identifier_protection() {
        let mut analyzer = Analyzer::from_dict(dictionary());
        for (text, numbers) in [
            ("前に,10本", vec![("10", "ジュッ", 2)]),
            ("一,10本", vec![("10", "ジュッ", 2)]),
            (
                "10本,20本",
                vec![("10", "ジュッ", 2), ("20", "ニジュッ", 3)],
            ),
            ("ISO ,10本", vec![("10", "ジュッ", 2)]),
            ("１ ,10本", vec![("10", "ジュッ", 2)]),
            ("一,2.50分", vec![("2.50", "ニテンゴゼロ", 6)]),
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            for (surface, expected, morae) in numbers {
                let token = out.iter().find(|t| t.surface.as_ref() == surface).unwrap();
                assert_eq!(token.pronunciation.as_ref(), expected, "{text}: {out:?}");
                assert_eq!(token.mora_count(), morae);
                // 小数点はこの辞書ではコスト9000の未知記号。
                assert_eq!(
                    token.word_cost,
                    if surface.contains('.') { 9000 } else { 0 }
                );
                assert!(!token.is_known);
            }
            for token in &out {
                assert_eq!(&text[token.start..token.end], token.surface.as_ref());
            }
        }
        for text in ["前に,１つ", "一,１つ", "ISO ,１つ", "１ ,１つ"] {
            let out = analyzer.try_tokenize(text).unwrap();
            let token = out
                .iter()
                .rev()
                .find(|t| t.surface.as_ref() == "１")
                .unwrap();
            assert_eq!(token.pronunciation.as_ref(), "ヒト", "{text}: {out:?}");
        }
        for text in [
            "A,１つ",
            "ISO,１つ",
            ",,１つ",
            "１,１つ",
            "-１つ",
            "+１つ",
            "_１つ",
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            let numbers: Vec<_> = out.iter().filter(|t| t.surface.as_ref() == "１").collect();
            assert!(!numbers.is_empty());
            assert!(
                numbers.iter().all(|t| t.pronunciation.as_ref() == "イチ"),
                "{text}: {out:?}"
            );
        }
        for text in [
            "A,10本",
            "ISO,10本",
            "１,10本",
            "1,23本",
            "1,,000本",
            ",,10本",
            "-10本",
        ] {
            let out = analyzer.try_tokenize(text).unwrap();
            assert!(
                out.iter()
                    .filter(|t| t.surface.bytes().all(|b| b.is_ascii_digit()))
                    .all(|t| t.surface.len() == 1),
                "{text}: {out:?}"
            );
        }
    }

    #[test]
    fn comma_boundary_survives_prepared_chunks_and_offsets() {
        let dict = dictionary();
        let mut workspace = crate::lattice::LatticeWorkspace::new();
        for parts in [&["ISO ", ",10本"][..], &["一,", "10本"][..]] {
            let input = parts.concat();
            let offset = 91;
            let plan = AsciiPlan::new(&input, offset);
            let mut cursor = 0;
            let mut position = offset;
            let mut out = workspace.tokenize("先", &dict).unwrap();
            for part in parts {
                let first = out.len();
                let previous = format!("{out:?}");
                workspace
                    .tokenize_into_prepared(part, &dict, position, &mut out, &plan, &mut cursor)
                    .unwrap();
                assert_eq!(format!("{:?}", &out[..first]), previous);
                position += part.len();
            }
            assert_eq!(cursor, plan.candidates.len());
            let token = out.iter().find(|t| t.surface.as_ref() == "10").unwrap();
            assert_eq!(token.pronunciation.as_ref(), "ジュッ");
            assert_eq!(token.start, offset + input.find("10").unwrap());
            assert_eq!(token.end, token.start + 2);
            assert_eq!(token.word_cost, 0);
            assert_eq!(token.mora_count(), 2);
        }
    }
}

#[cfg(test)]
mod distributed_tests {
    #[test]
    #[ignore = "配布辞書 3 個が必要（make dict-download）"]
    fn distributed_negative_decimals_include_sign_point_and_fraction() {
        for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("dict")
                .join(format!("{name}.hsd"));
            let mut analyzer = crate::Analyzer::load(path).unwrap();
            for (text, expected) in [
                ("-0.5", "マイナスゼロテンゴ"),
                ("-0.1本", "マイナスゼロテンイチホン"),
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                let reading: String = out.iter().map(|t| t.reading.as_ref()).collect();
                let pronunciation: String = out.iter().map(|t| t.pronunciation.as_ref()).collect();
                assert_eq!(reading, expected, "{name}: {text}");
                assert_eq!(pronunciation, expected, "{name}: {text}");
                for token in &out {
                    assert_eq!(&text[token.start..token.end], token.surface.as_ref());
                }
            }
        }
    }
    #[test]
    #[ignore = "配布辞書 3 個が必要（make dict-download）"]
    fn distributed_ascii_quantity_readings() {
        for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("dict")
                .join(format!("{name}.hsd"));
            let mut analyzer = crate::Analyzer::load(path).unwrap();
            for (text, expected) in [
                ("10分", "ジュップン"),
                ("2031年度", "ニセンサンジューイチネンド"),
                ("1,000円", "センエン"),
                ("2.5倍", "ニテンゴバイ"),
                ("0.5分", "ゼロテンゴフン"),
                ("1.0分", "イチテンゼロフン"),
                ("300本", "サンビャッポン"),
                ("14日", "ジューヨッカ"),
                ("10万", "ジューマン"),
                ("1兆", "イッチョー"),
                ("8兆", "ハッチョー"),
                ("10兆", "ジュッチョー"),
                ("一兆", "イッチョー"),
                ("八兆", "ハッチョー"),
                ("十兆", "ジュッチョー"),
                ("1000kg", "センキログラム"),
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                let actual: String = out.iter().map(|t| t.pronunciation.as_ref()).collect();
                assert_eq!(actual, expected, "{name}: {text}");
                for t in &out {
                    assert_eq!(&text[t.start..t.end], t.surface.as_ref());
                }
            }
            for (text, surface, expected) in [
                ("ISO 1兆", "1", "イチ"),
                ("ISO 8兆", "8", "ハチ"),
                ("−1兆", "1", "イチ"),
                ("1８兆", "８", "ハチ"),
                ("1八兆", "八", "ハチ"),
                ("８兆", "８", "ハチ"),
                ("１８兆", "８", "ハチ"),
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                let t = out
                    .iter()
                    .rev()
                    .find(|t| t.surface.ends_with(surface))
                    .unwrap_or_else(|| panic!("{name}: {text}: {out:?}"));
                // NEologd は「−1」を既知語として持つので、その辞書読みも保つ。
                assert!(t.reading.ends_with(expected), "{name}: {text}: {t:?}");
                assert!(t.pronunciation.ends_with(expected), "{name}: {text}: {t:?}");
            }
            for text in [
                "ISO\n9001円",
                "ISO 9001円",
                "03-1234-5678円",
                "123－4567円",
                "1.2.3円",
                "1．23円",
                "1,23円",
                "0010円",
                "10ms",
                "2031",
                "1000\n分",
            ] {
                let out = analyzer.try_tokenize(text).unwrap();
                assert!(
                    !out.iter().any(|t| !t.is_known
                        && t.surface.len() > 1
                        && t.surface
                            .bytes()
                            .all(|b| b.is_ascii_digit() || matches!(b, b',' | b'.'))
                        && t.base_form == t.surface
                        && t.conj_type.is_empty()
                        && !t.reading.is_empty()),
                    "{name}: {text}"
                );
            }
        }
    }
}
