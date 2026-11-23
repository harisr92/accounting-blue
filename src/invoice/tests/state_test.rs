use crate::invoice::state::*;
use crate::invoice::types::StateCode;
use std::collections::HashSet;

#[test]
fn test_state_codes_are_unique_two_digit_codes() {
    let codes: HashSet<&str> = GST_STATES.iter().map(|(code, _)| *code).collect();
    assert_eq!(codes.len(), GST_STATES.len());
    assert!(GST_STATES.iter().all(|(code, name)| code.len() == 2
        && code.bytes().all(|b| b.is_ascii_digit())
        && !name.is_empty()));
}

#[test]
fn test_karnataka_is_code_29() {
    assert_eq!(state_name("29"), Some("Karnataka"));
    assert_eq!(state_label("29"), "29 - Karnataka");
}

#[test]
fn test_unknown_code_labels_as_itself() {
    assert_eq!(state_name("00"), None);
    assert_eq!(state_label("00"), "00");
    assert_eq!(state_label("x"), "x");
}

#[test]
fn test_every_place_of_supply_has_a_name() {
    let accepted: Vec<String> = (0..100)
        .map(|n| format!("{n:02}"))
        .filter(|code| StateCode::parse(code).is_ok())
        .collect();
    assert_eq!(accepted.len(), 39);
    for code in accepted {
        assert!(state_name(&code).is_some(), "no name for {code}");
    }
}

#[test]
fn test_centre_jurisdiction_has_a_name() {
    assert_eq!(state_name("99"), Some("Centre Jurisdiction"));
}
