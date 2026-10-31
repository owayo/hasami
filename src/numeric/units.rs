//! 単位の読み表と、数詞・単位の全体が揃った数量の範囲。

use crate::lattice::Token;
use std::{ops::Range, sync::Arc};

fn ascii_alias_requires_frame(folded: &str) -> bool {
    matches!(folded, "bit" | "mph" | "kcal" | "km2" | "km/h" | "m/s")
}

/// 保護の判定は読みと同じ英字幅で行う。物理量として認可する元表記とは分ける。
pub(crate) fn requires_frame(surface: &str) -> bool {
    ascii_alias_requires_frame(fold_alphabet_width(surface).as_ref())
}

fn unit_reading_char(c: char) -> Option<&'static str> {
    match c {
        'W' => Some("ワット"),
        'A' => Some("アンペア"),
        'V' => Some("ボルト"),
        'J' => Some("ジュール"),
        'N' => Some("ニュートン"),
        'T' => Some("テスラ"),
        'F' => Some("ファラド"),
        'H' => Some("ヘンリー"),
        'K' => Some("ケルビン"),
        'm' => Some("メートル"),
        'g' => Some("グラム"),
        't' => Some("トン"),
        'L' | 'l' => Some("リットル"),
        _ => None,
    }
}

/// 数字の後に来る複数文字の単位読みを返す
fn unit_reading_multi(s: &str) -> Option<&'static str> {
    match s {
        "bit" => Some("ビット"),
        "mph" => Some("エムピーエイチ"),
        "kcal" => Some("キロカロリー"),
        "km2" => Some("ヘイホウキロメートル"),
        "km/h" => Some("キロメートルマイジ"),
        "m/s" => Some("メートルマイビョウ"),
        "Hz" => Some("ヘルツ"),
        "kHz" => Some("キロヘルツ"),
        "MHz" => Some("メガヘルツ"),
        "GHz" => Some("ギガヘルツ"),
        "km" => Some("キロメートル"),
        "cm" => Some("センチメートル"),
        "mm" => Some("ミリメートル"),
        "nm" => Some("ナノメートル"),
        "kg" => Some("キログラム"),
        "mg" => Some("ミリグラム"),
        "mL" | "ml" => Some("ミリリットル"),
        "kW" => Some("キロワット"),
        "MW" => Some("メガワット"),
        "mA" => Some("ミリアンペア"),
        "kV" => Some("キロボルト"),
        "dB" => Some("デシベル"),
        "Pa" => Some("パスカル"),
        "hPa" => Some("ヘクトパスカル"),
        "kPa" => Some("キロパスカル"),
        "MPa" => Some("メガパスカル"),
        "KB" => Some("キロバイト"),
        "MB" => Some("メガバイト"),
        "GB" => Some("ギガバイト"),
        "TB" => Some("テラバイト"),
        "Wh" => Some("ワットアワー"),
        "kWh" => Some("キロワットアワー"),
        "Ah" => Some("アンペアアワー"),
        "mAh" => Some("ミリアンペアアワー"),
        "cc" => Some("シーシー"),
        "hp" => Some("馬力"),
        "rpm" => Some("アールピーエム"),
        "fps" => Some("エフピーエス"),
        "bps" => Some("ビーピーエス"),
        "Mbps" => Some("メガビーピーエス"),
        "Gbps" => Some("ギガビーピーエス"),
        _ => None,
    }
}

fn fold_alphabet_width(surface: &str) -> std::borrow::Cow<'_, str> {
    if surface.is_ascii() {
        std::borrow::Cow::Borrowed(surface)
    } else {
        std::borrow::Cow::Owned(
            surface
                .chars()
                .map(|c| match c {
                    'Ａ'..='Ｚ' | 'ａ'..='ｚ' => char::from_u32(c as u32 - 0xfee0).unwrap(),
                    _ => c,
                })
                .collect(),
        )
    }
}

/// 数字の後に来るアルファベットの単位読みを返す
pub(crate) fn reading(surface: &str) -> Option<&'static str> {
    let folded = fold_alphabet_width(surface);
    // 追加した単位はASCII元表記だけ認可する。従来の全角単位の読みは維持する。
    if !surface.is_ascii() && ascii_alias_requires_frame(folded.as_ref()) {
        return None;
    }
    let surface = folded.as_ref();
    let mut chars = surface.chars();
    if let Some(c) = chars.next()
        && chars.next().is_none()
    {
        return unit_reading_char(c);
    }
    unit_reading_multi(surface)
}

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub number: Range<usize>,
    pub end: usize,
    pub reading: &'static str,
}

