//! アナライザー - 形態素解析の高レベルAPI

use crate::hsd::{DictError, Dictionary};
use crate::lattice::{LatticeWorkspace, Token};
use crate::sentence::{Sentence, SplitOptions, Splitter};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 既定の辞書のパスを指す環境変数
pub const DICT_ENV: &str = "HASAMI_DICT";

/// 既定の辞書のディレクトリで優先して選ぶ辞書（推奨順）
const PREFERRED_DICTS: [&str; 3] = [
    "ipadic-neologd-sudachi.hsd",
    "ipadic-neologd.hsd",
    "ipadic.hsd",
];

/// 形態素解析器
///
/// # 並行解析
/// `Analyzer` は `Clone` を実装しており、辞書（mmap）を共有しつつ
/// 各クローンが独自のラティスワークスペースを持つ。
/// 複数スレッドで並行に解析する場合は、ワーカーごとに `analyzer.clone()` する。
///
/// ```ignore
/// let analyzer = Analyzer::load("dict.hsd")?;
/// std::thread::scope(|s| {
///     for input in inputs.chunks(100) {
///         let mut worker = analyzer.clone();
///         s.spawn(move || {
///             for text in input {
///                 let _tokens = worker.tokenize(text);
///             }
///         });
///     }
/// });
/// ```
///
/// # 辞書ファイルの扱い
/// 辞書は mmap で読み込む。読み込み中の辞書ファイルを書き換えたり切り詰めたりしてはいけない
/// （未定義動作になりうる）。辞書を更新するときは別名で書いてから rename で差し替える
/// （`hasami build` / `merge` / `repair` の出力はそうしている）。
pub struct Analyzer {
    dict: Arc<Dictionary>,
    workspace: LatticeWorkspace,
    /// 解析の前分割（ラティスを小さく保つための区切り）に使う文分割器
    splitter: Splitter,
}

impl Clone for Analyzer {
    /// 辞書を共有しつつ、新しいワークスペースを持つアナライザーを生成
    ///
    /// 辞書は `Arc` 共有のためゼロコピー。ワークスペースのみ新規確保される。
    fn clone(&self) -> Self {
        Analyzer {
            dict: Arc::clone(&self.dict),
            workspace: LatticeWorkspace::new(),
            splitter: self.splitter.clone(),
        }
    }
}

impl Analyzer {
    /// .hsd 辞書ファイルからアナライザーを生成
    pub fn load<P: AsRef<Path>>(dict_path: P) -> Result<Self, DictError> {
        Ok(Self::from_dict(Dictionary::load(dict_path)?))
    }

    /// 既定の場所の辞書を探して読み込む（探す順は [`default_dict_path`]）
    ///
    /// 見つからなければ [`DictError::NotFound`] を返す（探した場所を持つ）。辞書が無くても動く
    /// 利用者は、このエラーのときだけ辞書なしに切り替えればよい。
    pub fn load_default() -> Result<Self, DictError> {
        Self::load(default_dict_path()?)
    }

    /// 辞書から生成（`DictBuilder::build` で作ったメモリ上の辞書など）
    pub fn from_dict(dict: Dictionary) -> Self {
        Self::from_shared(Arc::new(dict))
    }

    /// 共有の辞書から生成
    pub fn from_shared(dict: Arc<Dictionary>) -> Self {
        Analyzer {
            dict,
            workspace: LatticeWorkspace::new(),
            splitter: Splitter::default(),
        }
    }

    /// 使っている辞書
    pub fn dictionary(&self) -> &Arc<Dictionary> {
        &self.dict
    }

    /// 解析で最初に触れる辞書のページを先に読み込む
    ///
    /// mmap した辞書は触れたページから読み込まれるので、起動直後の最初の解析が遅くなる。
    /// 待ち時間を先に払っておきたいとき（サーバーの起動時など）に呼ぶ。
    pub fn prewarm(&self) {
        self.dict.prewarm();
    }

