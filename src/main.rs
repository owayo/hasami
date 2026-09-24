//! hasami CLI - 日本語形態素解析コマンドラインツール

use clap::{Parser, Subcommand};
use hasami::analyzer::{Analyzer, push_mecab, push_wakachi};
use hasami::dict::DictBuilder;
use hasami::hsd::meta;
use hasami::hsd::{Dictionary, Meta, PosScheme, WriteOptions};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "hasami", version, about = "高速日本語形態素解析エンジン")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// 出力形式
#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    /// MeCab互換形式
    Mecab,
    /// 分かち書き形式
    Wakachi,
    /// JSON形式
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// 辞書を構築（MeCab形式のCSVから）
    Build {
        /// 辞書CSVファイルのディレクトリ
        #[arg(short, long)]
        input: PathBuf,

        /// 出力辞書ファイル (.hsd)
        #[arg(short, long)]
        output: PathBuf,

        #[command(flatten)]
        write: WriteArgs,
    },

    /// 既存辞書にMeCab形式CSVを追加（マージ）
    Merge {
        /// 既存の .hsd 辞書ファイル
        #[arg(short, long)]
        dict: PathBuf,

        /// 追加するCSVファイルまたはディレクトリ
        #[arg(short, long)]
        input: PathBuf,

        /// 出力辞書ファイル (.hsd)。省略時は既存辞書を上書き
        #[arg(short, long)]
        output: Option<PathBuf>,

        #[command(flatten)]
        write: WriteArgs,
    },

    /// テキストを形態素解析
    Tokenize {
        /// 辞書ファイルのパス (.hsd)。省略時は環境変数 HASAMI_DICT、
        /// $XDG_DATA_HOME/hasami/（未設定なら ~/.local/share/hasami/）の *.hsd の順に探す
        #[arg(short, long)]
        dict: Option<PathBuf>,

        /// 出力形式
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Mecab)]
        format: OutputFormat,

        /// 解析するテキスト（省略時は標準入力から読み込み）
        text: Option<String>,

        /// 標準入力の行を並列に解析するスレッド数（0 = CPU の数）。出力の順序は入力どおり
        #[arg(short = 'j', long, default_value_t = 0)]
        threads: usize,
    },

    /// ベンチマーク実行
    Bench {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// テストテキスト（--file を指定したときは使わない）
        #[arg(short, long, default_value = "東京都に住んでいる人々が増えている。")]
        text: String,

        /// 1 行 1 文のテキストファイル。指定すると全行の解析を 1 回として時間を測る
        #[arg(short, long, value_name = "PATH")]
        file: Option<PathBuf>,

        /// 繰り返し回数（既定: --text は 10000 回、--file はファイル全体を 3 回）
        #[arg(short, long)]
        iterations: Option<NonZeroUsize>,
    },

    /// 辞書情報を表示
    Info {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// 全体を検証する（全 trie ノード・全エントリ群・全素性レコード。時間がかかる）
        #[arg(long)]
        verify: bool,
    },

    /// 文分割の例外表（文末記号を含む語）を辞書から抽出する
    ///
    /// 出力は src/sentence/builtin_exceptions.txt と同じ書式（先頭にコメント、1 行 1 語）。
    /// 組み込みの表は推奨辞書から作る:
    /// `hasami export-sentence-exceptions --dict dict/ipadic-neologd-sudachi.hsd --output src/sentence/builtin_exceptions.txt`
    ExportSentenceExceptions {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// 出力ファイル。省略時は標準出力
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 辞書のエントリを MeCab 形式の CSV（13 列）に書き出す
    ///
    /// 書き出すのはエントリ（lexicon）だけで、matrix.def・char.def・unk.def は出さない。
    /// 並びは表層形のバイト順（同じ表層形の中は辞書の順）。
    Export {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// 出力 CSV ファイル。省略時は標準出力
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 辞書の壊れた読み・誤読エントリを修復
    Repair {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// 出力辞書ファイル (.hsd)。省略時は既存辞書を上書き
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 接続行列の範囲外の文脈 ID を持つエントリを削除する
        /// (解析時に接続コスト 0 として扱われ、不当に有利になるエントリ)
        #[arg(long)]
        drop_invalid_context_ids: bool,

        /// 壊れた発音の修復 (常時) を行わない。削除リストだけを適用したいときに使う
        /// (発音の修復は、読みも発音もカタカナでない語 (ラテン文字の読み等) の読みを空にする。記号は変えない)
        #[arg(long)]
        no_pronunciation_repair: bool,

        /// 活用語・機能語と衝突する異表記エントリを削除する
        /// (「高い」→「高位(コウイ)」等の表記ゆれ正規化エントリ)
        #[arg(long)]
        drop_ortho_variants: bool,

        /// 漢数字を数以外に読ませるエントリを削除する
        /// (「十五(トウゴ)」「二十八(ツチヤ)」等の人名・地名エントリ)
        #[arg(long)]
        drop_numeral_misreadings: bool,

        /// 削除するエントリを列挙した CSV (`表層形,読み[,品詞]`)。複数指定可。
        /// 3 列目の品詞 (例 "名詞,固有名詞,人名") を書くと、その品詞で始まるエントリだけを削除する
        #[arg(long, value_name = "CSV")]
        remove: Vec<PathBuf>,

        /// 一般語に付いた「名詞,固有名詞,一般」を一般名詞に降格する。値は IPAdic 単体の辞書 (.hsd)。
        /// 表層形をこの辞書で解析して「一般名詞・サ変接続・形容動詞語幹 + 一般名詞を作る接尾辞」に
        /// 分かれ、読みがその接尾辞の読みで終わる語 (成果物・多角的・可視化・安全性・担当者 等) を、
        /// 名詞,一般 (語末が「化」なら名詞,サ変接続、「的」なら名詞,形容動詞語幹) にし、文脈 ID も
        /// その品詞のものにする。参照辞書は修復する辞書と同じ接続行列を持つこと
        #[arg(long, value_name = "IPADIC_HSD")]
        demote_common_proper_nouns: Option<PathBuf>,

        /// 修復後に追加マージする MeCab 形式 CSV またはそのディレクトリ。複数指定可
        #[arg(long, value_name = "PATH")]
        merge: Vec<PathBuf>,

        #[command(flatten)]
        write: WriteArgs,
    },
}

/// 辞書を書き出すコマンド共通の設定
#[derive(clap::Args)]
struct WriteArgs {
    /// メタデータを設定する（`key=value`、複数指定可）。`name`・`pos_scheme`（ipadic / unidic）・
    /// `sources` など。build では `name` の既定は出力ファイル名、`pos_scheme` の既定は ipadic。
    /// merge・repair は入力辞書のメタデータを引き継いで上書きする
    #[arg(long = "meta", value_name = "KEY=VALUE")]
    meta: Vec<String>,

    /// 同じ表層形・同じ文脈 ID の中でコストが最小でないエントリを除く（配布用の最終辞書向け）。
    /// 1-best の解析結果は変わらない。除いた辞書は merge・repair の入力にできない
    #[arg(long)]
    prune_dominated: bool,
}

fn main() {
    // io::Error の Debug 表示は改行をエスケープして 1 行に潰すので、Display で出す
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            input,
            output,
            write,
        } => cmd_build(&input, &output, &write),
        Commands::Merge {
            dict,
            input,
            output,
            write,
        } => cmd_merge(&dict, &input, output.as_deref(), &write),
        Commands::Tokenize {
            dict,
            format,
            text,
            threads,
        } => {
            let dict = match dict {
                Some(path) => path,
                None => hasami::analyzer::default_dict_path()?,
            };
            let threads = match threads {
                0 => std::thread::available_parallelism().map_or(1, NonZeroUsize::get),
                n => n,
            };
            cmd_tokenize(&dict, format, text, threads)
        }
        Commands::ExportSentenceExceptions { dict, output } => {
            cmd_export_sentence_exceptions(&dict, output.as_deref())
        }
        Commands::Bench {
            dict,
            text,
            file,
            iterations,
        } => match file {
            Some(file) => cmd_bench_file(&dict, &file, iterations.map_or(3, NonZeroUsize::get)),
            None => cmd_bench(&dict, &text, iterations.map_or(10_000, NonZeroUsize::get)),
        },
        Commands::Info { dict, verify } => cmd_info(&dict, verify),
        Commands::Export { dict, output } => cmd_export(&dict, output.as_deref()),
        Commands::Repair {
            dict,
            output,
            drop_invalid_context_ids,
            no_pronunciation_repair,
            drop_ortho_variants,
            drop_numeral_misreadings,
            remove,
            demote_common_proper_nouns,
            merge,
            write,
        } => cmd_repair(
            &dict,
            output.as_deref(),
            &write,
            RepairOptions {
                drop_invalid_context_ids,
                repair_pronunciation: !no_pronunciation_repair,
                drop_ortho_variants,
                drop_numeral_misreadings,
                remove: &remove,
                demote_common_proper_nouns: demote_common_proper_nouns.as_deref(),
                merge: &merge,
            },
        ),
    }
}

