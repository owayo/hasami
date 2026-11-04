//! 対応対象外として除去したフルネームの登録が復活しないことを確認する。
#![cfg(feature = "analyzer")]

use hasami::Dictionary;

#[test]
#[ignore = "配布辞書3種が必要"]
fn distributed_excluded_person_names_stay_unregistered() {
    let dir = std::env::var("HASAMI_TEST_DICT_DIR")
        .unwrap_or_else(|_| format!("{}/dict", env!("CARGO_MANIFEST_DIR")));
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"] {
        let dictionary = Dictionary::load(format!("{dir}/{name}.hsd")).unwrap();
        for surface in ["張豊猷", "張栩"] {
            assert!(
                dictionary
                    .lookup(surface)
                    .unwrap()
                    .iter()
                    .all(|(length, _)| *length < surface.len()),
                "{name}: {surface} must not have a full-name entry"
            );
        }
    }
}
