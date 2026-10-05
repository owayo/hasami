//! Compare format variants with the same analyzer API (standalone rustc program).
//! Usage: eval bench|dump|load|load-once|verify DICT [CORPUS] [PASSES]
//! Compile against each release libhasami.rlib; dump is length-prefixed binary.
//! Windows: HASAMI_BENCH_AFFINITY sets a decimal logical-CPU mask. CPU uses
//! GetProcessTimes (coarse for short intervals); peak working set is on stderr.
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
#[cfg(windows)]
mod windows {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    impl FileTime {
        fn seconds(&self) -> f64 {
            ((u64::from(self.high) << 32) | u64::from(self.low)) as f64 / 10_000_000.0
        }
    }
    #[repr(C)]
    #[derive(Default)]
    struct MemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut MemoryCounters,
            size: u32,
        ) -> i32;
        fn SetProcessAffinityMask(process: *mut c_void, mask: usize) -> i32;
    }
    pub fn configure() {
        if let Ok(mask) = std::env::var("HASAMI_BENCH_AFFINITY") {
            let mask: usize = mask.parse().expect("decimal CPU affinity mask");
            assert_ne!(
                unsafe { SetProcessAffinityMask(GetCurrentProcess(), mask) },
                0
            );
        }
    }
    pub fn cpu() -> f64 {
        let (mut creation, mut exit, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        assert_ne!(
            unsafe {
                GetProcessTimes(
                    GetCurrentProcess(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
            },
            0
        );
        kernel.seconds() + user.seconds()
    }
    pub fn report_memory() {
        let size = std::mem::size_of::<MemoryCounters>() as u32;
        let mut counters = MemoryCounters {
            cb: size,
            ..Default::default()
        };
        assert_ne!(
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) },
            0
        );
        eprintln!("peak_working_set_bytes={}", counters.peak_working_set_size);
    }
}
fn cpu() -> f64 {
    #[cfg(target_os = "macos")]
    {
        unsafe { clock() as f64 / 1_000_000.0 }
    }
    #[cfg(windows)]
    {
        windows::cpu()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
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
    #[cfg(windows)]
    windows::configure();
    #[cfg(target_os = "macos")]
    {
        assert_eq!(unsafe { pthread_set_qos_class_self_np(0x21, 0) }, 0);
    }
    let args: Vec<String> = std::env::args().collect();
    let mode = &args[1];
    let dict = &args[2];
    if mode == "verify" {
        let a = Analyzer::load(dict).unwrap();
        println!("{:?}", a.dictionary().verify().unwrap());
        return;
    }
    if mode == "load-once" {
        let time = Instant::now();
        let c = cpu();
        let mut a = Analyzer::load(dict).unwrap();
        let count = black_box(
            a.try_tokenize("東京都に住んでいる人々が増えている。")
                .unwrap(),
        )
        .len();
        println!(
            "load 0 wall_us={:.3} cpu_us={:.3} tokens={count}",
            time.elapsed().as_secs_f64() * 1e6,
            (cpu() - c) * 1e6
        );
        #[cfg(windows)]
        windows::report_memory();
        return;
    }
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
        #[cfg(windows)]
        windows::report_memory();
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
        #[cfg(windows)]
        windows::report_memory();
    }
}
