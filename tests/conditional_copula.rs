//! 条件・話題を表す助動詞「なら」と、動詞「なる」の未然形「なら」の区別の回帰。
//!
//! 同じ表層・同じ読みのまま品詞と原形だけが入れ替わると、利用側（読み上げの
//! アクセント辞書など）は発音文字列の比較では気づけない。条件・話題の「なら」が
//! 助動詞「だ」の仮定形を保ち、否定形に続く「なら」が動詞「なる」の未然形になる
//! ことを、語境界と全素性つきで固定する。
#![cfg(feature = "analyzer")]

use hasami::Analyzer;

/// 「なら」の 1 トークンを取り出し、トークン列が入力を隙間なく覆うことも確かめる。
fn single_nara(analyzer: &mut Analyzer, text: &str, name: &str) -> hasami::Token {
    let tokens = analyzer.try_tokenize(text).unwrap();
    for token in &tokens {
        assert_eq!(
            &text[token.start..token.end],
            token.surface.as_ref(),
            "{name}: {text}: {tokens:?}"
        );
    }
    assert!(
        tokens.windows(2).all(|pair| pair[0].end == pair[1].start),
        "{name}: {text}: {tokens:?}"
    );
    assert_eq!(tokens.last().unwrap().end, text.len(), "{name}: {text}");
    let found: Vec<_> = tokens
        .iter()
        .filter(|token| token.surface.as_ref() == "なら")
        .collect();
    assert_eq!(found.len(), 1, "{name}: {text}: {tokens:?}");
    found[0].clone()
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_conditional_nara_stays_auxiliary_apart_from_negated_verb() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();

        // 条件・話題の「なら」。体言・用言のどちらに続いても助動詞「だ」の仮定形を保つ。
        for text in [
            "猫なら飼える。",
            "必要なら対応する。",
            "学生なら割引がある。",
            "雨なら中止だ。",
            "明日なら空いている。",
            "日本語なら読める。",
            "君が行くなら私も行く。",
            "安いなら買おう。",
            // 同じ文に動詞「なる」があっても、条件の「なら」は助動詞のまま。
            "会員なら無料になる。",
        ] {
            let token = single_nara(&mut analyzer, text, name);
            assert_eq!(&*token.pos, "助動詞,*,*,*", "{name}: {text}: {token:?}");
            assert_eq!(&*token.conj_type, "特殊・ダ", "{name}: {text}: {token:?}");
            assert_eq!(&*token.conj_form, "仮定形", "{name}: {text}: {token:?}");
            assert_eq!(&*token.base_form, "だ", "{name}: {text}: {token:?}");
            assert_eq!(&*token.reading, "ナラ", "{name}: {text}: {token:?}");
            assert_eq!(&*token.pronunciation, "ナラ", "{name}: {text}: {token:?}");
            assert!(token.is_known, "{name}: {text}: {token:?}");
        }

        // 否定形に続く「なら」は動詞「なる」の未然形。自立・非自立の別は前接で決まる。
        for (text, pos) in [
            ("それでは話にならない。", "動詞,自立,*,*"),
            ("期限までに終わらせなければならない。", "動詞,非自立,*,*"),
            ("どうにもならん。", "動詞,自立,*,*"),
            ("気をつけなければならぬ。", "動詞,非自立,*,*"),
        ] {
            let token = single_nara(&mut analyzer, text, name);
            assert_eq!(&*token.pos, pos, "{name}: {text}: {token:?}");
            assert_eq!(&*token.conj_type, "五段・ラ行", "{name}: {text}: {token:?}");
            assert_eq!(&*token.conj_form, "未然形", "{name}: {text}: {token:?}");
            assert_eq!(&*token.base_form, "なる", "{name}: {text}: {token:?}");
            assert_eq!(&*token.reading, "ナラ", "{name}: {text}: {token:?}");
            assert_eq!(&*token.pronunciation, "ナラ", "{name}: {text}: {token:?}");
            assert!(token.is_known, "{name}: {text}: {token:?}");
        }
    }
}
