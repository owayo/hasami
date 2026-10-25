//! 配布辞書の取得・配置・状態確認を行う CLI。

use clap::Subcommand;
use clap::builder::PossibleValuesParser;
use hasami::download::{self, Catalog, DistributedDict, DownloadOptions, Outcome, Verification};
use hasami::hsd::{Dictionary, meta};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(crate) enum DictCommand {
    /// 配布辞書をリリースから取得して置き場所に置く
    ///
    /// 取得元は、この hasami と同じ版のリリース（--tag で別の版、--base-url でミラー）。リリースの目録
    /// （dictionaries.json）の大きさと SHA-256 で確かめ、辞書として読めることも確かめてから置く。圧縮版
    /// （.hsd.zst）があればそちらを取って展開する（展開したものも確かめる）。途中で失敗しても、すでにある
    /// ファイルは消さず、壊さない。正しいファイルがすでにあれば通信しない。
    Download(DictDownloadArgs),
    /// 配布辞書と、置き場所にある辞書の状態を表示する（通信しない。--remote でリリースの目録と照合する）
    List(DictListArgs),
    /// 使われる辞書のパスを 1 行で表示する（-d "$(hasami dict path)" のように使う）
    ///
    /// NAME を省くと tokenize が --dict なしで使う辞書、NAME を渡すと置き場所のその辞書。
    Path(DictPathArgs),
    /// 手元の辞書ファイル（.hsd / .hsd.zst）を確かめて置き場所に置く（ネットワークに出られない環境向け）
    ///
    /// 同じディレクトリにリリースの目録（dictionaries.json）があるか --catalog で渡せば、その大きさと
    /// SHA-256 で確かめる。目録がなければ、辞書全体を検証する。
    Install(DictInstallArgs),
    /// 配布辞書の目録（dictionaries.json）を作る（リリースの作成・ミラー用）
    ///
    /// DIR の ipadic.hsd・ipadic-neologd.hsd・ipadic-neologd-sudachi.hsd（とあれば .hsd.zst）の大きさと
    /// SHA-256 を書く。圧縮版は展開して元の辞書と同じになることを確かめる。
    Manifest(DictManifestArgs),
}

/// 配布辞書の取得元
#[derive(clap::Args)]
struct DictSource {
    /// 取得するリリースのタグ（既定はこの hasami と同じ版）
    #[arg(long, value_name = "TAG", conflicts_with = "base_url")]
    tag: Option<String>,

    /// 取得元の URL の接頭辞（ミラー。<URL>/dictionaries.json と <URL>/<ファイル名> を取る）
    #[arg(long, value_name = "URL")]
    base_url: Option<String>,
}

#[derive(clap::Args)]
pub(crate) struct DictDownloadArgs {
    /// 取得する辞書（省略すると推奨の ipadic-neologd-sudachi）
    #[arg(
        value_name = "NAME",
        conflicts_with = "all",
        value_parser = PossibleValuesParser::new(hasami::analyzer::DISTRIBUTED_DICTS)
    )]
    names: Vec<String>,

    /// 配布辞書をすべて取得する
    #[arg(long)]
    all: bool,

    /// 置き場所（既定は tokenize が辞書を探すディレクトリ）
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,

    #[command(flatten)]
    source: DictSource,

    /// 正しいファイルがあっても取り直す。中身の違うファイルも置き換える
    #[arg(long)]
    force: bool,

    /// 圧縮版（.hsd.zst）ではなく、生の辞書（.hsd）を取る
    #[arg(long)]
    uncompressed: bool,

    /// 進み具合と結果を表示しない（エラーだけ）
    #[arg(short, long, conflicts_with = "json")]
    quiet: bool,

    /// 結果（置いたパス・大きさ・SHA-256）を JSON で標準出力に書く
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub(crate) struct DictListArgs {
    /// 確かめる置き場所（既定は tokenize が辞書を探すディレクトリ）
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,

    /// リリースの目録を取って、大きさと SHA-256 で照合する
    #[arg(long)]
    remote: bool,

    #[command(flatten)]
    source: DictSource,

    /// 結果を JSON で標準出力に書く
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub(crate) struct DictPathArgs {
    /// 置き場所の辞書の名前（<NAME>.hsd を指す）
    #[arg(value_name = "NAME")]
    name: Option<String>,

    /// 置き場所（既定は tokenize が辞書を探すディレクトリ。渡すと HASAMI_DICT は見ない）
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,
}

#[derive(clap::Args)]
pub(crate) struct DictInstallArgs {
    /// 置く辞書ファイル（<名前>.hsd か <名前>.hsd.zst）
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// 確かめるのに使うリリースの目録（既定は FILE と同じディレクトリの dictionaries.json）
    #[arg(long, value_name = "FILE")]
    catalog: Option<PathBuf>,

    /// 置き場所（既定は tokenize が辞書を探すディレクトリ）
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,

    /// 中身の違うファイルがあっても置き換える
    #[arg(long)]
    force: bool,
}

#[derive(clap::Args)]
pub(crate) struct DictManifestArgs {
    /// 配布辞書のあるディレクトリ
    #[arg(value_name = "DIR")]
    dir: PathBuf,

    /// 出力ファイル。省略時は標準出力
    #[arg(short, long)]
    output: Option<PathBuf>,
}

impl DictCommand {
    pub(crate) fn run(self) -> io::Result<()> {
        match self {
            Self::Download(args) => cmd_dict_download(args),
            Self::List(args) => cmd_dict_list(args),
            Self::Path(args) => cmd_dict_path(args),
            Self::Install(args) => cmd_dict_install(args),
            Self::Manifest(args) => cmd_dict_manifest(args),
        }
    }
}

/// 置き場所（`--dir` か、tokenize が辞書を探すディレクトリ）
fn dict_dir(dir: Option<PathBuf>) -> io::Result<PathBuf> {
    dir.or_else(hasami::analyzer::data_dir).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "cannot determine the data directory ({}, XDG_DATA_HOME and HOME are not set); pass --dir",
                hasami::analyzer::DATA_DIR_ENV
            ),
        )
    })
}