pub(crate) fn crosses_boundary(
    _input: &str,
    _offset: usize,
    plan: &super::AsciiPlan,
    start: usize,
    end: usize,
) -> bool {
    plan.unit_frame_at(start).is_some_and(|frame| {
        start < frame.number.end
            && frame.number.end < end
            && end <= frame.end
            && !(start <= frame.number.start && frame.end <= end)
    })
}

fn unit_piece(token: &Token) -> bool {
    if !token.conj_type.is_empty()
        || !token.conj_form.is_empty()
        || (token.is_known && token.base_form != token.surface)
        || token.pos.contains(",人名,")
        || token.pos.contains(",地域,")
        || (token.is_known && token.pos.contains(",組織,"))
    {
        return false;
    }
    if !token.is_known && token.reading.is_empty() && token.pronunciation.is_empty() {
        return true;
    }
    if token.surface.as_ref() == "2"
        && super::pos_has(&token.pos, "名詞,数")
        && token.reading.as_ref() == "ニ"
        && token.pronunciation.as_ref() == "ニ"
    {
        return true;
    }
    let spelling = crate::lattice::alphabet_reading(&token.surface);
    let allowed = |s: &str| {
        reading(&token.surface) == Some(s)
            || spelling.as_deref() == Some(s)
            || (matches!(token.surface.as_ref(), "/" | "2" | "²") && s == token.surface.as_ref())
    };
    allowed(&token.reading) && allowed(&token.pronunciation)
}

pub(super) fn number_piece(t: &Token) -> bool {
    if !t.conj_type.is_empty() || !t.conj_form.is_empty() || t.base_form != t.surface {
        return false;
    }
    if t.surface.as_ref() == "." || t.surface.as_ref() == "," {
        return super::pos_has(&t.pos, "記号")
            && ((t.reading.is_empty() && t.pronunciation.is_empty())
                || [
                    t.surface.as_ref(),
                    if t.surface.as_ref() == "." {
                        "．"
                    } else {
                        "，"
                    },
                ]
                .contains(&t.reading.as_ref())
                    && [
                        t.surface.as_ref(),
                        if t.surface.as_ref() == "." {
                            "．"
                        } else {
                            "，"
                        },
                    ]
                    .contains(&t.pronunciation.as_ref()));
    }
    if !super::pos_has(&t.pos, "名詞,数") {
        return false;
    }
    let Some(parsed) = super::ascii::parse(&t.surface) else {
        return false;
    };
    if !t.is_known && t.reading.is_empty() && t.pronunciation.is_empty() {
        return true;
    }
    let (r, p) = super::ascii::readings(&t.surface, parsed);
    super::ascii::standard_readings(&t.surface, &t.reading, &t.pronunciation)
        || ([r.as_str(), p.as_str()].contains(&t.reading.as_ref())
            && [r.as_str(), p.as_str()].contains(&t.pronunciation.as_ref()))
}

/// 入力全体で認可済みの単位の断片だけを合成する。
/// 数詞のコストはそのまま残し、選択された単位のコストだけを一度合計する。
pub(crate) fn coalesce_units(
    input: &str,
    offset: usize,
    out: &mut Vec<Token>,
    first: usize,
    plan: &super::AsciiPlan,
) {
    let frames = plan.unit_frames_in(offset, offset + input.len());
    if frames.is_empty() {
        return;
    }
    let (mut read, mut write) = (first, first);
    for frame in frames {
        let begin = out[read..].partition_point(|t| t.end <= frame.number.start) + read;
        let finish = out[read..].partition_point(|t| t.end <= frame.number.end) + read;
        if begin >= finish
            || out[begin].start != frame.number.start
            || out[finish - 1].end != frame.number.end
            || !out[begin..finish].iter().all(number_piece)
        {
            continue;
        }
        while read < out.len() && out[read].end <= frame.number.end {
            out.swap(write, read);
            write += 1;
            read += 1;
        }
        let start = read;
        let mut limit = start;
        while limit < out.len() && out[limit].end <= frame.end {
            limit += 1;
        }
        if start >= limit
            || out[start].start != frame.number.end
            || out[limit - 1].end != frame.end
            || !out[start..limit].windows(2).all(|p| p[0].end == p[1].start)
            || out[start..limit].iter().any(|t| {
                let (Some(a), Some(b)) = (t.start.checked_sub(offset), t.end.checked_sub(offset))
                else {
                    return true;
                };
                input.get(a..b) != Some(t.surface.as_ref())
            })
        {
            continue;
        }
        // 選択された一語の単位は、辞書の素性と明示された読みを保持する。
        if limit - start == 1 {
            continue;
        }
        if !out[start..limit].iter().all(unit_piece) {
            continue;
        }
        let cost: i64 = out[start..limit]
            .iter()
            .map(|t| i64::from(t.word_cost))
            .sum();
        let surface: Arc<str> = Arc::from(&input[frame.number.end - offset..frame.end - offset]);
        out[write] = Token {
            surface: Arc::clone(&surface),
            base_form: surface,
            start: frame.number.end,
            end: frame.end,
            pos: Arc::from("名詞,接尾,助数詞,*"),
            conj_type: Arc::from(""),
            conj_form: Arc::from(""),
            reading: Arc::from(frame.reading),
            pronunciation: Arc::from(frame.reading),
            word_cost: cost.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
            is_known: false,
        };
        write += 1;
        read = limit;
    }
    while read < out.len() {
        out.swap(write, read);
        write += 1;
        read += 1;
    }
    out.truncate(write);
}

