//! 錨（[`crate::anchors`]）で補正の段と下調べを絞った解析が、絞り込まない参照経路
//! （[`crate::anchors::ungated`]）と同じ結果を返すことを確かめる。
//!
//! 比べるのは、トークンの全フィールド、文脈の読み・数量の照会、文ごとの解析、位置をずらして既存の出力に
//! 足す `LatticeWorkspace::tokenize_into`。どれかの段の `may_apply` / `may_find` が、規則が発火しうる区間で
//! 偽を返すと、ここで食い違う。

use crate::anchors::ungated;
use crate::lattice::LatticeWorkspace;
use crate::sentence::SplitOptions;
use crate::{Analyzer, Token};
use std::fmt::Write as _;

/// 1 つの入力について、比べる結果をすべて文字列にする
fn analyses(analyzer: &mut Analyzer, input: &str) -> String {
    fn push(s: &mut String, tokens: &[Token]) {
        for token in tokens {
            writeln!(s, "{token:?}").unwrap();
        }
    }
    let mut s = String::new();
    push(&mut s, &analyzer.try_tokenize(input).unwrap());
    s.push_str("-- contextual\n");
    for span in analyzer.contextual_reading_spans(input).unwrap() {
        writeln!(s, "{span:?}").unwrap();
    }
    s.push_str("-- quantity\n");
    for span in analyzer.quantity_spans(input).unwrap() {
        writeln!(s, "{span:?}").unwrap();
    }
    s.push_str("-- sentences\n");
    for (sentence, tokens) in analyzer
        .try_tokenize_sentences(input, &SplitOptions::default())
        .unwrap()
    {
        writeln!(s, "{sentence:?}").unwrap();
        push(&mut s, &tokens);
    }
    // 位置をずらし、前の結果が残る出力に続けて足す（補正は足した範囲だけに掛かる）
    s.push_str("-- workspace\n");
    let dictionary = analyzer.dictionary().clone();
    let mut workspace = LatticeWorkspace::new();
    let mut out = Vec::new();
    workspace
        .tokenize_into(input, &dictionary, 7, &mut out)
        .unwrap();
    workspace
        .tokenize_into(input, &dictionary, 8 + input.len(), &mut out)
        .unwrap();
    push(&mut s, &out);
    s
}

/// 各入力で、絞り込んだ解析と参照経路の結果が同じこと
fn assert_matches_reference<'a>(
    analyzer: &mut Analyzer,
    inputs: impl IntoIterator<Item = &'a str>,
) {
    for input in inputs {
        let gated = analyses(analyzer, input);
        let reference = ungated(|| analyses(analyzer, input));
        assert_eq!(gated, reference, "{input:?}");
    }
}

