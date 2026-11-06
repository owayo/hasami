//! 単位の読み表と、数詞・単位の全体が揃った数量の範囲。

#[cfg(test)]
mod area_tests;

use crate::lattice::Token;
use std::{ops::Range, sync::Arc};

fn ascii_alias_requires_frame(folded: &str) -> bool {
    matches!(
        folded,
        "bit" | "mph" | "kcal" | "kcal/h" | "km2" | "km/h" | "m/s" | "mg/dL" | "m2" | "cm2" | "m3"
    )
}

/// 保護の判定は読みと同じ英字幅で行う。物理量として認可する元表記とは分ける。
pub(crate) fn requires_frame(surface: &str) -> bool {
    let folded = fold_alphabet_width(surface);
    ascii_alias_requires_frame(folded.as_ref()) || area_readings(folded.as_ref()).is_some()
}

/// 互換文字のNFKC表記と上付き表記。読みと長音を含む発音を区別する。
pub(super) fn area_readings(surface: &str) -> Option<(&'static str, &'static str)> {
    match surface {
        "m2" | "m²" => Some(("ヘイホウメートル", "ヘイホーメートル")),
        "cm2" | "cm²" => Some(("ヘイホウセンチメートル", "ヘイホーセンチメートル")),
        "m3" | "m³" => Some(("リッポウメートル", "リッポーメートル")),
        _ => None,
    }
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
        "kcal/h" => Some("キロカロリーマイジ"),
        "km2" => Some("ヘイホウキロメートル"),
        "km/h" => Some("キロメートルマイジ"),
        "m/s" => Some("メートルマイビョウ"),
        "mg/dL" => Some("ミリグラムマイデシリットル"),
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
        "hp" => Some("バリキ"),
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
    if let Some((reading, _)) = area_readings(surface) {
        return Some(reading);
    }
    let folded = fold_alphabet_width(surface);
    // 追加した単位はASCII元表記だけ認可する。従来の全角単位の読みは維持する。
    if !surface.is_ascii()
        && (ascii_alias_requires_frame(folded.as_ref()) || area_readings(folded.as_ref()).is_some())
    {
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
    pub allow_unknown_pos: bool,
}

pub(crate) fn crosses_boundary(
    _input: &str,
    _offset: usize,
    plan: &super::AsciiPlan,
    start: usize,
    end: usize,
) -> bool {
    plan.distance_crosses_boundary(start, end)
        || plan.unit_frame_at(start).is_some_and(|frame| {
            start < frame.number.end
                && frame.number.end < end
                && end <= frame.end
                && !(start <= frame.number.start && frame.end <= end)
        })
}

fn unit_piece(token: &Token) -> bool {
    if !token.is_uninflected()
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

/// 完全な mg/dL の中だけで、標準の綴り読みを持つ mg/MG を単位断片として扱う。
/// 他の人名・原形・明示読みへの一般的な例外にはしない。
fn concentration_unit_piece(token: &Token, unit_start: usize) -> bool {
    if token.base_form == token.surface {
        return unit_piece(token);
    }
    token.is_known
        && token.start == unit_start
        && token.surface.as_ref() == "mg"
        && token.base_form.as_ref() == "MG"
        && token.pos.as_ref() == "名詞,固有名詞,人名,一般"
        && token.is_uninflected()
        && token.reading == token.pronunciation
        && matches!(token.reading.as_ref(), "ミリグラム" | "エムジー")
}

pub(super) fn number_piece(t: &Token) -> bool {
    if !t.is_uninflected() || t.base_form != t.surface {
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
        let unit = input.get(frame.number.end - offset..frame.end - offset);
        let concentration = unit == Some("mg/dL");
        let area = unit.and_then(area_readings);
        let strict_scalar = concentration || unit == Some("kcal/h") || area.is_some();
        let begin = out[read..].partition_point(|t| t.end <= frame.number.start) + read;
        let finish = out[read..].partition_point(|t| t.end <= frame.number.end) + read;
        if begin >= finish
            || out[begin].start != frame.number.start
            || out[finish - 1].end != frame.number.end
            || !out[begin..finish].iter().all(number_piece)
            || strict_scalar
                && (!out[begin..finish]
                    .windows(2)
                    .all(|p| p[0].end == p[1].start)
                    || out[begin..finish].iter().any(|token| {
                        let (Some(a), Some(b)) = (
                            token.start.checked_sub(offset),
                            token.end.checked_sub(offset),
                        ) else {
                            return true;
                        };
                        input.get(a..b) != Some(token.surface.as_ref())
                    }))
        {
            continue;
        }
        let allow_unknown_pos = frame.allow_unknown_pos
            && out[begin..finish]
                .windows(2)
                .all(|p| p[0].end == p[1].start)
            && out[begin..finish].iter().all(|token| {
                token
                    .start
                    .checked_sub(offset)
                    .zip(token.end.checked_sub(offset))
                    .and_then(|(a, b)| input.get(a..b))
                    == Some(token.surface.as_ref())
            });
        // まだ何も結合していない間は write == read なので、入れ替えない
        while read < out.len() && out[read].end <= frame.number.end {
            if write != read {
                out.swap(write, read);
            }
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
            let token = &mut out[start];
            // 標準の数量に直結した未知英字の組織名だけを、合成した単位と同じ品詞にする。
            // 読みの補完は後段で行い、他の素性・既知語・独自読みは変更しない。
            if allow_unknown_pos
                && !token.is_known
                && token.pos.as_ref() == "名詞,固有名詞,組織,*"
                && token.base_form == token.surface
                && token.surface.bytes().all(|b| b.is_ascii_alphabetic())
                && unit_piece(token)
            {
                token.pos = Arc::from("名詞,接尾,助数詞,*");
            }
            // 完全な面積・体積フレームの中で、空欄または標準の単位読みだけを補完する。
            // 独自読み・人名・原形の異なる語は、単位表記と同形でも上書きしない。
            if let Some((reading, pronunciation)) = area {
                let token = &mut out[start];
                if token.base_form == token.surface
                    && token.is_uninflected()
                    && (super::pos_has(&token.pos, "名詞,一般")
                        || super::pos_has(&token.pos, "名詞,接尾,助数詞")
                        || !token.is_known && super::pos_has(&token.pos, "記号"))
                    && ["", reading, pronunciation].contains(&token.reading.as_ref())
                    && ["", reading, pronunciation].contains(&token.pronunciation.as_ref())
                {
                    token.reading = Arc::from(reading);
                    token.pronunciation = Arc::from(pronunciation);
                }
            }
            continue;
        }
        if !out[start..limit].iter().all(|token| {
            if concentration {
                concentration_unit_piece(token, frame.number.end)
            } else if area.is_some()
                && token.surface.as_ref() == "3"
                && token.base_form == token.surface
                && token.is_uninflected()
                && super::pos_has(&token.pos, "名詞,数")
                && token.reading.as_ref() == "サン"
                && token.pronunciation.as_ref() == "サン"
            {
                true
            } else {
                unit_piece(token)
            }
        }) {
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
            pronunciation: Arc::from(
                area.map_or(frame.reading, |(_, pronunciation)| pronunciation),
            ),
            word_cost: cost.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
            is_known: false,
        };
        write += 1;
        read = limit;
    }
    // 1 つも結合しなければ write == read のままで、残りはもう並んでいる
    if write == read {
        write = out.len();
    } else {
        while read < out.len() {
            out.swap(write, read);
            write += 1;
            read += 1;
        }
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
        || !number.is_uninflected()
        || input.get(..number.end) != Some(number.surface.as_ref())
        || input.get(suffix.start..) != Some(suffix.surface.as_ref())
    {
        return None;
    }
    let parsed = super::ascii::parse(&number.surface)?;
    let counter = super::counter(suffix);
    let unit_reading = reading(&suffix.surface);
    if let Some(unit) = unit_reading {
        let pronunciation = area_readings(&suffix.surface).map_or(unit, |(_, p)| p);
        if suffix.reading.as_ref() != unit
            || suffix.pronunciation.as_ref() != pronunciation
            || !(suffix.base_form == suffix.surface
                || (suffix.surface.is_ascii()
                    && suffix.base_form.is_ascii()
                    && suffix.surface.eq_ignore_ascii_case(&suffix.base_form)))
            || !suffix.is_uninflected()
        {
            return None;
        }
    } else if !(suffix.is_known_base_form() && counter.is_some()) {
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

    fn unknown_power_unit(offset: usize) -> Vec<Token> {
        let mut number = token("75", offset, "ナナジュウゴ");
        number.pos = "名詞,数,*,*".into();
        number.pronunciation = "ナナジューゴ".into();
        let mut unit = token("kW", offset + 2, "");
        unit.pos = "名詞,固有名詞,組織,*".into();
        unit.is_known = false;
        vec![number, unit]
    }

    #[test]
    fn unknown_ascii_unit_changes_only_organization_pos() {
        let text = "75kW";
        let mut out = unknown_power_unit(31);
        let prior = token("前", 0, "マエ");
        out.insert(0, prior);
        let mut expected = out.clone();
        expected[2].pos = "名詞,接尾,助数詞,*".into();
        coalesce_units(
            text,
            31,
            &mut out,
            1,
            &super::super::AsciiPlan::new(text, 31),
        );
        assert_eq!(format!("{out:?}"), format!("{expected:?}"));
    }

    #[test]
    fn unknown_unit_pos_preserves_nonstandard_fields_and_known_units() {
        for mutation in 0..9 {
            let mut out = unknown_power_unit(31);
            match mutation {
                0 => out[1].is_known = true,
                1 => out[1].reading = "カスタム".into(),
                2 => out[1].base_form = "別の原形".into(),
                3 => out[1].conj_type = "custom".into(),
                4 => out[1].pos = "名詞,固有名詞,人名,姓".into(),
                5 => out[0].reading = "ナナゴ".into(),
                6 => out[0].base_form = "76".into(),
                7 => out[1].start += 1,
                8 => out[1].pos = "名詞,一般,*,*".into(),
                _ => unreachable!(),
            }
            let before = format!("{out:?}");
            coalesce_units(
                "75kW",
                31,
                &mut out,
                0,
                &super::super::AsciiPlan::new("75kW", 31),
            );
            assert_eq!(format!("{out:?}"), before, "mutation {mutation}");
        }
    }

    #[test]
    fn unknown_unit_pos_uses_full_input_identifier_guards_across_chunks() {
        for (full, offset) in [
            ("ID75kW", 2),
            ("型番75kW", 6),
            ("コード75kW", 9),
            ("型番(75kW)", 7),
            ("第75kW", 3),
            ("075kW", 1),
            ("75kWX", 0),
            ("75kW_2", 0),
            ("75kWＡ", 0),
            ("75kW－2", 0),
            ("75kW＿2", 0),
            ("75kW．2", 0),
        ] {
            let mut out = unknown_power_unit(offset);
            let before = format!("{out:?}");
            coalesce_units(
                "75kW",
                offset,
                &mut out,
                0,
                &super::super::AsciiPlan::new(full, 0),
            );
            assert_eq!(format!("{out:?}"), before, "{full}");
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

    #[test]
    fn concentration_frames_keep_identifiers_widths_and_partial_chunks_outside_the_frame() {
        for text in ["250mg/dL", "5.5mg/dL", "1,250mg/dL", "値は84mg/dLです。"] {
            let plan = super::super::AsciiPlan::new(text, 47);
            let frames = plan.unit_frames_in(47, 47 + text.len());
            assert_eq!(frames.len(), 1, "{text}");
            assert_eq!(frames[0].reading, "ミリグラムマイデシリットル");
            assert!(
                plan.unit_frames_in(frames[0].number.start + 1, 47 + text.len())
                    .is_empty()
            );
            assert!(plan.unit_frames_in(47, frames[0].end - 1).is_empty());
        }
        for text in [
            "ID250mg/dL",
            "ISO 250mg/dL",
            "型番250mg/dL",
            "250mg/dLX",
            "250mg/dL2",
            "250mg/dL_1",
            "250mg/dLＡ",
            "250mg/dL２",
            "https://example.invalid/250mg/dL",
            "x=250mg/dL",
            "250mg/dL+5",
            "250mg/dL/2",
            "250mg/dL*2",
            "250mg/dL²",
            "250mg/dL×2",
            "250mg/dL＝2",
            "250mg/dL＿A",
            "250mg/dL．5",
            "250mg/dL—2",
            "２５０mg/dL",
            "250ｍｇ/dL",
            "250mg／dL",
            "250mg/ｄＬ",
            "0250mg/dL",
            "250MG/DL",
            "250mg/dl",
            "250mgdL",
            "mg/dL",
            "ID,250mg/dL",
            "ISO ,250mg/dL",
            "250mg / dL",
        ] {
            let plan = super::super::AsciiPlan::new(text, 47);
            assert!(
                plan.unit_frames_in(47, 47 + text.len())
                    .iter()
                    .all(|frame| frame.reading != "ミリグラムマイデシリットル"),
                "{text}"
            );
        }
        assert_eq!(reading("dL"), None);
    }

    fn concentration_tokens(offset: usize) -> Vec<Token> {
        let mut number = token("250", offset, "ニヒャクゴジュウ");
        number.pos = "名詞,数,*,*".into();
        number.pronunciation = "ニヒャクゴジュー".into();
        number.word_cost = 612;
        let mut mg = token("mg", offset + 3, "エムジー");
        mg.base_form = "MG".into();
        mg.pos = "名詞,固有名詞,人名,一般".into();
        mg.word_cost = 7587;
        let mut slash = token("/", offset + 5, "/");
        slash.pos = "記号,一般,*,*".into();
        slash.word_cost = 1758;
        let mut dl = token("dL", offset + 6, "");
        dl.is_known = false;
        dl.pos = "名詞,固有名詞,組織,*".into();
        dl.word_cost = 13835;
        vec![number, mg, slash, dl]
    }

    #[test]
    fn concentration_compaction_preserves_number_prior_output_and_sums_unit_costs_once() {
        let input = "250mg/dL";
        let plan = super::super::AsciiPlan::new(input, 47);
        let mut out = vec![token("前", 0, "マエ")];
        out.extend(concentration_tokens(47));
        let before = out.clone();
        coalesce_units(input, 47, &mut out, 1, &plan);
        assert_eq!(out.len(), 3);
        assert_eq!(format!("{:?}", &out[..2]), format!("{:?}", &before[..2]));
        let unit = &out[2];
        assert_eq!(unit.surface.as_ref(), "mg/dL");
        assert_eq!(unit.base_form, unit.surface);
        assert_eq!((unit.start, unit.end), (50, 55));
        assert_eq!(unit.pos.as_ref(), "名詞,接尾,助数詞,*");
        assert_eq!(unit.conj_type.as_ref(), "");
        assert_eq!(unit.conj_form.as_ref(), "");
        assert_eq!(unit.reading.as_ref(), "ミリグラムマイデシリットル");
        assert_eq!(unit.pronunciation, unit.reading);
        assert_eq!(unit.word_cost, 7587 + 1758 + 13835);
        assert!(!unit.is_known);
        let mut high = concentration_tokens(47);
        high[1..].iter_mut().for_each(|t| t.word_cost = 20000);
        coalesce_units(input, 47, &mut high, 0, &plan);
        assert_eq!(high[0].word_cost, 612);
        assert_eq!(high[1].word_cost, i16::MAX);
        let mut wide = concentration_tokens(47);
        for (t, cost) in wide[1..].iter_mut().zip([30000, 30000, -30000]) {
            t.word_cost = cost;
        }
        coalesce_units(input, 47, &mut wide, 0, &plan);
        assert_eq!(wide[1].word_cost, 30000);
        let mut low = concentration_tokens(47);
        low[1..].iter_mut().for_each(|t| t.word_cost = -20000);
        coalesce_units(input, 47, &mut low, 0, &plan);
        assert_eq!(low[1].word_cost, i16::MIN);
    }

    #[test]
    fn concentration_alias_is_local_and_custom_features_keep_every_field() {
        let input = "250mg/dL";
        let plan = super::super::AsciiPlan::new(input, 47);
        let baseline = concentration_tokens(47);
        assert!(!unit_piece(&baseline[1]));
        assert!(!concentration_unit_piece(&baseline[1], 51));
        for mutation in 0..15 {
            let mut out = baseline.clone();
            match mutation {
                0 => out[1].base_form = "CUSTOM".into(),
                1 => out[1].pos = "名詞,固有名詞,人名,姓".into(),
                2 => out[1].pos = "名詞,固有名詞,地域,一般".into(),
                3 => out[1].pos = "名詞,固有名詞,組織,*".into(),
                4 => out[1].conj_type = "カスタム".into(),
                5 => out[1].conj_form = "カスタム".into(),
                6 => out[1].reading = "カスタム".into(),
                7 => out[1].pronunciation = "カスタム".into(),
                8 => out[1].reading = "".into(),
                9 => out[1].pronunciation = "".into(),
                10 => out[1].is_known = false,
                11 => out[2].reading = "ワル".into(),
                12 => out[3].reading = "カスタム".into(),
                13 => out[3].base_form = "CUSTOM".into(),
                14 => out[1].pronunciation = "ミリグラム".into(),
                _ => unreachable!(),
            }
            let before = format!("{out:?}");
            coalesce_units(input, 47, &mut out, 0, &plan);
            assert_eq!(format!("{out:?}"), before, "mutation {mutation}");
        }
        for partial in [0..7, 1..8] {
            let mut out = baseline.clone();
            let before = format!("{out:?}");
            coalesce_units(
                &input[partial.clone()],
                47 + partial.start,
                &mut out,
                0,
                &plan,
            );
            assert_eq!(format!("{out:?}"), before);
        }
        let mut complete_unit = vec![baseline[0].clone(), token("mg/dL", 50, "カスタム")];
        let before = format!("{complete_unit:?}");
        coalesce_units(input, 47, &mut complete_unit, 0, &plan);
        assert_eq!(format!("{complete_unit:?}"), before);
        let mut whole = vec![token(input, 47, "カスタム")];
        let before = format!("{whole:?}");
        coalesce_units(input, 47, &mut whole, 0, &plan);
        assert_eq!(format!("{whole:?}"), before);
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

    #[test]
    fn horsepower_keeps_scalar_reading_and_selected_unit_metadata() {
        let build = |whole: bool| {
            let mut b = DictBuilder::new();
            for (surface, pos, base, r, cost) in [
                ("1", "名詞,数,*,*", "1", "イチ", 102),
                ("2", "名詞,数,*,*", "2", "ニ", 103),
                ("3", "名詞,数,*,*", "3", "サン", 104),
                (".", "記号,一般,*,*", ".", "．", 205),
                ("hp", "名詞,固有名詞,一般,*", "HP", "エイチピー", 206),
            ] {
                b.add_entry(DictEntry {
                    surface: surface.into(),
                    base_form: base.into(),
                    pos: pos.into(),
                    reading: r.into(),
                    pronunciation: r.into(),
                    cost,
                    ..Default::default()
                });
            }
            if whole {
                b.add_entry(DictEntry {
                    surface: "12hp".into(),
                    base_form: "custom-base".into(),
                    pos: "名詞,固有名詞,一般,*".into(),
                    reading: "カスタム".into(),
                    pronunciation: "カスタム".into(),
                    cost: -20000,
                    ..Default::default()
                });
            }
            Analyzer::from_dict(b.build().unwrap())
        };
        let mut a = build(false);
        for (text, number, r, p, cost) in [
            ("12hp", "12", "ジュウニ", "ジューニ", 102 + 103),
            ("2.3hp", "2.3", "ニテンサン", "ニテンサン", 103 + 205 + 104),
        ] {
            let tokens = a.tokenize(text);
            assert_eq!(tokens.len(), 2, "{text}: {tokens:?}");
            assert_eq!(tokens[0].surface.as_ref(), number);
            assert_eq!(tokens[0].reading.as_ref(), r);
            assert_eq!(tokens[0].pronunciation.as_ref(), p);
            assert_eq!(tokens[0].word_cost, cost);
            assert!(!tokens[0].is_known);
            assert_eq!(tokens[1].surface.as_ref(), "hp");
            assert_eq!(tokens[1].base_form.as_ref(), "HP");
            assert_eq!(tokens[1].reading.as_ref(), "バリキ");
            assert_eq!(tokens[1].pronunciation.as_ref(), "バリキ");
            assert_eq!(tokens[1].word_cost, 206);
            assert!(tokens[1].is_known);
            assert_eq!(tokens[1].pos.as_ref(), "名詞,固有名詞,一般,*");
            assert_eq!((tokens[0].start, tokens[0].end), (0, number.len()));
            assert_eq!((tokens[1].start, tokens[1].end), (number.len(), text.len()));
        }
        let tokens = a.tokenize("hp");
        assert_eq!(tokens[0].pronunciation.as_ref(), "エイチピー");
        let mut a = build(true);
        let tokens = a.tokenize("12hp");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].base_form.as_ref(), "custom-base");
        assert_eq!(tokens[0].pronunciation.as_ref(), "カスタム");
        assert_eq!(tokens[0].word_cost, -20000);
    }

    #[test]
    fn concentration_unit_reuses_complete_scalar_compaction_and_keeps_known_wholes() {
        let build = |whole: bool, custom_mg: bool| {
            let mut b = DictBuilder::new();
            for (i, r) in [
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
            ]
            .iter()
            .enumerate()
            {
                let surface = i.to_string();
                b.add_entry(DictEntry {
                    surface: surface.clone().into(),
                    base_form: surface.into(),
                    pos: "名詞,数,*,*".into(),
                    reading: (*r).into(),
                    pronunciation: (*r).into(),
                    cost: 10 + i as i16,
                    ..Default::default()
                });
            }
            for (surface, pos, base, r, cost) in [
                (".", "記号,一般,*,*", ".", "．", 99),
                (",", "記号,読点,*,*", ",", "，", 101),
                (
                    "mg",
                    "名詞,固有名詞,人名,一般",
                    "MG",
                    if custom_mg {
                        "カスタム"
                    } else {
                        "エムジー"
                    },
                    700,
                ),
                ("/", "記号,一般,*,*", "/", "/", 200),
                (
                    "5.5m",
                    "名詞,固有名詞,一般,*",
                    "5.5m",
                    "ゴテンゴメートル",
                    -10000,
                ),
            ] {
                b.add_entry(DictEntry {
                    surface: surface.into(),
                    base_form: base.into(),
                    pos: pos.into(),
                    reading: r.into(),
                    pronunciation: r.into(),
                    cost,
                    ..Default::default()
                });
            }
            if whole {
                b.add_entry(DictEntry {
                    surface: "250mg/dL".into(),
                    base_form: "custom-base".into(),
                    pos: "名詞,固有名詞,一般,*".into(),
                    reading: "カスタム".into(),
                    pronunciation: "カスタム".into(),
                    cost: -20000,
                    ..Default::default()
                });
            }
            Analyzer::from_dict(b.build().unwrap())
        };
        let mut a = build(false, false);
        for (text, raw, r, p, cost) in [
            (
                "250mg/dL",
                "250",
                "ニヒャクゴジュウ",
                "ニヒャクゴジュー",
                12 + 15 + 10,
            ),
            ("5.5mg/dL", "5.5", "ゴテンゴ", "ゴテンゴ", 15 + 99 + 15),
            (
                "1,250mg/dL",
                "1,250",
                "センニヒャクゴジュウ",
                "センニヒャクゴジュー",
                11 + 101 + 12 + 15 + 10,
            ),
        ] {
            let tokens = a.tokenize(text);
            assert_eq!(tokens.len(), 2, "{text}: {tokens:?}");
            let number = &tokens[0];
            assert_eq!(number.surface.as_ref(), raw);
            assert_eq!(number.base_form.as_ref(), raw);
            assert_eq!(number.pos.as_ref(), "名詞,数,*,*");
            assert!(number.conj_type.is_empty() && number.conj_form.is_empty());
            assert_eq!(number.reading.as_ref(), r);
            assert_eq!(number.pronunciation.as_ref(), p);
            assert_eq!(number.word_cost, cost);
            assert!(!number.is_known);
            assert_eq!((number.start, number.end), (0, raw.len()));
            let unit = &tokens[1];
            assert_eq!(unit.surface.as_ref(), "mg/dL");
            assert_eq!(unit.base_form, unit.surface);
            assert_eq!(unit.pos.as_ref(), "名詞,接尾,助数詞,*");
            assert!(unit.conj_type.is_empty() && unit.conj_form.is_empty());
            assert_eq!(unit.reading.as_ref(), "ミリグラムマイデシリットル");
            assert_eq!(unit.pronunciation, unit.reading);
            assert!(!unit.is_known);
            assert_eq!((unit.start, unit.end), (raw.len(), text.len()));
            assert_eq!(
                a.quantity_pronunciation(text).unwrap(),
                Some(format!("{p}ミリグラムマイデシリットル"))
            );
        }
        let mut a = build(true, false);
        let tokens = a.tokenize("250mg/dL");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].base_form.as_ref(), "custom-base");
        assert_eq!(tokens[0].word_cost, -20000);
        assert_eq!(tokens[0].pronunciation.as_ref(), "カスタム");
        assert_eq!(a.quantity_pronunciation("250mg/dL").unwrap(), None);
        let mut a = build(false, true);
        let tokens = a.tokenize("250mg/dL");
        assert!(tokens.iter().any(|t| t.surface.as_ref() == "mg"));
        assert!(tokens.iter().all(|t| t.surface.as_ref() != "mg/dL"));
        assert_eq!(a.quantity_pronunciation("250mg/dL").unwrap(), None);
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
            ("mg/dL", "ｍｇ/ｄＬ", "mｇ/dL"),
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

#[cfg(test)]
mod core_c_ascii_base_alias_tests {
    use super::*;

    fn token(s: &str, start: usize, pos: &str, r: &str, p: &str) -> Token {
        Token {
            surface: s.into(),
            base_form: s.into(),
            start,
            end: start + s.len(),
            pos: pos.into(),
            conj_type: "".into(),
            conj_form: "".into(),
            reading: r.into(),
            pronunciation: p.into(),
            word_cost: 345,
            is_known: true,
        }
    }

    fn pair(n: &str, r: &str, p: &str, unit: &str, base: &str, ur: &str) -> Vec<Token> {
        let mut suffix = token(unit, n.len(), "名詞,固有名詞,組織,*", ur, ur);
        suffix.base_form = base.into();
        vec![token(n, 0, "名詞,数,*,*", r, p), suffix]
    }

    fn assert_selector(input: &str, tokens: &[Token], expected: Option<&str>) {
        let before = format!("{tokens:?}");
        assert_eq!(
            quantity_pronunciation(input, tokens).as_deref(),
            expected,
            "{input}"
        );
        assert_eq!(
            format!("{tokens:?}"),
            before,
            "selector changed Token11: {input}"
        );
    }

    #[test]
    fn getter_accepts_only_case_aliases_with_already_complete_unit_readings() {
        for (n, r, p, u, base, ur, expected) in [
            (
                "12",
                "ジュウニ",
                "ジューニ",
                "hp",
                "HP",
                "バリキ",
                "ジューニバリキ",
            ),
            (
                "6.4",
                "ロクテンヨン",
                "ロクテンヨン",
                "hp",
                "HP",
                "バリキ",
                "ロクテンヨンバリキ",
            ),
            (
                "2,800",
                "ニセンハッピャク",
                "ニセンハッピャク",
                "rpm",
                "RPM",
                "アールピーエム",
                "ニセンハッピャクアールピーエム",
            ),
            (
                "250",
                "ニヒャクゴジュウ",
                "ニヒャクゴジュー",
                "mg",
                "MG",
                "ミリグラム",
                "ニヒャクゴジューミリグラム",
            ),
            (
                "12",
                "ジュウニ",
                "ジューニ",
                "mL",
                "ML",
                "ミリリットル",
                "ジューニミリリットル",
            ),
            ("6", "ロク", "ロク", "bit", "BIT", "ビット", "ロクビット"),
            (
                "6",
                "ロッ",
                "ロッ",
                "kg",
                "KG",
                "キログラム",
                "ロッキログラム",
            ),
        ] {
            let input = format!("{n}{u}");
            let ts = pair(n, r, p, u, base, ur);
            assert_selector(&input, &ts, Some(expected));
            let mut exact = ts.clone();
            exact[1].base_form = exact[1].surface.clone();
            assert_selector(&input, &exact, Some(expected));
        }
        let mut mg = pair(
            "250",
            "ニヒャクゴジュウ",
            "ニヒャクゴジュー",
            "mg",
            "MG",
            "ミリグラム",
        );
        mg[1].pos = "名詞,固有名詞,人名,一般".into();
        assert_selector("250mg", &mg, Some("ニヒャクゴジューミリグラム"));
        let bad_kg = pair("6", "ロク", "ロク", "kg", "KG", "キログラム");
        assert_selector("6kg", &bad_kg, None);
    }

    #[test]
    fn getter_does_not_approve_custom_bases_readings_or_noncanonical_scalars() {
        for defect in 0..16 {
            let mut ts = pair("12", "ジュウニ", "ジューニ", "hp", "HP", "バリキ");
            match defect {
                0 => ts[1].base_form = "custom-base".into(),
                1 => ts[1].base_form = "H-P".into(),
                2 => ts[1].base_form = "ＨＰ".into(),
                3 => ts[1].base_form = "hp1".into(),
                4 => ts[1].reading = "エイチピー".into(),
                5 => ts[1].pronunciation = "エイチピー".into(),
                6 => ts[1].reading = "".into(),
                7 => ts[1].pronunciation = "".into(),
                8 => ts[1].conj_type = "custom-type".into(),
                9 => ts[1].conj_form = "基本形".into(),
                10 => ts[0].base_form = "custom-number".into(),
                11 => ts[0].reading = "ワンツー".into(),
                12 => ts[0].pronunciation = "ワンツー".into(),
                13 => ts[0].conj_type = "custom-type".into(),
                14 => ts[0].pos = "名詞,固有名詞,一般,*".into(),
                _ => ts[1].start += 1,
            }
            assert_selector("12hp", &ts, None);
        }
        for n in ["012", "1,20", "1.2.3"] {
            let ts = pair(n, "ジュウニ", "ジューニ", "hp", "HP", "バリキ");
            assert_selector(&format!("{n}hp"), &ts, None);
        }
        let ts = pair("12", "ジュウニ", "ジューニ", "hp", "HP", "バリキ");
        for input in ["ID12hp", "12hpX", "12 hp", "１２hp"] {
            assert_selector(input, &ts, None);
        }
        assert_selector("12hp", &ts[..1], None);
        // 全体が既知の一語なら、標準表記でも数詞と単位の組として認可しない。
        let whole = token("12hp", 0, "名詞,固有名詞,一般,*", "カスタム", "カスタム");
        assert_selector("12hp", &[whole], None);
    }
}

#[cfg(test)]
mod kcal_per_hour_frame_tests {
    use super::*;

    #[test]
    fn kcal_per_hour_requires_a_complete_ascii_scalar_and_right_boundary() {
        assert_eq!(reading("kcal/h"), Some("キロカロリーマイジ"));
        for text in ["720kcal/h", "38,000kcal/h", "0.75kcal/h"] {
            let plan = super::super::AsciiPlan::new(text, 31);
            let frames = plan.unit_frames_in(31, 31 + text.len());
            assert_eq!(frames.len(), 1, "{text}");
            let end = text.len() - "kcal/h".len();
            assert_eq!(frames[0].number, 31..31 + end);
            assert_eq!(frames[0].end, 31 + text.len());
            assert_eq!(frames[0].reading, "キロカロリーマイジ");
            assert!(plan.unit_frames_in(32, 31 + text.len()).is_empty());
            assert!(plan.unit_frames_in(31, 30 + text.len()).is_empty());
        }
        for text in [
            "ID720kcal/h",
            "ISO 720kcal/h",
            "型番720kcal/h",
            "コード720kcal/h",
            "https://example/720kcal/h",
            "720kcal/hX",
            "720kcal/h_1",
            "720kcal/h/ID",
            "720kcal/h+1",
            "720kcal/h=1",
            "720kcal/hＡ",
            "720kcal/h１",
            "720kcal/h＋1",
            "720kcal/h／x",
            "720kcal/h×2",
            "720kcal/h－1",
            "0720kcal/h",
            "7,20kcal/h",
            "7.2.0kcal/h",
            "７２０kcal/h",
            "7２0kcal/h",
            "720ｋｃａｌ/ｈ",
            "720KCAL/H",
            "720kcal/hr",
            "720kcal/hz",
        ] {
            let plan = super::super::AsciiPlan::new(text, 0);
            assert!(plan.unit_frames_in(0, text.len()).is_empty(), "{text}");
        }
        for unit in ["ｋｃａｌ/ｈ", "kｃal/h", "kcal/ｈ"] {
            assert!(requires_frame(unit));
            assert_eq!(reading(unit), None);
        }
        assert_eq!(reading("kcal"), Some("キロカロリー"));
        assert_eq!(reading("km/h"), Some("キロメートルマイジ"));
        assert_eq!(reading("mg/dL"), Some("ミリグラムマイデシリットル"));
        assert_eq!(reading("hp"), Some("バリキ"));
    }

    fn token(s: &str, start: usize, pos: &str, r: &str, cost: i16) -> Token {
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
            word_cost: cost,
            is_known: true,
        }
    }

    fn parts() -> Vec<Token> {
        vec![
            token("7", 31, "名詞,数,*,*", "ナナ", 17),
            token("2", 32, "名詞,数,*,*", "ニ", 12),
            token("0", 33, "名詞,数,*,*", "ゼロ", 10),
            token("kcal", 34, "名詞,固有名詞,一般,*", "キロカロリー", 400),
            token("/", 38, "記号,一般,*,*", "/", 200),
            token("h", 39, "記号,アルファベット,*,*", "エイチ", 300),
        ]
    }

    #[test]
    fn kcal_compaction_keeps_number_fields_and_sums_only_approved_unit_costs() {
        let plan = super::super::AsciiPlan::new("720kcal/h", 31);
        let mut out = parts();
        let number_before = format!("{:?}", &out[..3]);
        coalesce_units("720kcal/h", 31, &mut out, 0, &plan);
        assert_eq!(out.len(), 4);
        assert_eq!(format!("{:?}", &out[..3]), number_before);
        let suffix = &out[3];
        assert_eq!(suffix.surface.as_ref(), "kcal/h");
        assert_eq!(suffix.base_form, suffix.surface);
        assert_eq!((suffix.start, suffix.end), (34, 40));
        assert_eq!(suffix.pos.as_ref(), "名詞,接尾,助数詞,*");
        assert!(suffix.conj_type.is_empty() && suffix.conj_form.is_empty());
        assert_eq!(suffix.reading.as_ref(), "キロカロリーマイジ");
        assert_eq!(suffix.pronunciation, suffix.reading);
        assert_eq!(suffix.word_cost, 900);
        assert!(!suffix.is_known);
        for defect in 0..10 {
            let mut out = parts();
            match defect {
                0 => out[3].base_form = "KCAL".into(),
                1 => out[3].reading = "カスタム".into(),
                2 => out[3].pronunciation = "カスタム".into(),
                3 => out[3].pos = "名詞,固有名詞,人名,一般".into(),
                4 => out[3].conj_type = "custom-type".into(),
                5 => out[5].pronunciation = "カスタム".into(),
                6 => {
                    out[1].start += 1;
                    out[1].end += 1;
                }
                7 => out[0].surface = "8".into(),
                8 => out[0].base_form = "custom-number".into(),
                _ => out[0].conj_form = "custom-form".into(),
            }
            let before = format!("{out:?}");
            coalesce_units("720kcal/h", 31, &mut out, 0, &plan);
            assert_eq!(format!("{out:?}"), before, "defect {defect}");
        }
        let mut out = parts();
        for t in &mut out[3..] {
            t.word_cost = 20000;
        }
        coalesce_units("720kcal/h", 31, &mut out, 0, &plan);
        assert_eq!(out[3].word_cost, i16::MAX);
        assert_eq!(format!("{:?}", &out[..3]), number_before);
    }
}

#[cfg(all(test, feature = "build"))]
mod kcal_per_hour_analyzer_tests {
    use crate::Analyzer;
    use crate::dict::{DictBuilder, DictEntry};

    fn analyzer(whole: Option<&str>, custom_unit: bool) -> Analyzer {
        let mut b = DictBuilder::new();
        for (i, r) in [
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
        ]
        .iter()
        .enumerate()
        {
            let s = i.to_string();
            b.add_entry(DictEntry {
                surface: s.clone().into(),
                base_form: s.into(),
                pos: "名詞,数,*,*".into(),
                reading: (*r).into(),
                pronunciation: (*r).into(),
                cost: 10 + i as i16,
                ..Default::default()
            });
        }
        for (s, pos, r, cost) in [
            (".", "記号,一般,*,*", "．", 99),
            (",", "記号,読点,*,*", "，", 101),
            (
                "kcal",
                "名詞,固有名詞,一般,*",
                if custom_unit {
                    "カスタム"
                } else {
                    "キロカロリー"
                },
                400,
            ),
            ("/", "記号,一般,*,*", "/", 200),
            ("h", "記号,アルファベット,*,*", "エイチ", 300),
        ] {
            b.add_entry(DictEntry {
                surface: s.into(),
                base_form: s.into(),
                pos: pos.into(),
                reading: r.into(),
                pronunciation: r.into(),
                cost,
                ..Default::default()
            });
        }
        if let Some(s) = whole {
            b.add_entry(DictEntry {
                surface: s.into(),
                base_form: "custom-whole".into(),
                pos: "名詞,固有名詞,一般,*".into(),
                reading: "カスタム".into(),
                pronunciation: "カスタム".into(),
                cost: -20000,
                ..Default::default()
            });
        }
        Analyzer::from_dict(b.build().unwrap())
    }

    #[test]
    fn complete_kcal_per_hour_quantities_reuse_num_cost_and_offsets() {
        let mut a = analyzer(None, false);
        for (text, raw, r, p, cost) in [
            (
                "720kcal/h",
                "720",
                "ナナヒャクニジュウ",
                "ナナヒャクニジュー",
                39,
            ),
            (
                "38,000kcal/h",
                "38,000",
                "サンマンハッセン",
                "サンマンハッセン",
                162,
            ),
            (
                "0.75kcal/h",
                "0.75",
                "ゼロテンナナゴ",
                "ゼロテンナナゴ",
                141,
            ),
        ] {
            let out = a.tokenize(text);
            assert_eq!(out.len(), 2, "{text}: {out:?}");
            let n = &out[0];
            let u = &out[1];
            assert_eq!(n.surface.as_ref(), raw);
            assert_eq!(n.base_form, n.surface);
            assert_eq!(n.pos.as_ref(), "名詞,数,*,*");
            assert!(n.conj_type.is_empty() && n.conj_form.is_empty());
            assert_eq!((n.reading.as_ref(), n.pronunciation.as_ref()), (r, p));
            assert_eq!(n.word_cost, cost);
            assert!(!n.is_known);
            assert_eq!((n.start, n.end), (0, raw.len()));
            assert_eq!(u.surface.as_ref(), "kcal/h");
            assert_eq!(u.base_form, u.surface);
            assert_eq!(u.pos.as_ref(), "名詞,接尾,助数詞,*");
            assert!(u.conj_type.is_empty() && u.conj_form.is_empty());
            assert_eq!(u.reading.as_ref(), "キロカロリーマイジ");
            assert_eq!(u.pronunciation, u.reading);
            assert_eq!(u.word_cost, 900);
            assert!(!u.is_known);
            assert_eq!((u.start, u.end), (raw.len(), text.len()));
            let before = format!("{out:?}");
            assert_eq!(
                a.quantity_pronunciation(text).unwrap(),
                Some(format!("{p}キロカロリーマイジ"))
            );
            assert_eq!(format!("{out:?}"), before);
        }
        for text in [
            "ID720kcal/h",
            "720kcal/hＡ",
            "720kcal/h＋1",
            "0720kcal/h",
            "７２０kcal/h",
        ] {
            let out = a.tokenize(text);
            assert!(!out.iter().any(|t| t.surface.as_ref() == "kcal/h"));
            assert_eq!(a.quantity_pronunciation(text).unwrap(), None);
        }
    }

    #[test]
    fn kcal_frames_keep_known_wholes_beyond_frame_and_custom_unit_readings() {
        for text in ["720kcal/h", "720kcal/h装置"] {
            let mut a = analyzer(Some(text), false);
            let out = a.tokenize(text);
            assert_eq!(out.len(), 1, "{out:?}");
            let t = &out[0];
            assert_eq!(t.surface.as_ref(), text);
            assert_eq!((t.start, t.end), (0, text.len()));
            assert_eq!(t.base_form.as_ref(), "custom-whole");
            assert_eq!(t.pos.as_ref(), "名詞,固有名詞,一般,*");
            assert!(t.conj_type.is_empty() && t.conj_form.is_empty());
            assert_eq!(t.reading.as_ref(), "カスタム");
            assert_eq!(t.pronunciation, t.reading);
            assert_eq!(t.word_cost, -20000);
            assert!(t.is_known);
            assert_eq!(a.quantity_pronunciation(text).unwrap(), None);
        }
        let mut a = analyzer(None, true);
        let out = a.tokenize("720kcal/h");
        assert!(
            out.iter()
                .any(|t| t.surface.as_ref() == "kcal" && t.pronunciation.as_ref() == "カスタム")
        );
        assert!(!out.iter().any(|t| t.surface.as_ref() == "kcal/h"));
        assert_eq!(a.quantity_pronunciation("720kcal/h").unwrap(), None);
    }
}