/// 明示された単一数量の発音を返す。既知の一語や識別子はNoneにする。
pub(crate) fn quantity_pronunciation(input: &str, tokens: &[Token]) -> Option<String> {
    let [number, suffix] = tokens else {
        return None;
    };
    if number.start != 0
        || number.end != suffix.start
        || suffix.end != input.len()
        || !super::pos_has(&number.pos, "名詞,数")
        || number.base_form != number.surface
        || !number.conj_type.is_empty()
        || !number.conj_form.is_empty()
        || input.get(..number.end) != Some(number.surface.as_ref())
        || input.get(suffix.start..) != Some(suffix.surface.as_ref())
    {
        return None;
    }
    let parsed = super::ascii::parse(&number.surface)?;
    let counter = super::counter(suffix);
    let unit_reading = reading(&suffix.surface);
    if let Some(unit) = unit_reading {
        if suffix.reading.as_ref() != unit
            || suffix.pronunciation.as_ref() != unit
            || suffix.base_form != suffix.surface
            || !suffix.conj_type.is_empty()
            || !suffix.conj_form.is_empty()
        {
            return None;
        }
    } else if !(suffix.is_known
        && suffix.base_form == suffix.surface
        && suffix.conj_type.is_empty()
        && suffix.conj_form.is_empty()
        && counter.is_some())
    {
        return None;
    }
    let (r, p) = super::ascii::readings(&number.surface, parsed);
    let mut expected = tokens.to_vec();
    if unit_reading.is_some() {
        // 選択された未知単位の品詞で音便が適用されなかった場合も、共通規則の期待と照合する。
        // 読みが未完成ならNoneにする。比較用のコピーで実トークンを置き換えない。
        expected[1].pos = Arc::from("名詞,一般,*,*");
    }
    expected[0].reading = Arc::from(r);
    expected[0].pronunciation = Arc::from(p);
    super::apply_counter_readings(&mut expected);
    if tokens
        .iter()
        .zip(&expected)
        .any(|(a, b)| a.reading != b.reading || a.pronunciation != b.pronunciation)
    {
        return None;
    }
    Some(tokens.iter().map(|t| t.pronunciation.as_ref()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_frames_require_the_complete_global_scalar_and_unit() {
        for text in ["32bit", "2.3mph", "6,000kcal", "57km2", "50km/h", "50m/s"] {
            let plan = super::super::AsciiPlan::new(text, 47);
            let frames = plan.unit_frames_in(47, 47 + text.len());
            assert_eq!(frames.len(), 1, "{text}");
            assert!(
                plan.unit_frames_in(48, 47 + text.len()).is_empty(),
                "{text}"
            );
            assert!(
                plan.unit_frames_in(47, 46 + text.len()).is_empty(),
                "{text}"
            );
            let boundary = frames[0].number.end;
            assert!(crosses_boundary(
                text,
                47,
                &plan,
                boundary - 1,
                boundary + 1
            ));
            assert!(!crosses_boundary(text, 47, &plan, 47, 47 + text.len()));
            assert!(!crosses_boundary(text, 47, &plan, 46, 48 + text.len()));
            assert!(!crosses_boundary(
                text,
                47,
                &plan,
                boundary - 1,
                48 + text.len()
            ));
        }
        for text in [
            "ISO 32bit",
            "ID32bit",
            "コード32bit",
            "型番32bit",
            "32bitX",
            "32bit_1",
            "032bit",
            "32bits",
            "32BIT",
            "32bit/ID",
            "32.3.2mph",
            "3２bit",
        ] {
            let plan = super::super::AsciiPlan::new(text, 0);
            assert!(plan.unit_frames_in(0, text.len()).is_empty(), "{text}");
        }
        assert_eq!(reading("ｋＷ"), Some("キロワット"));
        assert_eq!(reading("KW"), None);
    }

    fn token(surface: &str, start: usize, reading: &str) -> Token {
        Token {
            surface: surface.into(),
            base_form: surface.into(),
            start,
            end: start + surface.len(),
            pos: "名詞,一般,*,*".into(),
            conj_type: "".into(),
            conj_form: "".into(),
            reading: reading.into(),
            pronunciation: reading.into(),
            word_cost: 123,
            is_known: true,
        }
    }

    #[test]
    fn unit_compaction_sums_only_unit_costs_and_keeps_prior_output_and_custom_names() {
        let text = "2.3mph";
        let plan = super::super::AsciiPlan::new(text, 31);
        let prior = token("前", 0, "マエ");
        let mut number = token("2.3", 31, "ニテンサン");
        number.pos = "名詞,数,*,*".into();
        let mut out = vec![
            prior.clone(),
            number.clone(),
            token("m", 34, "エム"),
            token("ph", 35, "ピーエイチ"),
        ];
        coalesce_units(text, 31, &mut out, 1, &plan);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].surface, prior.surface);
        assert_eq!(out[0].word_cost, prior.word_cost);
        assert_eq!(out[1].reading, number.reading);
        assert_eq!(out[1].word_cost, number.word_cost);
        assert_eq!(out[2].surface.as_ref(), "mph");
        assert_eq!(out[2].reading.as_ref(), "エムピーエイチ");
        assert_eq!(out[2].word_cost, 246);
        assert!(!out[2].is_known);
        assert_eq!((out[2].start, out[2].end), (34, 37));
        let mut custom = vec![number, token("m", 34, "エム"), token("ph", 35, "ヨミ")];
        let before = custom.clone();
        coalesce_units(text, 31, &mut custom, 0, &plan);
        assert_eq!(
            custom
                .iter()
                .map(|t| (&t.surface, &t.reading, t.word_cost))
                .collect::<Vec<_>>(),
            before
                .iter()
                .map(|t| (&t.surface, &t.reading, t.word_cost))
                .collect::<Vec<_>>()
        );
        let mut whole = vec![token(text, 31, "カスタム")];
        coalesce_units(text, 31, &mut whole, 0, &plan);
        assert_eq!(whole.len(), 1);
        assert_eq!(whole[0].reading.as_ref(), "カスタム");
    }
}