#[cfg(feature = "build")]
mod fixture {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};

    const NUMBER: &str = "名詞,数,*,*";
    const COUNTER: &str = "名詞,接尾,助数詞,*";

    fn entry(surface: &str, pos: &str, reading: &str, pronunciation: &str, cost: i16) -> DictEntry {
        DictEntry {
            surface: surface.into(),
            pos: pos.into(),
            reading: reading.into(),
            pronunciation: pronunciation.into(),
            base_form: surface.into(),
            cost,
            ..Default::default()
        }
    }

    fn inflected(surface: &str, pos: &str, kind: &str, reading: &str) -> DictEntry {
        DictEntry {
            conj_type: kind.into(),
            conj_form: "基本形".into(),
            ..entry(surface, pos, reading, reading, 0)
        }
    }

    fn analyzer(entries: impl IntoIterator<Item = DictEntry>) -> Analyzer {
        let mut builder = DictBuilder::new();
        for entry in entries {
            builder.add_entry(entry);
        }
        Analyzer::from_dict(builder.build().unwrap())
    }

    /// 数詞・助数詞・暦（助数詞の読み、月日の合成、ASCII・全角・漢数字の数量）
    fn numbers() -> Analyzer {
        let mut entries = Vec::new();
        let readings = [
            ("ゼロ", "ゼロ"),
            ("イチ", "イチ"),
            ("ニ", "ニ"),
            ("サン", "サン"),
            ("ヨン", "ヨン"),
            ("ゴ", "ゴ"),
            ("ロク", "ロク"),
            ("ナナ", "ナナ"),
            ("ハチ", "ハチ"),
            ("キュウ", "キュー"),
        ];
        for (n, (reading, pronunciation)) in readings.into_iter().enumerate() {
            for surface in [
                n.to_string(),
                char::from_u32('０' as u32 + n as u32).unwrap().to_string(),
                "〇一二三四五六七八九".chars().nth(n).unwrap().to_string(),
            ] {
                entries.push(entry(&surface, NUMBER, reading, pronunciation, -100));
            }
        }
        for (surface, reading, pronunciation) in [
            ("十", "ジュウ", "ジュー"),
            ("百", "ヒャク", "ヒャク"),
            ("千", "セン", "セン"),
            ("万", "マン", "マン"),
            ("億", "オク", "オク"),
        ] {
            entries.push(entry(surface, NUMBER, reading, pronunciation, -100));
        }
        for (surface, reading) in [
            ("本", "ホン"),
            ("回", "カイ"),
            ("分", "フン"),
            ("人", "ニン"),
            ("日", "ニチ"),
            ("年", "ネン"),
            ("個", "コ"),
            ("段", "ダン"),
            ("円", "エン"),
        ] {
            entries.push(entry(surface, COUNTER, reading, reading, 0));
        }
        for (surface, pos, reading, pronunciation, cost) in [
            ("月", "名詞,一般,*,*", "ツキ", "ツキ", 0),
            ("一月", "名詞,副詞可能,*,*", "イチガツ", "イチガツ", -500),
            (
                "十一月",
                "名詞,副詞可能,*,*",
                "ジュウイチガツ",
                "ジューイチガツ",
                -1000,
            ),
            (
                "11月",
                "名詞,副詞可能,*,*",
                "ジュウイチガツ",
                "ジューイチガツ",
                -1000,
            ),
            ("一日", "名詞,副詞可能,*,*", "ツイタチ", "ツイタチ", -100),
            (
                "十分",
                "名詞,形容動詞語幹,*,*",
                "ジュウブン",
                "ジューブン",
                -100,
            ),
            ("前", "名詞,副詞可能,*,*", "マエ", "マエ", 0),
            ("後", "名詞,接尾,副詞可能,*", "ゴ", "ゴ", 0),
            ("八段", "名詞,固有名詞,人名,姓", "ヤタベ", "ヤタベ", -300),
            ("姓", "名詞,一般,*,*", "セイ", "セイ", 0),
            ("値", "名詞,一般,*,*", "ネ", "ネ", 0),
            ("間", "名詞,一般,*,*", "アイダ", "アイダ", 0),
            (
                "震度7",
                "名詞,固有名詞,一般,*",
                "シンドセブン",
                "シンドセブン",
                -500,
            ),
            ("は", "助詞,係助詞,*,*", "ハ", "ワ", 0),
            ("の", "助詞,連体化,*,*", "ノ", "ノ", 0),
            ("に", "助詞,格助詞,一般,*", "ニ", "ニ", 0),
            ("も", "助詞,係助詞,*,*", "モ", "モ", 0),
            ("。", "記号,句点,*,*", "。", "。", 0),
            ("、", "記号,読点,*,*", "、", "、", 0),
            ("-", "記号,一般,*,*", "-", "-", 0),
            (".", "記号,一般,*,*", ".", ".", 0),
        ] {
            entries.push(entry(surface, pos, reading, pronunciation, cost));
        }
        entries.push(inflected("です", "助動詞,*,*,*", "特殊・デス", "デス"));
        analyzer(entries)
    }

    /// 面積の構文（tests/katakana_area.rs と同じ素性）
    fn area() -> Analyzer {
        let with = |surface: &str, base: &str, pos: &str, reading: &str| DictEntry {
            base_form: base.into(),
            ..entry(surface, pos, reading, reading, -1000)
        };
        let mut copula = inflected("です", "助動詞,*,*,*", "特殊・デス", "デス");
        copula.cost = -1000;
        analyzer([
            with("面積", "面積", "名詞,一般,*,*", "メンセキ"),
            with("名前", "名前", "名詞,一般,*,*", "ナマエ"),
            DictEntry {
                pronunciation: "ワ".into(),
                ..with("は", "は", "助詞,係助詞,*,*", "ハ")
            },
            with("ニジュウナナ", "27", "名詞,固有名詞,一般,*", "ニジュウナナ"),
            with("ヘイホー", "Hey Ho", "名詞,固有名詞,一般,*", "ヘイホー"),
            with("メートル", "メートル", "名詞,接尾,助数詞,*", "メートル"),
            with("。", "。", "記号,句点,*,*", "。"),
            with("、", "、", "記号,読点,*,*", "、"),
            copula,
        ])
    }

    /// 成句と「等」（読みだけ・品詞も変える規則）
    fn lexical() -> Analyzer {
        let particle =
            |surface: &str, pos: &str, reading: &str| entry(surface, pos, reading, reading, -500);
        analyzer([
            inflected("行く", "動詞,自立,*,*", "五段・カ行促音便", "イク"),
            inflected("受ける", "動詞,自立,*,*", "一段", "ウケル"),
            particle("か", "助詞,副助詞／並立助詞／終助詞,*,*", "カ"),
            particle("とか", "助詞,並立助詞,*,*", "トカ"),
            particle("等", "名詞,固有名詞,人名,名", "ヒトシ"),
            particle("真に", "副詞,一般,*,*", "シンニ"),
            particle("真", "名詞,一般,*,*", "シン"),
            particle("に", "助詞,格助詞,一般,*", "ニ"),
            particle("、", "記号,読点,*,*", "、"),
            particle("。", "記号,句点,*,*", "。"),
        ])
    }

    /// 錨のある入力（規則が発火するもの・しないもの）と、錨の無い入力。文末記号で区間を分けたものも入れる
    const INPUTS: &[&str] = &[
        "",
        "かなだけの文です。",
        "十分前に出た。十分に間に合う。",
        "二人で三日、三百本と六百本を数えた。",
        "日付は11月1日です。十一月一日、一月一日も見る。",
        "2024年11月1日と２０２４年１１月１日。",
        "一〇月と一一月と一零月。年一〇月。",
        "体重が12.5kg減った。測定は3回で、ばらつきは5%を超えた。",
        "見積もりは30万円、納期は来月の15日。",
        "-1.5と-0.25と=-3.5は負の小数。",
        "面積はニジュウナナヘイホーメートルです。名前はニジュウナナヘイホーメートルです。",
        "面積は30平方メートル、面積は３０㎡。",
        "姓は八段です。八段の段位。",
        "行くか等、行くとか等。等。",
        "値は3です。その値を返す。",
        "真に受ける。真に。真",
        "間もない。分の悪い。",
        "震度7の揺れ。震度７。",
        "四〇〇〇万円と一〇万円。",
        "第3章と中23位と１２３個。",
        "「一一月」は引用。“二十日”も。",
        "A1一一月 B2三月 C3年",
        "三 本、三\t本、三\n本。",
        "１日と一日と1日。",
    ];

    #[test]
    fn gated_analysis_matches_the_ungated_reference() {
        let whole = INPUTS.join("\n");
        for mut analyzer in [numbers(), area(), lexical()] {
            assert_matches_reference(&mut analyzer, INPUTS.iter().copied());
            // 文の単位でなく、全部を 1 つの入力にしても同じ（区間と入力全体の錨が違う）
            assert_matches_reference(&mut analyzer, [whole.as_str()]);
        }
    }

    fn joined(analyzer: &mut Analyzer, text: &str, field: impl Fn(&Token) -> String) -> String {
        analyzer
            .try_tokenize(text)
            .unwrap()
            .iter()
            .map(field)
            .collect::<Vec<_>>()
            .join("/")
    }

    #[test]
    fn fixtures_fire_the_gated_rules() {
        // 比べている補正が、これらの辞書で実際に発火していること（発火しなければ比べても意味が無い）
        let pronunciation = |t: &Token| t.pronunciation.to_string();
        let mut numbers = numbers();
        assert_eq!(
            joined(&mut numbers, "十分前", pronunciation),
            "ジュップン/マエ"
        );
        assert_eq!(
            joined(&mut numbers, "三百本", pronunciation),
            "サン/ビャッ/ポン"
        );
        assert!(joined(&mut numbers, "日付は11月1日です。", pronunciation).contains("ツイタチ"));
        let mut area = area();
        let surface_pos = |t: &Token| format!("{}:{}", t.surface, t.pos);
        assert!(
            joined(
                &mut area,
                "面積はニジュウナナヘイホーメートルです。",
                surface_pos
            )
            .contains("ヘイホーメートル:名詞,接尾,助数詞,*")
        );
        let mut lexical = lexical();
        // 「真に」1 語でも「真 / に」でも、成句の読みはマニになる
        assert!(
            joined(&mut lexical, "真に受ける", pronunciation)
                .replace('/', "")
                .starts_with("マニウケル")
        );
        assert!(
            joined(&mut lexical, "行くか等、", |t| format!(
                "{}:{}",
                t.surface, t.pronunciation
            ))
            .contains("等:ナド")
        );
    }
}

