//! 文分割の組み込みの例外表（src/sentence/builtin_exceptions.txt）の索引と版の識別子を作る
//!
//! 索引を実行時に組み立てると初回の分割に数十 ms かかるので、ビルド時に作って埋め込む。
//! 索引の形式と組み立ては src/sentence/index.rs（字の分類は chars.rs）をそのまま共有する。

use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "src/sentence/chars.rs"]
mod chars;
#[allow(dead_code)]
#[path = "src/sentence/index.rs"]
mod index;

const TABLE: &str = "src/sentence/builtin_exceptions.txt";

fn main() {
    for path in [
        "build.rs",
        TABLE,
        "src/sentence/chars.rs",
        "src/sentence/index.rs",
    ] {
        println!("cargo::rerun-if-changed={path}");
    }
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));

    let table = fs::read_to_string(manifest.join(TABLE)).expect("例外表を読めない");
    let index = index::Index::build(&table, true);
    fs::write(out.join("builtin_exceptions.idx"), index.to_bytes()).expect("索引を書けない");

    // 版の識別子: 語の数と、語の行（コメントと空行を除く）の FNV-1a 64
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for word in table
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        for &b in word.as_bytes().iter().chain(b"\n") {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let version = format!("{}-{hash:016x}", index.words);
    fs::write(
        out.join("builtin_exceptions_version.rs"),
        format!("\"{version}\"\n"),
    )
    .expect("版の識別子を書けない");
}
