//! Indian formatting for amounts of rupees
//!
//! [`format_inr`] groups digits the Indian way (`12,34,567.89`) and [`amount_in_words`] spells an
//! amount out in crore, lakh and thousand, as printed on a tax invoice. Both round to the paisa
//! with [`round_to_paise`] first, so they agree with the invoice totals.

use crate::tax::round_to_paise;
use bigdecimal::{BigDecimal, Signed};

/// Digits below a crore: one crore is 1,00,00,000
const DIGITS_BELOW_CRORE: usize = 7;
/// One lakh
const LAKH: u32 = 100_000;
/// One thousand
const THOUSAND: u32 = 1_000;
/// One hundred
const HUNDRED: u32 = 100;
/// Digits in the lowest group, before the Indian pairs start
const LOWEST_GROUP: usize = 3;
/// Digits in each group above the lowest
const HIGHER_GROUP: usize = 2;

const ONES: [&str; 20] = [
    "Zero",
    "One",
    "Two",
    "Three",
    "Four",
    "Five",
    "Six",
    "Seven",
    "Eight",
    "Nine",
    "Ten",
    "Eleven",
    "Twelve",
    "Thirteen",
    "Fourteen",
    "Fifteen",
    "Sixteen",
    "Seventeen",
    "Eighteen",
    "Nineteen",
];

const TENS: [&str; 10] = [
    "", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
];

/// Format an amount of rupees with Indian digit grouping and two decimal places
///
/// ```
/// use accounting_core::utils::formatting::format_inr;
/// use bigdecimal::BigDecimal;
///
/// let amount: BigDecimal = "1234567.891".parse().unwrap();
/// assert_eq!(format_inr(&amount), "12,34,567.89");
/// assert_eq!(format_inr(&BigDecimal::from(-1000)), "-1,000.00");
/// ```
#[must_use]
pub fn format_inr(amount: &BigDecimal) -> String {
    let (rupees, paise) = split_rupees(amount);
    let sign = if amount.is_negative() && (rupees != "0" || paise != "00") {
        "-"
    } else {
        ""
    };
    format!("{sign}{}.{paise}", group_indian(&rupees))
}

/// Spell out an amount of rupees in words, the way it is printed on an Indian invoice
///
/// ```
/// use accounting_core::utils::formatting::amount_in_words;
/// use bigdecimal::BigDecimal;
///
/// assert_eq!(
///     amount_in_words(&BigDecimal::from(17700)),
///     "Rupees Seventeen Thousand Seven Hundred Only"
/// );
/// let amount: BigDecimal = "1250000.5".parse().unwrap();
/// assert_eq!(
///     amount_in_words(&amount),
///     "Rupees Twelve Lakh Fifty Thousand and Fifty Paise Only"
/// );
/// ```
#[must_use]
pub fn amount_in_words(amount: &BigDecimal) -> String {
    let (rupees, paise) = split_rupees(amount);
    let paise = digits_value(&paise);
    let sign = if amount.is_negative() && (rupees != "0" || paise != 0) {
        "Minus "
    } else {
        ""
    };
    let words = match (rupees.as_str(), paise) {
        ("0", 0) => "Rupees Zero".to_string(),
        ("0", paise) => format!("{} Paise", below_hundred(paise)),
        (rupees, 0) => format!("Rupees {}", integer_words(rupees)),
        (rupees, paise) => format!(
            "Rupees {} and {} Paise",
            integer_words(rupees),
            below_hundred(paise)
        ),
    };
    format!("{sign}{words} Only")
}

/// The whole rupees and the two paise digits of `amount`, rounded to the paisa, without a sign
fn split_rupees(amount: &BigDecimal) -> (String, String) {
    let plain = round_to_paise(&amount.abs()).to_plain_string();
    match plain.split_once('.') {
        Some((rupees, paise)) => (rupees.to_string(), paise.to_string()),
        None => (plain, "00".to_string()),
    }
}

/// Insert commas into a string of digits: the lowest three together, then pairs
fn group_indian(digits: &str) -> String {
    if digits.len() <= LOWEST_GROUP {
        return digits.to_string();
    }
    let (upper, lowest) = digits.split_at(digits.len() - LOWEST_GROUP);
    let (lead, pairs) = upper.split_at(upper.len() % HIGHER_GROUP);
    let groups = std::iter::once(lead)
        .filter(|group| !group.is_empty())
        .chain(
            pairs
                .as_bytes()
                .chunks(HIGHER_GROUP)
                .filter_map(|pair| std::str::from_utf8(pair).ok()),
        )
        .chain(std::iter::once(lowest));
    groups.collect::<Vec<_>>().join(",")
}

/// Numeric value of a short string of ASCII digits
fn digits_value(digits: &str) -> u32 {
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .fold(0, |value, digit| value * 10 + digit)
}

/// Words for a whole number of any size given as digits, counting in crore above ninety-nine lakh
fn integer_words(digits: &str) -> String {
    if digits.len() <= DIGITS_BELOW_CRORE {
        return below_crore(digits_value(digits));
    }
    let (crores, rest) = digits.split_at(digits.len() - DIGITS_BELOW_CRORE);
    let rest = digits_value(rest);
    let crores = format!("{} Crore", integer_words(crores));
    if rest == 0 {
        crores
    } else {
        format!("{crores} {}", below_crore(rest))
    }
}

/// Words for a number below one crore, in lakh, thousand and hundred
fn below_crore(n: u32) -> String {
    if n == 0 {
        return ONES[0].to_string();
    }
    let parts = [
        (n / LAKH, "Lakh"),
        (n / THOUSAND % HUNDRED, "Thousand"),
        (n / HUNDRED % 10, "Hundred"),
    ];
    parts
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, unit)| format!("{} {unit}", below_hundred(*count)))
        .chain((n % HUNDRED > 0).then(|| below_hundred(n % HUNDRED)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words for a number below one hundred
fn below_hundred(n: u32) -> String {
    let n = n % HUNDRED;
    let ones = |i: u32| ONES.get(i as usize).copied().unwrap_or_default();
    let tens = TENS.get((n / 10) as usize).copied().unwrap_or_default();
    match n {
        0..=19 => ones(n).to_string(),
        _ if n % 10 == 0 => tens.to_string(),
        _ => format!("{tens} {}", ones(n % 10)),
    }
}