fn download_error(e: download::DownloadError) -> io::Error {
    io::Error::other(e)
}

impl DictSource {
    /// 辞書を取る URL の接頭辞と、そこの目録
    fn catalog(&self) -> io::Result<(String, Catalog)> {
        match &self.base_url {
            Some(url) => {
                let catalog = download::catalog_from(url).map_err(download_error)?;
                Ok((url.trim_end_matches('/').to_string(), catalog))
            }
            None => {
                let tag = self.label();
                let catalog = download::catalog(&tag).map_err(download_error)?;
                Ok((download::release_url(&tag), catalog))
            }
        }
    }

    /// 取得元の表示（タグか URL）
    fn label(&self) -> String {
        match (&self.base_url, &self.tag) {
            (Some(url), _) => url.clone(),
            (None, Some(tag)) => tag.clone(),
            (None, None) => download::CURRENT_TAG.to_string(),
        }
    }
}

fn cmd_dict_download(args: DictDownloadArgs) -> io::Result<()> {
    let dir = dict_dir(args.dir)?;
    let label = args.source.label();
    let (base_url, catalog) = args.source.catalog()?;
    catalog.check_format().map_err(download_error)?;
    let names: Vec<String> = if args.all {
        catalog
            .dictionaries
            .iter()
            .map(|d| d.name.clone())
            .collect()
    } else if args.names.is_empty() {
        let recommended = if catalog.recommended.is_empty() {
            download::RECOMMENDED
        } else {
            &catalog.recommended
        };
        vec![recommended.to_string()]
    } else {
        let mut names: Vec<String> = Vec::with_capacity(args.names.len());
        for name in args.names {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    };
    let mut dicts = Vec::with_capacity(names.len());
    for name in &names {
        let dict = catalog.find(name).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{name} is not in the dictionary catalog of {label}"),
            )
        })?;
        dicts.push(dict);
    }

    let show = !args.quiet && !args.json;
    let mut results = Vec::with_capacity(dicts.len());
    for dict in dicts {
        let outcome = download_one(
            dict,
            &dir,
            &base_url,
            &label,
            args.force,
            !args.uncompressed,
            show,
        )?;
        if show {
            let path = display_path(outcome.path());
            match outcome {
                Outcome::Present(_) => println!(
                    "{}: already downloaded (size and SHA-256 verified): {path}",
                    dict.name
                ),
                Outcome::Downloaded(_) => println!(
                    "{}: downloaded (size, SHA-256 and format verified): {path}",
                    dict.name
                ),
            }
        }
        results.push(serde_json::json!({
            "name": dict.name,
            "path": outcome.path().display().to_string(),
            "status": match outcome {
                Outcome::Present(_) => "present",
                Outcome::Downloaded(_) => "downloaded",
            },
            "size": dict.size,
            "sha256": dict.sha256,
            "source": base_url,
        }));
    }
    if args.json {
        let json = serde_json::to_string_pretty(&results).expect("JSON values always serialize");
        println!("{json}");
    } else if show {
        print_default_note(&dir);
    }
    Ok(())
}

