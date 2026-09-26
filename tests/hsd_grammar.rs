//! Grammar sharing must preserve public roundtrips and reject bad references.
#![cfg(feature = "build")]

use hasami::dict::DictBuilder;
use hasami::{DictEntry, Dictionary};
use std::sync::atomic::{AtomicUsize, Ordering};

fn fixture() -> Vec<u8> {
    static ID: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "hasami-grammar-{}-{}.hsd",
        std::process::id(),
        ID.fetch_add(1, Ordering::Relaxed)
    ));
    let mut b = DictBuilder::new();
    for (surface, pos, kind, form, base, reading) in [
        (
            "東京",
            "名詞,固有名詞,地域,一般",
            "*",
            "*",
            "東京",
            "トウキョウ",
        ),
        (
            "走り",
            "動詞,自立,*,*",
            "五段・ラ行",
            "連用形",
            "走る",
            "ハシリ",
        ),
    ] {
        b.add_entry(DictEntry {
            surface: surface.into(),
            pos: pos.into(),
            conj_type: kind.into(),
            conj_form: form.into(),
            base_form: base.into(),
            reading: reading.into(),
            pronunciation: reading.into(),
            cost: -5000,
            ..DictEntry::default()
        });
    }
    b.write_hsd(&path, &b.write_options(), |_, _| {}).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    bytes
}

fn section(bytes: &[u8], id: u32) -> (usize, usize, usize) {
    let n = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    for i in 0..n as usize {
        let p = 64 + i * 24;
        if u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) == id {
            return (
                p,
                u64::from_le_bytes(bytes[p + 8..p + 16].try_into().unwrap()) as usize,
                u64::from_le_bytes(bytes[p + 16..p + 24].try_into().unwrap()) as usize,
            );
        }
    }
    panic!("missing section {id}")
}

#[test]
fn grammar_table_is_required_and_validated_on_load() {
    let good = fixture();
    Dictionary::from_bytes(&good).unwrap().verify().unwrap();
    let (entry, offset, len) = section(&good, 18);
    for length in [0, len - 1] {
        let mut bad = good.clone();
        bad[entry + 16..entry + 24].copy_from_slice(&(length as u64).to_le_bytes());
        assert!(Dictionary::from_bytes(&bad).is_err());
    }
    for field in 0..3 {
        let mut bad = good.clone();
        bad[offset + field * 2..offset + field * 2 + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(Dictionary::from_bytes(&bad).is_err());
    }
    let mut missing = good.clone();
    missing[entry..entry + 4].copy_from_slice(&999u32.to_le_bytes());
    assert!(Dictionary::from_bytes(&missing).is_err());
}

#[test]
fn invalid_grammar_reference_is_a_recoverable_error() {
    let good = fixture();
    let (_, features, _) = section(&good, 8);
    let (_, offsets, _) = section(&good, 7);
    let offset = u32::from_le_bytes(good[offsets..offsets + 4].try_into().unwrap()) as usize;
    for malformed in [&[0x7f][..], &[0xff, 0xff, 0xff, 0xff, 0x10][..]] {
        let mut bad = good.clone();
        bad[features + offset..features + offset + malformed.len()].copy_from_slice(malformed);
        let dict = Dictionary::from_bytes(&bad).unwrap();
        assert!(dict.verify().is_err());
        assert!(dict.for_each_entry(|_| Ok(())).is_err());
    }
}

#[test]
fn old_v4_is_rejected_with_rebuild_instructions() {
    let mut bytes = fixture();
    bytes[8..12].copy_from_slice(&4u32.to_le_bytes());
    let error = Dictionary::from_bytes(&bytes).unwrap_err().to_string();
    assert!(error.contains("v4"));
    assert!(error.contains("build-dict.sh"));
}
