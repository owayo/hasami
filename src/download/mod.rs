//! リリースに添付した配布辞書の取得（`download` feature）
//!
//! hasami のリリースには、配布辞書（`<名前>.hsd`）、それを zstd で圧縮したもの（`<名前>.hsd.zst`）、
//! 目録（[`CATALOG_FILE`]。[`Catalog`]）、`SHA256SUMS` を添付している。
//!
//! - [`catalog`] でリリースの目録を取り、[`download`] で辞書を置き場所に置く。置き場所の既定は
//!   [`crate::analyzer::data_dir`]（[`crate::analyzer::default_dict_path`] が探す場所）
//! - 既定の取得元は、この hasami と同じ版のリリース（[`CURRENT_TAG`]）。辞書の形式や repair は版ごとに
//!   変わりうるので、版をそろえる
//! - 取得した中身は目録の大きさと SHA-256（圧縮版なら、受け取ったものと展開したものの両方）で確かめ、
//!   辞書として読めることも確かめてから、同じディレクトリの一時ファイルから rename で置く。途中で失敗しても、
//!   置き場所にある既存のファイルは消さず、壊さない
//! - 大きさと SHA-256 を自分のソースに固定したい利用者は、[`DistributedDict`] を組み立てて [`download`] に
//!   渡す（取得元を信用しきらずに使える）
//!
//! ```no_run
//! use hasami::download::{self, DownloadOptions};
//!
//! let catalog = download::catalog(download::CURRENT_TAG)?;
//! let dict = catalog.find(download::RECOMMENDED).expect("推奨の辞書は目録にある");
//! let dir = hasami::analyzer::data_dir().expect("置き場所が決まる");
//! let outcome = download::download(dict, &dir, DownloadOptions::default())?;
//! let analyzer = hasami::Analyzer::load(outcome.path())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::analyzer::DISTRIBUTED_DICTS;
use crate::hsd::meta::KEY_SOURCES;
use crate::hsd::{Dictionary, FORMAT_VERSION};
use ruzstd::decoding::StreamingDecoder;
use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use ureq::config::ConfigBuilder;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::typestate::AgentScope;

#[cfg(test)]
mod tests;

/// 配布辞書を添付したリリースの URL の接頭辞（この後に `/<タグ>/<ファイル名>` が続く）
pub const RELEASES_URL: &str = "https://github.com/owayo/hasami/releases/download";

/// リリースに添付した目録のファイル名
pub const CATALOG_FILE: &str = "dictionaries.json";

/// この hasami と同じ版のリリースのタグ（`v<版>`）。[`download`] の既定の取得元
pub const CURRENT_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// 推奨の配布辞書（最大の語彙）
pub const RECOMMENDED: &str = DISTRIBUTED_DICTS[0];

/// 配布辞書の中身（[`DISTRIBUTED_DICTS`] と同じ順）。[`Catalog::from_dir`] が目録の `summary` に書く
const SUMMARIES: [&str; 3] = [
    "IPAdic + NEologd + SudachiDict (recommended, largest vocabulary)",
    "IPAdic + NEologd",
    "IPAdic",
];

/// 配布辞書の中身の説明（配布辞書の名前でなければ `None`）
pub fn summary(name: &str) -> Option<&'static str> {
    DISTRIBUTED_DICTS
        .iter()
        .position(|&n| n == name)
        .map(|i| SUMMARIES[i])
}

/// タグ `tag` のリリースの添付ファイルの URL の接頭辞（`<RELEASES_URL>/<tag>`）
pub fn release_url(tag: &str) -> String {
    format!("{RELEASES_URL}/{tag}")
}

/// 読み書きのバッファの大きさ
const BUFFER_BYTES: usize = 256 * 1024;
/// 接続（TLS のハンドシェイクを含む）の上限
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// 要求を送ってから応答のヘッダーを受け取るまでの上限
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
/// 取得全体の上限。238MB を遅い回線で受け取る時間を見込む（1 時間で受け取るには約 0.53 Mbps 要る）
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// これより古い一時ファイルは、前回の強制終了で残ったものとみなして消す。取得の上限
/// （[`GLOBAL_TIMEOUT`]）より十分長くして、ほかのプロセスが書いている途中のものは消さない
const STALE_PART_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// 一時ファイルの名前の末尾（名前は `.<ファイル名>.<乱数>.part`）
const PART_SUFFIX: &str = ".part";
/// 目録の大きさの上限（辞書 3 つで 2KB ほど）
const MAX_CATALOG_BYTES: u64 = 1024 * 1024;

// ---------------------------------------------------------------------------
// 目録
// ---------------------------------------------------------------------------

/// リリースの配布辞書の目録（[`CATALOG_FILE`]）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    /// 辞書を作った hasami の版（リリースのタグは `v<版>`）
    pub hasami_version: String,
    /// 辞書の形式の版（[`FORMAT_VERSION`] と同じでなければ、この hasami では読めない）
    pub format_version: u32,
    /// 推奨の辞書の名前
    pub recommended: String,
    /// 配布辞書
    pub dictionaries: Vec<DistributedDict>,
}

/// 配布辞書 1 つ（目録の 1 項目）
///
/// 利用者が自分で組み立てて [`download`] に渡してもよい。大きさと SHA-256 を自分のソースに固定すれば、
/// 取得元（目録）を信用しきらずに使える。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributedDict {
    /// 名前（`ipadic` など）
    pub name: String,
    /// 中身の説明
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// ファイル名（`<名前>.hsd`）。置き場所にもこの名前で置く
    pub file: String,
    /// 大きさ（バイト）
    pub size: u64,
    /// SHA-256（小文字の 16 進）
    pub sha256: String,
    /// zstd で圧縮した同じ辞書
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed: Option<CompressedFile>,
    /// 辞書のソースの版（辞書のメタデータの `sources`）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sources: String,
}

