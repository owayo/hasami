use hasami::{Analyzer, Dictionary};

#[test]
#[ignore = "現代語候補を含む配布辞書2種が必要"]
fn distributed_modern_lexical_readings_keep_dictionary_features() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic-neologd", "ipadic-neologd-sudachi"] {
        let path = format!("{dir}/{name}.hsd");
        let dictionary = Dictionary::load(&path).unwrap();
        let mut analyzer = Analyzer::load(&path).unwrap();
        for (surface, reading, pronunciation, pos) in [
            ("圧潰", "アッカイ", "アッカイ", "名詞,サ変接続,*,*"),
            ("鼻閉", "ビヘイ", "ビヘイ", "名詞,サ変接続,*,*"),
            ("頸飾り", "クビカザリ", "クビカザリ", "名詞,一般,*,*"),
        ] {
            let entries: Vec<_> = dictionary
                .lookup(surface)
                .unwrap()
                .into_iter()
                .filter(|(end, _)| *end == surface.len())
                .flat_map(|(_, entries)| entries)
                .collect();
            // 独自登録と上流変換を二重にマージして同じ読みの候補を増やさない。
            // 別の品詞・原形・読みの正当な候補まで一律に制限しない。
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| {
                        entry.surface.as_ref() == surface
                            && entry.base_form.as_ref() == surface
                            && entry.pos.as_ref() == pos
                            && entry.reading.as_ref() == reading
                            && entry.pronunciation.as_ref() == pronunciation
                    })
                    .count(),
                1,
                "{name}: {surface}: the canonical candidate must be merged once"
            );
            let mut inputs = vec![
                surface.to_owned(),
                format!("{surface}を調べる。"),
                format!("「{surface}」を確認する。"),
                format!("ID_{surface}_12"),
            ];
            if surface == "頸飾り" {
                inputs.push("頸飾りを選ぶ。".to_owned());
            }
            for input in inputs.iter().cycle().take(inputs.len() * 2) {
                let start = input.find(surface).unwrap();
                let end = start + surface.len();
                let tokens = analyzer.try_tokenize(input).unwrap();
                let focal: Vec<_> = tokens
                    .iter()
                    .filter(|t| t.start < end && start < t.end)
                    .collect();
                assert_eq!(focal.len(), 1, "{name}: {input}: {tokens:?}");
                let token = focal[0];
                assert_eq!((token.start, token.end), (start, end));
                assert_eq!(token.surface.as_ref(), surface);
                assert_eq!(token.base_form.as_ref(), surface);
                assert_eq!(token.pos.as_ref(), pos);
                assert_eq!(token.reading.as_ref(), reading);
                assert_eq!(token.pronunciation.as_ref(), pronunciation);
                assert!(token.is_known);
                assert!(token.conj_type.is_empty() && token.conj_form.is_empty());
                assert!(entries.iter().any(|entry| {
                    entry.surface == token.surface
                        && entry.base_form == token.base_form
                        && entry.pos == token.pos
                        && entry.reading == token.reading
                        && entry.pronunciation == token.pronunciation
                        && entry.cost == token.word_cost
                        && matches!(entry.conj_type.as_ref(), "" | "*")
                        && matches!(entry.conj_form.as_ref(), "" | "*")
                }));
                let mut previous = 0;
                for token in &tokens {
                    assert!(previous <= token.start && token.start < token.end);
                    assert_eq!(&input[token.start..token.end], token.surface.as_ref());
                    previous = token.end;
                }
                assert_eq!(tokens.first().unwrap().start, 0);
                assert_eq!(tokens.last().unwrap().end, input.len());
                assert!(tokens.windows(2).all(|pair| pair[0].end == pair[1].start));
                if input.starts_with("ID_") {
                    let digits: Vec<_> = tokens.iter().filter(|t| t.start > end).collect();
                    assert_eq!(
                        digits
                            .iter()
                            .map(|t| t.surface.as_ref())
                            .collect::<Vec<_>>(),
                        ["1", "2"]
                    );
                    assert_eq!(
                        digits
                            .iter()
                            .map(|t| t.pronunciation.as_ref())
                            .collect::<String>(),
                        "イチニ"
                    );
                }
            }
        }
    }
}
