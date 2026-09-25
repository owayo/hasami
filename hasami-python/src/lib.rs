//! hasami Python バインディング (PyO3)

use ::hasami::analyzer::{Analyzer as RustAnalyzer, format_mecab, format_wakachi};
use ::hasami::dict::DictBuilder as RustDictBuilder;
use ::hasami::hsd::DictError;
use ::hasami::lattice::Token as RustToken;
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;

/// 辞書のエラーを Python の例外にする（ファイルの読み書きは IOError、それ以外は ValueError）
fn dict_error(context: &str, e: DictError) -> PyErr {
    match e {
        DictError::Io(e) => PyIOError::new_err(format!("{context}: {e}")),
        e => PyValueError::new_err(format!("{context}: {e}")),
    }
}

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
    /// 活用型（活用しない語・未知語は空文字列）
    #[pyo3(get)]
    conj_type: String,
    /// 活用形（活用しない語・未知語は空文字列）
    #[pyo3(get)]
    conj_form: String,
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
    /// 辞書の品詞体系をそろえた粗い品詞（"Noun"、"CaseParticle"、"Period" など。Rust の CoarsePos の名前）
    #[pyo3(get)]
    coarse_pos: String,
    /// 否定の形態素か（助動詞「ない」「ぬ」「ん」「ず」、形容詞「ない」）
    #[pyo3(get)]
    is_negation: bool,
    /// 発音（無ければ読み）から数えたモーラ数
    #[pyo3(get)]
    mora_count: usize,
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
            coarse_pos: format!("{:?}", t.coarse_pos()),
            is_negation: t.is_negation(),
            mora_count: t.mora_count(),
            surface: t.surface.to_string(),
            start: t.start,
            end: t.end,
            pos: t.pos.to_string(),
            conj_type: t.conj_type.to_string(),
            conj_form: t.conj_form.to_string(),
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
            .map_err(|e| dict_error("Failed to load dictionary", e))?;
        Ok(Analyzer { inner: analyzer })
    }

    /// 辞書を共有しつつ、新しいワークスペースを持つアナライザーを複製
    ///
    /// 複数スレッドで並行に解析する場合、各スレッドがそれぞれ自分の Analyzer
    /// インスタンス（クローン）を持つことで、辞書（mmap）はゼロコピー共有しつつ
    /// ラティスワークスペースだけ独立に持てる。
    fn clone_for_worker(&self) -> Self {
        Analyzer {
            inner: self.inner.clone(),
        }
    }

    /// 解析で最初に触れる辞書のページを先に読み込む
    ///
    /// 起動直後の最初の解析が遅くなるのを避けたいとき（サーバーの起動時など）に呼ぶ。
    fn prewarm(&self, py: Python<'_>) {
        py.detach(|| self.inner.prewarm());
    }

    /// テキストを形態素解析（GIL を解放して実行）。壊れた辞書では ValueError
    fn tokenize(&mut self, py: Python<'_>, text: &str) -> PyResult<Vec<Token>> {
        let tokens = py
            .detach(|| self.inner.try_tokenize(text))
            .map_err(|e| dict_error("Failed to tokenize", e))?;
        Ok(tokens.into_iter().map(Token::from).collect())
    }

    /// 複数テキストをバッチ処理（GIL を解放して実行）
    fn tokenize_batch(&mut self, py: Python<'_>, texts: Vec<String>) -> PyResult<Vec<Vec<Token>>> {
        let results = py
            .detach(|| {
                let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                self.inner.try_tokenize_batch(&refs)
            })
            .map_err(|e| dict_error("Failed to tokenize", e))?;
        Ok(results
            .into_iter()
            .map(|tokens| tokens.into_iter().map(Token::from).collect())
            .collect())
    }

    /// MeCab互換形式で出力
    #[allow(clippy::wrong_self_convention)]
    fn to_mecab(&mut self, py: Python<'_>, text: &str) -> PyResult<String> {
        py.detach(|| self.inner.try_tokenize(text).map(|t| format_mecab(&t)))
            .map_err(|e| dict_error("Failed to tokenize", e))
    }

    /// 分かち書き
    fn wakachi(&mut self, py: Python<'_>, text: &str) -> PyResult<String> {
        py.detach(|| self.inner.try_tokenize(text).map(|t| format_wakachi(&t)))
            .map_err(|e| dict_error("Failed to tokenize", e))
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
            .map_err(|e| dict_error("Failed to load dictionary", e))
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
    ///
    /// 接続行列の範囲外の文脈 ID を持つエントリなど、辞書にできない入力があれば
    /// ビルダーを消費せずに ValueError を送出する。
    fn build(&mut self, output_path: &str) -> PyResult<()> {
        let builder = self
            .inner
            .as_ref()
            .ok_or_else(|| PyIOError::new_err("Builder already consumed"))?;
        builder
            .check_context_ids()
            .map_err(|e| PyValueError::new_err(format!("Invalid dictionary: {}", e)))?;
        let opts = builder.write_options();
        builder
            .write_hsd(output_path, &opts, |_, _| {})
            .map_err(|e| dict_error("Failed to build dictionary", e))?;
        self.inner = None;
        Ok(())
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