/// zstd で圧縮した配布辞書（`<名前>.hsd.zst`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressedFile {
    /// ファイル名
    pub file: String,
    /// 大きさ（バイト）
    pub size: u64,
    /// SHA-256（小文字の 16 進）
    pub sha256: String,
}

impl Catalog {
    /// 名前で辞書を探す
    pub fn find(&self, name: &str) -> Option<&DistributedDict> {
        self.dictionaries.iter().find(|d| d.name == name)
    }

    /// 目録の JSON を読む。ファイル名・SHA-256 の書き方も確かめる（置き場所の外を指す名前は拒む）
    pub fn parse(json: &str) -> Result<Catalog, String> {
        let catalog: Catalog = serde_json::from_str(json).map_err(|e| e.to_string())?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// 目録の JSON（整形し、末尾に改行を付ける）
    pub fn to_json(&self) -> String {
        let mut json =
            serde_json::to_string_pretty(self).expect("the catalog is always serializable");
        json.push('\n');
        json
    }

    /// 項目の書き方と、名前・ファイル名が重ならないことを確かめる
    pub fn validate(&self) -> Result<(), String> {
        for (i, dict) in self.dictionaries.iter().enumerate() {
            dict.validate()?;
            if self.dictionaries[..i]
                .iter()
                .any(|d| d.name == dict.name || d.file == dict.file)
            {
                return Err(format!("duplicate dictionary: {}", dict.name));
            }
        }
        if !self.recommended.is_empty() && self.find(&self.recommended).is_none() {
            return Err(format!(
                "the recommended dictionary is not listed: {}",
                self.recommended
            ));
        }
        Ok(())
    }

    /// この hasami が目録の辞書の形式を読めるか確かめる
    pub fn check_format(&self) -> Result<(), DownloadError> {
        if self.format_version == FORMAT_VERSION {
            Ok(())
        } else {
            Err(DownloadError::FormatVersion {
                hasami_version: self.hasami_version.clone(),
                found: self.format_version,
            })
        }
    }

    /// `dir` の配布辞書（`<名前>.hsd` と、あれば `<名前>.hsd.zst`）から目録を作る（リリースの作成とミラー用）
    ///
    /// 3 つの配布辞書がすべて要る。辞書として読めること、圧縮版を展開すると元の辞書と同じになることを
    /// 確かめる。`hasami_version` はこの hasami の版。
    pub fn from_dir(dir: &Path) -> Result<Catalog, DownloadError> {
        let mut dictionaries = Vec::with_capacity(DISTRIBUTED_DICTS.len());
        for (name, summary) in DISTRIBUTED_DICTS.iter().zip(SUMMARIES) {
            let file = format!("{name}.hsd");
            let path = dir.join(&file);
            let from = path.display().to_string();
            let size = fs::metadata(&path)
                .map_err(|source| DownloadError::Inspect {
                    path: path.clone(),
                    source,
                })?
                .len();
            let sources = {
                let dict = Dictionary::load(&path).map_err(|e| DownloadError::NotDictionary {
                    from: from.clone(),
                    reason: e.to_string(),
                })?;
                dict.meta().get(KEY_SOURCES).unwrap_or_default().to_string()
            };
            let sha256 = sha256_file(&path).map_err(|source| DownloadError::Inspect {
                path: path.clone(),
                source,
            })?;
            let zst = dir.join(format!("{file}.zst"));
            let compressed = if zst.is_file() {
                Some(compressed_of(&zst, size, &sha256)?)
            } else {
                None
            };
            dictionaries.push(DistributedDict {
                name: name.to_string(),
                summary: summary.to_string(),
                file,
                size,
                sha256,
                compressed,
                sources,
            });
        }
        Ok(Catalog {
            hasami_version: env!("CARGO_PKG_VERSION").to_string(),
            format_version: FORMAT_VERSION,
            recommended: RECOMMENDED.to_string(),
            dictionaries,
        })
    }
}

/// 圧縮版 `zst` の大きさと SHA-256。展開すると元の辞書（`size`・`sha256`）と同じになることを確かめる
fn compressed_of(zst: &Path, size: u64, sha256: &str) -> Result<CompressedFile, DownloadError> {
    let from = zst.display().to_string();
    let inspect = |source| DownloadError::Inspect {
        path: zst.to_path_buf(),
        source,
    };
    let file = File::open(zst).map_err(inspect)?;
    let compressed_size = file.metadata().map_err(inspect)?.len();
    let mut noop = |_: u64, _: u64| {};
    let mut input = Tee::new(file, compressed_size, &mut noop);
    let mut sink = Sink::new(io::sink(), size);
    decode_zstd(&mut input, &mut sink)
        .map_err(|e| input.fail(&from, e, &format!("{from} (decompressed)"), size))?;
    let (decoded_size, decoded_sha256) = sink.finish();
    if decoded_size != size || decoded_sha256 != sha256 {
        return Err(DownloadError::Checksum {
            from: format!("{from} (decompressed)"),
            expected: sha256.to_string(),
            actual: decoded_sha256,
        });
    }
    Ok(CompressedFile {
        file: zst
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: compressed_size,
        sha256: input.finish(),
    })
}

impl DistributedDict {
    /// 大きさと SHA-256 だけで項目を作る（ファイル名は `<名前>.hsd`、圧縮版なし）
    pub fn new(name: &str, size: u64, sha256: &str) -> Self {
        DistributedDict {
            name: name.to_string(),
            summary: String::new(),
            file: format!("{name}.hsd"),
            size,
            sha256: sha256.to_string(),
            compressed: None,
            sources: String::new(),
        }
    }

