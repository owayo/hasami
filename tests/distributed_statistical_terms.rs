#![cfg(feature = "analyzer")]

use hasami::Analyzer;
use std::path::Path;

#[test]
#[ignore = "通常の生成手順で作ったNEologd系の配布辞書を使う"]
fn distributed_statistical_mode_has_its_standard_reading() {
    // 総務省統計局の統計用語辞典「最頻値（さいひんち）」を読みの根拠とする。
    // https://www.stat.go.jp/naruhodo/13_yougo/sa-gyo.html
    for name in ["ipadic-neologd.hsd", "ipadic-neologd-sudachi.hsd"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("dict")
            .join(name);
        let mut analyzer = Analyzer::load(&path).unwrap();
        for text in ["最頻値", "最頻値を調べる。", "この表では最頻値が二つある。"]
        {
            let tokens = analyzer.try_tokenize(text).unwrap();
            let token = tokens
                .iter()
                .find(|token| token.surface.as_ref() == "最頻値")
                .unwrap_or_else(|| panic!("{name}: {text}: {tokens:?}"));
            assert!(token.is_known, "{name}: {text}");
            assert_eq!(token.pos.as_ref(), "名詞,一般,*,*", "{name}: {text}");
            assert_eq!(token.base_form.as_ref(), "最頻値", "{name}: {text}");
            assert_eq!(token.reading.as_ref(), "サイヒンチ", "{name}: {text}");
            assert_eq!(token.pronunciation.as_ref(), "サイヒンチ", "{name}: {text}");
        }
    }
}
