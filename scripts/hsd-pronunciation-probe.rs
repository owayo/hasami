//! Trusted v5 inputs only; experimental pronunciation patches, not a migration tool.
//! rustc -O --edition 2024 scripts/hsd-pronunciation-probe.rs -o /tmp/pron-probe
//! pron-probe OUTPUT_DIRECTORY DICTIONARY...
use std::{collections::HashMap, fs, hint::black_box, path::Path, time::Instant};

const PATCH: u8 = 16;

fn word(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn var(b: &[u8], p: &mut usize) -> Result<u32, &'static str> {
    let mut v = 0;
    for i in 0..5 {
        let c = *b.get(*p).ok_or("truncated varint")?;
        *p += 1;
        if i == 4 && c & 127 > 15 {
            return Err("varint overflow");
        }
        v |= u32::from(c & 127) << (i * 7);
        if c < 128 {
            return Ok(v);
        }
    }
    Err("varint too long")
}
fn bytes<'a>(b: &'a [u8], p: &mut usize) -> Result<&'a [u8], &'static str> {
    let n = var(b, p)? as usize;
    let end = p.checked_add(n).ok_or("length overflow")?;
    let s = b.get(*p..end).ok_or("truncated string")?;
    *p = end;
    Ok(s)
}
fn push(v: &mut Vec<u8>, mut n: u32) {
    while n >= 128 {
        v.push(n as u8 | 128);
        n >>= 7;
    }
    v.push(n as u8);
}
fn push_bytes(v: &mut Vec<u8>, b: &[u8]) {
    push(v, u32::try_from(b.len()).unwrap());
    v.extend_from_slice(b);
}