    /// 受け取るバイト数（`compressed` が真で圧縮版があれば、圧縮版の大きさ）
    pub fn transfer_size(&self, compressed: bool) -> u64 {
        match &self.compressed {
            Some(c) if compressed => c.size,
            _ => self.size,
        }
    }

    /// 名前・ファイル名・SHA-256 の書き方を確かめる。ファイル名は置き場所の中の、隠しファイルでない名前に限る
    pub fn validate(&self) -> Result<(), String> {
        if !is_safe_file_name(&self.name) {
            return Err(format!("invalid dictionary name: {:?}", self.name));
        }
        if !is_safe_file_name(&self.file) || !self.file.ends_with(".hsd") {
            return Err(format!(
                "{}: the file name must be a plain name ending with .hsd: {:?}",
                self.name, self.file
            ));
        }
        if !is_sha256(&self.sha256) {
            return Err(format!(
                "{}: SHA-256 must be 64 lowercase hex digits: {:?}",
                self.name, self.sha256
            ));
        }
        if let Some(c) = &self.compressed {
            if !is_safe_file_name(&c.file) || !c.file.ends_with(".zst") {
                return Err(format!(
                    "{}: the compressed file name must be a plain name ending with .zst: {:?}",
                    self.name, c.file
                ));
            }
            if !is_sha256(&c.sha256) {
                return Err(format!(
                    "{}: SHA-256 of the compressed file must be 64 lowercase hex digits: {:?}",
                    self.name, c.sha256
                ));
            }
        }
        Ok(())
    }
}

/// パスの区切りを含まず、`.` で始まらない（`.`・`..` と隠しファイルにならない）名前
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn is_sha256(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// タグ `tag` のリリースの目録を取る。目録の `hasami_version` がタグの版と合うことも確かめる
pub fn catalog(tag: &str) -> Result<Catalog, DownloadError> {
    let base_url = release_url(tag);
    let catalog = catalog_from(&base_url)?;
    ensure_version(catalog, tag, &catalog_url(&base_url))
}

/// 目録の `hasami_version` がタグ `tag`（`v<版>`）の版と合うことを確かめる
fn ensure_version(catalog: Catalog, tag: &str, url: &str) -> Result<Catalog, DownloadError> {
    if tag.strip_prefix('v').unwrap_or(tag) == catalog.hasami_version {
        Ok(catalog)
    } else {
        Err(DownloadError::CatalogVersion {
            url: url.to_string(),
            tag: tag.to_string(),
            found: catalog.hasami_version,
        })
    }
}

/// 取得元 `base_url`（URL の接頭辞。ミラーなど）の目録（`<base_url>/dictionaries.json`）を取る
pub fn catalog_from(base_url: &str) -> Result<Catalog, DownloadError> {
    catalog_with(&agent(), base_url)
}

fn catalog_url(base_url: &str) -> String {
    format!("{}/{CATALOG_FILE}", base_url.trim_end_matches('/'))
}

fn catalog_with(agent: &ureq::Agent, base_url: &str) -> Result<Catalog, DownloadError> {
    let url = catalog_url(base_url);
    let mut response = get(agent, &url)?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_CATALOG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| DownloadError::Receive {
            from: url.clone(),
            source,
        })?;
    if bytes.len() as u64 > MAX_CATALOG_BYTES {
        return Err(DownloadError::Catalog {
            url,
            reason: format!("larger than {MAX_CATALOG_BYTES} bytes"),
        });
    }
    let json = String::from_utf8(bytes).map_err(|_| DownloadError::Catalog {
        url: url.clone(),
        reason: "not UTF-8".to_string(),
    })?;
    Catalog::parse(&json).map_err(|reason| DownloadError::Catalog { url, reason })
}

// ---------------------------------------------------------------------------
// 置き場所のファイルの確認
// ---------------------------------------------------------------------------

/// 置き場所のファイルの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    /// ファイルがない
    Missing,
    /// 大きさと SHA-256 が配布辞書と一致した
    Verified,
    /// 中身が違う（hasami の別の版か、壊れている）
    Differs,
}

/// `path` のファイルを配布辞書の大きさと SHA-256 で確かめる。大きさが違えばハッシュを計算しない
pub fn verify(path: &Path, dict: &DistributedDict) -> io::Result<Verification> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Verification::Missing),
        Err(e) => return Err(e),
    };
    if !metadata.is_file() || metadata.len() != dict.size {
        return Ok(Verification::Differs);
    }
    Ok(if sha256_file(path)? == dict.sha256 {
        Verification::Verified
    } else {
        Verification::Differs
    })
}

/// ファイルの SHA-256（小文字の 16 進）。流し読みで計算する
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_BYTES];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buffer[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(to_hex(&hasher.finalize()))
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        hex.push(char::from(DIGITS[usize::from(b >> 4)]));
        hex.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    hex
}

// ---------------------------------------------------------------------------
// 取得
// ---------------------------------------------------------------------------