fn ensure_hsd_extension(path: &Path) -> PathBuf {
    if path.extension().is_none_or(|ext| ext != "hsd") {
        path.with_extension("hsd")
    } else {
        path.to_path_buf()
    }
}

fn make_trie_progress_bar() -> ProgressBar {
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} Building trie [{bar:40.cyan/blue}] {pos}/{len} keys ({percent}%) [{elapsed_precise}<{eta_precise}, {per_sec}]"
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb
}

/// `--meta KEY=VALUE` をメタデータに反映する
fn apply_meta_args(meta: &mut Meta, args: &[String]) -> io::Result<()> {
    for kv in args {
        let (key, value) = kv.split_once('=').ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("--meta expects KEY=VALUE, got `{kv}`"),
            )
        })?;
        meta.set(key, value)?;
    }
    Ok(())
}

/// メタデータの `repairs` に今回の操作を書き足す（これまでの操作の後ろにカンマでつなぐ）
fn append_repairs(meta: &mut Meta, ops: &[String]) -> io::Result<()> {
    if ops.is_empty() {
        return Ok(());
    }
    let mut all: Vec<String> = meta
        .get(meta::KEY_REPAIRS)
        .filter(|v| !v.is_empty())
        .map(|v| v.split(',').map(String::from).collect())
        .unwrap_or_default();
    all.extend(ops.iter().cloned());
    meta.set(meta::KEY_REPAIRS, &all.join(","))?;
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// 辞書を書き出して概要を表示する
fn write_dict(
    builder: &DictBuilder,
    output: &Path,
    mut meta: Meta,
    args: &WriteArgs,
    verb: &str,
    start: Instant,
) -> io::Result<()> {
    apply_meta_args(&mut meta, &args.meta)?;
    let opts = WriteOptions {
        meta,
        prune_dominated: args.prune_dominated,
    };
    eprintln!("Building trie with {} entries...", builder.entry_count());
    let pb = make_trie_progress_bar();
    let stats = builder.write_hsd(output, &opts, |done, total| {
        pb.set_length(total as u64);
        pb.set_position(done as u64);
    });
    pb.finish_and_clear();
    let stats = stats?;
    let pruned = if args.prune_dominated {
        format!(", {} dominated entries pruned", stats.pruned)
    } else {
        String::new()
    };
    eprintln!(
        "{verb} in {:.2}s: {} entries ({} surfaces, {} feature records{pruned}), {:.1} MB -> {}",
        start.elapsed().as_secs_f64(),
        stats.entries,
        stats.surfaces,
        stats.features,
        stats.bytes as f64 / 1024.0 / 1024.0,
        output.display()
    );
    Ok(())
}

fn cmd_build(input: &Path, output: &Path, write: &WriteArgs) -> io::Result<()> {
    eprintln!("Building dictionary from: {}", input.display());
    let start = Instant::now();

    let mut builder = DictBuilder::new();

    // matrix.def があれば読み込み。CSV より先に読むと、範囲外の文脈 ID を持つ行を
    // 行番号付きで検出できる
    let matrix_path = input.join("matrix.def");
    if matrix_path.exists() {
        builder.load_matrix(&matrix_path)?;
    } else {
        eprintln!("Warning: matrix.def not found, all connection costs are 0");
    }

    // CSVファイルを読み込み
    builder.add_csv_dir(input)?;

    // char.def があれば読み込み
    let char_def_path = input.join("char.def");
    if char_def_path.exists() {
        builder.load_char_def(&char_def_path)?;
    }

    // unk.def があれば読み込み
    let unk_path = input.join("unk.def");
    if unk_path.exists() {
        builder.load_unk(&unk_path)?;
    }
    builder.check_context_ids()?;

    let output = ensure_hsd_extension(output);
    let name = output
        .file_stem()
        .map_or_else(|| "hasami".into(), |s| s.to_string_lossy().into_owned());
    let meta = Meta::new(&name, PosScheme::Ipadic);
    write_dict(&builder, &output, meta, write, "Dictionary built", start)
}

fn cmd_merge(
    dict_path: &Path,
    input: &Path,
    output: Option<&Path>,
    write: &WriteArgs,
) -> io::Result<()> {
    eprintln!("Loading existing dictionary: {}", dict_path.display());
    let start = Instant::now();

    let mut builder = DictBuilder::new();

    // 既存辞書をインポート
    builder.load_hsd(dict_path)?;
    let old_count = builder.entry_count();

    // CSVを追加
    if input.is_dir() {
        builder.add_csv_dir(input)?;
    } else {
        builder.add_csv(input)?;
    }
    let new_count = builder.entry_count() - old_count;
    eprintln!("Added {} new entries", new_count);
    builder.check_context_ids()?;

    let output_path = output.map_or_else(|| dict_path.to_path_buf(), |p| p.to_path_buf());
    let output_path = ensure_hsd_extension(&output_path);
    let mut meta = builder.write_options().meta;
    append_repairs(&mut meta, &[format!("merge:{}", file_name(input))])?;
    write_dict(&builder, &output_path, meta, write, "Merged", start)
}

/// 標準入力を 1 回の read で読むバイト数の上限（ファイルを流し込むと毎回この大きさのブロックになる）
const READ_CHUNK: usize = 1 << 20;
/// 標準出力のバッファの大きさ
const WRITE_BUFFER: usize = 1 << 16;
/// これより短いブロックは並列に解析しない（スレッドを立てる費用の方が大きい）
const PARALLEL_MIN_BYTES: usize = 1 << 15;
/// 並列に解析するとき、スレッドが 1 度に取る行のまとまりのバイト数の目安
const PIECE_BYTES: usize = 1 << 14;

fn cmd_tokenize(
    dict_path: &Path,
    format: OutputFormat,
    text: Option<String>,
    threads: usize,
) -> io::Result<()> {
    let start = Instant::now();
    let analyzer = Analyzer::load(dict_path)?;
    eprintln!(
        "Dictionary loaded in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let stdout = io::stdout();
    let mut out = io::BufWriter::with_capacity(WRITE_BUFFER, stdout.lock());
    let mut workers = vec![Worker::new(analyzer)];

    if let Some(text) = text {
        let worker = &mut workers[0];
        let tokens = worker.analyzer.try_tokenize(&text)?;
        write_output(&mut worker.out, &mut worker.line, &tokens, format);
        out.write_all(&worker.out)?;
        return out.flush();
    }

    // 標準入力を完結した行のブロックごとに解析する。読み込みは待つことがあるので、その前にそれまでの
    // 出力を書き出す（行を送って結果を待つ相手とも詰まらない。大量の入力では読むたびに 1 回だけ書く）
    let mut stdin = io::stdin().lock();
    let mut pending: Vec<u8> = Vec::new();
    loop {
        out.flush()?;
        let len = pending.len();
        pending.resize(len + READ_CHUNK, 0);
        let n = loop {
            match stdin.read(&mut pending[len..]) {
                Ok(n) => break n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        };
        pending.truncate(len + n);
        let eof = n == 0;
        // 最後の改行までが完結した行（入力の終わりなら残りも 1 行）。前に読んだ残りには改行が無いので、
        // 今読んだ部分だけを探す（1 行がとても長い入力で、読み足すたびに全体を探し直さない）
        let end = if eof {
            pending.len()
        } else {
            match pending[len..].iter().rposition(|&b| b == b'\n') {
                Some(p) => len + p + 1,
                None => continue,
            }
        };
        match std::str::from_utf8(&pending[..end]) {
            Ok(block) => process_block(block, &mut workers, threads, format, &mut out)?,
            Err(e) => {
                // 壊れた UTF-8 を含む行の手前までは解析して出す（行ごとに読んでいたときと同じ）
                let valid = &pending[..e.valid_up_to()];
                let cut = valid.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
                let block = std::str::from_utf8(&valid[..cut]).map_err(io::Error::other)?;
                process_block(block, &mut workers, threads, format, &mut out)?;
                out.flush()?;
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                ));
            }
        }
        pending.drain(..end);
        if eof {
            break;
        }
    }
    out.flush()
}

/// 解析と書式化を受け持つワーカー（解析器と出力のバッファ）
struct Worker {
    analyzer: Analyzer,
    /// 書式化した出力
    out: Vec<u8>,
    /// 1 行分を書式化する作業用の文字列
    line: String,
}

impl Worker {
    fn new(analyzer: Analyzer) -> Self {
        Worker {
            analyzer,
            out: Vec::new(),
            line: String::new(),
        }
    }

    /// 行を順に解析して `out` に書く。空行（空白だけの行を含む）は飛ばす
    ///
    /// エラーのときは、それまでの行の出力を `out` に残して返す
    fn process(&mut self, lines: &[&str], format: OutputFormat) -> io::Result<()> {
        for line in lines {
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            let tokens = self.analyzer.try_tokenize(text)?;
            write_output(&mut self.out, &mut self.line, &tokens, format);
        }
        Ok(())
    }
}

/// 完結した行のブロックを解析して `out` に書く
///
/// ブロックが大きく `threads` が 2 以上なら、行を連続したまとまり（[`PIECE_BYTES`] ほど）に分け、
/// スレッドが空いた順にまとまりを取って解析する（P コアと E コアのように速さの違うコアが混ざっても
/// 遅いスレッドを待たない）。出力は入力の順に書く。エラーは入力の順で最初のものを返し、その手前の行の
/// 出力だけを書く（1 行ずつ順に解析したときと同じ出力とエラーになる）。
fn process_block(
    block: &str,
    workers: &mut Vec<Worker>,
    threads: usize,
    format: OutputFormat,
    out: &mut impl Write,
) -> io::Result<()> {
    let lines: Vec<&str> = block.lines().collect();
    if threads < 2 || block.len() < PARALLEL_MIN_BYTES {
        let worker = &mut workers[0];
        let result = worker.process(&lines, format);
        out.write_all(&worker.out)?;
        worker.out.clear();
        return result;
    }
    let pieces = split_pieces(&lines, PIECE_BYTES);
    let threads = threads.min(pieces.len());
    while workers.len() < threads {
        let analyzer = workers[0].analyzer.clone();
        workers.push(Worker::new(analyzer));
    }
    let next = AtomicUsize::new(0);
    let done: Vec<Mutex<Option<PieceResult>>> = pieces.iter().map(|_| Mutex::new(None)).collect();
    let run = |worker: &mut Worker| {
        loop {
            let k = next.fetch_add(1, Ordering::Relaxed);
            let Some(range) = pieces.get(k) else {
                break;
            };
            let result = worker.process(&lines[range.clone()], format);
            let output = std::mem::take(&mut worker.out);
            *done[k].lock().unwrap_or_else(PoisonError::into_inner) = Some((output, result));
        }
    };
    std::thread::scope(|s| {
        let (first, rest) = workers[..threads]
            .split_first_mut()
            .expect("at least one worker");
        for worker in rest {
            s.spawn(move || run(worker));
        }
        run(first);
    });
    for slot in done {
        let (output, result) = slot
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .expect("every piece is processed");
        out.write_all(&output)?;
        result?;
    }
    Ok(())
}

/// 並列に解析した行のまとまりの結果（書式化した出力と、解析のエラー）
type PieceResult = (Vec<u8>, io::Result<()>);

/// 行を、先頭から順に `piece_bytes` バイトほどずつの連続した区間に分ける
fn split_pieces(lines: &[&str], piece_bytes: usize) -> Vec<std::ops::Range<usize>> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (i, line) in lines.iter().enumerate() {
        bytes += line.len() + 1;
        if bytes >= piece_bytes {
            pieces.push(start..i + 1);
            start = i + 1;
            bytes = 0;
        }
    }
    if start < lines.len() {
        pieces.push(start..lines.len());
    }
    pieces
}