#[cfg(all(test, feature = "build"))]
mod analyzer_tests {
    use crate::{
        Analyzer,
        dict::{DictBuilder, DictEntry},
    };

    fn dictionary(whole: bool) -> crate::Dictionary {
        let mut b = DictBuilder::new();
        for (surface, pos, r, cost) in [
            ("0", "名詞,数,*,*", "ゼロ", 101),
            ("1", "名詞,数,*,*", "イチ", 102),
            ("2", "名詞,数,*,*", "ニ", 103),
            ("3", "名詞,数,*,*", "サン", 104),
            ("5", "名詞,数,*,*", "ゴ", 105),
            ("7", "名詞,数,*,*", "ナナ", 106),
            ("g", "名詞,一般,*,*", "グラム", 201),
            ("kW", "名詞,一般,*,*", "キロワット", 202),
            ("m", "名詞,一般,*,*", "エム", 203),
            ("ph", "名詞,一般,*,*", "ピーエイチ", 204),
            (".", "記号,一般,*,*", "．", 205),
            ("200g", "名詞,固有名詞,一般,*", "ニヒャクグラム", -10000),
            (
                "75k",
                "名詞,固有名詞,一般,*",
                "ナナジュウゴケルビン",
                -10000,
            ),
            ("3K", "名詞,固有名詞,一般,*", "サンケイ", -10000),
        ] {
            b.add_entry(DictEntry {
                surface: surface.into(),
                base_form: surface.into(),
                pos: pos.into(),
                reading: r.into(),
                pronunciation: r.into(),
                cost,
                ..Default::default()
            });
        }
        if whole {
            b.add_entry(DictEntry {
                surface: "1200g".into(),
                base_form: "custom-base".into(),
                pos: "名詞,固有名詞,人名,姓".into(),
                reading: "カスタム".into(),
                pronunciation: "カスタム".into(),
                cost: -20000,
                ..Default::default()
            });
        }
        b.build().unwrap()
    }

