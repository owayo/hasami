use super::container::{self, FLAG_SYMMETRIC_ENTRIES, SectionId};
use super::records::EntrySlice;
use super::{DictError, Dictionary};
use crate::dict::{ConnectionMatrix, DictBuilder, DictEntry, UnkEntry};
use crate::lattice::LatticeWorkspace;
use std::sync::atomic::{AtomicUsize, Ordering};

fn word(left: u16, right: u16, cost: i16, reading: &str) -> DictEntry {
    DictEntry {
        surface: "対称検査".into(),
        base_form: "対称検査".into(),
        left_id: left,
        right_id: right,
        cost,
        pos: "名詞,一般,*,*".into(),
        reading: reading.into(),
        pronunciation: reading.into(),
        ..Default::default()
    }
}

fn fixture(left: u16, right: u16, nl: u32, nr: u32) -> DictBuilder {
    let mut builder = DictBuilder::new();
    builder.set_matrix(ConnectionMatrix::zeros(nl, nr));
    builder.add_entry(word(left, right, i16::MIN, "サキ"));
    builder
}

fn bytes(builder: &DictBuilder) -> Vec<u8> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "hasami-symmetric-{}-{}.hsd",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    builder
        .write_hsd(&path, &builder.write_options(), |_, _| {})
        .unwrap();
    let data = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    data
}

#[test]
fn symmetric_ids_allow_rectangular_matrix_and_keep_asymmetric_unknowns() {
    let mut builder = fixture(1, 1, 2, 3);
    builder.set_unk_entries(std::collections::HashMap::from([(
        "ALPHA".into(),
        vec![UnkEntry {
            char_class: "ALPHA".into(),
            left_id: 0,
            right_id: 2,
            cost: 1234,
            pos: "名詞,一般,*,*".into(),
        }],
    )]));
    let dict = builder.build().unwrap();
    assert!(matches!(dict.view().entries, EntrySlice::Symmetric(_)));
    assert_eq!(dict.matrix_dims(), (2, 3));
    assert_eq!(
        dict.section_sizes()
            .into_iter()
            .find(|(n, _)| *n == "ENTRIES")
            .unwrap()
            .1,
        4
    );
    let entries = dict.lookup("対称検査").unwrap();
    let entry = &entries[0].1[0];
    assert_eq!(
        (entry.left_id, entry.right_id, entry.cost),
        (1, 1, i16::MIN)
    );
    dict.verify().unwrap();
    let mut analyzer = crate::Analyzer::from_dict(dict);
    let tokens = analyzer.try_tokenize("XYZ").unwrap();
    assert!(tokens.iter().all(|t| !t.is_known));
    assert_eq!(tokens[0].word_cost, 1234);
}

#[test]
fn full_entries_keep_high_right_id_and_do_not_mask_it_to_fifteen_bits() {
    let dict = fixture(0, 32768, 1, 32769).build().unwrap();
    assert!(matches!(dict.view().entries, EntrySlice::Full(_)));
    assert_eq!(
        dict.section_sizes()
            .into_iter()
            .find(|(n, _)| *n == "ENTRIES")
            .unwrap()
            .1,
        6
    );
    assert_eq!(dict.lookup("対称検査").unwrap()[0].1[0].right_id, 32768);
    dict.verify().unwrap();
    let mut analyzer = crate::Analyzer::from_dict(dict);
    assert_eq!(
        &*analyzer.try_tokenize("対称検査").unwrap()[0].reading,
        "サキ"
    );
}

#[test]
fn both_layouts_reject_mode_mismatch_and_damaged_record_lengths() {
    for builder in [fixture(1, 1, 2, 3), fixture(0, 1, 2, 3)] {
        let original = bytes(&builder);
        let mut bad_mode = original.clone();
        bad_mode[12] ^= FLAG_SYMMETRIC_ENTRIES as u8;
        assert!(matches!(
            Dictionary::from_bytes(&bad_mode),
            Err(DictError::Corrupt(_))
        ));
        let mut bad_len = original.clone();
        let table_at = (0..container::SectionId::ALL.len())
            .map(|i| 64 + i * 24)
            .find(|&at| {
                u32::from_le_bytes(original[at..at + 4].try_into().unwrap())
                    == SectionId::Entries as u32
            })
            .unwrap();
        let length = u64::from_le_bytes(original[table_at + 16..table_at + 24].try_into().unwrap());
        bad_len[table_at + 16..table_at + 24].copy_from_slice(&(length - 1).to_le_bytes());
        assert!(matches!(
            Dictionary::from_bytes(&bad_len),
            Err(DictError::Corrupt(_))
        ));
    }
}

#[test]
fn compact_context_is_checked_against_both_matrix_dimensions() {
    let mut bad = bytes(&fixture(1, 1, 3, 2));
    let range = container::parse(&bad).unwrap().get(SectionId::Entries);
    bad[range.offset..range.offset + 2].copy_from_slice(&(2u16 | 0x8000).to_le_bytes());
    let dict = Dictionary::from_bytes(&bad).unwrap();
    assert!(matches!(dict.verify(), Err(DictError::Corrupt(_))));
    assert!(matches!(
        crate::Analyzer::from_dict(dict).try_tokenize("対称検査"),
        Err(DictError::Corrupt(_))
    ));
}

#[test]
fn equal_cost_order_and_token_cache_survive_layout_and_dictionary_switches() {
    let mut compact = fixture(1, 1, 2, 3);
    compact.add_entry(word(1, 1, i16::MIN, "アト"));
    let compact = compact.build().unwrap();
    let mut full = fixture(0, 1, 2, 3);
    full.add_entry(word(0, 1, i16::MIN + 1, "アト"));
    let full = full.build().unwrap();
    let mut workspace = LatticeWorkspace::new();
    let mut out = Vec::new();
    for dict in [&compact, &full, &compact] {
        out.clear();
        workspace
            .tokenize_into("対称検査", dict, 17, &mut out)
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            (&*out[0].reading, out[0].start, out[0].end),
            ("サキ", 17, 29)
        );
    }
}
