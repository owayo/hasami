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
    !["度","次","間","生"].iter().any(|suffix|after.starts_with(suffix))
        && !after.chars().next().is_some_and(|c|c.is_ascii_alphanumeric()
            ||matches!(c,'Ａ'..='Ｚ'|'ａ'..='ｚ'|'０'..='９'|'_'|'^'|'/'|'*'|'+'|'-'|'='|'²'|'³'|'.'|'．'|'，'|':'|'：'|'／'|'∕'|'÷'|'×'|'＋'|'－'|'−'|'‐'|'‑'|'‒'|'–'|'—'|'―'))
}
