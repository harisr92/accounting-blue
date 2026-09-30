use crate::invoice::hsn_lookup::*;

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
