use crate::gst::invoice::hsn_lookup::*;

#[test]
fn test_embedded_master_parses() {
    let master = HsnMaster::global();
    assert!(!master.entries().is_empty());
    assert!(master.entries().iter().all(|e| is_valid_hsn_sac(&e.code)));
}

#[test]
fn test_codes_in_chapter_99_are_services() {
    assert_eq!(HsnSacKind::of_code("998314"), HsnSacKind::Sac);
    assert_eq!(HsnSacKind::of_code("9954"), HsnSacKind::Sac);
    assert_eq!(HsnSacKind::of_code("1905"), HsnSacKind::Hsn);
    assert_eq!(HsnSacKind::of_code("84713010"), HsnSacKind::Hsn);
}

#[test]
fn test_master_kinds_agree_with_the_code_chapter() {
    assert!(HsnMaster::global()
        .entries()
        .iter()
        .all(|e| HsnSacKind::of_code(&e.code) == e.kind));
}

#[test]
fn test_embedded_master_lists_every_code_only_with_the_catalog() {
    assert_eq!(
        HsnMaster::global().lists_every_code(),
        cfg!(feature = "hsn-catalog")
    );
}

#[test]
fn test_a_callers_own_master_lists_every_code() {
    let master: HsnMaster = serde_json::from_str(
        r#"{ "schedule": "Custom", "effective_from": "2025-01-01",
             "entries": [{ "code": "998314", "description": "IT consulting", "gst_rate": "5" }] }"#,
    )
    .unwrap();
    assert!(master.lists_every_code());
}
