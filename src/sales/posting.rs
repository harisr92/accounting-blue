//! Posting sales documents: an invoice debits the receivable and credits revenue and output tax
//!
//! [`sales_legs`] turns a [`TaxBreakdown`] into the legs of an invoice's journal entry and
//! [`sales_entries`] maps them onto the accounts named in [`SalesAccounts`]. A credit note posts
//! the same legs on the opposite sides.

use crate::document::posting::{legs_to_entries, PostingError, PostingLeg};
use crate::document::tax::TaxBreakdown;
use crate::types::{Entry, EntryType};
use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Accounts a sales document posts to: the receivable, revenue, and output tax by tax code
///
/// # Example
///
/// ```
/// use accounting_core::document::PostingLeg;
/// use accounting_core::sales::SalesAccounts;
///
/// let accounts = SalesAccounts::new("receivable", "sales").with_output_tax("VAT", "vat-payable");
/// assert_eq!(accounts.account_for(&PostingLeg::output_tax("VAT")), Some("vat-payable"));
/// assert_eq!(accounts.account_for(&PostingLeg::output_tax("GST")), None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesAccounts {
    /// Debited with the total (accounts receivable or cash)
    pub receivable: String,
    /// Credited with the taxable value; pass a sales returns account to keep credit notes apart
    pub revenue: String,
    /// Credited with each output tax, by tax code
    pub output_tax: BTreeMap<String, String>,
}

impl SalesAccounts {
    /// Accounts for the receivable and revenue legs, with no output tax accounts yet
    pub fn new(receivable: impl Into<String>, revenue: impl Into<String>) -> Self {
        Self {
            receivable: receivable.into(),
            revenue: revenue.into(),
            output_tax: BTreeMap::new(),
        }
    }

    /// These accounts with output tax charged under `code` credited to `account`
    #[must_use]
    pub fn with_output_tax(self, code: impl Into<String>, account: impl Into<String>) -> Self {
        let mut output_tax = self.output_tax;
        output_tax.insert(code.into(), account.into());
        Self { output_tax, ..self }
    }

    /// Account id `leg` posts to; `None` for a tax with no account
    #[must_use]
    pub fn account_for(&self, leg: &PostingLeg) -> Option<&str> {
        match leg {
            PostingLeg::Receivable => Some(&self.receivable),
            PostingLeg::Revenue => Some(&self.revenue),
            PostingLeg::OutputTax(code) => self.output_tax.get(code).map(String::as_str),
        }
    }
}

/// Legs of an invoice's journal entry for `breakdown`, receivable first, leaving out those with a
/// zero amount
///
/// A zero leg would fail [`Transaction::validate`](crate::types::Transaction::validate).
#[must_use]
pub fn sales_legs(breakdown: &TaxBreakdown) -> Vec<(PostingLeg, BigDecimal)> {
    [
        (PostingLeg::Receivable, &breakdown.total),
        (PostingLeg::Revenue, &breakdown.taxable_value),
    ]
    .into_iter()
    .chain(breakdown.components.iter().map(|component| {
        (
            PostingLeg::output_tax(component.code.clone()),
            &component.amount,
        )
    }))
    .filter(|(_, amount)| !amount.is_zero())
    .map(|(leg, amount)| (leg, amount.clone()))
    .collect()
}

/// The entries for `breakdown`'s sales legs, each on the side `side` gives it
///
/// Pass [`PostingLeg::side`] for an invoice, or the opposite side for a credit note.
///
/// # Errors
///
/// [`PostingError::NothingToPost`] if every leg is zero, or [`PostingError::NoTaxAccount`] for a
/// tax `accounts` names no account for.
pub fn sales_entries(
    breakdown: &TaxBreakdown,
    accounts: &SalesAccounts,
    side: impl Fn(&PostingLeg) -> EntryType,
) -> Result<Vec<Entry>, PostingError> {
    legs_to_entries(sales_legs(breakdown), |leg| accounts.account_for(leg), side)
}
