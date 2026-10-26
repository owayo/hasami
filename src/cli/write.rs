//! 辞書を生成・更新するコマンド共通の入出力とメタデータ。

use hasami::dict::DictBuilder;
use hasami::hsd::{Meta, WriteOptions, meta};
use indicatif::{ProgressBar, ProgressStyle};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 辞書を書き出すコマンド共通の設定
#[derive(clap::Args)]
pub(super) struct WriteArgs {
    /// メタデータを設定する（`key=value`、複数指定可）。`name`・`pos_scheme`（ipadic / unidic）・
    /// `sources` など。build では `name` の既定は出力ファイル名、`pos_scheme` の既定は ipadic。
    /// merge・repair は入力辞書のメタデータを引き継いで上書きする
    #[arg(long = "meta", value_name = "KEY=VALUE")]
    pub(super) meta: Vec<String>,

    /// 同じ表層形・同じ文脈 ID の中でコストが最小でないエントリを除く（配布用の最終辞書向け）。
    /// 1-best の解析結果は変わらない。除いた辞書は merge・repair の入力にできない
    #[arg(long)]
    pub(super) prune_dominated: bool,
}

/// CSV ファイルかディレクトリを追加し、追加したエントリ数を返す。
pub(super) fn add_csv(builder: &mut DictBuilder, path: &Path) -> io::Result<usize> {
    let before = builder.entry_count();
    if path.is_dir() {
        builder.add_csv_dir(path)?;
    } else {
        builder.add_csv(path)?;
    }
    Ok(builder.entry_count() - before)
}

pub(super) fn ensure_hsd_extension(path: &Path) -> PathBuf {
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
pub(super) fn append_repairs(meta: &mut Meta, ops: &[String]) -> io::Result<()> {
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

pub(super) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// 辞書を書き出して概要を表示する
pub(super) fn write_dict(
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
}
