//! hasami Python バインディング (PyO3)

use ::hasami::analyzer::{format_mecab, format_wakachi, Analyzer as RustAnalyzer};
use ::hasami::dict::DictBuilder as RustDictBuilder;
use ::hasami::lattice::Token as RustToken;
use pyo3::exceptions::PyIOError;
use pyo3::prelude::*;

/// 形態素解析結果のトークン
#[pyclass(from_py_object)]
#[derive(Clone)]
struct Token {
    #[pyo3(get)]
    surface: String,
    #[pyo3(get)]
    start: usize,
    #[pyo3(get)]
    end: usize,
    #[pyo3(get)]
    pos: String,
    #[pyo3(get)]
    base_form: String,
    #[pyo3(get)]
    reading: String,
    #[pyo3(get)]
    pronunciation: String,
    #[pyo3(get)]
    word_cost: i16,
    #[pyo3(get)]
    is_known: bool,
}

#[pymethods]
impl Token {
    fn __repr__(&self) -> String {
        format!("Token(surface='{}', pos='{}')", self.surface, self.pos)
    }

    fn __str__(&self) -> String {
        format!("{}\t{}", self.surface, self.pos)
    }
}

impl From<RustToken> for Token {
    fn from(t: RustToken) -> Self {
        Token {
            surface: t.surface.to_string(),
            start: t.start,
            end: t.end,
            pos: t.pos.to_string(),
            base_form: t.base_form.to_string(),
            reading: t.reading.to_string(),
            pronunciation: t.pronunciation.to_string(),
            word_cost: t.word_cost,
            is_known: t.is_known,
        }
    }
}

/// 形態素解析器
#[pyclass]
struct Analyzer {
    inner: RustAnalyzer,
}

#[pymethods]
impl Analyzer {
    /// .hsd 辞書ファイルからアナライザーを生成
    #[new]
    fn new(dict_path: &str) -> PyResult<Self> {
        let analyzer = RustAnalyzer::load(dict_path)
            .map_err(|e| PyIOError::new_err(format!("Failed to load dictionary: {}", e)))?;
        Ok(Analyzer { inner: analyzer })
    }

    /// テキストを形態素解析
    fn tokenize(&mut self, text: &str) -> Vec<Token> {
        self.inner
            .tokenize(text)
            .into_iter()
            .map(Token::from)
            .collect()
    }

    /// 複数テキストをバッチ処理
    fn tokenize_batch(&mut self, texts: Vec<String>) -> Vec<Vec<Token>> {
        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        self.inner
            .tokenize_batch(&refs)
            .into_iter()
            .map(|tokens| tokens.into_iter().map(Token::from).collect())
            .collect()
    }

    /// MeCab互換形式で出力
    fn to_mecab(&mut self, text: &str) -> String {
        let tokens = self.inner.tokenize(text);
        format_mecab(&tokens)
    }

    /// 分かち書き
    fn wakachi(&mut self, text: &str) -> String {
        let tokens = self.inner.tokenize(text);
        format_wakachi(&tokens)
    }
}

/// 辞書ビルダー
#[pyclass]
struct DictBuilder {
    inner: Option<RustDictBuilder>,
}

#[pymethods]
impl DictBuilder {
    #[new]
    fn new() -> Self {
        DictBuilder {
            inner: Some(RustDictBuilder::new()),
        }
    }

    /// 既存の .hsd 辞書からエントリをインポート
    fn load_hsd(&mut self, path: &str) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyIOError::new_err("Builder already consumed"))?
            .load_hsd(path)
            .map_err(|e| PyIOError::new_err(format!("Failed to load dictionary: {}", e)))
    }

    /// CSVディレクトリからエントリを追加
    fn add_csv_dir(&mut self, dir: &str) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyIOError::new_err("Builder already consumed"))?
            .add_csv_dir(dir)
            .map_err(|e| PyIOError::new_err(format!("Failed to load CSV: {}", e)))
    }

    /// matrix.def を読み込み
    fn load_matrix(&mut self, path: &str) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyIOError::new_err("Builder already consumed"))?
            .load_matrix(path)
            .map_err(|e| PyIOError::new_err(format!("Failed to load matrix: {}", e)))
    }

    /// char.def を読み込み
    fn load_char_def(&mut self, path: &str) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyIOError::new_err("Builder already consumed"))?
            .load_char_def(path)
            .map_err(|e| PyIOError::new_err(format!("Failed to load char.def: {}", e)))
    }

    /// unk.def を読み込み
    fn load_unk(&mut self, path: &str) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyIOError::new_err("Builder already consumed"))?
            .load_unk(path)
            .map_err(|e| PyIOError::new_err(format!("Failed to load unk.def: {}", e)))
    }

    /// 辞書をビルドして .hsd ファイルに保存
    fn build(&mut self, output_path: &str) -> PyResult<()> {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| PyIOError::new_err("Builder already consumed"))?;
        let dict = builder.build();
        let mmap_builder = ::hasami::mmap_dict::MmapDictBuilder::from_dictionary(&dict);
        mmap_builder
            .write(output_path)
            .map_err(|e| PyIOError::new_err(format!("Failed to save dictionary: {}", e)))
    }
}

/// hasami Python モジュール
#[pymodule]
fn hasami(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Analyzer>()?;
    m.add_class::<Token>()?;
    m.add_class::<DictBuilder>()?;
    Ok(())
}
