//! hasami CLI - 日本語形態素解析コマンドラインツール

use clap::{Parser, Subcommand};
use hasami::analyzer::{Analyzer, format_mecab, format_wakachi};
use hasami::dict::DictBuilder;
use hasami::hsd::meta;
use hasami::hsd::{Dictionary, Meta, PosScheme, WriteOptions};
use indicatif::{ProgressBar, ProgressStyle};
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
        /// 辞書ファイルのパス (.hsd)
        #[arg(short, long)]
        dict: PathBuf,

        /// 出力形式
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Mecab)]
        format: OutputFormat,

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

        /// 全体を検証する（全 trie ノード・全エントリ群・全素性レコード。時間がかかる）
        #[arg(long)]
        verify: bool,
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
        /// (発音の修復は、発音も読みもカタカナでない記号などのエントリの読みを空にする)
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
        Commands::Tokenize { dict, format, text } => cmd_tokenize(&dict, format, text),
        Commands::Bench {
            dict,
            text,
            iterations,
        } => cmd_bench(&dict, &text, iterations.get()),
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

fn cmd_tokenize(dict_path: &Path, format: OutputFormat, text: Option<String>) -> io::Result<()> {
    let start = Instant::now();
    let mut analyzer = Analyzer::load(dict_path)?;
    eprintln!(
        "Dictionary loaded in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if let Some(text) = text {
        let tokens = analyzer.try_tokenize(&text)?;
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
            let tokens = analyzer.try_tokenize(line)?;
            write_output(&mut out, &tokens, format)?;
        }
    }

    Ok(())
}

fn write_output(
    out: &mut impl Write,
    tokens: &[hasami::Token],
    format: OutputFormat,
) -> io::Result<()> {
    match format {
        OutputFormat::Wakachi => {
            writeln!(out, "{}", format_wakachi(tokens))?;
        }
        OutputFormat::Json => {
            let json_tokens: Vec<serde_json::Value> = tokens
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
            writeln!(out, "{}", serde_json::to_string(&json_tokens).unwrap())?;
        }
        OutputFormat::Mecab => {
            write!(out, "{}", format_mecab(tokens))?;
        }
    }
    Ok(())
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

/// `hasami repair` のオプション
struct RepairOptions<'a> {
    drop_invalid_context_ids: bool,
    repair_pronunciation: bool,
    drop_ortho_variants: bool,
    drop_numeral_misreadings: bool,
    remove: &'a [PathBuf],
    merge: &'a [PathBuf],
}

/// 辞書を読み込み、次の順で修復して書き出す
///
/// 1. 範囲外の文脈 ID を持つエントリの除去（`--drop-invalid-context-ids`）。発音の修復が
///    これらを借用元に使わないよう、最初に落とす
/// 2. 壊れた発音の修復（`--no-pronunciation-repair` を付けなければ常に行う）
/// 3. 汎用フィルタによる除去（`--drop-ortho-variants` / `--drop-numeral-misreadings`）
/// 4. 削除リスト CSV の適用（`--remove`）
/// 5. CSV の追加マージ（`--merge`）
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
    let unchanged = fixed == 0 && dropped == 0 && added == 0;
    if unchanged && output_path == dict_path && !write.prune_dominated && write.meta.is_empty() {
        eprintln!("No entries to fix. Skipping rebuild.");
        return Ok(());
    }
    builder.check_context_ids()?;

    let mut meta = builder.write_options().meta;
    append_repairs(&mut meta, &ops)?;
    write_dict(&builder, &output_path, meta, write, "Repaired", start)
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
        None => match hasami::dict::write_lexicon_csv(&dict, io::BufWriter::new(io::stdout().lock())) {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
            result => result?,
        },
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
}