/// 辞書 1 つを取得する。取得を始めたら（取得済みでなければ）取得元と大きさを標準エラーに書き、
/// 標準エラーが端末なら受信量を表示する
fn download_one(
    dict: &DistributedDict,
    dir: &Path,
    base_url: &str,
    label: &str,
    force: bool,
    compressed: bool,
    show: bool,
) -> io::Result<Outcome> {
    let bar = if show {
        let bar = ProgressBar::new(0);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%) [{elapsed_precise}<{eta_precise}, {bytes_per_sec}]",
            )
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        bar
    } else {
        ProgressBar::hidden()
    };
    let mut started = false;
    let mut progress = |received: u64, total: u64| {
        if !started {
            started = true;
            if show {
                let size = match &dict.compressed {
                    Some(c) if compressed => {
                        format!(
                            "{}, {} decompressed",
                            megabytes(c.size),
                            megabytes(dict.size)
                        )
                    }
                    _ => megabytes(dict.size),
                };
                eprintln!(
                    "Downloading {} ({size}) from {label} to {}",
                    dict.name,
                    display_path(dir)
                );
            }
        }
        bar.set_length(total);
        bar.set_position(received);
    };
    let options = DownloadOptions {
        base_url: Some(base_url),
        compressed,
        force,
        progress: Some(&mut progress),
    };
    let mut events = |event| {
        if show && let download::DownloadEvent::UncompressedFallback { .. } = event {
            bar.suspend(|| {
                eprintln!(
                    "Compressed dictionary not found (HTTP 404); downloading uncompressed {} ({})",
                    dict.file,
                    megabytes(dict.size)
                );
            });
            bar.reset_elapsed();
        }
    };
    let result = download::Client::default().download_with_events(dict, dir, options, &mut events);
    bar.finish_and_clear();
    result.map_err(download_error)
}

/// tokenize が --dict なしで使う辞書を案内する
fn print_default_note(dir: &Path) {
    let env = hasami::analyzer::DICT_ENV;
    if let Some(path) = std::env::var_os(env).filter(|v| !v.is_empty()) {
        println!(
            "{env} is set, so `hasami tokenize` uses {} when --dict is omitted",
            display_path(Path::new(&path))
        );
    } else if !is_data_dir(dir) {
        println!(
            "`hasami tokenize` does not search {}; pass --dict to use it",
            display_path(dir)
        );
    } else if let Some(path) = hasami::analyzer::preferred_dict_in(dir) {
        println!(
            "`hasami tokenize` uses {} when --dict is omitted",
            display_path(&path)
        );
    }
}

/// 置き場所の辞書ファイルの状態（通信しない）
enum Local {
    Missing,
    /// 辞書として読める（作った hasami の版）
    Readable {
        size: u64,
        version: Option<String>,
    },
    Unreadable {
        size: u64,
        reason: String,
    },
}

fn inspect_local(path: &Path) -> Local {
    let size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Local::Missing,
        Err(e) => {
            return Local::Unreadable {
                size: 0,
                reason: e.to_string(),
            };
        }
    };
    match Dictionary::load(path) {
        Ok(dict) => Local::Readable {
            size,
            version: dict
                .meta()
                .get(meta::KEY_HASAMI_VERSION)
                .map(str::to_string),
        },
        Err(e) => Local::Unreadable {
            size,
            reason: e.to_string(),
        },
    }
}

/// `hasami dict list` の 1 行
struct ListRow {
    name: String,
    path: PathBuf,
    /// 大きさ（置き場所にあればそのファイルの、なければ目録の）
    size: Option<u64>,
    /// 機械向けの状態（missing / ok / other-version / unreadable / verified / differs）
    state: &'static str,
    /// 人向けの状態
    status: String,
    version: Option<String>,
    summary: String,
    default: bool,
}

