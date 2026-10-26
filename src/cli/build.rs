//! 辞書の構築と CSV の追加マージ。

use super::write::{
    WriteArgs, add_csv, append_repairs, ensure_hsd_extension, file_name, write_dict,
};
use hasami::dict::DictBuilder;
use hasami::hsd::{Meta, PosScheme};
use std::io;
use std::path::PathBuf;
use std::time::Instant;

#[derive(clap::Args)]
pub(crate) struct BuildArgs {
    /// 辞書CSVファイルのディレクトリ
    #[arg(short, long)]
    input: PathBuf,

    /// 出力辞書ファイル (.hsd)
    #[arg(short, long)]
    output: PathBuf,

    #[command(flatten)]
    write: WriteArgs,
}

#[derive(clap::Args)]
pub(crate) struct MergeArgs {
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
}

impl BuildArgs {
    pub(crate) fn run(&self) -> io::Result<()> {
        let Self {
            input,
            output,
            write,
        } = self;
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
}

impl MergeArgs {
    pub(crate) fn run(&self) -> io::Result<()> {
        let Self {
            dict: dict_path,
            input,
            output,
            write,
        } = self;
        let output = output.as_deref();
        eprintln!("Loading existing dictionary: {}", dict_path.display());
        let start = Instant::now();

        let mut builder = DictBuilder::new();

        // 既存辞書をインポート
        builder.load_hsd(dict_path)?;
        let new_count = add_csv(&mut builder, input)?;
        eprintln!("Added {} new entries", new_count);
        builder.check_context_ids()?;

        let output_path = ensure_hsd_extension(output.unwrap_or(dict_path));
        let mut meta = builder.write_options().meta;
        append_repairs(&mut meta, &[format!("merge:{}", file_name(input))])?;
        write_dict(&builder, &output_path, meta, write, "Merged", start)
    }
}