/// [`download`] の設定
pub struct DownloadOptions<'a> {
    /// 取得元（URL の接頭辞。`<base_url>/<ファイル名>` を取る）。`None` なら [`CURRENT_TAG`] のリリース。
    /// [`catalog`] で別の版の目録を取ったときは、その版の [`release_url`] を渡す
    pub base_url: Option<&'a str>,
    /// 圧縮版があればそちらを取って展開する（既定は true。受け取る量が 3 分の 1 ほどになる）
    pub compressed: bool,
    /// 正しいファイルがあっても取り直す。中身の違うファイルも置き換える
    pub force: bool,
    /// 進み具合（受信したバイト数、受信する全体のバイト数）。通信を始める前に 1 度（受信 0 で）、
    /// その後は受け取るたびに呼ぶ。通信しないときは呼ばない
    pub progress: Option<&'a mut dyn FnMut(u64, u64)>,
}

impl Default for DownloadOptions<'_> {
    fn default() -> Self {
        DownloadOptions {
            base_url: None,
            compressed: true,
            force: false,
            progress: None,
        }
    }
}

/// 取得や持ち込みの結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 正しいファイルがすでにあった（通信していない）
    Present(PathBuf),
    /// 取得して（持ち込んで）置いた
    Downloaded(PathBuf),
}

impl Outcome {
    /// 置き場所のファイル
    pub fn path(&self) -> &Path {
        match self {
            Outcome::Present(path) | Outcome::Downloaded(path) => path,
        }
    }
}

/// 配布辞書 `dict` を `dir` に取得する（置き場所は `<dir>/<dict.file>`）
///
/// 正しいファイルがすでにあれば、`force` でなければ通信せずに [`Outcome::Present`] を返す。中身の
/// 違うファイルがあれば、`force` でなければ [`DownloadError::Differs`] にする。
pub fn download(
    dict: &DistributedDict,
    dir: &Path,
    options: DownloadOptions<'_>,
) -> Result<Outcome, DownloadError> {
    download_with(&agent(), dict, dir, options)
}

