//! Trusted-v5 storage probe. Build with the repository's release libhasami.
//! `probe DICT OUTPUT` writes numeral-prefixed keys (group u32, len u32, UTF-8).
//! `trie DICT DROP_IDS OUTPUT` rebuilds the remaining trie, preserving values.
//! Output contains aggregate counts only; dictionary keys stay under target/.
use hasami::hsd::trie::{self, Node, Trie};
use std::{collections::BTreeMap, fs, io::Write, time::Instant};

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn section(bytes: &[u8], id: u32) -> &[u8] {
    assert_eq!(&bytes[..8], b"HSMDICT\0");
    assert_eq!(u32_at(bytes, 8), 5);
    for at in (64..64 + u32_at(bytes, 16) as usize * 24).step_by(24) {
        if u32_at(bytes, at) == id {
            let start = u64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap()) as usize;
            let len = u64::from_le_bytes(bytes[at + 16..at + 24].try_into().unwrap()) as usize;
            return &bytes[start..start + len];
        }
    }
    panic!("missing section {id}")
}
fn words(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
fn numeral(c: char) -> bool {
    c.is_ascii_digit()
        || matches!(
            c,
            '０'
                ..='９'
                    | '〇'
                    | '零'
                    | '一'
                    | '二'
                    | '三'
                    | '四'
                    | '五'
                    | '六'
                    | '七'
                    | '八'
                    | '九'
                    | '十'
                    | '百'
                    | '千'
                    | '万'
                    | '億'
                    | '兆'
        )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let bytes = fs::read(&args[2])?;
    let blocks = words(section(&bytes, 2));
    let tables = words(section(&bytes, 3));
    let nodes: Vec<_> = section(&bytes, 4)
        .chunks_exact(8)
        .map(|b| Node {
            base: u32_at(b, 0),
            check: u32_at(b, 4),
        })
        .collect();
    let trie = Trie::new(&blocks, &tables, &nodes, section(&bytes, 5))?;
    if args[1] == "probe" {
        let mut out = std::io::BufWriter::new(fs::File::create(&args[3])?);
        let mut count = 0;
        let mut numeric = 0;
        trie.for_each_key(|value, key| {
            count += 1;
            let mut chars = key.chars();
            let first = chars.next();
            let starts_number = first.is_some_and(numeral)
                || (first.is_some_and(|c| matches!(c, '+' | '-' | '＋' | '－' | '−'))
                    && chars.next().is_some_and(numeral));
            if starts_number {
                numeric += 1;
                out.write_all(&value.to_le_bytes()).unwrap();
                out.write_all(&(key.len() as u32).to_le_bytes()).unwrap();
                out.write_all(key.as_bytes()).unwrap();
            }
            Ok(())
        })?;
        out.flush()?;
        println!("keys\t{count}\nprefixed_keys\t{numeric}");
        let mut pairs = BTreeMap::<(u16, u16), usize>::new();
        let mut equal = 0;
        for b in section(&bytes, 6).chunks_exact(6) {
            let left = u16::from_le_bytes(b[..2].try_into().unwrap()) & 0x7fff;
            let right = u16::from_le_bytes(b[2..4].try_into().unwrap());
            equal += usize::from(left == right);
            *pairs.entry((left, right)).or_default() += 1;
        }
        println!(
            "equal_context_entries\t{equal}\ncontext_pairs\t{}",
            pairs.len()
        );
        return Ok(());
    }
    assert_eq!(args[1], "trie");
    let drop_bytes = fs::read(&args[3])?;
    let drop_ids: std::collections::HashSet<_> =
        drop_bytes.chunks_exact(4).map(|b| u32_at(b, 0)).collect();
    let mut keys = trie.keys()?;
    keys.retain(|(value, _)| !drop_ids.contains(value));
    keys.sort_unstable_by(|a, b| a.1.cmp(&b.1));
    let values: Vec<_> = keys.iter().map(|(v, _)| *v).collect();
    let strings: Vec<_> = keys.iter().map(|(_, s)| s.as_str()).collect();
    let start = Instant::now();
    let parts = trie::build(&strings, &values)?;
    let elapsed = start.elapsed().as_secs_f64();
    let rebuilt = parts.as_trie()?;
    for (value, key) in &keys {
        assert_eq!(rebuilt.get(key)?, Some(*value));
    }
    let mut out = fs::File::create(&args[4])?;
    let output = std::path::Path::new(&args[4]);
    fs::write(
        output.with_extension("blocks"),
        parts
            .blocks
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )?;
    fs::write(
        output.with_extension("tables"),
        parts
            .tables
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )?;
    fs::write(
        output.with_extension("nodes"),
        parts
            .nodes
            .iter()
            .flat_map(|v| {
                v.base
                    .to_le_bytes()
                    .into_iter()
                    .chain(v.check.to_le_bytes())
            })
            .collect::<Vec<_>>(),
    )?;
    fs::write(output.with_extension("tails"), &parts.tails)?;
    for (name, len) in [
        ("char_blocks", parts.blocks.len() * 2),
        ("char_tables", parts.tables.len() * 2),
        ("nodes", parts.nodes.len() * 8),
        ("tails", parts.tails.len()),
    ] {
        writeln!(out, "{name}\t{len}")?;
    }
    writeln!(
        out,
        "keys\t{}\nbuild_seconds\t{elapsed}\nlookup_checked\t{}",
        keys.len(),
        keys.len()
    )?;
    Ok(())
}