    /// テキストを形態素解析（文分割で高速化）
    ///
    /// # Panics
    /// 辞書に不正な参照を見つけたとき（壊れた辞書）。`hasami info --verify` で検証済みの辞書では
    /// 起きない。検証していない辞書を扱うなら [`Analyzer::try_tokenize`] を使う。
    pub fn tokenize(&mut self, input: &str) -> Vec<Token> {
        self.try_tokenize(input)
            .unwrap_or_else(|e| panic!("hasami: {e}"))
    }

    /// テキストを形態素解析する。辞書に不正な参照を見つけたらエラーを返す
    pub fn try_tokenize(&mut self, input: &str) -> Result<Vec<Token>, DictError> {
        let mut tokens = Vec::with_capacity(input.len() / 3);
        self.tokenize_chunks(input, 0, &mut tokens)?;
        Ok(tokens)
    }

    /// 入力を文末記号・改行の直後で区間に分け、区間ごとに解析する（ラティスを小さく保つ）
    ///
    /// 区切りは [`Splitter::chunk_ends`]。文末記号を含む語（`Hey!Say!JUMP`、`Yahoo!ニュース` など、
    /// 例外表の語）の内側では区切らないので、辞書の語が前分割で割れない。トークンの位置は
    /// `offset` を足した入力全体のバイト位置にする。
    fn tokenize_chunks(
        &mut self,
        input: &str,
        offset: usize,
        out: &mut Vec<Token>,
    ) -> Result<(), DictError> {
        let mut start = 0;
        for end in self.splitter.chunk_ends(input) {
            let tokens = self.workspace.tokenize(&input[start..end], &self.dict)?;
            out.extend(tokens.into_iter().map(|mut t| {
                t.start += offset + start;
                t.end += offset + start;
                t
            }));
            start = end;
        }
        Ok(())
    }

    /// テキストを文に分け（[`crate::sentence`] の規則）、文ごとの範囲とトークン列を返す
    ///
    /// トークンの `start` / `end` は入力全体のバイト位置。文の前後の空白は解析しない。
    ///
    /// # Panics
    /// [`Analyzer::tokenize`] と同じ
    pub fn tokenize_sentences(
        &mut self,
        text: &str,
        options: &SplitOptions<'_>,
    ) -> Vec<(Sentence, Vec<Token>)> {
        self.try_tokenize_sentences(text, options)
            .unwrap_or_else(|e| panic!("hasami: {e}"))
    }

    /// テキストを文に分けて文ごとに解析する。辞書に不正な参照を見つけたらエラーを返す
    pub fn try_tokenize_sentences(
        &mut self,
        text: &str,
        options: &SplitOptions<'_>,
    ) -> Result<Vec<(Sentence, Vec<Token>)>, DictError> {
        let splitter = Splitter::new(options);
        let mut out = Vec::new();
        for sentence in splitter.split(text) {
            let mut tokens = Vec::new();
            let range = sentence.range.clone();
            self.tokenize_chunks(&text[range.clone()], range.start, &mut tokens)?;
            out.push((sentence, tokens));
        }
        Ok(out)
    }

    /// 複数テキストをバッチ処理
    ///
    /// # Panics
    /// [`Analyzer::tokenize`] と同じ
    pub fn tokenize_batch(&mut self, inputs: &[&str]) -> Vec<Vec<Token>> {
        inputs.iter().map(|input| self.tokenize(input)).collect()
    }

    /// 複数テキストをバッチ処理する。辞書に不正な参照を見つけたらエラーを返す
    pub fn try_tokenize_batch(&mut self, inputs: &[&str]) -> Result<Vec<Vec<Token>>, DictError> {
        inputs
            .iter()
            .map(|input| self.try_tokenize(input))
            .collect()
    }
}