/// 取得に使う HTTP の設定。プロキシは ureq の既定（環境変数 `HTTPS_PROXY`・`NO_PROXY` など）のまま
fn agent_config() -> ConfigBuilder<AgentScope> {
    ureq::Agent::config_builder()
        .user_agent(concat!("hasami/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        // 2xx 以外はすべて自分で確かめる（既定では、たどらない 3xx が成功として返る）
        .http_status_as_error(false)
        // provider と root_certs は明示する。root_certs の既定（WebPki）は同梱のルート証明書だけを
        // 信頼するので、社内の CA を OS に入れた環境（TLS を検査するプロキシの下など）で通らない
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::Rustls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
}

fn agent() -> ureq::Agent {
    agent_config().build().new_agent()
}

fn download_with(
    agent: &ureq::Agent,
    dict: &DistributedDict,
    dir: &Path,
    options: DownloadOptions<'_>,
) -> Result<Outcome, DownloadError> {
    dict.validate().map_err(DownloadError::InvalidDict)?;
    fs::create_dir_all(dir).map_err(|source| DownloadError::CreateDir {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = dir.join(&dict.file);
    // force なら中身を問わず取り直すので、既存のファイルのハッシュは計算しない
    if !options.force {
        match verify(&path, dict).map_err(|source| DownloadError::Inspect {
            path: path.clone(),
            source,
        })? {
            Verification::Verified => return Ok(Outcome::Present(path)),
            Verification::Differs => {
                return Err(DownloadError::Differs {
                    path,
                    name: dict.name.clone(),
                });
            }
            Verification::Missing => {}
        }
    }
    remove_stale_parts(dir, &dict.file, SystemTime::now());

    let base_url = match options.base_url {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => release_url(CURRENT_TAG),
    };
    let mut noop = |_, _| {};
    let progress: &mut dyn FnMut(u64, u64) = match options.progress {
        Some(progress) => progress,
        None => &mut noop,
    };
    let compressed = dict.compressed.as_ref().filter(|_| options.compressed);
    let from = match compressed {
        Some(c) => format!("{base_url}/{}", c.file),
        None => format!("{base_url}/{}", dict.file),
    };
    let placed = place(
        dir,
        &dict.file,
        |out| match compressed {
            Some(c) => receive_compressed(agent, &from, c, dict, out, progress),
            None => receive(agent, &from, dict, out, progress),
        },
        |part, ()| check_loadable(part, &from),
    )?;
    Ok(Outcome::Downloaded(placed))
}

/// 書き込みの失敗と、それ以外の失敗（書き込みの失敗は、一時ファイルのパスを添えて [`place`] がエラーにする）
enum Failure {
    Write(io::Error),
    Other(DownloadError),
}

impl From<DownloadError> for Failure {
    fn from(e: DownloadError) -> Self {
        Failure::Other(e)
    }
}

/// `dir` の一時ファイルに `write` で書き、`check` で確かめてから `<dir>/<file>` に rename で置く
/// （`check` は `write` が返した値も受け取る）
///
/// 失敗したら（エラーでもパニックでも）一時ファイルは drop で消え、置き場所にある既存のファイルには触れない。
fn place<T>(
    dir: &Path,
    file: &str,
    write: impl FnOnce(&mut File) -> Result<T, Failure>,
    check: impl FnOnce(&Path, T) -> Result<(), DownloadError>,
) -> Result<PathBuf, DownloadError> {
    let prefix = part_prefix(file);
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix).suffix(PART_SUFFIX);
    // 置いた辞書は File::create で作るファイルと同じく umask に従う権限にする（一時ファイルの既定の 0600 の
    // ままだと、共有の置き場所でほかの利用者が読めない）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o644));
    }
    let mut part = builder
        .tempfile_in(dir)
        .map_err(|source| DownloadError::TempFile {
            dir: dir.to_path_buf(),
            source,
        })?;
    let write_error = |path: &Path, source| DownloadError::Write {
        path: path.to_path_buf(),
        source,
    };
    let value = match write(part.as_file_mut()) {
        Ok(value) => value,
        Err(Failure::Write(e)) => return Err(write_error(part.path(), e)),
        Err(Failure::Other(e)) => return Err(e),
    };
    part.as_file_mut()
        .flush()
        .and_then(|()| part.as_file().sync_all())
        .map_err(|e| write_error(part.path(), e))?;
    // 読んだ辞書（ファイルの mmap）は置き換えの前に捨てる（Windows では開いたままのファイルを rename できない）
    check(part.path(), value)?;
    let path = dir.join(file);
    part.persist(&path).map_err(|e| DownloadError::Persist {
        path: path.clone(),
        source: e.error,
    })?;
    Ok(path)
}

/// 辞書として読めること（形式の版が合うこと）を確かめる
fn check_loadable(path: &Path, from: &str) -> Result<(), DownloadError> {
    Dictionary::load(path)
        .map(drop)
        .map_err(|e| DownloadError::NotDictionary {
            from: from.to_string(),
            reason: e.to_string(),
        })
}

/// `url` を GET する。2xx 以外はエラー
fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, DownloadError> {
    let response = agent
        .get(url)
        .call()
        .map_err(|source| DownloadError::Request {
            url: url.to_string(),
            source: Box::new(source),
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Status {
            url: url.to_string(),
            status: status.as_u16(),
        });
    }
    Ok(response)
}

/// 大きさが違うと分かっていれば、本体を読まずにやめる。ヘッダーを直に読む（ureq は `Content-Length: 0` を
/// 本体なしとみなし、`Body::content_length` では `None` を返すため）
fn check_content_length(
    response: &ureq::http::Response<ureq::Body>,
    url: &str,
    expected: u64,
) -> Result<(), DownloadError> {
    let declared = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    match declared {
        Some(actual) if actual != expected => Err(DownloadError::ContentLength {
            url: url.to_string(),
            expected,
            actual,
        }),
        _ => Ok(()),
    }
}

/// `url` の生の辞書を `out` に書く。大きさと SHA-256 を、受け取りながら確かめる
fn receive(
    agent: &ureq::Agent,
    url: &str,
    dict: &DistributedDict,
    out: &mut File,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), Failure> {
    progress(0, dict.size);
    let mut response = get(agent, url)?;
    check_content_length(&response, url, dict.size)?;
    let mut input = Tee::new(response.body_mut().as_reader(), dict.size, progress);
    let mut sink = Sink::new(out, dict.size);
    let mut buffer = vec![0u8; BUFFER_BYTES];
    loop {
        let n = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(input.fail(url, Decode::Input(e), url, dict.size).into()),
        };
        // 受け取った量は Tee が確かめるので、書き込みで上限を超えることはない
        sink.write(&buffer[..n]).map_err(|e| match e {
            SinkError::Write(e) => Failure::Write(e),
            SinkError::Over => Failure::Other(DownloadError::Oversized {
                from: url.to_string(),
                expected: dict.size,
            }),
        })?;
    }
    let received = input.received;
    let sha256 = input.finish();
    if received != dict.size {
        return Err(DownloadError::Truncated {
            from: url.to_string(),
            expected: dict.size,
            received,
        }
        .into());
    }
    if sha256 != dict.sha256 {
        return Err(DownloadError::Checksum {
            from: url.to_string(),
            expected: dict.sha256.clone(),
            actual: sha256,
        }
        .into());
    }
    Ok(())
}

/// `url` の圧縮版を受け取りながら展開して `out` に書く。受け取ったものと展開したものの両方を、
/// 大きさと SHA-256 で確かめる
fn receive_compressed(
    agent: &ureq::Agent,
    url: &str,
    compressed: &CompressedFile,
    dict: &DistributedDict,
    out: &mut File,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), Failure> {
    progress(0, compressed.size);
    let mut response = get(agent, url)?;
    check_content_length(&response, url, compressed.size)?;
    let mut input = Tee::new(response.body_mut().as_reader(), compressed.size, progress);
    let mut sink = Sink::new(out, dict.size);
    let decoded = format!("{url} (decompressed)");
    decode_zstd(&mut input, &mut sink).map_err(|e| match e {
        Decode::Output(SinkError::Write(e)) => Failure::Write(e),
        e => Failure::Other(input.fail(url, e, &decoded, dict.size)),
    })?;
    let received = input.received;
    let sha256 = input.finish();
    if received != compressed.size {
        return Err(DownloadError::Truncated {
            from: url.to_string(),
            expected: compressed.size,
            received,
        }
        .into());
    }
    if sha256 != compressed.sha256 {
        return Err(DownloadError::Checksum {
            from: url.to_string(),
            expected: compressed.sha256.clone(),
            actual: sha256,
        }
        .into());
    }
    finish_decoded(sink, &decoded, dict).map_err(Failure::Other)
}

/// 展開したものの大きさと SHA-256 を確かめる
fn finish_decoded<W: Write>(
    sink: Sink<W>,
    from: &str,
    dict: &DistributedDict,
) -> Result<(), DownloadError> {
    let (written, sha256) = sink.finish();
    if written != dict.size {
        return Err(DownloadError::Truncated {
            from: from.to_string(),
            expected: dict.size,
            received: written,
        });
    }
    if sha256 != dict.sha256 {
        return Err(DownloadError::Checksum {
            from: from.to_string(),
            expected: dict.sha256.clone(),
            actual: sha256,
        });
    }
    Ok(())
}

/// 読んだ量と SHA-256 を数えながら読む。`limit` を超えたら読むのをやめる
struct Tee<'p, R> {
    inner: R,
    hasher: Sha256,
    received: u64,
    limit: u64,
    progress: &'p mut dyn FnMut(u64, u64),
    /// `limit` を超えた
    over: bool,
    /// 読めなかった（`inner` が返したエラー）
    error: Option<io::Error>,
    /// 終わりまで読んだ
    eof: bool,
}