#[derive(Debug)]
struct Feature<'a> {
    grammar: u32,
    flags: u8,
    reading: &'a [u8],
    pron: Option<[&'a [u8]; 3]>,
    base: Option<&'a [u8]>,
    end: usize,
}
fn valid(b: &[u8], kana: bool) -> Result<(), &'static str> {
    if kana {
        if b.iter().any(|&c| c > 95) {
            return Err("invalid kana");
        }
    } else {
        std::str::from_utf8(b).map_err(|_| "invalid UTF-8")?;
    }
    Ok(())
}
fn decode(
    b: &[u8],
    start: usize,
    patch: bool,
    grammar_count: usize,
) -> Result<Feature<'_>, &'static str> {
    let mut p = start;
    let grammar = var(b, &mut p)?;
    if grammar as usize >= grammar_count {
        return Err("invalid grammar");
    }
    let flags = *b.get(p).ok_or("missing flags")?;
    p += 1;
    if flags & !(if patch { 31 } else { 15 }) != 0 || flags & 10 == 10 {
        return Err("invalid flags");
    }
    let reading = bytes(b, &mut p)?;
    valid(reading, flags & 4 != 0)?;
    let pron = if flags & PATCH != 0 {
        if flags & 14 != 12 {
            return Err("invalid patch flags");
        }
        let prefix = var(b, &mut p)? as usize;
        let suffix = var(b, &mut p)? as usize;
        let middle = bytes(b, &mut p)?;
        if prefix.checked_add(suffix).ok_or("patch overflow")? > reading.len() {
            return Err("patch range");
        }
        let length = prefix
            .checked_add(middle.len())
            .and_then(|n| n.checked_add(suffix))
            .ok_or("patch overflow")?;
        if length > u32::MAX as usize {
            return Err("patch length exceeds u32");
        }
        valid(middle, true)?;
        Some([
            &reading[..prefix],
            middle,
            &reading[reading.len() - suffix..],
        ])
    } else if flags & 2 != 0 {
        None
    } else {
        let pron = bytes(b, &mut p)?;
        valid(pron, flags & 8 != 0)?;
        Some([&[][..], pron, &[][..]])
    };
    let base = if flags & 1 != 0 {
        None
    } else {
        let s = bytes(b, &mut p)?;
        valid(s, false)?;
        Some(s)
    };
    Ok(Feature {
        grammar,
        flags,
        reading,
        pron,
        base,
        end: p,
    })
}
fn original(f: &Feature<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    push(&mut out, f.grammar);
    out.push(f.flags & !PATCH);
    push_bytes(&mut out, f.reading);
    if let Some(parts) = f.pron {
        push_bytes(&mut out, &parts.concat());
    }
    if let Some(base) = f.base {
        push_bytes(&mut out, base);
    }
    out
}
fn encode(f: &Feature<'_>) -> (Vec<u8>, bool) {
    let mut out = original(f);
    if f.flags & 14 != 12 {
        return (out, false);
    }
    let pron = f.pron.unwrap()[1];
    let prefix = f
        .reading
        .iter()
        .zip(pron)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = f.reading[prefix..]
        .iter()
        .rev()
        .zip(pron[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut encoded = Vec::new();
    push(&mut encoded, prefix as u32);
    push(&mut encoded, suffix as u32);
    push_bytes(&mut encoded, &pron[prefix..pron.len() - suffix]);
    let mut full = Vec::new();
    push_bytes(&mut full, pron);
    if encoded.len() >= full.len() {
        return (out, false);
    }
    out.clear();
    push(&mut out, f.grammar);
    out.push(f.flags | PATCH);
    push_bytes(&mut out, f.reading);
    out.extend_from_slice(&encoded);
    if let Some(base) = f.base {
        push_bytes(&mut out, base);
    }
    (out, true)
}
#[inline(never)]
fn consume(f: Feature<'_>) -> u64 {
    let mut sum = u64::from(f.grammar) + u64::from(f.flags & !PATCH);
    for b in f
        .reading
        .iter()
        .chain(f.pron.iter().flatten().flat_map(|part| part.iter()))
        .chain(f.base.into_iter().flatten())
    {
        sum = sum.wrapping_add(u64::from(*b));
    }
    sum
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn clock() -> std::ffi::c_long;
}
fn cpu() -> f64 {
    #[cfg(target_os = "macos")]
    {
        unsafe { clock() as f64 / 1_000_000.0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        0.0
    }
}
fn main() {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("OUTPUT_DIRECTORY DICTIONARY...");
    fs::create_dir_all(&output).unwrap();
    for path in args {
        let name = Path::new(&path).file_stem().unwrap().to_str().unwrap();
        let start = Instant::now();
        let data = fs::read(&path).unwrap();
        assert_eq!(&data[..8], b"HSMDICT\0");
        assert_eq!(word(&data, 8), 5);
        let mut sections = Vec::new();
        for i in 0..word(&data, 16) as usize {
            let p = 64 + i * 24;
            let offset = u64::from_le_bytes(data[p + 8..p + 16].try_into().unwrap()) as usize;
            let len = u64::from_le_bytes(data[p + 16..p + 24].try_into().unwrap()) as usize;
            sections.push((word(&data, p), &data[offset..offset + len]));
        }
        let section = |id| sections.iter().find(|(k, _)| *k == id).unwrap().1;
        let blob = section(8);
        let grammar_count = section(18).len() / 6;
        let mut new = Vec::with_capacity(blob.len());
        let mut remap = HashMap::new();
        let mut starts = Vec::new();
        let mut p = 0;
        let mut patched = 0;
        while p < blob.len() {
            let f = decode(blob, p, false, grammar_count).unwrap();
            let (encoded, used) = encode(&f);
            let next = u32::try_from(new.len()).unwrap();
            remap.insert(p as u32, next);
            starts.push((p, next as usize));
            new.extend_from_slice(&encoded);
            let roundtrip = decode(&new, next as usize, true, grammar_count).unwrap();
            assert_eq!(original(&roundtrip), blob[p..f.end], "feature at {p}");
            assert_eq!(roundtrip.end, new.len());
            patched += usize::from(used);
            p = f.end;
        }
        assert_eq!(p, blob.len());
        assert!(new.len() <= u32::MAX as usize);
        let offsets: Vec<u8> = section(7)
            .chunks_exact(4)
            .flat_map(|b| remap[&word(b, 0)].to_le_bytes())
            .collect();
        let mut header = data[..64].to_vec();
        header[8..12].copy_from_slice(&5006u32.to_le_bytes());
        let body = |id, old| match id {
            7 => offsets.as_slice(),
            8 => new.as_slice(),
            _ => old,
        };
        let mut position = 64 + sections.len() * 24;
        let mut table = Vec::new();
        for &(id, old) in &sections {
            position = (position + 63) & !63;
            table.extend_from_slice(&id.to_le_bytes());
            table.extend_from_slice(&0u32.to_le_bytes());
            table.extend_from_slice(&(position as u64).to_le_bytes());
            table.extend_from_slice(&(body(id, old).len() as u64).to_le_bytes());
            position += body(id, old).len();
        }
        header[24..32].copy_from_slice(&(position as u64).to_le_bytes());
        let mut file = header;
        file.extend_from_slice(&table);
        for &(id, old) in &sections {
            file.resize((file.len() + 63) & !63, 0);
            file.extend_from_slice(body(id, old));
        }
        assert_eq!(file.len(), position);
        fs::write(Path::new(&output).join(format!("{name}.hsd")), &file).unwrap();
        println!(
            "{{\"dictionary\":\"{name}\",\"v5_bytes\":{},\"patch_bytes\":{},\"features\":{},\"patched\":{patched},\"feature_bytes\":{},\"patch_feature_bytes\":{},\"conversion_wall\":{:.6},\"all_features_equal\":true}}",
            data.len(),
            file.len(),
            starts.len(),
            blob.len(),
            new.len(),
            start.elapsed().as_secs_f64()
        );
        drop(file);
        drop(remap);
        for access in ["sequential", "random"] {
            let mut seed = 20261004u64;
            let indices: Vec<usize> = (0..1_000_000)
                .map(|i| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    if access == "sequential" {
                        i % starts.len()
                    } else {
                        seed as usize % starts.len()
                    }
                })
                .collect();
            let mut sums = [0; 2];
            for round in 0..6 {
                for variant in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let (b, patch) = if variant == 0 {
                        (blob, false)
                    } else {
                        (new.as_slice(), true)
                    };
                    let begin = Instant::now();
                    let c = cpu();
                    let mut sum = 0u64;
                    for &i in &indices {
                        let offset = if variant == 0 {
                            starts[i].0
                        } else {
                            starts[i].1
                        };
                        sum = sum.wrapping_add(consume(
                            decode(black_box(b), black_box(offset), patch, grammar_count).unwrap(),
                        ));
                    }
                    sums[variant] = black_box(sum);
                    println!(
                        "{{\"dictionary\":\"{name}\",\"access\":\"{access}\",\"round\":{round},\"variant\":\"{}\",\"wall_ns\":{:.3},\"cpu_ns\":{:.3},\"checksum\":{sum}}}",
                        if patch { "patch" } else { "v5" },
                        begin.elapsed().as_secs_f64() * 1000.0,
                        (cpu() - c) * 1000.0
                    );
                }
                assert_eq!(sums[0], sums[1]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patch_rejects_invalid_ranges_flags_and_payloads() {
        for b in [
            vec![0, 29, 1, 1, 2, 0, 0],                // prefix exceeds reading
            vec![0, 29, 1, 1, 1, 1, 0],                // overlapping prefix and suffix
            vec![0, 29, 1, 1, 0, 0, 1, 96],            // invalid replacement kana
            vec![0, 29, 1, 1, 0, 0, 1],                // truncated replacement
            vec![0, 31, 1, 1, 0, 0, 0],                // omitted pronunciation + patch
            vec![0, 25, 1, 1, 0, 0, 0],                // UTF-8 reading + patch
            vec![0, 21, 1, 1, 0, 0, 0],                // UTF-8 pronunciation + patch
            vec![0, 29, 1, 1, 255, 255, 255, 255, 16], // u32 overflow
        ] {
            assert!(decode(&b, 0, true, 1).is_err(), "{b:?}");
        }
        let b = [0, 29, 1, 1, 0, 0, 1, 2];
        assert!(decode(&b, 0, true, 1).is_ok());
        assert!(decode(&b, 0, false, 1).is_err());
        for end in 0..b.len() {
            assert!(decode(&b[..end], 0, true, 1).is_err());
        }
    }
}
