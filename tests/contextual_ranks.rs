use hasami::{Analyzer, DictEntry, Dictionary, Token};

fn matches_entry(token: &Token, entry: &DictEntry) {
    fn conjugation(value: &str) -> &str {
        if value == "*" { "" } else { value }
    }
    assert_eq!(token.surface, entry.surface);
    assert_eq!(token.pos, entry.pos);
    assert_eq!(token.base_form, entry.base_form);
    assert_eq!(token.conj_type.as_ref(), conjugation(&entry.conj_type));
    assert_eq!(token.conj_form.as_ref(), conjugation(&entry.conj_form));
    assert_eq!(token.reading, entry.reading);
    assert_eq!(token.pronunciation, entry.pronunciation);
    assert_eq!(token.word_cost, entry.cost);
    assert!(token.is_known);
}

fn matches_split_rank(focused: &[&Token], entries: &[Vec<DictEntry>; 2]) {
    assert_eq!(focused.len(), 2);
    for (i, (surface, pos, reading)) in [
        ("八", "名詞,数,*,*", "ハチ"),
        ("段", "名詞,接尾,助数詞,*", "ダン"),
    ]
    .iter()
    .enumerate()
    {
        let token = focused[i];
        assert_eq!(token.surface.as_ref(), *surface);
        assert_eq!(token.pos.as_ref(), *pos);
        assert_eq!(token.reading.as_ref(), *reading);
        assert_eq!(token.pronunciation.as_ref(), *reading);
        let entry = entries[i]
            .iter()
            .find(|entry| {
                entry.pos == token.pos
                    && entry.reading == token.reading
                    && entry.pronunciation == token.pronunciation
                    && entry.cost == token.word_cost
            })
            .expect("split token must retain a genuine dictionary entry's complete features");
        matches_entry(token, entry);
    }
}