impl<'p, R: Read> Tee<'p, R> {
    fn new(inner: R, limit: u64, progress: &'p mut dyn FnMut(u64, u64)) -> Self {
        Tee {
            inner,
            hasher: Sha256::new(),
            received: 0,
            limit,
            progress,
            over: false,
            error: None,
            eof: false,
        }
    }

    /// 読んだものの SHA-256
    fn finish(self) -> String {
        to_hex(&self.hasher.finalize())
    }

    /// 読みながらの処理の失敗を、原因のエラーにする。受け取る側の失敗（大きすぎる・読めない・途中で切れた）を
    /// 展開の失敗より先に見る（途中で切れた圧縮版は、展開のエラーとしても現れるため）
    fn fail(&mut self, from: &str, e: Decode, decoded: &str, decoded_limit: u64) -> DownloadError {
        if self.over {
            return DownloadError::Oversized {
                from: from.to_string(),
                expected: self.limit,
            };
        }
        if let Some(source) = self.error.take() {
            return DownloadError::Receive {
                from: from.to_string(),
                source,
            };
        }
        if self.eof && self.received < self.limit {
            return DownloadError::Truncated {
                from: from.to_string(),
                expected: self.limit,
                received: self.received,
            };
        }
        match e {
            Decode::Output(SinkError::Over) => DownloadError::Oversized {
                from: decoded.to_string(),
                expected: decoded_limit,
            },
            Decode::Output(SinkError::Write(source)) => DownloadError::Receive {
                from: decoded.to_string(),
                source,
            },
            Decode::Input(source) => DownloadError::Decompress {
                from: from.to_string(),
                reason: source.to_string(),
            },
            Decode::Zstd(reason) => DownloadError::Decompress {
                from: from.to_string(),
                reason,
            },
        }
    }
}

impl<R: Read> Read for Tee<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = match self.inner.read(buf) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
            Err(e) => {
                let copy = io::Error::new(e.kind(), e.to_string());
                self.error = Some(e);
                return Err(copy);
            }
        };
        if n == 0 {
            self.eof = true;
            return Ok(0);
        }
        self.received += n as u64;
        if self.received > self.limit {
            self.over = true;
            return Err(io::Error::other("received more data than expected"));
        }
        self.hasher.update(&buf[..n]);
        (self.progress)(self.received, self.limit);
        Ok(n)
    }
}

/// 書いた量と SHA-256 を数えながら書く。`limit` を超える分は書かない
struct Sink<W> {
    out: W,
    hasher: Sha256,
    written: u64,
    limit: u64,
}

enum SinkError {
    /// `limit` を超えた
    Over,
    Write(io::Error),
}

impl<W: Write> Sink<W> {
    fn new(out: W, limit: u64) -> Self {
        Sink {
            out,
            hasher: Sha256::new(),
            written: 0,
            limit,
        }
    }

    fn write(&mut self, chunk: &[u8]) -> Result<(), SinkError> {
        if self.written + chunk.len() as u64 > self.limit {
            return Err(SinkError::Over);
        }
        self.hasher.update(chunk);
        self.out.write_all(chunk).map_err(SinkError::Write)?;
        self.written += chunk.len() as u64;
        Ok(())
    }

    /// 書いた量と SHA-256
    fn finish(self) -> (u64, String) {
        (self.written, to_hex(&self.hasher.finalize()))
    }
}

/// zstd の展開の失敗
enum Decode {
    /// 入力を読めない（展開の途中のエラーも、入力のエラーとして返ってくる）
    Input(io::Error),
    /// フレームのヘッダーが読めない
    Zstd(String),
    /// 出力の失敗
    Output(SinkError),
}

/// `input` の zstd のフレームをすべて展開して `sink` に書く（スキップ可能なフレームは読み飛ばす）
fn decode_zstd<R: Read, W: Write>(input: R, sink: &mut Sink<W>) -> Result<(), Decode> {
    let mut input = BufReader::with_capacity(BUFFER_BYTES, input);
    let mut buffer = vec![0u8; BUFFER_BYTES];
    loop {
        if input.fill_buf().map_err(Decode::Input)?.is_empty() {
            return Ok(());
        }
        let mut decoder = match StreamingDecoder::new(&mut input) {
            Ok(decoder) => decoder,
            Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
                length,
                ..
            })) => {
                let skipped = io::copy(&mut (&mut input).take(u64::from(length)), &mut io::sink())
                    .map_err(Decode::Input)?;
                if skipped < u64::from(length) {
                    return Err(Decode::Input(io::ErrorKind::UnexpectedEof.into()));
                }
                continue;
            }
            Err(FrameDecoderError::ReadFrameHeaderError(
                ReadFrameHeaderError::MagicNumberReadError(e)
                | ReadFrameHeaderError::FrameDescriptorReadError(e),
            )) => return Err(Decode::Input(e)),
            Err(e) => return Err(Decode::Zstd(e.to_string())),
        };
        loop {
            let n = match decoder.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Decode::Input(e)),
            };
            sink.write(&buffer[..n]).map_err(Decode::Output)?;
        }
    }
}