    #[test]
    fn internal_quantity_fragments_yield_to_a_complete_unit_without_fabricating_costs() {
        let mut a = Analyzer::from_dict(dictionary(false));
        let tokens = a.tokenize("1200g");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].surface.as_ref(), "1200");
        assert_eq!(tokens[0].word_cost, 102 + 103 + 101 + 101);
        assert_eq!(tokens[1].surface.as_ref(), "g");
        assert_eq!(tokens[1].word_cost, 201);
        assert_eq!(
            a.quantity_pronunciation("1200g").unwrap().as_deref(),
            Some("センニヒャクグラム")
        );
        assert_eq!(
            a.quantity_pronunciation("0.75kW").unwrap().as_deref(),
            Some("ゼロテンナナゴキロワット")
        );
        assert_eq!(
            a.quantity_pronunciation("2.3mph").unwrap().as_deref(),
            Some("ニテンサンエムピーエイチ")
        );
        assert_eq!(a.quantity_pronunciation("3K").unwrap(), None);
        let mut a = Analyzer::from_dict(dictionary(true));
        let tokens = a.tokenize("1200g");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].base_form.as_ref(), "custom-base");
        assert_eq!(tokens[0].reading.as_ref(), "カスタム");
        assert_eq!(tokens[0].word_cost, -20000);
        assert_eq!(a.quantity_pronunciation("1200g").unwrap(), None);
    }
}

#[cfg(test)]
mod selector_reference_tests {
    use super::*;
    fn token(s: &str, start: usize, pos: &str, r: &str) -> Token {
        Token {
            surface: s.into(),
            base_form: s.into(),
            start,
            end: start + s.len(),
            pos: pos.into(),
            conj_type: "".into(),
            conj_form: "".into(),
            reading: r.into(),
            pronunciation: r.into(),
            word_cost: 321,
            is_known: false,
        }
    }
    #[test]
    fn quantity_reference_does_not_approve_an_unapplied_known_counter_family() {
        let n = token("6", 0, "名詞,数,*,*", "ロク");
        let kg = token("kg", 1, "名詞,固有名詞,組織,*", "キログラム");
        let ts = vec![n.clone(), kg];
        let before = format!("{ts:?}");
        assert_eq!(quantity_pronunciation("6kg", &ts), None);
        assert_eq!(format!("{ts:?}"), before);
        let mut correct = ts.clone();
        correct[0].reading = "ロッ".into();
        correct[0].pronunciation = "ロッ".into();
        correct[1].pos = "名詞,一般,*,*".into();
        assert_eq!(
            quantity_pronunciation("6kg", &correct).as_deref(),
            Some("ロッキログラム")
        );
        correct[1].base_form = "custom".into();
        assert_eq!(quantity_pronunciation("6kg", &correct), None);
        let bit = token("bit", 1, "名詞,固有名詞,組織,*", "ビット");
        assert_eq!(
            quantity_pronunciation("6bit", &[n, bit]).as_deref(),
            Some("ロクビット")
        );
    }
}

#[cfg(test)]
mod width_guard_tests {
    use super::*;
    #[test]
    fn new_alias_protection_folds_width_without_admitting_new_raw_widths() {
        for (ascii, wide, mixed) in [
            ("bit", "ｂｉｔ", "bｉt"),
            ("mph", "ｍｐｈ", "mｐh"),
            ("kcal", "ｋｃａｌ", "kｃal"),
            ("km2", "ｋｍ2", "kｍ2"),
            ("km/h", "ｋｍ/ｈ", "kｍ/h"),
            ("m/s", "ｍ/ｓ", "m/ｓ"),
        ] {
            assert!(requires_frame(ascii));
            assert!(reading(ascii).is_some());
            for raw in [wide, mixed] {
                assert!(requires_frame(raw), "{raw}");
                assert_eq!(reading(raw), None, "{raw}");
            }
        }
        for (raw, r) in [
            ("ｋＷ", "キロワット"),
            ("ｋｇ", "キログラム"),
            ("ｋｍ", "キロメートル"),
            ("Ｈｚ", "ヘルツ"),
            ("ｍ", "メートル"),
        ] {
            assert!(!requires_frame(raw), "{raw}");
            assert_eq!(reading(raw), Some(r), "{raw}");
        }
    }
}