#[test]
#[ignore = "配布辞書3種と、姓/段位の両候補を持つNeo系辞書が必要"]
fn distributed_rank_and_surname_contexts_keep_real_dictionary_features() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let dictionary = Dictionary::load(format!("{dir}/{name}.hsd")).unwrap();
        let entries: Vec<_> = dictionary
            .lookup("八段")
            .unwrap()
            .into_iter()
            .filter(|(end, _)| *end == "八段".len())
            .flat_map(|(_, entries)| entries)
            .collect();
        let surname = entries
            .iter()
            .find(|e| {
                e.pos.as_ref() == "名詞,固有名詞,人名,姓"
                    && e.reading.as_ref() == "ハッタン"
                    && e.pronunciation.as_ref() == "ハッタン"
            })
            .cloned();
        let rank = entries
            .iter()
            .find(|e| {
                e.pos.as_ref() == "名詞,一般,*,*"
                    && e.reading.as_ref() == "ハチダン"
                    && e.pronunciation.as_ref() == "ハチダン"
            })
            .cloned();
        if name != "ipadic" {
            assert!(
                surname.is_some() && rank.is_some(),
                "{name}: both genuine candidates required"
            );
        }
        let split_entries = ["八", "段"].map(|surface| {
            dictionary
                .lookup(surface)
                .unwrap()
                .into_iter()
                .filter(|(end, _)| *end == surface.len())
                .flat_map(|(_, entries)| entries)
                .collect::<Vec<_>>()
        });
        let mut analyzer = Analyzer::from_dict(dictionary);
        for (input, is_rank) in [
            ("八段を目指す。", true),
            ("八段に昇段した。", true),
            ("段位は八段です。", true),
            ("段位は「八段」です。", true),
            ("棋士の田中八段が来た。", true),
            ("段位の八段を目指す。", true),
            ("姓は八段です。", false),
            ("同じ姓の八段を目指す。", false),
            ("ハッタンと読む姓の「八段」を目指す。", false),
            ("ハッタンと読む八段を目指す。", false),
            ("ハッタンと読む「八段」を目指す。", false),
            ("八段さんが来た。", false),
            ("段位は未定。姓は八段です。", false),
            ("八段", false),
            ("ハッタンと読む 八段を目指す。", false),
            ("ハッタンと読む 「八段」を目指す。", false),
        ] {
            let start = input.find("八段").unwrap();
            let end = start + "八段".len();
            let tokens = analyzer.try_tokenize(input).unwrap();
            let focused: Vec<_> = tokens
                .iter()
                .filter(|t| t.start < end && start < t.end)
                .collect();
            assert_eq!(focused.first().unwrap().start, start, "{name}: {input}");
            assert_eq!(focused.last().unwrap().end, end, "{name}: {input}");
            assert!(focused.windows(2).all(|pair| pair[0].end == pair[1].start));
            if name == "ipadic" {
                // stock IPAdicには姓が無い。姓の正解を主張せず、従来の数詞/助数詞分割を保持する。
                matches_split_rank(&focused, &split_entries);
            } else if is_rank && focused.len() == 2 {
                // 正しい八/段の分割はその全実素性と連続範囲を保つ。
                matches_split_rank(&focused, &split_entries);
            } else {
                assert_eq!(focused.len(), 1, "{name}: {input}: {tokens:?}");
                matches_entry(
                    focused[0],
                    if is_rank {
                        rank.as_ref().unwrap()
                    } else {
                        surname.as_ref().unwrap()
                    },
                );
            }
            for token in &tokens {
                assert_eq!(&input[token.start..token.end], token.surface.as_ref());
            }
            let spans = analyzer.contextual_reading_spans(input).unwrap();
            let explicit_name = matches!(input, "姓は八段です。" | "段位は未定。姓は八段です。");
            if name != "ipadic" && (is_rank || explicit_name) {
                // Viterbiがすでに一般候補を選べた文脈は実補正が無く、照会も空になる。
                // 元姓からの切替と厳密な1spanは、固定辞書のAnalyzer単体テストで検証する。
                assert!(spans.len() <= 1, "{name}: {input}: {spans:?}");
                for span in &spans {
                    assert_eq!((span.start, span.end), (start, end));
                    assert_eq!(
                        span.pronunciation,
                        if is_rank {
                            "ハチダン"
                        } else {
                            "ハッタン"
                        }
                    );
                }
            } else {
                assert!(spans.is_empty(), "{name}: {input}: {spans:?}");
            }
            assert_eq!(
                format!("{:?}", analyzer.try_tokenize(input).unwrap()),
                format!("{tokens:?}")
            );
        }
    }
}

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_rank_documents_keep_chunk_offsets_and_separate_contexts() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    let pieces = [
        "段位は「八段」です。\n",
        "姓は「八段」です。\n",
        "8段を目指す。\n",
        "ID_八段_12\n",
        "段位は未定。\n",
        "八段太郎が来た。\n",
    ];
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let mut analyzer = Analyzer::load(format!("{dir}/{name}.hsd")).unwrap();
        let bare = format!("{:?}", analyzer.try_tokenize("八段").unwrap());
        let isolated: Vec<_> = pieces
            .iter()
            .map(|piece| {
                (
                    analyzer.try_tokenize(piece).unwrap(),
                    analyzer.contextual_reading_spans(piece).unwrap(),
                )
            })
            .collect();
        // 長文の比較先も、段位・姓の独立した読みを満たす実解析に限定する。
        for (index, speech) in [(0, "ハチダン"), (1, "ハッタン"), (5, "ハッタン")] {
            if name == "ipadic" && index != 0 {
                continue; // IPAdicには姓候補がなく、姓の正解とは扱わない。
            }
            let start = pieces[index].find("八段").unwrap();
            let end = start + "八段".len();
            assert_eq!(
                isolated[index]
                    .0
                    .iter()
                    .filter(|t| t.start < end && start < t.end)
                    .map(|t| t.pronunciation.as_ref())
                    .collect::<String>(),
                speech,
                "{name}: {}",
                pieces[index]
            );
        }
        let mut text = String::new();
        let mut expected_tokens = Vec::new();
        let mut expected_spans = Vec::new();
        for _ in 0..16 {
            for (piece, (tokens, spans)) in pieces.iter().zip(&isolated) {
                let offset = text.len();
                expected_tokens.extend(tokens.iter().cloned().map(|mut token| {
                    token.start += offset;
                    token.end += offset;
                    token
                }));
                expected_spans.extend(spans.iter().cloned().map(|mut span| {
                    span.start += offset;
                    span.end += offset;
                    span
                }));
                text.push_str(piece);
            }
        }
        for _ in 0..2 {
            let actual = analyzer.try_tokenize(&text).unwrap();
            assert_eq!(
                format!("{actual:?}"),
                format!("{expected_tokens:?}"),
                "{name}"
            );
            assert_eq!(
                analyzer.contextual_reading_spans(&text).unwrap(),
                expected_spans
            );
            let sentences: Vec<_> = analyzer
                .try_tokenize_sentences(&text, &Default::default())
                .unwrap()
                .into_iter()
                .flat_map(|(_, tokens)| tokens)
                .collect();
            assert_eq!(format!("{actual:?}"), format!("{sentences:?}"), "{name}");
            for token in &actual {
                assert_eq!(
                    text.get(token.start..token.end),
                    Some(token.surface.as_ref())
                );
            }
            assert_eq!(
                format!("{:?}", analyzer.try_tokenize("八段").unwrap()),
                bare
            );
        }
    }
}