/// 一時ファイルの名前の先頭（`.<ファイル名>.`）
fn part_prefix(file: &str) -> String {
    format!(".{file}.")
}

/// 前回の強制終了で残った、同じ辞書の一時ファイル（[`STALE_PART_AGE`] より古いもの）を消す。
/// 消せなくても取得は続ける
fn remove_stale_parts(dir: &Path, file: &str, now: SystemTime) {
    let prefix = part_prefix(file);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !(name.starts_with(&prefix) && name.ends_with(PART_SUFFIX)) {
            continue;
        }
        // symlink はたどらずに確かめる（ファイルそのものだけを消す）
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let stale = metadata.is_file()
            && metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > STALE_PART_AGE);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

// ---------------------------------------------------------------------------
// 持ち込み
// ---------------------------------------------------------------------------

/// 手元の辞書ファイル（`.hsd`、zstd で圧縮した `.hsd.zst`）を確かめて `dir` に置く（ネットワークに
/// 出られない環境向け）
///
/// `dict` を渡せば、大きさと SHA-256 で確かめて `<dir>/<dict.file>` に置く（圧縮版なら展開したものを
/// 確かめる）。渡さなければ、辞書全体を検証（[`Dictionary::verify`]）して、`file` の名前から `.zst` を
/// 除いた名前で置く。正しいファイルがすでにあれば、`force` でなければ [`Outcome::Present`] を返す。
pub fn install(
    file: &Path,
    dict: Option<&DistributedDict>,
    dir: &Path,
    force: bool,
) -> Result<Outcome, DownloadError> {
    let from = file.display().to_string();
    let input_name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let compressed = input_name.ends_with(".zst");
    let target = match dict {
        Some(dict) => {
            dict.validate().map_err(DownloadError::InvalidDict)?;
            dict.file.clone()
        }
        None => {
            let name = input_name.strip_suffix(".zst").unwrap_or(input_name);
            if !is_safe_file_name(name) || !name.ends_with(".hsd") {
                return Err(DownloadError::InvalidDict(format!(
                    "{from}: the file name must be <name>.hsd or <name>.hsd.zst"
                )));
            }
            name.to_string()
        }
    };
    fs::create_dir_all(dir).map_err(|source| DownloadError::CreateDir {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = dir.join(&target);
    if let (Some(dict), false) = (dict, force) {
        match verify(&path, dict).map_err(|source| DownloadError::Inspect {
            path: path.clone(),
            source,
        })? {
            Verification::Verified => return Ok(Outcome::Present(path)),
            Verification::Differs => {
                return Err(DownloadError::Differs {
                    path,
                    name: dict.name.clone(),
                });
            }
            Verification::Missing => {}
        }
    }
    let open = |source| DownloadError::Inspect {
        path: file.to_path_buf(),
        source,
    };
    let input = File::open(file).map_err(open)?;
    let input_size = input.metadata().map_err(open)?.len();
    let limit = dict.map_or(u64::MAX, |d| d.size);
    let placed = place(
        dir,
        &target,
        |out| {
            let mut noop = |_: u64, _: u64| {};
            let mut input = Tee::new(input, input_size, &mut noop);
            let mut sink = Sink::new(out, limit);
            let decoded = if compressed {
                decode_zstd(&mut input, &mut sink)
            } else {
                copy(&mut input, &mut sink)
            };
            decoded.map_err(|e| match e {
                Decode::Output(SinkError::Write(e)) => Failure::Write(e),
                e => Failure::Other(input.fail(&from, e, &from, limit)),
            })?;
            let (written, sha256) = sink.finish();
            if let Some(dict) = dict {
                if written != dict.size {
                    return Err(Failure::Other(DownloadError::Truncated {
                        from: from.clone(),
                        expected: dict.size,
                        received: written,
                    }));
                }
                if sha256 != dict.sha256 {
                    return Err(Failure::Other(DownloadError::Checksum {
                        from: from.clone(),
                        expected: dict.sha256.clone(),
                        actual: sha256,
                    }));
                }
            }
            Ok(sha256)
        },
        |part, sha256| match dict {
            Some(_) => check_loadable(part, &from),
            None => check_verified(part, &from, &path, &sha256, force),
        },
    )?;
    Ok(Outcome::Downloaded(placed))
}

/// 大きさと SHA-256 の分からない辞書を、全体を検証して確かめる。置き場所に中身の違うファイルがあれば、
/// `force` でなければ置き換えない（同じ中身なら置き換えても変わらない）
fn check_verified(
    part: &Path,
    from: &str,
    path: &Path,
    sha256: &str,
    force: bool,
) -> Result<(), DownloadError> {
    let not_dictionary = |e: crate::DictError| DownloadError::NotDictionary {
        from: from.to_string(),
        reason: e.to_string(),
    };
    Dictionary::load(part)
        .map_err(not_dictionary)?
        .verify()
        .map_err(not_dictionary)?;
    if !force && path.exists() {
        let existing = sha256_file(path).map_err(|source| DownloadError::Inspect {
            path: path.to_path_buf(),
            source,
        })?;
        if existing != sha256 {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(DownloadError::Differs {
                path: path.to_path_buf(),
                name,
            });
        }
    }
    Ok(())
}

/// `input` をそのまま `sink` に書く
fn copy<R: Read, W: Write>(input: &mut R, sink: &mut Sink<W>) -> Result<(), Decode> {
    let mut buffer = vec![0u8; BUFFER_BYTES];
    loop {
        let n = match input.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Decode::Input(e)),
        };
        sink.write(&buffer[..n]).map_err(Decode::Output)?;
    }
}

