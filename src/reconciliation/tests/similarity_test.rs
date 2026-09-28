use crate::reconciliation::similarity::*;

#[test]
fn test_normalize_collapses_punctuation() {
    assert_eq!(
        normalize("NEFT/ACME LTD/HDFC0000123"),
        "neft acme ltd hdfc0000123"
    );
    assert_eq!(normalize("  ...Acme,  Ltd.  "), "acme ltd");
    assert_eq!(normalize("///"), "");
}

#[test]
fn test_identical_descriptions() {
    assert_eq!(similarity("Acme Ltd", "ACME  LTD."), 1.0);
    assert_eq!(similarity("", ""), 1.0);
}

#[test]
fn test_empty_against_non_empty() {
    assert_eq!(similarity("", "Acme Ltd"), 0.0);
    assert_eq!(similarity("Acme Ltd", ""), 0.0);
}

#[test]
fn test_containment_scores_well() {
    assert!(similarity("NEFT/ACME LTD/0012", "Acme Ltd") >= CONTAINMENT_SCORE);
}

#[test]
fn test_short_containment_does_not_win_credit() {
    // "ab" is contained in "abcdefghij" but is too short to be evidence of anything
    assert!(similarity("abcdefghij", "ab") < CONTAINMENT_SCORE);
}

#[test]
fn test_disjoint_descriptions_score_low() {
    assert!(similarity("Acme Ltd", "Globex Inc") < 0.4);
}

#[test]
fn test_typo_scores_high() {
    assert!(similarity("Payment from customer", "Payment frm customer") > 0.9);
}

#[test]
fn test_non_ascii_does_not_panic() {
    assert_eq!(similarity("देवनागरी", "देवनागरी"), 1.0);
    assert!(similarity("café münchen", "cafe munchen") > 0.5);
    assert_eq!(levenshtein("café", "cafe"), 1);
}

#[test]
fn test_levenshtein_basics() {
    assert_eq!(levenshtein("", ""), 0);
    assert_eq!(levenshtein("abc", ""), 3);
    assert_eq!(levenshtein("", "abc"), 3);
    assert_eq!(levenshtein("kitten", "sitting"), 3);
}
