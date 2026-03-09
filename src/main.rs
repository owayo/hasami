//! hasami CLI - 日本語形態素解析コマンドラインツール

use clap::{Parser, Subcommand};
use hasami::analyzer::{Analyzer, format_mecab, format_wakachi};
use hasami::dict::DictBuilder;
use std::io::{self, BufRead, Write};
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
    Build {
        /// 辞書CSVファイルのディレクトリ
        #[arg(short, long)]
        input: PathBuf,

        /// 出力辞書ファイル (.hsd)
        #[arg(short, long)]
        output: PathBuf,
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
    },

    /// テキストを形態素解析
    Tokenize {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// 出力形式: mecab, wakachi, json
        #[arg(short, long, default_value = "mecab")]
        format: String,

        /// 解析するテキスト（省略時は標準入力から読み込み）
        text: Option<String>,
    },

    /// ベンチマーク実行
    Bench {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// テストテキスト
        #[arg(short, long, default_value = "東京都に住んでいる人々が増えている。")]
        text: String,

        /// 繰り返し回数
        #[arg(short, long, default_value = "10000")]
        iterations: NonZeroUsize,
    },

    /// 辞書情報を表示
    Info {
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,
    },
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { input, output } => cmd_build(&input, &output),
        Commands::Merge {
            dict,
            input,
            output,
        } => cmd_merge(&dict, &input, output.as_deref()),
        Commands::Tokenize { dict, format, text } => cmd_tokenize(&dict, &format, text),
        Commands::Bench {
            dict,
            text,
            iterations,
        } => cmd_bench(&dict, &text, iterations.get()),
        Commands::Info { dict } => cmd_info(&dict),
    }
}

fn ensure_hsd_extension(path: &Path) -> PathBuf {
    if path.extension().is_none_or(|ext| ext != "hsd") {
        path.with_extension("hsd")
    } else {
        path.to_path_buf()
    }
}

fn cmd_build(input: &Path, output: &Path) -> io::Result<()> {
    eprintln!("Building dictionary from: {}", input.display());
    let start = Instant::now();

    let mut builder = DictBuilder::new();

    // CSVファイルを読み込み
    builder.add_csv_dir(input)?;

    // matrix.def があれば読み込み
    let matrix_path = input.join("matrix.def");
    if matrix_path.exists() {
        builder.load_matrix(&matrix_path)?;
    } else {
        eprintln!("Warning: matrix.def not found, using default connection costs");
    }

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

    // 辞書をビルド
    let dict = builder.build();
    let entry_count = dict.entries.len();

    // .hsd 形式で保存
    let output = ensure_hsd_extension(output);
    let v2_builder = hasami::mmap_dict::MmapDictBuilder::from_dictionary(&dict);
    v2_builder.write(&output)?;

    let elapsed = start.elapsed();
    if let Ok(meta) = std::fs::metadata(&output) {
        eprintln!(
            "Dictionary built in {:.2}s: {} entries, {:.1} MB (strings: {}, features: {}) -> {}",
            elapsed.as_secs_f64(),
            entry_count,
            meta.len() as f64 / 1024.0 / 1024.0,
            v2_builder.string_count(),
            v2_builder.feature_count(),
            output.display()
        );
    }

    Ok(())
}

fn cmd_merge(dict_path: &Path, input: &Path, output: Option<&Path>) -> io::Result<()> {
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

    // リビルド
    let dict = builder.build();
    let total = dict.entries.len();

    // 保存
    let output_path = output.map_or_else(|| dict_path.to_path_buf(), |p| p.to_path_buf());
    let output_path = ensure_hsd_extension(&output_path);
    let v2_builder = hasami::mmap_dict::MmapDictBuilder::from_dictionary(&dict);
    v2_builder.write(&output_path)?;

    let elapsed = start.elapsed();
    if let Ok(meta) = std::fs::metadata(&output_path) {
        eprintln!(
            "Merged in {:.2}s: {} total entries, {:.1} MB -> {}",
            elapsed.as_secs_f64(),
            total,
            meta.len() as f64 / 1024.0 / 1024.0,
            output_path.display()
        );
    }

    Ok(())
}

fn cmd_tokenize(dict_path: &Path, format: &str, text: Option<String>) -> io::Result<()> {
    let start = Instant::now();
    let mut analyzer = Analyzer::load(dict_path)?;
    eprintln!(
        "Dictionary loaded in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if let Some(text) = text {
        let tokens = analyzer.tokenize(&text);
        write_output(&mut out, &tokens, format)?;
    } else {
        // 標準入力から行ごとに読み込み
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let tokens = analyzer.tokenize(line);
            write_output(&mut out, &tokens, format)?;
        }
    }

    Ok(())
}

fn write_output(out: &mut impl Write, tokens: &[hasami::Token], format: &str) -> io::Result<()> {
    match format {
        "wakachi" => {
            writeln!(out, "{}", format_wakachi(tokens))?;
        }
        "json" => {
            let json_tokens: Vec<serde_json::Value> = tokens
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "surface": &*t.surface,
                        "start": t.start,
                        "end": t.end,
                        "pos": &*t.pos,
                        "base_form": &*t.base_form,
                        "reading": &*t.reading,
                        "is_known": t.is_known,
                    })
                })
                .collect();
            writeln!(out, "{}", serde_json::to_string(&json_tokens).unwrap())?;
        }
        _ => {
            // MeCab形式
            write!(out, "{}", format_mecab(tokens))?;
        }
    }
    Ok(())
}

fn cmd_bench(dict_path: &Path, text: &str, iterations: usize) -> io::Result<()> {
    let mut analyzer = Analyzer::load(dict_path)?;

    // ウォームアップ
    for _ in 0..100 {
        let _ = analyzer.tokenize(text);
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

fn cmd_info(dict_path: &Path) -> io::Result<()> {
    let start = Instant::now();
    let dict = hasami::MmapDictionary::load(dict_path)?;
    let load_time = start.elapsed();

    println!("Dictionary: {}", dict_path.display());
    println!("Load time: {:.1}ms", load_time.as_secs_f64() * 1000.0);
    println!("Entries: {}", dict.entry_count());
    println!("Strings: {}", dict.string_count());
    println!("Features: {}", dict.feature_count());
    if let Ok(meta) = std::fs::metadata(dict_path) {
        println!("File size: {:.1} MB", meta.len() as f64 / 1024.0 / 1024.0);
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
}
