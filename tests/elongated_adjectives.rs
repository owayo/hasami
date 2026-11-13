//! 語末を引き伸ばした形容詞のエントリが、次の語の先頭を取り込まないこと。
#![cfg(feature = "analyzer")]

use hasami::Analyzer;

fn analyzer(name: &str) -> Analyzer {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    Analyzer::load(format!("{dir}/{name}.hsd")).unwrap()
}

/// 文中の語の並びを「表層(発音)」の列にする。
fn spell(analyzer: &mut Analyzer, text: &str) -> String {
    analyzer
        .try_tokenize(text)
        .unwrap()
        .iter()
        .map(|t| format!("{}({})", t.surface, t.pronunciation))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 引用の「って」の促音が、手前の形容詞へ入らない。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_quotative_tte_stays_whole() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (text, expected) in [
            ("それは関係ないって言われた。", "ない(ナイ) って(ッテ)"),
            ("今日は寒いって話していた。", "寒い(サムイ) って(ッテ)"),
            (
                "試験はむずかしいって聞いた。",
                "むずかしい(ムズカシイ) って(ッテ)",
            ),
            ("あの選手は強いって評判だ。", "強い(ツヨイ) って(ッテ)"),
            ("どのくらいって聞かれた。", "くらい(クライ) って(ッテ)"),
        ] {
            let got = spell(&mut analyzer, text);
            assert!(got.contains(expected), "{name}: {text}: {got}");
        }
    }
}

/// 促音便の「いっ」が、手前の語の末尾へ吸われない。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_sokuon_of_the_next_verb_is_not_absorbed() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (text, expected) in [
            ("準備は上手くいったと思う。", "上手く(ウマク) いっ(イッ)"),
            ("体調からいって無理だろう。", "から(カラ) いっ(イッ)"),
        ] {
            let got = spell(&mut analyzer, text);
            assert!(got.contains(expected), "{name}: {text}: {got}");
        }
    }
}

/// 次の語が「い」で始まっても、手前の語がその「い」を取り込まない。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_adjectives_do_not_swallow_a_following_i() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (text, expected) in [
            ("もういいから休もう。", "もう(モー) いい(イイ)"),
            ("ふといい考えが浮かんだ。", "ふと(フト) いい(イイ)"),
            ("別名を気圧計ともいいます。", "とも(トモ) いい(イイ)"),
        ] {
            let got = spell(&mut analyzer, text);
            assert!(got.contains(expected), "{name}: {text}: {got}");
        }
    }
}

/// 引き伸ばしでない表記ゆれは残る。名詞の長音表記と、確かめた形容詞の異表記。
#[test]
#[ignore = "配布辞書が必要"]
fn distributed_attested_variants_remain() {
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = analyzer(name);
        for (text, expected) in [
            (
                "3Dプリンターで部品を作る。",
                "3Dプリンター(スリーディープリンター)",
            ),
            ("可愛いい人形を飾る。", "可愛いい(カワイイ)"),
        ] {
            let got = spell(&mut analyzer, text);
            assert!(got.contains(expected), "{name}: {text}: {got}");
        }
    }
}