// ---------------------------------------------------------------------------
// エラー
// ---------------------------------------------------------------------------

/// 取得・持ち込み・目録の失敗
#[derive(Debug)]
#[non_exhaustive]
pub enum DownloadError {
    /// 辞書の項目が使えない（名前・ファイル名・SHA-256 の書き方）
    InvalidDict(String),
    /// 目録を読めない（JSON の形、項目の書き方）
    Catalog { url: String, reason: String },
    /// 目録が別の版のもの
    CatalogVersion {
        url: String,
        tag: String,
        found: String,
    },
    /// 目録の辞書の形式を、この hasami が読めない
    FormatVersion { hasami_version: String, found: u32 },
    /// 置き場所のディレクトリを作れない
    CreateDir { path: PathBuf, source: io::Error },
    /// ファイルを確かめられない（読めない）
    Inspect { path: PathBuf, source: io::Error },
    /// 置き場所に中身の違うファイルがある
    Differs { path: PathBuf, name: String },
    /// 一時ファイルを作れない
    TempFile { dir: PathBuf, source: io::Error },
    /// 要求を送れない、応答を受け取れない
    Request {
        url: String,
        source: Box<ureq::Error>,
    },
    /// 2xx 以外の応答
    Status { url: String, status: u16 },
    /// `Content-Length` が期待する大きさと違う
    ContentLength {
        url: String,
        expected: u64,
        actual: u64,
    },
    /// 受信・読み込みに失敗した
    Receive { from: String, source: io::Error },
    /// 期待する大きさを超えた
    Oversized { from: String, expected: u64 },
    /// 期待する大きさに届かないまま終わった
    Truncated {
        from: String,
        expected: u64,
        received: u64,
    },
    /// SHA-256 が違う
    Checksum {
        from: String,
        expected: String,
        actual: String,
    },
    /// zstd を展開できない
    Decompress { from: String, reason: String },
    /// 一時ファイルに書き込めない
    Write { path: PathBuf, source: io::Error },
    /// 辞書として読めない（形式の版が違う、壊れている）
    NotDictionary { from: String, reason: String },
    /// 置き場所に置けない（rename の失敗）
    Persist { path: PathBuf, source: io::Error },
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DownloadError::InvalidDict(reason) => write!(f, "invalid dictionary entry: {reason}"),
            DownloadError::Catalog { url, reason } => {
                write!(f, "invalid dictionary catalog {url}: {reason}")
            }
            DownloadError::CatalogVersion { url, tag, found } => write!(
                f,
                "the dictionary catalog {url} is for hasami {found}, not {tag}"
            ),
            DownloadError::FormatVersion {
                hasami_version,
                found,
            } => write!(
                f,
                "the dictionaries of hasami {hasami_version} use format v{found}, but this hasami reads v{FORMAT_VERSION}"
            ),
            DownloadError::CreateDir { path, source } => {
                write!(f, "cannot create {}: {source}", path.display())
            }
            DownloadError::Inspect { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            DownloadError::Differs { path, name } => write!(
                f,
                "{} is not the distributed {name} (another version or a damaged file); replace it with --force",
                path.display()
            ),
            DownloadError::TempFile { dir, source } => {
                write!(
                    f,
                    "cannot create a temporary file in {}: {source}",
                    dir.display()
                )
            }
            DownloadError::Request { url, source } => write!(f, "cannot fetch {url}: {source}"),
            DownloadError::Status { url, status } => {
                write!(f, "cannot fetch {url} (HTTP {status})")
            }
            DownloadError::ContentLength {
                url,
                expected,
                actual,
            } => write!(
                f,
                "{url} has an unexpected size (Content-Length is {actual} bytes, expected {expected})"
            ),
            DownloadError::Receive { from, source } => write!(f, "cannot read {from}: {source}"),
            DownloadError::Oversized { from, expected } => {
                write!(f, "{from} is larger than the expected {expected} bytes")
            }
            DownloadError::Truncated {
                from,
                expected,
                received,
            } => write!(f, "{from} ended early ({received} of {expected} bytes)"),
            DownloadError::Checksum {
                from,
                expected,
                actual,
            } => write!(
                f,
                "SHA-256 of {from} does not match (expected {expected}, got {actual})"
            ),
            DownloadError::Decompress { from, reason } => {
                write!(f, "cannot decompress {from}: {reason}")
            }
            DownloadError::Write { path, source } => {
                write!(f, "cannot write {}: {source}", path.display())
            }
            DownloadError::NotDictionary { from, reason } => {
                write!(
                    f,
                    "{from} is not a dictionary this hasami can read: {reason}"
                )
            }
            DownloadError::Persist { path, source } => {
                write!(
                    f,
                    "cannot put the dictionary at {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for DownloadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DownloadError::CreateDir { source, .. }
            | DownloadError::Inspect { source, .. }
            | DownloadError::TempFile { source, .. }
            | DownloadError::Receive { source, .. }
            | DownloadError::Write { source, .. }
            | DownloadError::Persist { source, .. } => Some(source),
            DownloadError::Request { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