/// 1 行分の解析結果を書式化して `out` に足す。`line` は作業用
fn write_output(
    out: &mut Vec<u8>,
    line: &mut String,
    tokens: &[hasami::Token],
    format: OutputFormat,
) {
    match format {
        OutputFormat::Wakachi => {
            line.clear();
            push_wakachi(line, tokens);
            line.push('\n');
            out.extend_from_slice(line.as_bytes());
        }
        OutputFormat::Json => write_json(out, tokens),
        OutputFormat::Mecab => {
            line.clear();
            push_mecab(line, tokens);
            out.extend_from_slice(line.as_bytes());
        }
    }
}

const WRITE_VEC: &str = "writing to a Vec<u8> does not fail";

/// トークンの配列を 1 行の JSON で書く（キーはアルファベット順。serde_json の Value と同じ並び）
fn write_json(out: &mut Vec<u8>, tokens: &[hasami::Token]) {
    // 文字列のエスケープは serde_json に任せる（Vec<u8> への書き込みは失敗しない）
    fn string(out: &mut Vec<u8>, s: &str) {
        serde_json::to_writer(&mut *out, s).expect(WRITE_VEC);
    }
    out.push(b'[');
    for (i, t) in tokens.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        out.extend_from_slice(b"{\"base_form\":");
        string(out, &t.base_form);
        out.extend_from_slice(b",\"conj_form\":");
        string(out, &t.conj_form);
        out.extend_from_slice(b",\"conj_type\":");
        string(out, &t.conj_type);
        write!(out, ",\"end\":{},\"is_known\":{}", t.end, t.is_known).expect(WRITE_VEC);
        out.extend_from_slice(b",\"pos\":");
        string(out, &t.pos);
        out.extend_from_slice(b",\"pronunciation\":");
        string(out, &t.pronunciation);
        out.extend_from_slice(b",\"reading\":");
        string(out, &t.reading);
        write!(out, ",\"start\":{},\"surface\":", t.start).expect(WRITE_VEC);
        string(out, &t.surface);
        out.push(b'}');
    }
    out.extend_from_slice(b"]\n");
}