/// Rust のソースから非 ASCII を含む文字列リテラルを集める（`\\` `\"` `\n` `\t` `\r` `\0` `\'` `\u{..}` を戻す）
fn japanese_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = source.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut literal = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => literal.push('\n'),
                    Some('t') => literal.push('\t'),
                    Some('r') => literal.push('\r'),
                    Some('0') => literal.push('\0'),
                    Some('u') => {
                        let hex: String = chars
                            .by_ref()
                            .skip_while(|&c| c == '{')
                            .take_while(|&c| c != '}')
                            .collect();
                        if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            literal.push(c);
                        }
                    }
                    Some(c) => literal.push(c),
                    None => break,
                },
                c => literal.push(c),
            }
        }
        if !literal.is_ascii() {
            out.push(literal);
        }
    }
    out
}

#[test]
fn literals_are_collected_with_escapes() {
    let source = r#"let a = "日本\n語"; let b = "abc"; let c = "\u{3007}月\"";"#;
    assert_eq!(japanese_literals(source), ["日本\n語", "〇月\""]);
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_gated_analysis_matches_the_ungated_reference() {
    // リポジトリのテストとソースにある日本語の文字列（規則の正例・負例）を、前後に文脈を付けて流す
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    let mut dirs = vec![root.join("src"), root.join("tests")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut inputs = std::collections::BTreeSet::new();
    for file in &files {
        for literal in japanese_literals(&std::fs::read_to_string(file).unwrap()) {
            for input in [
                literal.clone(),
                format!("今日は{literal}です。"),
                format!("「{literal}」と書いた。"),
                format!("A1{literal}"),
                format!("{literal}3"),
            ] {
                inputs.insert(input);
            }
        }
    }
    assert!(inputs.len() > 10_000, "{}", inputs.len());
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        assert_matches_reference(&mut analyzer, inputs.iter().map(String::as_str));
        // 文書のように 1 つの入力にまとめたとき（入力全体の錨は区間の錨より広い）
        let documents: Vec<String> = inputs
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .chunks(64)
            .map(|chunk| chunk.join("\n"))
            .collect();
        assert_matches_reference(&mut analyzer, documents.iter().map(String::as_str));
    }
}
