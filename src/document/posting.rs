//! Ledger posting: the legs of a document's journal entry, and turning them into entries
//!
//! A document posts as one journal entry whose legs balance by construction: a breakdown's total
//! is its taxable value plus its tax. Each side of trade names its legs and accounts: see
//! [`sales::posting`](crate::sales::posting). [`legs_to_entries`] maps any legs onto accounts.

use crate::types::{Entry, EntryType};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// One leg of a document's journal entry
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostingLeg {
    /// Debit: the total the customer owes
    Receivable,
    /// Credit: the taxable value earned as revenue
    Revenue,
    /// Credit: the output tax charged under this code, owed to the tax authority
    OutputTax(String),
}

impl PostingLeg {
    /// The leg for output tax charged under `code`
    pub fn output_tax(code: impl Into<String>) -> Self {
        Self::OutputTax(code.into())
    }

    /// Side of an invoice's journal entry this leg posts to; a credit note posts the opposite
    #[must_use]
    pub fn side(&self) -> EntryType {
        match self {
            Self::Receivable => EntryType::Debit,
            Self::Revenue | Self::OutputTax(_) => EntryType::Credit,
        }
    }
}

impl fmt::Display for PostingLeg {
    /// The entry memo: `Receivable`, `Sales`, or the tax code followed by `output`, as in
    /// `VAT output`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Receivable => f.write_str("Receivable"),
            Self::Revenue => f.write_str("Sales"),
            Self::OutputTax(code) => write!(f, "{code} output"),
        }
    }
}

/// Why a breakdown could not be posted
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PostingError {
    /// The total is zero, so there is nothing to post
    #[error("total is zero, so there is nothing to post")]
    NothingToPost,
    /// The breakdown charges a tax that no account is named for
    #[error("no account is named for tax {code}")]
    NoTaxAccount {
        /// The tax code with no account
        code: String,
    },
}

/// One entry per leg, on the account `account_for` names and the side `side` gives it, with the
/// leg as its memo
///
/// # Errors
///
/// [`PostingError::NothingToPost`] if there are no legs, or [`PostingError::NoTaxAccount`] for a
/// tax leg `account_for` names no account for.
pub fn legs_to_entries<'a>(
    legs: Vec<(PostingLeg, BigDecimal)>,
    account_for: impl Fn(&PostingLeg) -> Option<&'a str>,
    side: impl Fn(&PostingLeg) -> EntryType,
) -> Result<Vec<Entry>, PostingError> {
    if legs.is_empty() {
        return Err(PostingError::NothingToPost);
    }
    legs.into_iter()
        .map(|(leg, amount)| {
            let account = account_for(&leg).ok_or_else(|| PostingError::NoTaxAccount {
                code: match &leg {
                    PostingLeg::OutputTax(code) => code.clone(),
                    PostingLeg::Receivable | PostingLeg::Revenue => String::new(),
                },
            })?;
            Ok(Entry::new(
                account,
                side(&leg),
                amount,
                Some(leg.to_string()),
            ))
        })
        .collect()
}