fn cmd_bench(dict_path: &Path, text: &str, iterations: usize) -> io::Result<()> {
    let mut analyzer = Analyzer::load(dict_path)?;

    // ウォームアップ（壊れた辞書ならここでエラーにする）
    for _ in 0..100 {
        analyzer.try_tokenize(text)?;
    }

    let start = Instant::now();
    for _ in 0..iterations {
        let _ = analyzer.tokenize(text);
    }
    let elapsed = start.elapsed();
    let per_sentence = elapsed.as_nanos() as f64 / iterations as f64;
    let sentences_per_sec = 1_000_000_000.0 / per_sentence;

    println!("Text: {}", text);
    println!("Iterations: {}", iterations);
    println!("Total time: {:.3}s", elapsed.as_secs_f64());
    println!("Per sentence: {:.0}ns", per_sentence);
    println!("Throughput: {:.0} sentences/sec", sentences_per_sec);

    Ok(())
}

/// ファイルの全行（空行を除く）の解析を 1 回として `passes` 回測り、最速の回を出す
fn cmd_bench_file(dict_path: &Path, file: &Path, passes: usize) -> io::Result<()> {
    let mut analyzer = Analyzer::load(dict_path)?;
    let content = std::fs::read_to_string(file)?;
    let lines: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let bytes: usize = lines.iter().map(|l| l.len()).sum();
    let chars: usize = lines.iter().map(|l| l.chars().count()).sum();

    // ウォームアップ（辞書のページを読み込む。壊れた辞書ならここでエラーにする）
    let mut tokens = 0;
    for line in &lines {
        tokens += analyzer.try_tokenize(line)?.len();
    }

    let mut best = f64::MAX;
    for _ in 0..passes {
        let start = Instant::now();
        for line in &lines {
            std::hint::black_box(analyzer.tokenize(line));
        }
        best = best.min(start.elapsed().as_secs_f64());
    }

    println!("File: {}", file.display());
    println!(
        "Lines: {} ({chars} chars, {bytes} bytes, {tokens} tokens)",
        lines.len()
    );
    println!("Passes: {passes}");
    println!("Best pass: {best:.3}s");
    println!(
        "Throughput: {:.0} lines/sec, {:.2} MB/s, {:.0} tokens/sec",
        lines.len() as f64 / best,
        bytes as f64 / best / 1e6,
        tokens as f64 / best
    );

    Ok(())
}

