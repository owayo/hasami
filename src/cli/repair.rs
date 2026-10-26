//! 辞書の修復・削除・降格と、修復後の CSV マージ。

use super::write::{
    WriteArgs, add_csv, append_repairs, ensure_hsd_extension, file_name, write_dict,
};
use hasami::dict::DictBuilder;
use hasami::hsd::Dictionary;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[derive(clap::Args)]
pub(crate) struct RepairArgs {
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

    /// 文や句を 1 語にした名詞を削除する。値は IPAdic 単体の辞書 (.hsd)。
    /// ひらがなか文末記号を含む名詞の表層形をこの辞書で解析し、文法に合う文や句
    /// (述語・助詞で終わる並び「どうでしょう」「一緒に」、内容語 1 つ以下の文 + 文末記号「好きだ。」、
    /// 機能語だけの並び「なのか」、感動詞「こんにちは。」、記号 + 文末記号「…。」、表記ゆれの副詞・用言
    /// 「および」=お呼び) になるエントリを削除する。この辞書自身の語は削除しない
    #[arg(long, value_name = "IPADIC_HSD")]
    drop_sentence_like_nouns: Option<PathBuf>,

    /// 数と単位の記号だけの表層形を 1 語にした固有名詞 (「50%」「0.1℃」「30℃」) を削除する。
    /// 値は IPAdic 単体の辞書 (.hsd)。「名詞,固有名詞,一般」の表層形をこの辞書で解析し、数 (名詞,数) と
    /// 単位 (名詞,接尾) だけに分かれるものを削除する。人名・組織 (「100%ORANGE」「4℃」) は残す
    #[arg(long, value_name = "IPADIC_HSD")]
    drop_quantity_nouns: Option<PathBuf>,

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
}

/// 参照辞書（IPAdic 単体）を読み込む。`--drop-sentence-like-nouns`・`--drop-quantity-nouns`・
/// `--demote-common-proper-nouns` に同じ辞書を渡したときは 1 回だけ読み込む
fn load_reference(
    cache: &mut Option<(PathBuf, Arc<Dictionary>)>,
    path: &Path,
) -> io::Result<Arc<Dictionary>> {
    if let Some((cached, dict)) = cache.as_ref()
        && cached == path
    {
        return Ok(Arc::clone(dict));
    }
    eprintln!("Loading reference dictionary: {}", path.display());
    let dict = Arc::new(Dictionary::load(path)?);
    *cache = Some((path.to_path_buf(), Arc::clone(&dict)));
    Ok(dict)
}

impl RepairArgs {
    /// 辞書を読み込み、次の順で修復して書き出す
    ///
    /// 1. 範囲外の文脈 ID を持つエントリの除去（`--drop-invalid-context-ids`）。発音の修復が
    ///    これらを借用元に使わないよう、最初に落とす
    /// 2. 壊れた発音の修復（`--no-pronunciation-repair` を付けなければ常に行う）
    /// 3. 汎用フィルタによる除去（`--drop-ortho-variants` / `--drop-numeral-misreadings`）
    /// 4. 削除リスト CSV の適用（`--remove`）。削除リストは上流の辞書の品詞で書くので、降格より先に適用する
    /// 5. 文や句を 1 語にした名詞の除去（`--drop-sentence-like-nouns`）と、数と単位の組の固有名詞の除去
    ///    （`--drop-quantity-nouns`）
    /// 6. 一般語の固有名詞の降格（`--demote-common-proper-nouns`）
    /// 7. CSV の追加マージ（`--merge`）。追加する語は削除・降格の対象にしない
    ///
    /// 行った操作はメタデータの `repairs` に書き足す。
    pub(crate) fn run(&self) -> io::Result<()> {
        let dict_path = self.dict.as_path();
        let output = self.output.as_deref();
        let write = &self.write;
        eprintln!("Loading dictionary: {}", dict_path.display());
        let start = Instant::now();

        let mut builder = DictBuilder::new();
        builder.load_hsd(dict_path)?;
        let mut ops: Vec<String> = Vec::new();

        let mut dropped = 0;
        if self.drop_invalid_context_ids {
            let n = builder.drop_invalid_context_ids();
            eprintln!("Dropped {} entries with out-of-range context IDs", n);
            dropped += n;
            ops.push("drop-invalid-context-ids".into());
        }
        // 範囲外の ID が残っていると書き出しで失敗するので、時間のかかる処理の前に止める
        builder.check_context_ids()?;

        let fixed = if !self.no_pronunciation_repair {
            let n = builder.repair_pronunciation();
            eprintln!("Fixed {} entries with non-katakana pronunciation", n);
            ops.push("pronunciation".into());
            n
        } else {
            0
        };

        if self.drop_ortho_variants {
            let n = builder.drop_conflicting_ortho_variants();
            eprintln!("Dropped {} conflicting ortho-variant entries", n);
            dropped += n;
            ops.push("drop-ortho-variants".into());
        }
        if self.drop_numeral_misreadings {
            let n = builder.drop_numeral_misreadings();
            eprintln!("Dropped {} numeral misreading entries", n);
            dropped += n;
            ops.push("drop-numeral-misreadings".into());
        }
        for path in &self.remove {
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

        let mut reference_cache: Option<(PathBuf, Arc<Dictionary>)> = None;
        if let Some(reference_path) = self.drop_sentence_like_nouns.as_deref() {
            let reference = load_reference(&mut reference_cache, reference_path)?;
            let stats = builder.drop_sentence_like_nouns(&reference)?;
            eprintln!(
                "Dropped {} of {} noun entries that {} parses as sentences or phrases",
                stats.dropped,
                stats.examined,
                file_name(reference_path)
            );
            let reasons: Vec<String> = stats
                .by_reason
                .iter()
                .map(|(r, n)| format!("{}:{n}", r.name()))
                .collect();
            eprintln!("  by reason: {}", reasons.join(" "));
            for (reason, sample) in &stats.samples {
                eprintln!("  drop [{}]: {sample}", reason.name());
            }
            dropped += stats.dropped;
            ops.push("drop-sentence-like-nouns".into());
        }
        if let Some(reference_path) = self.drop_quantity_nouns.as_deref() {
            let reference = load_reference(&mut reference_cache, reference_path)?;
            let stats = builder.drop_quantity_nouns(&reference)?;
            eprintln!(
                "Dropped {} of {} number-and-unit proper nouns that {} parses as a number and a suffix",
                stats.dropped,
                stats.examined,
                file_name(reference_path)
            );
            for sample in &stats.samples {
                eprintln!("  drop: {sample}");
            }
            dropped += stats.dropped;
            ops.push("drop-quantity-nouns".into());
        }

        let mut demoted = 0;
        if let Some(reference_path) = self.demote_common_proper_nouns.as_deref() {
            let reference = load_reference(&mut reference_cache, reference_path)?;
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
        for path in &self.merge {
            let n = add_csv(&mut builder, path)?;
            eprintln!("Merged {} entries from {}", n, path.display());
            added += n;
            ops.push(format!("merge:{}", file_name(path)));
        }

        let output_path = ensure_hsd_extension(output.unwrap_or(dict_path));
        let unchanged = fixed == 0 && dropped == 0 && demoted == 0 && added == 0;
        if unchanged && output_path == dict_path && !write.prune_dominated && write.meta.is_empty()
        {
            eprintln!("No entries to fix. Skipping rebuild.");
            return Ok(());
        }
        builder.check_context_ids()?;

        let mut meta = builder.write_options().meta;
        append_repairs(&mut meta, &ops)?;
        write_dict(&builder, &output_path, meta, write, "Repaired", start)
    }
}
