//! Compare format variants with the same analyzer API (standalone rustc program).
//! Usage: eval bench|dump|load DICT CORPUS [PASSES]
//! Compile against each release libhasami.rlib; dump is length-prefixed binary.
use hasami::Analyzer;
use std::{
    hint::black_box,
    io::{self, Write},
    time::Instant,
};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_set_qos_class_self_np(class: u32, relative_priority: i32) -> i32;
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
fn number(w: &mut impl Write, n: usize) {
    w.write_all(&(n as u64).to_le_bytes()).unwrap();
}
fn string(w: &mut impl Write, s: &str) {
    number(w, s.len());
    w.write_all(s.as_bytes()).unwrap();
}
fn main() {
    #[cfg(target_os = "macos")]
    {
        assert_eq!(unsafe { pthread_set_qos_class_self_np(0x21, 0) }, 0);
    }
    let args: Vec<String> = std::env::args().collect();
    let mode = &args[1];
    let dict = &args[2];
    if mode == "load" {
        for i in 0..31 {
            let time = Instant::now();
            let c = cpu();
            let mut a = Analyzer::load(dict).unwrap();
            let count = black_box(
                a.try_tokenize("東京都に住んでいる人々が増えている。")
                    .unwrap(),
            )
            .len();
            println!(
                "load {i} wall_us={:.3} cpu_us={:.3} tokens={count}",
                time.elapsed().as_secs_f64() * 1e6,
                (cpu() - c) * 1e6
            );
        }
        return;
    }
    let data = std::fs::read_to_string(&args[3]).unwrap();
    let lines: Vec<&str> = data.lines().collect();
    let mut a = Analyzer::load(dict).unwrap();
    if mode == "dump" {
        let mut w = io::BufWriter::with_capacity(1 << 20, io::stdout().lock());
        number(&mut w, lines.len());
        let mut total = 0;
        for line in lines {
            let tokens = a.try_tokenize(line).unwrap();
            number(&mut w, tokens.len());
            total += tokens.len();
            for t in tokens {
                number(&mut w, t.start);
                number(&mut w, t.end);
                for s in [
                    &t.surface,
                    &t.pos,
                    &t.conj_type,
                    &t.conj_form,
                    &t.base_form,
                    &t.reading,
                    &t.pronunciation,
                ] {
                    string(&mut w, s);
                }
                w.write_all(&t.word_cost.to_le_bytes()).unwrap();
                w.write_all(&[u8::from(t.is_known)]).unwrap();
            }
        }
        w.flush().unwrap();
        eprintln!("tokens={total}");
    } else {
        assert_eq!(mode, "bench");
        let passes: usize = args.get(4).map_or(1, |s| s.parse().unwrap());
        // Warm pages and the analyzer, then report each measured pass separately.
        for line in &lines {
            black_box(a.try_tokenize(line).unwrap());
        }
        for i in 0..passes {
            let start = Instant::now();
            let c = cpu();
            let mut total = 0;
            for line in &lines {
                total += black_box(a.try_tokenize(black_box(line)).unwrap()).len();
            }
            println!(
                "pass={i} wall={:.6} cpu={:.6} lines={} tokens={total}",
                start.elapsed().as_secs_f64(),
                cpu() - c,
                lines.len()
            );
        }
    }
}