fn list_row(
    name: &str,
    dir: &Path,
    remote: Option<(&str, &Catalog)>,
    default: Option<&Path>,
) -> io::Result<ListRow> {
    let entry = remote.and_then(|(_, catalog)| catalog.find(name));
    let file = entry.map_or_else(|| format!("{name}.hsd"), |e| e.file.clone());
    let path = dir.join(file);
    let local = inspect_local(&path);
    let version = match &local {
        Local::Readable { version, .. } => version.clone(),
        _ => None,
    };
    let built_by = |version: &Option<String>| match version {
        Some(v) => format!("hasami {v}"),
        None => "unknown hasami".to_string(),
    };
    let (state, status) = match (&local, entry, remote) {
        (Local::Missing, _, _) => ("missing", "not downloaded".to_string()),
        (Local::Unreadable { reason, .. }, _, _) => {
            ("unreadable", format!("unreadable ({reason})"))
        }
        (Local::Readable { .. }, Some(entry), Some((label, _))) => {
            match download::verify(&path, entry)? {
                Verification::Verified => ("verified", format!("verified (matches {label})")),
                _ => (
                    "differs",
                    format!("differs from {label} ({})", built_by(&version)),
                ),
            }
        }
        (Local::Readable { .. }, _, _) => {
            if version.as_deref() == Some(env!("CARGO_PKG_VERSION")) {
                ("ok", format!("ok ({})", built_by(&version)))
            } else {
                (
                    "other-version",
                    format!("other version ({})", built_by(&version)),
                )
            }
        }
    };
    let size = match &local {
        Local::Readable { size, .. } | Local::Unreadable { size, .. } => Some(*size),
        Local::Missing => entry.map(|e| e.size),
    };
    let summary = entry
        .map(|e| e.summary.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| download::summary(name).map(str::to_string))
        .unwrap_or_default();
    Ok(ListRow {
        name: name.to_string(),
        default: default == Some(path.as_path()),
        path,
        size,
        state,
        status,
        version,
        summary,
    })
}

fn cmd_dict_list(args: DictListArgs) -> io::Result<()> {
    let dir = dict_dir(args.dir)?;
    let label = args.source.label();
    let remote = if args.remote {
        let (_, catalog) = args.source.catalog()?;
        Some(catalog)
    } else {
        None
    };
    let env_dict = std::env::var_os(hasami::analyzer::DICT_ENV).filter(|v| !v.is_empty());
    let default = if env_dict.is_none() && is_data_dir(&dir) {
        hasami::analyzer::preferred_dict_in(&dir)
    } else {
        None
    };

    // 配布辞書（--remote なら目録の順）の後に、置き場所のほかの *.hsd を名前順で
    let mut names: Vec<String> = match &remote {
        Some(catalog) => catalog
            .dictionaries
            .iter()
            .map(|d| d.name.clone())
            .collect(),
        None => hasami::analyzer::DISTRIBUTED_DICTS
            .iter()
            .map(|n| n.to_string())
            .collect(),
    };
    let mut others: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".hsd")?.to_string();
            (!stem.starts_with('.') && !names.contains(&stem)).then_some(stem)
        })
        .collect();
    others.sort();
    names.extend(others);

    let remote = remote.as_ref().map(|catalog| (label.as_str(), catalog));
    let rows = names
        .iter()
        .map(|name| list_row(name, &dir, remote, default.as_deref()))
        .collect::<io::Result<Vec<_>>>()?;

    if args.json {
        let rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "name": row.name,
                    "path": row.path.display().to_string(),
                    "state": row.state,
                    "size": row.size,
                    "hasami_version": row.version,
                    "summary": row.summary,
                    "default": row.default,
                })
            })
            .collect();
        let json = serde_json::to_string_pretty(&rows).expect("JSON values always serialize");
        println!("{json}");
        return Ok(());
    }

    println!(
        "Dictionaries in {} (this hasami: {}{})",
        display_path(&dir),
        env!("CARGO_PKG_VERSION"),
        if args.remote {
            format!(", compared with {label}")
        } else {
            String::new()
        }
    );
    println!();
    let size = |row: &ListRow| row.size.map_or_else(|| "-".to_string(), megabytes);
    let name_width = rows.iter().map(|r| r.name.len()).max().unwrap_or(0).max(4);
    let size_width = rows.iter().map(|r| size(r).len()).max().unwrap_or(0).max(4);
    let status_width = rows
        .iter()
        .map(|r| r.status.len())
        .max()
        .unwrap_or(0)
        .max(6);
    println!(
        "  {:<name_width$}  {:>size_width$}  {:<status_width$}  DESCRIPTION",
        "NAME", "SIZE", "STATUS"
    );
    for row in &rows {
        println!(
            "{} {:<name_width$}  {:>size_width$}  {:<status_width$}  {}",
            if row.default { '*' } else { ' ' },
            row.name,
            size(row),
            row.status,
            row.summary
        );
    }
    println!();
    if let Some(path) = env_dict {
        println!(
            "{} is set, so `hasami tokenize` uses {} when --dict is omitted",
            hasami::analyzer::DICT_ENV,
            display_path(Path::new(&path))
        );
    } else if default.is_some() {
        println!(
            "* `hasami tokenize` uses this dictionary when --dict is omitted (preferred order)"
        );
    } else if !is_data_dir(&dir) {
        println!("`hasami tokenize` does not search this directory; pass --dict to use these");
    } else {
        println!("No dictionary yet; `hasami dict download` fetches the recommended one");
    }
    Ok(())
}

