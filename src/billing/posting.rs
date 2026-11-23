//! Ledger posting for invoices and credit notes
//!
//! An issued invoice is one journal entry: the buyer owes the total, the taxable value is sales
//! revenue, and each tax is owed to the authority that charges it. [`posting_legs`] turns a
//! [`TaxBreakdown`] into those legs and [`breakdown_entries`] maps them onto the accounts named in
//! [`PostingAccounts`]. The legs always balance, because a breakdown's total is its taxable value
//! plus its tax.
//!
//! A credit note posts the same legs on the opposite sides, reversing the part of the invoice it
//! credits.

use crate::billing::tax::TaxBreakdown;
use crate::types::{Entry, EntryType};
use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// One leg of an invoice's journal entry
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostingLeg {
    /// Debit: the total the buyer owes
    Receivable,
    /// Credit: the taxable value earned as revenue
    Sales,
    /// Credit: the tax charged under this code, owed to the tax authority
    Tax(String),
}

impl PostingLeg {
    /// The leg for the tax charged under `code`
    pub fn tax(code: impl Into<String>) -> Self {
        Self::Tax(code.into())
    }

    /// Side of an invoice's journal entry this leg posts to
    #[must_use]
    pub fn side(&self) -> EntryType {
        match self {
            Self::Receivable => EntryType::Debit,
            Self::Sales | Self::Tax(_) => EntryType::Credit,
        }
    }
}

impl fmt::Display for PostingLeg {
    /// `Receivable`, `Sales`, or the tax code followed by `output`, as in `VAT output`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Receivable => f.write_str("Receivable"),
            Self::Sales => f.write_str("Sales"),
            Self::Tax(code) => write!(f, "{code} output"),
        }
    }
}

/// Legs of the journal entry for `breakdown`, receivable first, leaving out those with a zero
/// amount
///
/// A zero leg would fail [`Transaction::validate`](crate::types::Transaction::validate).
#[must_use]
pub fn posting_legs(breakdown: &TaxBreakdown) -> Vec<(PostingLeg, BigDecimal)> {
    [
        (PostingLeg::Receivable, &breakdown.total),
        (PostingLeg::Sales, &breakdown.taxable_value),
    ]
    .into_iter()
    .chain(
        breakdown
            .components
            .iter()
            .map(|component| (PostingLeg::tax(component.code.clone()), &component.amount)),
    )
    .filter(|(_, amount)| !amount.is_zero())
    .map(|(leg, amount)| (leg, amount.clone()))
    .collect()
}

/// Accounts a document posts to: one for the receivable, one for sales, and one per tax code
///
/// # Example
///
/// ```
/// use accounting_core::billing::{PostingAccounts, PostingLeg};
///
/// let accounts = PostingAccounts::new("receivable", "sales").with_tax("VAT", "vat-payable");
/// assert_eq!(accounts.account_for(&PostingLeg::tax("VAT")), Some("vat-payable"));
/// assert_eq!(accounts.account_for(&PostingLeg::tax("GST")), None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingAccounts {
    /// Debited with the total (accounts receivable or cash)
    pub receivable: String,
    /// Credited with the taxable value; pass a sales returns account to keep credit notes apart
    pub sales: String,
    /// Credited with each tax, by tax code
    pub tax: BTreeMap<String, String>,
}

impl PostingAccounts {
    /// Accounts for the receivable and sales legs, with no tax accounts yet
    pub fn new(receivable: impl Into<String>, sales: impl Into<String>) -> Self {
        Self {
            receivable: receivable.into(),
            sales: sales.into(),
            tax: BTreeMap::new(),
        }
    }

    /// These accounts with tax charged under `code` credited to `account`
    #[must_use]
    pub fn with_tax(self, code: impl Into<String>, account: impl Into<String>) -> Self {
        let mut tax = self.tax;
        tax.insert(code.into(), account.into());
        Self { tax, ..self }
    }

    /// Account id `leg` posts to; `None` for a tax with no account
    #[must_use]
    pub fn account_for(&self, leg: &PostingLeg) -> Option<&str> {
        match leg {
            PostingLeg::Receivable => Some(&self.receivable),
            PostingLeg::Sales => Some(&self.sales),
            PostingLeg::Tax(code) => self.tax.get(code).map(String::as_str),
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

/// The entries for `breakdown`'s legs, each on the side `side` gives it
///
/// Pass [`PostingLeg::side`] for an invoice, or the opposite side for a credit note.
///
/// # Errors
///
/// [`PostingError::NothingToPost`] if every leg is zero, or [`PostingError::NoTaxAccount`] for a
/// tax `accounts` names no account for.
pub fn breakdown_entries(
    breakdown: &TaxBreakdown,
    accounts: &PostingAccounts,
    side: impl Fn(&PostingLeg) -> EntryType,
) -> Result<Vec<Entry>, PostingError> {
    let legs = posting_legs(breakdown);
    if legs.is_empty() {
        return Err(PostingError::NothingToPost);
    }
    legs.into_iter()
        .map(|(leg, amount)| {
            let account = accounts
                .account_for(&leg)
                .ok_or_else(|| PostingError::NoTaxAccount {
                    code: match &leg {
                        PostingLeg::Tax(code) => code.clone(),
                        PostingLeg::Receivable | PostingLeg::Sales => String::new(),
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
