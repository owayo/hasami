// Trusted v4 research inputs only. This is not a production dictionary reader.
use std::{
    collections::{HashMap, HashSet},
    fs,
    hint::black_box,
    time::Instant,
};
fn u32at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn var(b: &[u8], p: &mut usize) -> usize {
    let mut v = 0;
    let mut s = 0;
    loop {
        let c = b[*p];
        *p += 1;
        v |= ((c & 127) as usize) << s;
        if c < 128 {
            return v;
        }
        s += 7;
    }
}
fn string<'a>(b: &'a [u8], p: &mut usize) -> &'a [u8] {
    let n = var(b, p);
    let x = &b[*p..*p + n];
    *p += n;
    x
}
fn vlen(n: usize) -> usize {
    let mut n = n;
    let mut len = 1;
    while n >= 128 {
        n >>= 7;
        len += 1;
    }
    len
}
fn checksum(blob: &[u8], offset: usize, compact: bool, tuples: &[[u8; 6]]) -> u64 {
    let (t, mut p) = if compact {
        let id = u16::from_le_bytes(blob[offset..offset + 2].try_into().unwrap()) as usize;
        (&tuples[id][..], offset + 2)
    } else {
        (&blob[offset..offset + 6], offset + 6)
    };
    let mut sum = t.iter().map(|&x| x as u64).sum::<u64>();
    let flags = blob[p];
    p += 1;
    sum += flags as u64;
    for present in [true, flags & 2 == 0, flags & 1 == 0] {
        if present {
            for &x in string(blob, &mut p) {
                sum = sum.wrapping_add(x as u64);
            }
        }
    }
    sum
}
fn main() {
    for path in std::env::args().skip(1) {
        let begin = Instant::now();
        let data = fs::read(&path).unwrap();
        assert_eq!(&data[..8], b"HSMDICT\0");
        assert_eq!(u32at(&data, 8), 4);
        let mut sec = HashMap::new();
        for i in 0..u32at(&data, 16) as usize {
            let p = 64 + i * 24;
            let id = u32at(&data, p);
            let off = u64::from_le_bytes(data[p + 8..p + 16].try_into().unwrap()) as usize;
            let len = u64::from_le_bytes(data[p + 16..p + 24].try_into().unwrap()) as usize;
            sec.insert(id, &data[off..off + len]);
        }
        println!("DICT {path} bytes={}", data.len());
        for id in 1..=17 {
            println!("section_{id}={}", sec[&id].len());
        }
        let nodes = sec[&4];
        let mut kinds = [0usize; 4];
        let mut holes = 0;
        for x in nodes.chunks_exact(8) {
            if u32at(x, 4) == u32::MAX {
                holes += 1;
            } else {
                kinds[(u32at(x, 0) >> 30) as usize] += 1;
            }
        }
        println!(
            "slots={} holes={} internal={} leaf={} tail={}",
            nodes.len() / 8,
            holes,
            kinds[0],
            kinds[2],
            kinds[3]
        );
        let tails = sec[&5];
        let mut tail_unique = HashSet::new();
        let (mut p, mut nt, mut chars, mut tail_code_lengths) = (0, 0, 0, 0);
        while p < tails.len() {
            p += 4;
            let s = string(tails, &mut p);
            let count = std::str::from_utf8(s).unwrap().chars().count();
            chars += count;
            tail_code_lengths += vlen(count);
            tail_unique.insert(s);
            nt += 1;
        }
        let pool: usize = tail_unique.iter().map(|s| s.len() + vlen(s.len())).sum();
        println!(
            "tail_count={nt} tail_unique={} tail_pool_bytes={} tail_shared_total={} tail_u16_codes_total={}",
            tail_unique.len(),
            pool,
            nt * 8 + pool,
            nt * 4 + tail_code_lengths + chars * 2
        );
        drop(tail_unique);
        let f = sec[&8];
        let offsets = sec[&7];
        let mut tuple_map = HashMap::new();
        let mut tuples = Vec::<[u8; 6]>::new();
        let mut compact = Vec::new();
        let mut remap = HashMap::new();
        let (
            mut p,
            mut n,
            mut base_n,
            mut pron_n,
            mut reading_bytes,
            mut pron_bytes,
            mut base_bytes,
        ) = (0, 0, 0, 0, 0, 0, 0);
        let mut maximum = [0u16; 3];
        let mut strings = HashSet::new();
        let mut delta_pron_bytes = 0;
        let mut old_pron_bytes = 0;
        let mut tuple_frequency = HashMap::<[u8; 6], usize>::new();
        while p < f.len() {
            let start = p;
            let tuple: [u8; 6] = f[p..p + 6].try_into().unwrap();
            *tuple_frequency.entry(tuple).or_default() += 1;
            for i in 0..3 {
                maximum[i] = maximum[i].max(u16::from_le_bytes(
                    tuple[i * 2..i * 2 + 2].try_into().unwrap(),
                ));
            }
            let next = u16::try_from(tuples.len())
                .expect("prototype requires fewer than 65536 grammar tuples");
            let id = *tuple_map.entry(tuple).or_insert_with(|| {
                tuples.push(tuple);
                next
            });
            remap.insert(start as u32, compact.len() as u32);
            let flags = f[p + 6];
            p += 7;
            let reading = string(f, &mut p);
            reading_bytes += reading.len();
            strings.insert((flags & 4 != 0, reading));
            if flags & 2 == 0 {
                pron_n += 1;
                let pron = string(f, &mut p);
                pron_bytes += pron.len();
                old_pron_bytes += vlen(pron.len()) + pron.len();
                strings.insert((flags & 8 != 0, pron));
                let mut prefix = 0;
                let mut suffix = 0;
                if (flags & 4 != 0) == (flags & 8 != 0) {
                    prefix = reading.iter().zip(pron).take_while(|(a, b)| a == b).count();
                    suffix = reading[prefix..]
                        .iter()
                        .rev()
                        .zip(pron[prefix..].iter().rev())
                        .take_while(|(a, b)| a == b)
                        .count();
                }
                let replacement = pron.len() - prefix - suffix;
                delta_pron_bytes += (vlen(pron.len()) + pron.len())
                    .min(vlen(prefix) + vlen(suffix) + vlen(replacement) + replacement);
            }
            if flags & 1 == 0 {
                base_n += 1;
                let base = string(f, &mut p);
                base_bytes += base.len();
                strings.insert((false, base));
            }
            let new_start = compact.len();
            compact.extend_from_slice(&id.to_le_bytes());
            compact.extend_from_slice(&f[start + 6..p]);
            assert_eq!(&tuples[id as usize], &tuple);
            assert_eq!(&compact[new_start + 2..], &f[start + 6..p]);
            n += 1;
        }
        let pool_size: usize = strings.iter().map(|(_, s)| vlen(s.len()) + s.len()).sum();
        println!(
            "string_unique={} string_pool_bytes={} string_shared_feature_total={} delta_pron_bytes={delta_pron_bytes} delta_pron_saving={}",
            strings.len(),
            pool_size,
            n * 7 + (n + pron_n + base_n) * 4 + pool_size,
            old_pron_bytes - delta_pron_bytes
        );
        drop(strings);
        let mut freq: Vec<usize> = tuple_frequency.values().copied().collect();
        freq.sort_unstable_by(|a, b| b.cmp(a));
        let id_bytes: usize = freq
            .iter()
            .enumerate()
            .map(|(id, &count)| vlen(id) * count)
            .sum();
        println!(
            "grammar_varint_id_bytes={id_bytes} grammar_varint_saving={}",
            n * 6 - id_bytes - tuples.len() * 6
        );
        assert!(tuples.len() < 65536);
        println!(
            "features={n} tuples={} max_ids={maximum:?} base_explicit={base_n} pron_explicit={pron_n} reading_bytes={reading_bytes} pron_bytes={pron_bytes} base_bytes={base_bytes} compact_features={} tuple_bytes={} saved={}",
            tuples.len(),
            compact.len(),
            tuples.len() * 6,
            f.len() - compact.len() - tuples.len() * 6
        );
        let old: Vec<u32> = offsets.chunks_exact(4).map(|x| u32at(x, 0)).collect();
        let new: Vec<u32> = old.iter().map(|x| remap[x]).collect();
        if let Ok(dir) = std::env::var("HSD_PROBE_PARTS_DIR") {
            let name = std::path::Path::new(&path)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap();
            fs::create_dir_all(&dir).unwrap();
            let mut original_parts = offsets.to_vec();
            original_parts.extend_from_slice(f);
            let mut compact_parts = Vec::new();
            for &o in &new {
                compact_parts.extend_from_slice(&o.to_le_bytes());
            }
            for tuple in &tuples {
                compact_parts.extend_from_slice(tuple);
            }
            compact_parts.extend_from_slice(&compact);
            fs::write(format!("{dir}/{name}.original.parts"), original_parts).unwrap();
            fs::write(format!("{dir}/{name}.compact.parts"), compact_parts).unwrap();
        }
        drop(remap);
        for (&a, &b) in old.iter().zip(&new) {
            assert_eq!(
                checksum(f, a as usize, false, &tuples),
                checksum(&compact, b as usize, true, &tuples)
            );
        }
        for block in [32, 64, 128, 256] {
            let mut wide = 0;
            let mut size = 0;
            for chunk in old.chunks(block) {
                let range = chunk.iter().max().unwrap() - chunk.iter().min().unwrap();
                let width = if range < 1 << 24 {
                    3
                } else {
                    wide += 1;
                    4
                };
                // u32 block offset + u32 minimum + u8 width + payload.
                size += 4 + 4 + 1 + chunk.len() * width;
            }
            println!(
                "offset_block={block} wide_blocks={wide} size={size} saved={}",
                offsets.len() as i64 - size as i64
            );
        }
        let m = sec[&12];
        let l = u32at(m, 0) as usize;
        let r = u32at(m, 4) as usize;
        let rows: HashSet<&[u8]> = m[8..].chunks_exact(r * 2).collect();
        let cols: HashSet<Vec<u8>> = (0..r)
            .map(|c| {
                (0..l)
                    .flat_map(|row| {
                        m[8 + (row * r + c) * 2..8 + (row * r + c) * 2 + 2]
                            .iter()
                            .copied()
                    })
                    .collect()
            })
            .collect();
        println!(
            "matrix={l}x{r} unique_rows={} unique_cols={} quotient_bytes={}",
            rows.len(),
            cols.len(),
            rows.len() * cols.len() * 2 + (l + r) * 2 + 8
        );
        for round in 0..6 {
            for variant in [round % 2, 1 - round % 2] {
                let t = Instant::now();
                let mut sum = 0u64;
                let mut state = 42u64;
                for _ in 0..1_000_000 {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let index = state as usize % old.len();
                    let v = if variant == 0 {
                        checksum(black_box(f), old[index] as usize, false, &tuples)
                    } else {
                        checksum(black_box(&compact), new[index] as usize, true, &tuples)
                    };
                    sum = sum.wrapping_add(v);
                }
                println!(
                    "decode_round={round} variant={variant} ns={:.2} checksum={sum}",
                    t.elapsed().as_nanos() as f64 / 1e6
                );
            }
        }
        println!("scan_seconds={:.3}", begin.elapsed().as_secs_f64());
    }
}