/// 既定の辞書の場所を探す
///
/// 1. 環境変数 `HASAMI_DICT`（辞書ファイルのパス）。設定されていて辞書が無ければ、ほかを探さずにエラー
/// 2. `$XDG_DATA_HOME/hasami/`（未設定なら `~/.local/share/hasami/`）の `*.hsd`。複数あれば推奨順
///    （ipadic-neologd-sudachi → ipadic-neologd → ipadic → そのほかの名前順）
///
/// 見つからなければ、探した場所を並べた [`DictError::NotFound`] を返す。
pub fn default_dict_path() -> Result<PathBuf, DictError> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        });
    find_dict(std::env::var_os(DICT_ENV), data_home)
}

/// [`default_dict_path`] の本体（環境変数の値を受け取る。テストで環境を書き換えずに済むように分ける）
fn find_dict(env_dict: Option<OsString>, data_home: Option<PathBuf>) -> Result<PathBuf, DictError> {
    if let Some(path) = env_dict.filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(DictError::NotFound(vec![format!(
            "{DICT_ENV}={}",
            path.display()
        )]));
    }
    let mut searched = vec![format!("{DICT_ENV} (not set)")];
    if let Some(dir) = data_home.map(|d| d.join("hasami")) {
        if let Some(path) = PREFERRED_DICTS
            .iter()
            .map(|name| dir.join(name))
            .find(|p| p.is_file())
        {
            return Ok(path);
        }
        let mut others: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "hsd") && p.is_file())
            .collect();
        others.sort();
        if let Some(path) = others.into_iter().next() {
            return Ok(path);
        }
        searched.push(format!("{}/*.hsd", dir.display()));
    }
    Err(DictError::NotFound(searched))
}

/// MeCab互換の出力フォーマット
pub fn format_mecab(tokens: &[Token]) -> String {
    let mut output = String::with_capacity(tokens.len() * 48 + 4);
    for token in tokens {
        output.push_str(&token.surface);
        output.push('\t');
        output.push_str(&token.pos);
        if !token.base_form.is_empty() {
            output.push(',');
            output.push_str(&token.base_form);
        }
        if !token.reading.is_empty() {
            output.push(',');
            output.push_str(&token.reading);
        }
        if !token.pronunciation.is_empty() {
            output.push(',');
            output.push_str(&token.pronunciation);
        }
        output.push('\n');
    }
    output.push_str("EOS\n");
    output
}