/// `hasami repair` のオプション
struct RepairOptions<'a> {
    drop_invalid_context_ids: bool,
    repair_pronunciation: bool,
    drop_ortho_variants: bool,
    drop_numeral_misreadings: bool,
    remove: &'a [PathBuf],
    demote_common_proper_nouns: Option<&'a Path>,
    merge: &'a [PathBuf],
}

/// 辞書を読み込み、次の順で修復して書き出す
///
/// 1. 範囲外の文脈 ID を持つエントリの除去（`--drop-invalid-context-ids`）。発音の修復が
///    これらを借用元に使わないよう、最初に落とす
/// 2. 壊れた発音の修復（`--no-pronunciation-repair` を付けなければ常に行う）
/// 3. 汎用フィルタによる除去（`--drop-ortho-variants` / `--drop-numeral-misreadings`）
/// 4. 削除リスト CSV の適用（`--remove`）。削除リストは上流の辞書の品詞で書くので、降格より先に適用する
/// 5. 一般語の固有名詞の降格（`--demote-common-proper-nouns`）
/// 6. CSV の追加マージ（`--merge`）。追加する語は降格の対象にしない
///
/// 行った操作はメタデータの `repairs` に書き足す。
fn cmd_repair(
    dict_path: &Path,
    output: Option<&Path>,
    write: &WriteArgs,
    opts: RepairOptions<'_>,
) -> io::Result<()> {
    eprintln!("Loading dictionary: {}", dict_path.display());
    let start = Instant::now();

    let mut builder = DictBuilder::new();
    builder.load_hsd(dict_path)?;
    let mut ops: Vec<String> = Vec::new();

    let mut dropped = 0;
    if opts.drop_invalid_context_ids {
        let n = builder.drop_invalid_context_ids();
        eprintln!("Dropped {} entries with out-of-range context IDs", n);
        dropped += n;
        ops.push("drop-invalid-context-ids".into());
    }
    // 範囲外の ID が残っていると書き出しで失敗するので、時間のかかる処理の前に止める
    builder.check_context_ids()?;

    let fixed = if opts.repair_pronunciation {
        let n = builder.repair_pronunciation();
        eprintln!("Fixed {} entries with non-katakana pronunciation", n);
        ops.push("pronunciation".into());
        n
    } else {
        0
    };

    if opts.drop_ortho_variants {
        let n = builder.drop_conflicting_ortho_variants();
        eprintln!("Dropped {} conflicting ortho-variant entries", n);
        dropped += n;
        ops.push("drop-ortho-variants".into());
    }
    if opts.drop_numeral_misreadings {
        let n = builder.drop_numeral_misreadings();
        eprintln!("Dropped {} numeral misreading entries", n);
        dropped += n;
        ops.push("drop-numeral-misreadings".into());
    }
    for path in opts.remove {
        let stats = builder.drop_entries_from_csv(path)?;
        eprintln!(
            "Dropped {} entries listed in {} ({} rows, {} rows matched nothing)",
            stats.dropped,
            path.display(),
            stats.rows,
            stats.unmatched_rows
        );
        for sample in &stats.unmatched_samples {
            eprintln!("  unmatched: line {}", sample);
        }
        dropped += stats.dropped;
        ops.push(format!("remove:{}", file_name(path)));
    }

    let mut demoted = 0;
    if let Some(reference_path) = opts.demote_common_proper_nouns {
        eprintln!(
            "Loading reference dictionary for demotion: {}",
            reference_path.display()
        );
        let reference = Arc::new(Dictionary::load(reference_path)?);
        let stats = builder.demote_common_proper_nouns(&reference)?;
        eprintln!(
            "Demoted {} of {} 名詞,固有名詞,一般 entries that {} splits into common nouns + a suffix",
            stats.demoted,
            stats.examined,
            file_name(reference_path)
        );
        for p in &stats.by_pos {
            eprintln!(
                "  -> {}: {} entries (left_id={}, right_id={})",
                p.pos, p.entries, p.left_id, p.right_id
            );
        }
        let suffixes: Vec<String> = stats
            .by_suffix
            .iter()
            .map(|(s, n)| format!("{s}:{n}"))
            .collect();
        eprintln!("  by suffix: {}", suffixes.join(" "));
        for sample in &stats.samples {
            eprintln!("  demote: {sample}");
        }
        demoted = stats.demoted;
        ops.push("demote-common-proper-nouns".into());
    }

    let mut added = 0;
    for path in opts.merge {
        let before = builder.entry_count();
        if path.is_dir() {
            builder.add_csv_dir(path)?;
        } else {
            builder.add_csv(path)?;
        }
        let n = builder.entry_count() - before;
        eprintln!("Merged {} entries from {}", n, path.display());
        added += n;
        ops.push(format!("merge:{}", file_name(path)));
    }

    let output_path = output.map_or_else(|| dict_path.to_path_buf(), |p| p.to_path_buf());
    let output_path = ensure_hsd_extension(&output_path);
    let unchanged = fixed == 0 && dropped == 0 && demoted == 0 && added == 0;
    if unchanged && output_path == dict_path && !write.prune_dominated && write.meta.is_empty() {
        eprintln!("No entries to fix. Skipping rebuild.");
        return Ok(());
    }
    builder.check_context_ids()?;

    let mut meta = builder.write_options().meta;
    append_repairs(&mut meta, &ops)?;
    write_dict(&builder, &output_path, meta, write, "Repaired", start)
}

