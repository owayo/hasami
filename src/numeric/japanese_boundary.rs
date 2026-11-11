//! 和文commaを数量の区切りとみなす条件。英字IDや数字列を句読点として分断しない。
pub(super) fn comma_prefix(prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches(char::is_whitespace);
    let Some(before) = prefix.strip_suffix(',') else {
        return false;
    };
    before.trim_end().chars().next_back().is_some_and(|c| {
        matches!(c,'ぁ'..='ゖ'|'ァ'..='ヺ'|'\u{3400}'..='\u{9fff}')
            && !matches!(
                c,
                '〇' | '零'
                    | '一'
                    | '二'
                    | '三'
                    | '四'
                    | '五'
                    | '六'
                    | '七'
                    | '八'
                    | '九'
                    | '十'
                    | '百'
                    | '千'
                    | '万'
                    | '億'
                    | '兆'
            )
    }) && ![
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
    .any(|label| before.trim_end().ends_with(label))
}
pub(super) fn single_year_tail(rest: &str) -> bool {
    let Some(after) = rest.strip_prefix('年') else {
        return false;
    };
    year_suffix_boundary(after)
}

pub(super) fn year_suffix_boundary(after: &str) -> bool {
    !["度","次","間","生"].iter().any(|suffix|after.starts_with(suffix))
        && !after.chars().next().is_some_and(|c|c.is_ascii_alphanumeric()
            ||matches!(c,'Ａ'..='Ｚ'|'ａ'..='ｚ'|'０'..='９'|'_'|'^'|'/'|'*'|'+'|'-'|'='|'²'|'³'|'.'|'．'|'，'|':'|'：'|'／'|'∕'|'÷'|'×'|'＋'|'－'|'−'|'‐'|'‑'|'‒'|'–'|'—'|'―'))
}

/// 年に続く月日だけを読みの補正に許す。識別子との境界の条件は単独の年と同じ。
pub(super) fn calendar_year_tail(rest: &str) -> bool {
    fn component(input: &str, suffix: char, max: u32) -> Option<&str> {
        let mut chars = input.char_indices();
        let (_, first) = chars.next()?;
        let base = if first.is_ascii_digit() {
            '0' as u32
        } else if matches!(first, '０'..='９') {
            '０' as u32
        } else {
            return None;
        };
        let mut value = first as u32 - base;
        for (i, c) in chars.take(2) {
            if c == suffix {
                return (1..=max)
                    .contains(&value)
                    .then_some(&input[i + c.len_utf8()..]);
            }
            let digit = (c as u32).checked_sub(base)?;
            if digit > 9 {
                return None;
            }
            value = value * 10 + digit;
        }
        None
    }

    let Some(after) = rest.strip_prefix('年').and_then(|s| component(s, '月', 12)) else {
        return false;
    };
    if after
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || matches!(c, '０'..='９'))
    {
        component(after, '日', 31).is_some_and(year_suffix_boundary)
    } else {
        year_suffix_boundary(after)
    }
}
