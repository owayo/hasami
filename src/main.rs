//! hasami CLI - 日本語形態素解析コマンドラインツール

mod cli;

use clap::{Parser, Subcommand};
use cli::build::{BuildArgs, MergeArgs};
use cli::dict::DictCommand;
use cli::repair::RepairArgs;
use cli::tokenize::{self, OutputFormat};
use hasami::analyzer::Analyzer;
use hasami::hsd::Dictionary;
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "hasami", version, about = "高速日本語形態素解析エンジン")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 辞書を構築（MeCab形式のCSVから）
    Build(BuildArgs),

    /// 既存辞書にMeCab形式CSVを追加（マージ）
    Merge(MergeArgs),

    /// 配布辞書（リリースの添付ファイル）の取得と、置き場所の辞書の確認
    ///
    /// 置き場所は HASAMI_DATA_DIR、$XDG_DATA_HOME/hasami、%LOCALAPPDATA%\hasami（Windows）、
    /// ~/.local/share/hasami の順に決まる。tokenize は --dict を省くと、ここの辞書を推奨順に使う。
    Dict {
        #[command(subcommand)]
        command: DictCommand,
    },

    /// テキストを形態素解析
    Tokenize {
        /// 辞書ファイルのパス (.hsd)。省略時は環境変数 HASAMI_DICT、置き場所
        /// （hasami dict download が辞書を置く場所。既定は ~/.local/share/hasami/）の *.hsd の順に探す
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
    Repair(RepairArgs),
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
        Commands::Build(args) => args.run(),
        Commands::Merge(args) => args.run(),
        Commands::Dict { command } => command.run(),
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
            tokenize::run(&dict, format, text, threads)
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
        Commands::Repair(args) => args.run(),
    }
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
    fn test_bench_rejects_zero_iterations() {
        let parsed =
            Cli::try_parse_from(["hasami", "bench", "--dict", "dict.hsd", "--iterations", "0"]);
        assert!(parsed.is_err());
    }
}