fn cmd_dict_path(args: DictPathArgs) -> io::Result<()> {
    let path = match (args.name, args.dir) {
        (Some(name), dir) => {
            let dir = dict_dir(dir)?;
            let file = if name.ends_with(".hsd") {
                name.clone()
            } else {
                format!("{name}.hsd")
            };
            if name.is_empty() || name.starts_with('.') || file.contains(['/', '\\', ':']) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a dictionary name: {name}"),
                ));
            }
            let path = dir.join(&file);
            if !path.is_file() {
                let stem = file.strip_suffix(".hsd").unwrap_or(&file);
                let hint = if hasami::analyzer::DISTRIBUTED_DICTS.contains(&stem) {
                    format!("; `hasami dict download {stem}` fetches it")
                } else {
                    String::new()
                };
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} does not exist{hint}", path.display()),
                ));
            }
            path
        }
        (None, Some(dir)) => hasami::analyzer::preferred_dict_in(&dir).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no dictionary (*.hsd) in {}", dir.display()),
            )
        })?,
        (None, None) => hasami::analyzer::default_dict_path()?,
    };
    println!("{}", path.display());
    Ok(())
}

fn cmd_dict_install(args: DictInstallArgs) -> io::Result<()> {
    let dir = dict_dir(args.dir)?;
    let file_name = args
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let catalog_path = args.catalog.clone().or_else(|| {
        let parent = args
            .file
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let path = parent.join(download::CATALOG_FILE);
        path.is_file().then_some(path)
    });
    let entry = match &catalog_path {
        Some(path) => {
            let json = std::fs::read_to_string(path)?;
            let catalog = Catalog::parse(&json).map_err(|reason| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {reason}", path.display()),
                )
            })?;
            let entry = catalog.dictionaries.into_iter().find(|d| {
                d.file == file_name || d.compressed.as_ref().is_some_and(|c| c.file == file_name)
            });
            Some(entry.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{file_name} is not listed in {}", path.display()),
                )
            })?)
        }
        None => None,
    };
    let outcome =
        download::install(&args.file, entry.as_ref(), &dir, args.force).map_err(download_error)?;
    let how = match &catalog_path {
        Some(path) => format!("size and SHA-256 verified with {}", display_path(path)),
        None => "the whole dictionary verified".to_string(),
    };
    let path = display_path(outcome.path());
    match outcome {
        Outcome::Present(_) => println!("{file_name}: already installed ({how}): {path}"),
        Outcome::Downloaded(_) => println!("{file_name}: installed ({how}): {path}"),
    }
    print_default_note(&dir);
    Ok(())
}

fn cmd_dict_manifest(args: DictManifestArgs) -> io::Result<()> {
    let catalog = Catalog::from_dir(&args.dir).map_err(download_error)?;
    let json = catalog.to_json();
    match &args.output {
        Some(path) => std::fs::write(path, &json)?,
        None => io::stdout().lock().write_all(json.as_bytes())?,
    }
    eprintln!(
        "Catalog of {} dictionaries (hasami {}, format v{}){}",
        catalog.dictionaries.len(),
        catalog.hasami_version,
        catalog.format_version,
        args.output
            .as_ref()
            .map_or_else(String::new, |p| format!(" -> {}", p.display()))
    );
    Ok(())
}

/// 大きさを 10 進の MB（1,000,000 バイト）で、小数 1 桁で表す
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// 利用者に見せるパス。ホームディレクトリの下なら `~` から書く
fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::home_dir()
        && !home.as_os_str().is_empty()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display())
        };
    }
    path.display().to_string()
}

/// `dir` が tokenize の探す置き場所か（相対パスや末尾の区切りの違いは問わない）
fn is_data_dir(dir: &Path) -> bool {
    let absolute = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    hasami::analyzer::data_dir().is_some_and(|d| absolute(&d) == absolute(dir))
}
