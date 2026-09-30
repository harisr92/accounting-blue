use crate::invoice::pdf::text::*;

/// Every character is one millimetre wide
fn measure(text: &str) -> f32 {
    text.chars().count() as f32
}

#[test]
fn test_text_that_fits_is_kept() {
    assert_eq!(fit("Consulting", 10.0, measure), "Consulting");
}

#[test]
fn test_text_too_wide_is_cut_with_an_ellipsis() {
    assert_eq!(fit("IT consulting services", 10.0, measure), "IT cons...");
    assert_eq!(fit("IT consulting", 6.0, measure), "IT...");
    assert_eq!(fit("Consulting", 2.0, measure), "");
}

#[test]
fn test_text_wraps_at_spaces() {
    let lines = wrap("Rupees One Lakh Only", 11.0, measure);
    assert_eq!(lines, ["Rupees One", "Lakh Only"]);
}

#[test]
fn test_a_word_wider_than_the_line_is_cut() {
    let lines = wrap("Pay Supercalifragilistic now", 10.0, measure);
    assert_eq!(lines, ["Pay", "Superca...", "now"]);
}

#[test]
fn test_empty_text_wraps_to_no_lines() {
    assert!(wrap("   ", 10.0, measure).is_empty());
}