/// Wakachi（分かち書き）出力
pub fn format_wakachi(tokens: &[Token]) -> String {
    let mut output = String::with_capacity(tokens.len() * 4);
    for (i, t) in tokens.iter().enumerate() {
        if i > 0 {
            output.push(' ');
        }
        output.push_str(&t.surface);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};

    fn make_analyzer() -> Analyzer {
        let mut builder = DictBuilder::new();
        let words = vec![
            ("私", 1, 1, 3000, "名詞,代名詞,一般,*", "ワタシ"),
            ("は", 2, 2, 4000, "助詞,係助詞,*,*", "ハ"),
            ("猫", 3, 3, 3500, "名詞,一般,*,*", "ネコ"),
            ("です", 4, 4, 4000, "助動詞,*,*,*", "デス"),
        ];

        for (surface, lid, rid, cost, pos, reading) in words {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: lid,
                right_id: rid,
                cost,
                pos: pos.into(),
                base_form: surface.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
                ..Default::default()
            });
        }

        Analyzer::from_dict(builder.build().unwrap())
    }

    #[test]
    fn test_basic_tokenize() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私は猫です");

        assert!(!tokens.is_empty());
        let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(surfaces, vec!["私", "は", "猫", "です"]);
    }

    #[test]
    fn test_wakachi() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私は猫です");
        let result = format_wakachi(&tokens);
        assert_eq!(result, "私 は 猫 です");
    }

    #[test]
    fn test_empty_input() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_workspace_reuse_perf() {
        let mut analyzer = make_analyzer();
        for _ in 0..100 {
            let tokens = analyzer.tokenize("私は猫です");
            assert_eq!(tokens.len(), 4);
        }
    }

    // --- 追加テスト ---

    #[test]
    fn test_chunking_does_not_split_exception_words() {
        // 文末記号を含む語（例外表の語）は、前分割で割れずに 1 語として引ける
        let mut builder = DictBuilder::new();
        for (surface, pos) in [
            ("Hey!Say!JUMP", "名詞,固有名詞,組織,*"),
            ("の", "助詞,連体化,*,*"),
            ("ライブ", "名詞,一般,*,*"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                cost: 100,
                pos: pos.into(),
                base_form: surface.into(),
                ..Default::default()
            });
        }
        let mut analyzer = Analyzer::from_dict(builder.build().unwrap());
        let tokens = analyzer.tokenize("Hey!Say!JUMPのライブ。");
        let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
        assert_eq!(surfaces[..3], ["Hey!Say!JUMP", "の", "ライブ"]);
    }

    #[test]
    fn test_tokenize_sentences_returns_ranges_and_absolute_positions() {
        let mut analyzer = make_analyzer();
        let text = "私は猫です。  「私は猫です？」と私は猫です。";
        let sentences = analyzer.tokenize_sentences(text, &SplitOptions::default());
        let ranges: Vec<&str> = sentences.iter().map(|(s, _)| &text[s.range.clone()]).collect();
        assert_eq!(ranges, ["私は猫です。", "「私は猫です？」と私は猫です。"]);
        for (sentence, tokens) in &sentences {
            assert_eq!(tokens.first().unwrap().start, sentence.range.start);
            assert_eq!(tokens.last().unwrap().end, sentence.range.end);
            for t in tokens {
                assert_eq!(&*t.surface, &text[t.start..t.end]);
            }
        }
    }

    #[test]
    fn test_find_dict_order_and_errors() {
        let dir = std::env::temp_dir().join(format!("hasami-find-dict-{}", std::process::id()));
        let hasami = dir.join("hasami");
        std::fs::create_dir_all(&hasami).unwrap();

        // 何も無ければ、探した場所を並べて NotFound
        match find_dict(None, Some(dir.clone())) {
            Err(DictError::NotFound(searched)) => {
                assert!(searched.iter().any(|s| s.contains("hasami")), "{searched:?}")
            }
            other => panic!("{other:?}"),
        }
        // 推奨順の辞書が無ければ、ほかの .hsd を名前順で
        std::fs::write(hasami.join("zzz.hsd"), b"").unwrap();
        std::fs::write(hasami.join("custom.hsd"), b"").unwrap();
        assert_eq!(find_dict(None, Some(dir.clone())).unwrap(), hasami.join("custom.hsd"));
        // 推奨順の辞書が優先
        std::fs::write(hasami.join("ipadic.hsd"), b"").unwrap();
        std::fs::write(hasami.join("ipadic-neologd-sudachi.hsd"), b"").unwrap();
        assert_eq!(
            find_dict(None, Some(dir.clone())).unwrap(),
            hasami.join("ipadic-neologd-sudachi.hsd")
        );
        // 環境変数が最優先。指した先が無ければ、ほかを探さずにエラー
        let explicit = hasami.join("custom.hsd");
        assert_eq!(
            find_dict(Some(explicit.clone().into()), Some(dir.clone())).unwrap(),
            explicit
        );
        let missing = hasami.join("missing.hsd");
        assert!(matches!(
            find_dict(Some(missing.into()), Some(dir.clone())),
            Err(DictError::NotFound(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_tokenize_with_sentence_boundary() {
        let mut analyzer = make_analyzer();
        // "。" is a sentence boundary, each segment should be analyzed independently
        let tokens = analyzer.tokenize("私は猫です。私は猫です。");
        // Tokens should include the boundary character as unknown
        let surfaces: Vec<&str> = tokens.iter().map(|t| &*t.surface).collect();
        // Both sentences should be tokenized
        assert!(surfaces.contains(&"私"));
        assert!(surfaces.contains(&"猫"));
    }

    #[test]
    fn test_tokenize_only_boundary_chars() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("。！？");
        // Boundary chars should still produce tokens (as unknown words)
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_format_mecab_basic() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私は猫です");
        let output = format_mecab(&tokens);
        assert!(output.contains("私\t"));
        assert!(output.ends_with("EOS\n"));
    }

    #[test]
    fn test_format_mecab_empty() {
        let output = format_mecab(&[]);
        assert_eq!(output, "EOS\n");
    }

    #[test]
    fn test_format_wakachi_empty() {
        let output = format_wakachi(&[]);
        assert_eq!(output, "");
    }

    #[test]
    fn test_tokenize_batch() {
        let mut analyzer = make_analyzer();
        let inputs = vec!["私は猫です", "私は猫です"];
        let results = analyzer.tokenize_batch(&inputs);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), results[1].len());
        // Same input should produce same output
        let s0: Vec<&str> = results[0].iter().map(|t| &*t.surface).collect();
        let s1: Vec<&str> = results[1].iter().map(|t| &*t.surface).collect();
        assert_eq!(s0, s1);
    }

    #[test]
    fn test_tokenize_batch_empty() {
        let mut analyzer = make_analyzer();
        let inputs: Vec<&str> = vec![];
        let results = analyzer.tokenize_batch(&inputs);
        assert!(results.is_empty());
    }

    #[test]
    fn test_token_positions() {
        let mut analyzer = make_analyzer();
        let input = "私は猫です";
        let tokens = analyzer.tokenize(input);
        // Verify token positions cover the entire input without gaps
        let mut pos = 0;
        for t in &tokens {
            assert_eq!(t.start, pos, "Gap in token positions at byte {}", pos);
            assert!(t.end > t.start);
            // Verify surface matches the input slice
            assert_eq!(&*t.surface, &input[t.start..t.end]);
            pos = t.end;
        }
        assert_eq!(pos, input.len());
    }

    #[test]
    fn test_token_fields() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私は猫です");
        let watashi = &tokens[0];
        assert_eq!(&*watashi.surface, "私");
        assert_eq!(&*watashi.pos, "名詞,代名詞,一般,*");
        assert_eq!(&*watashi.base_form, "私");
        assert_eq!(&*watashi.reading, "ワタシ");
        assert!(watashi.is_known);
    }

    #[test]
    fn test_unknown_word_handling() {
        let mut analyzer = make_analyzer();
        // "犬" is not in our test dictionary, should be handled as unknown
        let tokens = analyzer.tokenize("犬");
        assert!(!tokens.is_empty());
        let dog = &tokens[0];
        assert_eq!(&*dog.surface, "犬");
        assert!(!dog.is_known);
    }

    #[test]
    fn test_whitespace_only() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("   ");
        // Whitespace should be tokenized as unknown words
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_mixed_known_unknown() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私はDOGです");
        assert!(tokens.len() >= 3);
        // "私" should be known, "DOG" unknown, "です" known
        let known: Vec<bool> = tokens.iter().map(|t| t.is_known).collect();
        assert!(known[0]); // 私
    }

    #[test]
    fn test_format_mecab_with_reading() {
        let mut analyzer = make_analyzer();
        let tokens = analyzer.tokenize("私は猫です");
        let output = format_mecab(&tokens);
        // Should include reading
        assert!(output.contains("ワタシ"));
    }

    #[test]
    fn test_tokenize_consistency() {
        let mut analyzer = make_analyzer();
        // Same input should always produce same output
        let t1 = analyzer.tokenize("私は猫です");
        let t2 = analyzer.tokenize("私は猫です");
        assert_eq!(t1.len(), t2.len());
        for (a, b) in t1.iter().zip(t2.iter()) {
            assert_eq!(&*a.surface, &*b.surface);
        }
    }
}