fn cmd_export_sentence_exceptions(dict_path: &Path, output: Option<&Path>) -> io::Result<()> {
    let dict = Dictionary::load(dict_path)?;
    let surfaces = dict.surfaces()?;
    let words = hasami::sentence::extract_candidates(surfaces.iter().map(String::as_str));
    let source = dict_path.display();
    let mut text = String::new();
    text.push_str("# 文末記号を含む語の例外表（sentence モジュールの組み込みの例外表）\n#\n");
    text.push_str(&format!("# 生成元: {source} の全表層形\n"));
    text.push_str("# 生成手順（リポジトリのルートで実行）:\n");
    text.push_str(&format!(
        "#   ./target/release/hasami export-sentence-exceptions --dict {source} \\\n"
    ));
    text.push_str("#     --output src/sentence/builtin_exceptions.txt\n");
    text.push_str(
        "# 抽出規則: sentence::extract_candidates（全角の英数字・記号を半角に畳み、文末記号\n",
    );
    text.push_str(
        "#   `。！？!?‼⁇⁈⁉．｡` を含む語だけを残し、記号だけの語・2 文字未満の語・文末記号で始まる語・\n",
    );
    text.push_str(
        "#   この書式で書けない語・照合の上限を超える語を除く。重複を除いてバイト順に並べる）\n",
    );
    text.push_str("# 書式: 1 行 1 語。# で始まる行と空行は読み飛ばす。照合の索引は build.rs がビルド時に作る\n");
    let sources = dict.meta().get("sources").unwrap_or("不明");
    text.push_str(&format!(
        "# 出典とライセンス: 辞書のソース（{sources}）の表層形から選んで表記を整えた派生データ。\n"
    ));
    text.push_str(
        "#   mecab-ipadic は NAIST-2003、mecab-ipadic-NEologd と SudachiDict は Apache-2.0、SudachiDict が\n",
    );
    text.push_str(
        "#   含む UniDic は BSD-3-Clause。この表を含むものを配布するときは、同じディレクトリの\n",
    );
    text.push_str(
        "#   builtin_exceptions.NOTICE（著作権表示と条文。THIRD_PARTY_LICENSES.md も参照）を添える\n",
    );
    text.push_str(&format!(
        "# 件数: {} 語（辞書の {} 表層形から抽出）\n",
        words.len(),
        surfaces.len()
    ));
    for word in &words {
        text.push_str(word);
        text.push('\n');
    }
    match output {
        Some(path) => std::fs::write(path, text)?,
        None => io::stdout().lock().write_all(text.as_bytes())?,
    }
    eprintln!(
        "Extracted {} words from {} surfaces{}",
        words.len(),
        surfaces.len(),
        output.map_or_else(String::new, |p| format!(" -> {}", p.display()))
    );
    Ok(())
}

