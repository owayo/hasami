//! 有限節の列挙に限り、疑問・並立の助詞に続く「等」を副助詞へ補正する。
//!
//! 小学館デジタル大辞泉は「等（など）」を副助詞とし、引用句・文や一部の助詞を
//! 受ける用法を挙げる。名乗りの「ひとし」、接尾語の「とう・ら」は別の用法。
//! https://kotobank.jp/word/%E7%AD%89-579345
//!
//! IPAdic の既知・基本形と連続した表層だけを照合する。人名・独自読み・保護範囲を
//! 広く書き換えず、列挙が明確な形だけ品詞・読み・発音を直す。単語コストは元の
//! Viterbi 候補の値なので維持する。

use super::Token;
use std::{ops::Range, sync::Arc};
fn fixed(t: &Token, surface: &str, pos: &str, reading: &str) -> bool {
    t.is_known
        && &*t.surface == surface
        && &*t.base_form == surface
        && &*t.pos == pos
        && &*t.reading == reading
        && &*t.pronunciation == reading
        && t.conj_type.is_empty()
        && t.conj_form.is_empty()
}
fn finite(t: &Token) -> bool {
    t.is_known
        && t.conj_form.as_ref() == "基本形"
        && !t.conj_type.is_empty()
        && !t.base_form.is_empty()
        && !t.reading.is_empty()
        && !t.pronunciation.is_empty()
        && ["動詞,", "形容詞,", "助動詞,"]
            .iter()
            .any(|p| t.pos.starts_with(p))
}
fn ka(t: &Token) -> bool {
    fixed(t, "か", "助詞,副助詞／並立助詞／終助詞,*,*", "カ")
}
fn toka(t: &Token) -> bool {
    fixed(t, "とか", "助詞,並立助詞,*,*", "トカ")
}
fn allowed_right(t: Option<&Token>) -> bool {
    t.is_none_or(|t| {
        t.is_known
            && (["記号,読点,", "記号,句点,", "記号,括弧閉,"]
                .iter()
                .any(|p| t.pos.starts_with(p))
                || fixed(t, "も", "助詞,係助詞,*,*", "モ"))
    })
}
pub(super) fn apply(tokens: &mut [Token], protected: &[Range<usize>]) {
    for i in 0..tokens.len() {
        if i == 0
            || !fixed(&tokens[i], "等", "名詞,固有名詞,人名,名", "ヒトシ")
            || !allowed_right(tokens.get(i + 1))
        {
            continue;
        }
        let mut j = i - 1;
        if toka(&tokens[j]) {
            let Some(previous) = j.checked_sub(1) else {
                continue;
            };
            j = previous;
            if ka(&tokens[j]) {
                let Some(previous) = j.checked_sub(1) else {
                    continue;
                };
                j = previous;
            }
        } else if ka(&tokens[j]) {
            let Some(previous) = j.checked_sub(1) else {
                continue;
            };
            j = previous;
        } else {
            continue;
        }
        if fixed(&tokens[j], "の", "名詞,非自立,一般,*", "ノ") {
            let Some(previous) = j.checked_sub(1) else {
                continue;
            };
            j = previous;
        }
        if !finite(&tokens[j]) || !tokens[j..=i].windows(2).all(|w| w[0].end == w[1].start) {
            continue;
        }
        if j > 0
            && tokens[j - 1].end == tokens[j].start
            && tokens[j - 1]
                .surface
                .chars()
                .any(|c| c.is_ascii_alphanumeric())
        {
            continue;
        }
        let frame = tokens[j].start..tokens[i].end;
        if protected
            .iter()
            .any(|r| r.start < frame.end && frame.start < r.end)
        {
            continue;
        }
        tokens[i].pos = Arc::from("助詞,副助詞,*,*");
        tokens[i].reading = Arc::from("ナド");
        tokens[i].pronunciation = Arc::from("ナド");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(surface: &str, pos: &str, base: &str, kind: &str, form: &str, reading: &str) -> Token {
        Token {
            surface: Arc::from(surface),
            start: 0,
            end: 0,
            pos: Arc::from(pos),
            conj_type: Arc::from(kind),
            conj_form: Arc::from(form),
            base_form: Arc::from(base),
            reading: Arc::from(reading),
            pronunciation: Arc::from(reading),
            word_cost: 9665,
            is_known: true,
        }
    }

    fn place(tokens: &mut [Token]) {
        let mut offset = 17;
        for (i, token) in tokens.iter_mut().enumerate() {
            token.start = offset;
            offset += token.surface.len();
            token.end = offset;
            token.word_cost += i as i16;
        }
    }

    fn clause(nominalizer: bool, question: bool, parallel: bool) -> Vec<Token> {
        let mut tokens = vec![token(
            "行く",
            "動詞,自立,*,*",
            "行く",
            "五段・カ行促音便",
            "基本形",
            "イク",
        )];
        if nominalizer {
            tokens.push(token("の", "名詞,非自立,一般,*", "の", "", "", "ノ"));
        }
        if question {
            tokens.push(token(
                "か",
                "助詞,副助詞／並立助詞／終助詞,*,*",
                "か",
                "",
                "",
                "カ",
            ));
        }
        if parallel {
            tokens.push(token("とか", "助詞,並立助詞,*,*", "とか", "", "", "トカ"));
        }
        tokens.push(token("等", "名詞,固有名詞,人名,名", "等", "", "", "ヒトシ"));
        place(&mut tokens);
        tokens
    }

    fn unchanged(before: &[Token], after: &[Token]) {
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
    }

    fn only_grammatical_fields_changed(before: &[Token], after: &[Token]) {
        assert_eq!(before.len(), after.len());
        let mut restored = after.to_vec();
        for (token, original) in restored.iter_mut().zip(before) {
            token.pos = Arc::clone(&original.pos);
            token.reading = Arc::clone(&original.reading);
            token.pronunciation = Arc::clone(&original.pronunciation);
        }
        unchanged(before, &restored);
    }

    #[test]
    fn finite_clause_enumeration_corrects_pos_and_both_readings_only() {
        for (nominalizer, question, parallel) in [
            (false, true, false),
            (true, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            for tail in [
                None,
                Some(("も", "助詞,係助詞,*,*", "モ")),
                Some(("、", "記号,読点,*,*", "、")),
                Some(("）", "記号,括弧閉,*,*", "）")),
            ] {
                let mut tokens = clause(nominalizer, question, parallel);
                let target = tokens.len() - 1;
                if let Some((surface, pos, reading)) = tail {
                    tokens.push(token(surface, pos, surface, "", "", reading));
                    place(&mut tokens);
                }
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                assert_eq!(&*tokens[target].pos, "助詞,副助詞,*,*");
                assert_eq!(&*tokens[target].reading, "ナド");
                assert_eq!(&*tokens[target].pronunciation, "ナド");
                only_grammatical_fields_changed(&before, &tokens);
                let once = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&once, &tokens);
            }
        }
    }

    #[test]
    fn names_other_readings_and_nonfinite_or_inconsistent_features_are_preserved() {
        for (nominalizer, question, parallel) in [
            (false, true, false),
            (true, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            let base = clause(nominalizer, question, parallel);
            let target = base.len() - 1;
            for i in 0..base.len() {
                let mut tokens = base.clone();
                tokens[i].is_known = false;
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&before, &tokens);
            }
            for (reading, pronunciation) in [
                ("トウ", "トー"),
                ("ラ", "ラ"),
                ("ナド", "ナド"),
                ("トモ", "トモ"),
                ("ヒトシ", "トクシュ"),
            ] {
                let mut tokens = base.clone();
                tokens[target].reading = Arc::from(reading);
                tokens[target].pronunciation = Arc::from(pronunciation);
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&before, &tokens);
            }
            for variant in 0..7 {
                let mut tokens = base.clone();
                match variant {
                    0 => tokens[target].pos = Arc::from("名詞,固有名詞,人名,姓"),
                    1 => tokens[target].base_form = Arc::from("別"),
                    2 => tokens[target].conj_form = Arc::from("基本形"),
                    3 => tokens[0].conj_form = Arc::from("連用形"),
                    4 => tokens[0].conj_type = Arc::from(""),
                    5 => tokens[0].pos = Arc::from("名詞,固有名詞,人名,名"),
                    _ => tokens[target - 1].pos = Arc::from("動詞,自立,*,*"),
                }
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&before, &tokens);
            }
        }
    }

    #[test]
    fn gaps_protection_ascii_adjacency_and_name_continuations_are_preserved() {
        let base = clause(true, true, true);
        for i in 0..base.len() {
            let mut tokens = base.clone();
            let protected = base[i].start..base[i].end;
            apply(&mut tokens, std::slice::from_ref(&protected));
            unchanged(&base, &tokens);
            if i > 0 {
                let mut tokens = base.clone();
                tokens[i].start += 1;
                let before = tokens.clone();
                apply(&mut tokens, &[]);
                unchanged(&before, &tokens);
            }
        }
        for (surface, pos, reading) in [
            ("さん", "名詞,接尾,人名,*", "サン"),
            ("氏", "名詞,接尾,人名,*", "シ"),
            ("が", "助詞,格助詞,一般,*", "ガ"),
            ("の", "助詞,連体化,*,*", "ノ"),
            ("か", "助詞,副助詞／並立助詞／終助詞,*,*", "カ"),
            ("だ", "助動詞,*,*,*", "ダ"),
        ] {
            let mut tokens = base.clone();
            tokens.push(token(surface, pos, surface, "", "", reading));
            place(&mut tokens);
            let before = tokens.clone();
            apply(&mut tokens, &[]);
            unchanged(&before, &tokens);
        }
        let mut tokens = vec![token("X", "名詞,一般,*,*", "X", "", "", "エックス")];
        tokens.extend(base.clone());
        place(&mut tokens);
        let before = tokens.clone();
        apply(&mut tokens, &[]);
        unchanged(&before, &tokens);
        let mut tokens = base.clone();
        let end = tokens.last().unwrap().end;
        let touching = end..end + 1;
        apply(&mut tokens, std::slice::from_ref(&touching));
        assert_eq!(&*tokens.last().unwrap().reading, "ナド");
    }

    #[test]
    #[cfg(feature = "build")]
    fn analyzer_uses_the_real_name_candidate_without_a_particle_dictionary_entry() {
        use crate::dict::{DictBuilder, DictEntry};
        let mut builder = DictBuilder::new();
        for (surface, pos, kind, form, reading) in [
            (
                "行く",
                "動詞,自立,*,*",
                "五段・カ行促音便",
                "基本形",
                "イク",
            ),
            ("か", "助詞,副助詞／並立助詞／終助詞,*,*", "", "", "カ"),
            ("とか", "助詞,並立助詞,*,*", "", "", "トカ"),
            ("の", "名詞,非自立,一般,*", "", "", "ノ"),
            ("も", "助詞,係助詞,*,*", "", "", "モ"),
            ("等", "名詞,固有名詞,人名,名", "", "", "ヒトシ"),
            ("山田", "名詞,固有名詞,人名,姓", "", "", "ヤマダ"),
            ("さん", "名詞,接尾,人名,*", "", "", "サン"),
            ("が", "助詞,格助詞,一般,*", "", "", "ガ"),
            ("「", "記号,括弧開,*,*", "", "", "「"),
            ("」", "記号,括弧閉,*,*", "", "", "」"),
            ("、", "記号,読点,*,*", "", "", "、"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: 1,
                right_id: 1,
                cost: 77,
                pos: pos.into(),
                conj_type: kind.into(),
                conj_form: form.into(),
                base_form: surface.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
            });
        }
        let dictionary = builder.build().unwrap();
        let entries = dictionary.lookup("等").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.len(), 1);
        assert_eq!(&*entries[0].1[0].reading, "ヒトシ");
        let mut analyzer = crate::Analyzer::from_dict(dictionary);
        for text in [
            "行くか等",
            "行くのか等",
            "行くとか等",
            "行くのかとか等も",
            "「行くか等」",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let target = tokens.iter().find(|t| t.surface.as_ref() == "等").unwrap();
            assert_eq!(&*target.pos, "助詞,副助詞,*,*", "{text}");
            assert_eq!(&*target.reading, "ナド", "{text}");
            assert_eq!(&*target.pronunciation, "ナド", "{text}");
            assert_eq!(target.word_cost, 77);
            assert!(target.is_known);
            for t in tokens {
                assert_eq!(&text[t.start..t.end], &*t.surface);
            }
        }
        for text in [
            "山田等",
            "等さん",
            "等が",
            "山田か等さん",
            "行くか、等さん",
            "「等」",
        ] {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let target = tokens.iter().find(|t| t.surface.as_ref() == "等").unwrap();
            assert_eq!(&*target.pos, "名詞,固有名詞,人名,名", "{text}");
            assert_eq!(&*target.reading, "ヒトシ", "{text}");
            assert_eq!(&*target.pronunciation, "ヒトシ", "{text}");
        }
    }
}
