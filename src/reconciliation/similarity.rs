//! Text normalisation and similarity scoring for transaction descriptions
//!
//! Bank narrations wrap the useful part in channel codes, slashes and reference fragments
//! (`"NEFT/ACME LTD/HDFC0000123"` for what the ledger calls `"Acme Ltd"`). Similarity therefore
//! combines three views and takes the most generous: token overlap, edit distance, and containment.

use std::collections::HashSet;

/// Longest input considered by the edit-distance comparison
const MAX_COMPARE_LEN: usize = 256;

/// Shortest string that may win credit purely by being contained in the other
const MIN_CONTAINMENT_LEN: usize = 4;

/// Credit awarded when one description contains the other
const CONTAINMENT_SCORE: f64 = 0.8;

/// Lowercase `text`, replace every run of non-alphanumeric characters with a single space, and
/// trim.
///
/// ```
/// use accounting_core::reconciliation::similarity::normalize;
/// assert_eq!(normalize("NEFT/ACME LTD/HDFC0000123"), "neft acme ltd hdfc0000123");
/// ```
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_space = true;
        }
    }

    out
}

/// Score how alike two descriptions are, in `0.0..=1.0`.
///
/// Two empty descriptions count as identical; one empty description scores zero.
///
/// ```
/// use accounting_core::reconciliation::similarity::similarity;
/// assert_eq!(similarity("Acme Ltd", "ACME  LTD."), 1.0);
/// assert!(similarity("NEFT/ACME LTD/0012", "Acme Ltd") >= 0.8);
/// assert!(similarity("Acme Ltd", "Globex Inc") < 0.4);
/// ```
pub fn similarity(left: &str, right: &str) -> f64 {
    let left = normalize(left);
    let right = normalize(right);

    match (left.is_empty(), right.is_empty()) {
        (true, true) => return 1.0,
        (true, false) | (false, true) => return 0.0,
        (false, false) => {}
    }

    if left == right {
        return 1.0;
    }

    let mut score = token_similarity(&left, &right).max(edit_similarity(&left, &right));

    let shorter_len = left.chars().count().min(right.chars().count());
    if shorter_len >= MIN_CONTAINMENT_LEN && (left.contains(&right) || right.contains(&left)) {
        score = score.max(CONTAINMENT_SCORE);
    }

    score
}

/// Jaccard index over whitespace-separated tokens
fn token_similarity(left: &str, right: &str) -> f64 {
    let left_tokens: HashSet<&str> = left.split(' ').collect();
    let right_tokens: HashSet<&str> = right.split(' ').collect();

    let union = left_tokens.union(&right_tokens).count();
    if union == 0 {
        return 0.0;
    }

    left_tokens.intersection(&right_tokens).count() as f64 / union as f64
}

/// Edit distance rescaled against the longer input
fn edit_similarity(left: &str, right: &str) -> f64 {
    let left_chars: Vec<char> = left.chars().take(MAX_COMPARE_LEN).collect();
    let right_chars: Vec<char> = right.chars().take(MAX_COMPARE_LEN).collect();

    let longest = left_chars.len().max(right_chars.len());
    if longest == 0 {
        return 1.0;
    }

    1.0 - (levenshtein_chars(&left_chars, &right_chars) as f64 / longest as f64)
}

/// Levenshtein edit distance between two strings, in characters
pub fn levenshtein(left: &str, right: &str) -> usize {
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    levenshtein_chars(&left_chars, &right_chars)
}

/// Two-row dynamic programming over characters, so multi-byte text is never split
fn levenshtein_chars(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];

    for (i, left_ch) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_ch) in right.iter().enumerate() {
            let substitution = previous[j] + usize::from(left_ch != right_ch);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