fn cmd_export(dict_path: &Path, output: Option<&Path>) -> io::Result<()> {
    let start = Instant::now();
    let dict = Dictionary::load(dict_path)?;
    let count = match output {
        Some(path) => {
            let file = std::fs::File::create(path)?;
            hasami::dict::write_lexicon_csv(&dict, io::BufWriter::new(file))?
        }
        // `| head` などで読み手が先に閉じたら、そこで静かに終える
        None => {
            match hasami::dict::write_lexicon_csv(&dict, io::BufWriter::new(io::stdout().lock())) {
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
                result => result?,
            }
        }
    };
    eprintln!(
        "Exported {} entries in {:.2}s{}",
        count,
        start.elapsed().as_secs_f64(),
        output.map_or_else(String::new, |p| format!(" -> {}", p.display()))
    );
    Ok(())
}

fn cmd_info(dict_path: &Path, verify: bool) -> io::Result<()> {
    let start = Instant::now();
    let dict = Dictionary::load(dict_path)?;
    let load_time = start.elapsed();

    println!("Dictionary: {}", dict_path.display());
    println!("Load time: {:.1}ms", load_time.as_secs_f64() * 1000.0);
    println!(
        "File size: {:.1} MB",
        dict.byte_len() as f64 / 1024.0 / 1024.0
    );
    println!("Entries: {}", dict.entry_count());
    let (num_left, num_right) = dict.matrix_dims();
    println!("Matrix: {} left IDs x {} right IDs", num_left, num_right);
    println!(
        "Tables: {} parts of speech, {} conjugation types, {} conjugation forms",
        dict.pos_count(),
        dict.conj_type_count(),
        dict.conj_form_count()
    );
    println!("Pruned dominated entries: {}", dict.is_pruned());
    println!("Metadata:");
    for (key, value) in dict.meta().iter() {
        println!("  {key}={value}");
    }
    println!("Sections:");
    for (name, len) in dict.section_sizes() {
        println!("  {:<18} {:>12} bytes", name, len);
    }
    if dict.unknown_section_count() > 0 {
        println!(
            "  ({} unknown sections ignored)",
            dict.unknown_section_count()
        );
    }

    if verify {
        let start = Instant::now();
        let report = dict.verify()?;
        println!("Verify: OK in {:.2}s", start.elapsed().as_secs_f64());
        let t = &report.trie;
        println!(
            "  Trie: {} slots ({} used, {:.1}%), {} internal, {} leaves, {} tails ({} bytes), {} keys, {} character codes",
            t.node_count,
            t.used,
            100.0 * t.used as f64 / t.node_count.max(1) as f64,
            t.internal,
            t.leaves,
            t.tails,
            t.tail_bytes,
            t.keys,
            t.max_code
        );
        println!(
            "  Groups: {} (largest {}), feature records: {}",
            report.groups, report.max_group, report.features
        );
        let sizes: Vec<String> = report
            .group_sizes
            .iter()
            .map(|(size, n)| format!("{size}:{n}"))
            .collect();
        println!("  Group sizes (size:count): {}", sizes.join(" "));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_hsd_extension_adds_missing_extension() {
        let path = ensure_hsd_extension(Path::new("dict"));
        assert_eq!(path, PathBuf::from("dict.hsd"));
    }

    #[test]
    fn test_ensure_hsd_extension_preserves_existing_extension() {
        let path = ensure_hsd_extension(Path::new("dict.hsd"));
        assert_eq!(path, PathBuf::from("dict.hsd"));
    }

    #[test]
    fn test_bench_rejects_zero_iterations() {
        let parsed =
            Cli::try_parse_from(["hasami", "bench", "--dict", "dict.hsd", "--iterations", "0"]);
        assert!(parsed.is_err());
    }

    #[test]
    fn test_split_pieces_covers_all_lines_in_order() {
        let lines = ["a", "bbbb", "", "cc", "dddddddd", "e", "ff"];
        for piece_bytes in 1..=30 {
            let pieces = split_pieces(&lines, piece_bytes);
            assert_eq!(pieces[0].start, 0);
            assert_eq!(pieces.last().unwrap().end, lines.len());
            for pair in pieces.windows(2) {
                assert_eq!(pair[0].end, pair[1].start);
            }
            assert!(pieces.iter().all(|p| !p.is_empty()));
        }
        assert_eq!(split_pieces(&lines, 5), vec![0..2, 2..5, 5..7]);
        assert!(split_pieces(&[], 3).is_empty());
    }

    fn test_analyzer() -> Analyzer {
        use hasami::DictEntry;
        let mut builder = DictBuilder::new();
        for (surface, pos, reading) in [
            ("東京", "名詞,固有名詞,地域,一般", "トウキョウ"),
            ("都", "名詞,接尾,地域,*", "ト"),
            ("に", "助詞,格助詞,一般,*", "ニ"),
            ("住む", "動詞,自立,*,*", "スム"),
            ("\"引用\"", "名詞,一般,*,*", "インヨウ"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                cost: 100,
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
    fn test_json_output_matches_serde_json_values() {
        let mut analyzer = test_analyzer();
        for text in ["東京都に住む", "\"引用\"とA\\B\u{1}\t\u{7f}é", "X"] {
            let tokens = analyzer.tokenize(text);
            let values: Vec<serde_json::Value> = tokens
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "surface": &*t.surface,
                        "start": t.start,
                        "end": t.end,
                        "pos": &*t.pos,
                        "conj_type": &*t.conj_type,
                        "conj_form": &*t.conj_form,
                        "base_form": &*t.base_form,
                        "reading": &*t.reading,
                        "pronunciation": &*t.pronunciation,
                        "is_known": t.is_known,
                    })
                })
                .collect();
            let expected = format!("{}\n", serde_json::to_string(&values).unwrap());
            let mut out = Vec::new();
            write_json(&mut out, &tokens);
            assert_eq!(String::from_utf8(out).unwrap(), expected);
        }
    }

    #[test]
    fn test_parallel_blocks_match_sequential_output() {
        let base = test_analyzer();
        // 空行・空白だけの行・CRLF を含む
        let block: String = (0..2000)
            .map(|i| match i % 7 {
                0 => "\n".to_string(),
                1 => "  \t \r\n".to_string(),
                2 => format!("東京都に住む{i}\r\n"),
                _ => format!("{}X{i}\n", "東京に住む".repeat(i % 5 + 1)),
            })
            .collect();
        for format in [
            OutputFormat::Mecab,
            OutputFormat::Wakachi,
            OutputFormat::Json,
        ] {
            let mut expected = Vec::new();
            let mut workers = vec![Worker::new(base.clone())];
            process_block(&block, &mut workers, 1, format, &mut expected).unwrap();
            assert!(!expected.is_empty());
            for threads in [2, 3, 8] {
                let mut out = Vec::new();
                let mut workers = vec![Worker::new(base.clone())];
                process_block(&block, &mut workers, threads, format, &mut out).unwrap();
                assert!(workers.len() > 1, "block is large enough to split");
                assert_eq!(out, expected, "threads={threads}");
            }
        }
    }
}
