//! 読みの補完とモーラ数で共有する仮名の字幅・濁点の扱い。
//! 入力や辞書のキーは書き換えず、読みを作るときだけ使う。

/// ひらがな・半角カナを全角カタカナにする。仮名以外はそのまま。
pub fn to_katakana(c: char) -> char {
    const HALFWIDTH: &[char] = &[
        'ヲ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ', 'ッ', 'ー', 'ア', 'イ', 'ウ', 'エ',
        'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ', 'チ', 'ツ', 'テ',
        'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'マ', 'ミ', 'ム', 'メ',
        'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ', 'ン',
    ];
    match c {
        'ぁ'..='ゖ' | 'ゝ' | 'ゞ' => char::from_u32(c as u32 + 0x60).unwrap(),
        'ｦ'..='ﾝ' => HALFWIDTH[(c as u32 - 'ｦ' as u32) as usize],
        'ﾞ' => '\u{3099}',
        'ﾟ' => '\u{309A}',
        _ => c,
    }
}

/// 字幅・仮名の種類・結合する濁点だけをそろえる。
/// 長音・小書き仮名・大文字小文字・数字の読みは同一視しない。
pub fn reading_key(text: &str) -> String {
    let mut chars: Vec<char> = Vec::with_capacity(text.chars().count());
    for c in text.chars() {
        let c = to_katakana(c);
        if matches!(c, '\u{3099}' | '\u{309a}')
            && let Some(last) = chars.last_mut()
            && let Some(composed) = voice(*last, c == '\u{309a}')
        {
            *last = composed;
            continue;
        }
        chars.push(match c {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            '\u{3000}' => ' ',
            _ => c,
        });
    }
    chars.into_iter().collect()
}

/// 全角カタカナをひらがなにする。半角カナや仮名以外はそのまま。
pub fn to_hiragana(c: char) -> char {
    match c {
        'ァ'..='ヶ' | 'ヽ' | 'ヾ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

/// 直前の仮名と結合する濁点・半濁点（間隔を持つ「゛」「゜」は含めない）。
pub(crate) fn is_voicing_mark(c: char) -> bool {
    matches!(c, '\u{3099}' | '\u{309A}' | 'ﾞ' | 'ﾟ')
}

/// 全角カタカナに濁点・半濁点を付ける。合成できない組は None。
pub(crate) fn voice(c: char, semi: bool) -> Option<char> {
    if semi {
        return matches!(c, 'ハ' | 'ヒ' | 'フ' | 'ヘ' | 'ホ')
            .then(|| char::from_u32(c as u32 + 2).unwrap());
    }
    match c {
        'ウ' => Some('ヴ'),
        'カ' | 'キ' | 'ク' | 'ケ' | 'コ' | 'サ' | 'シ' | 'ス' | 'セ' | 'ソ' | 'タ' | 'チ'
        | 'ツ' | 'テ' | 'ト' | 'ハ' | 'ヒ' | 'フ' | 'ヘ' | 'ホ' => {
            char::from_u32(c as u32 + 1)
        }
        'ワ'..='ヲ' => char::from_u32(c as u32 + 8),
        _ => None,
    }
}

/// 踊り字「ヽ」「ヾ」が繰り返す、濁点・半濁点を除いた仮名。
pub(crate) fn unvoice(c: char) -> char {
    match c {
        'ヴ' => 'ウ',
        'ガ' | 'ギ' | 'グ' | 'ゲ' | 'ゴ' | 'ザ' | 'ジ' | 'ズ' | 'ゼ' | 'ゾ' | 'ダ' | 'ヂ'
        | 'ヅ' | 'デ' | 'ド' | 'バ' | 'ビ' | 'ブ' | 'ベ' | 'ボ' => {
            char::from_u32(c as u32 - 1).unwrap()
        }
        'パ' | 'ピ' | 'プ' | 'ペ' | 'ポ' => char::from_u32(c as u32 - 2).unwrap(),
        'ヷ'..='ヺ' => char::from_u32(c as u32 - 8).unwrap(),
        _ => c,
    }
}

#[cfg(test)]
mod ime_tests {
    use super::*;
    #[test]
    fn reading_equivalence_is_deliberately_narrow() {
        assert_eq!(reading_key("がっこう"), reading_key("ｶﾞｯｺｳ"));
        assert_eq!(reading_key("か\u{3099}"), reading_key("ガ"));
        assert_eq!(reading_key("ﾊﾟひ\u{309a}プ"), "パピプ");
        assert_eq!(reading_key("Ａ１　ｶﾅ"), "A1 カナ");
        assert_eq!(
            "カタカナヽヾ".chars().map(to_hiragana).collect::<String>(),
            "かたかなゝゞ"
        );
        assert_eq!(to_hiragana('ｶ'), 'ｶ');
        for (a, b) in [
            ("コー", "コウ"),
            ("きや", "きゃ"),
            ("は", "ば"),
            ("2", "に"),
            ("A", "a"),
        ] {
            assert_ne!(reading_key(a), reading_key(b));
        }
    }
}
