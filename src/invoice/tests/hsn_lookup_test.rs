use crate::invoice::hsn_lookup::*;

#[test]
fn test_embedded_master_parses() {
    let master = HsnMaster::global();
    assert!(!master.entries().is_empty());
    assert!(master.entries().iter().all(|e| is_valid_hsn_sac(&e.code)));
}
