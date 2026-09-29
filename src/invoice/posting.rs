//! Ledger posting for a GST invoice
//!
//! An issued invoice is one journal entry: the buyer owes the total, the taxable value is sales
//! revenue, and each tax component is owed to the government. [`posting_legs`] turns a
//! [`GstBreakdown`] into those legs and [`GstInvoice::to_entries`] maps them onto accounts. The
//! legs always balance, because the breakdown's total is its taxable value plus its tax.

use super::types::{GstBreakdown, GstInvoice, InvoiceError};
use super::validation::{compliance_errors, ComplianceIssue};
use crate::types::{Entry, EntryType};
use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;

/// One leg of an invoice's journal entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostingLeg {
    /// Debit: the total the buyer owes
    Receivable,
    /// Credit: the taxable value earned as revenue
    Sales,
    /// Credit: central GST owed on an intra-state supply
    CgstOutput,
    /// Credit: state GST owed on an intra-state supply
    SgstOutput,
    /// Credit: integrated GST owed on an inter-state supply
    IgstOutput,
}

impl PostingLeg {
    /// Every leg, debit first
    pub const ALL: [PostingLeg; 5] = [
        Self::Receivable,
        Self::Sales,
        Self::CgstOutput,
        Self::SgstOutput,
        Self::IgstOutput,
    ];

    /// Side of the journal entry this leg posts to
    #[must_use]
    pub fn side(self) -> EntryType {
        match self {
            Self::Receivable => EntryType::Debit,
            Self::Sales | Self::CgstOutput | Self::SgstOutput | Self::IgstOutput => {
                EntryType::Credit
            }
        }
    }

    /// The amount of `breakdown` this leg carries
    #[must_use]
    pub fn amount(self, breakdown: &GstBreakdown) -> &BigDecimal {
        match self {
            Self::Receivable => &breakdown.total,
            Self::Sales => &breakdown.taxable_value,
            Self::CgstOutput => &breakdown.cgst,
            Self::SgstOutput => &breakdown.sgst,
            Self::IgstOutput => &breakdown.igst,
        }
    }
}

impl fmt::Display for PostingLeg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Receivable => "Receivable",
            Self::Sales => "Sales",
            Self::CgstOutput => "CGST output",
            Self::SgstOutput => "SGST output",
            Self::IgstOutput => "IGST output",
        })
    }
}

/// Legs of the journal entry for `breakdown`, leaving out those with a zero amount
///
/// A zero leg would fail [`Transaction::validate`](crate::types::Transaction::validate), and an
/// invoice only ever carries CGST + SGST or IGST, never both.
#[must_use]
pub fn posting_legs(breakdown: &GstBreakdown) -> Vec<(PostingLeg, BigDecimal)> {
    PostingLeg::ALL
        .into_iter()
        .map(|leg| (leg, leg.amount(breakdown)))
        .filter(|(_, amount)| !amount.is_zero())
        .map(|(leg, amount)| (leg, amount.clone()))
        .collect()
}

/// Why an invoice could not be posted to the ledger
#[derive(Debug, thiserror::Error)]
pub enum PostingError {
    /// The invoice breaks compliance rules; carries the error-severity issues
    #[error("invoice is not compliant ({} error(s)), first: {}", .0.len(), .0.first().map_or(String::new(), ToString::to_string))]
    NotCompliant(Vec<ComplianceIssue>),
    /// The invoice total is zero, so there is nothing to post
    #[error("invoice total is zero, so there is nothing to post")]
    NothingToPost,
    /// The invoice's tax could not be calculated
    #[error(transparent)]
    Invoice(#[from] InvoiceError),
}

/// Accounts an invoice posts to, one per [`PostingLeg`]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceAccounts {
    /// Debited with the invoice total (accounts receivable or cash)
    pub receivable: String,
    /// Credited with the taxable value
    pub sales: String,
    /// Credited with CGST
    pub cgst_output: String,
    /// Credited with SGST
    pub sgst_output: String,
    /// Credited with IGST
    pub igst_output: String,
}

impl InvoiceAccounts {
    /// Name the account for each leg
    pub fn new(
        receivable: impl Into<String>,
        sales: impl Into<String>,
        cgst_output: impl Into<String>,
        sgst_output: impl Into<String>,
        igst_output: impl Into<String>,
    ) -> Self {
        Self {
            receivable: receivable.into(),
            sales: sales.into(),
            cgst_output: cgst_output.into(),
            sgst_output: sgst_output.into(),
            igst_output: igst_output.into(),
        }
    }

    /// Account id `leg` posts to
    #[must_use]
    pub fn account_for(&self, leg: PostingLeg) -> &str {
        match leg {
            PostingLeg::Receivable => &self.receivable,
            PostingLeg::Sales => &self.sales,
            PostingLeg::CgstOutput => &self.cgst_output,
            PostingLeg::SgstOutput => &self.sgst_output,
            PostingLeg::IgstOutput => &self.igst_output,
        }
    }
}

impl GstInvoice {
    /// Journal entries that record this invoice in the ledger
    ///
    /// The invoice must pass the error-severity compliance rules of
    /// [`validate_invoice`](super::validate_invoice), checked as of its own date: every error rule
    /// applies except the future-date check, which depends on when the caller posts it. Warnings
    /// don't block the posting. The entries balance: the receivable debit equals the sales and tax
    /// credits. Pass them to [`TransactionBuilder::entry`](crate::ledger::TransactionBuilder::entry)
    /// to build the transaction.
    ///
    /// # Errors
    ///
    /// [`PostingError::NotCompliant`] with the error-severity issues if the invoice breaks a
    /// compliance rule, [`PostingError::NothingToPost`] if its total is zero, or
    /// [`PostingError::Invoice`] for any error from [`GstInvoice::breakdown`].
    ///
    /// # Example
    ///
    /// ```
    /// use accounting_core::invoice::{GstInvoice, GstLineItem, Gstin, InvoiceAccounts};
    /// use accounting_core::EntryType;
    /// use bigdecimal::BigDecimal;
    /// use chrono::NaiveDate;
    ///
    /// # fn main() -> Result<(), accounting_core::Error> {
    /// let item = GstLineItem::new("998314", "IT consulting", BigDecimal::from(1),
    ///     BigDecimal::from(1000), BigDecimal::from(18))?;
    /// let invoice = GstInvoice::new("INV-001", NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
    ///     Gstin::parse("27AAPFU0939F1ZV")?, Gstin::parse("29AAPFU0939F1ZR")?, vec![item])?;
    ///
    /// let accounts = InvoiceAccounts::new("receivable", "sales", "cgst", "sgst", "igst");
    /// let entries = invoice.to_entries(&accounts)?;
    ///
    /// // Inter-state: receivable 1180 = sales 1000 + IGST 180
    /// assert_eq!(entries.len(), 3);
    /// assert_eq!(entries[0].entry_type, EntryType::Debit);
    /// assert_eq!(entries[0].amount, BigDecimal::from(1180));
    /// assert_eq!(entries[2].account_id, "igst");
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_entries(&self, accounts: &InvoiceAccounts) -> Result<Vec<Entry>, PostingError> {
        let errors = compliance_errors(self, self.invoice_date);
        if !errors.is_empty() {
            return Err(PostingError::NotCompliant(errors));
        }

        let legs = posting_legs(&self.breakdown()?);
        if legs.is_empty() {
            return Err(PostingError::NothingToPost);
        }

        Ok(legs
            .into_iter()
            .map(|(leg, amount)| {
                Entry::new(
                    accounts.account_for(leg),
                    leg.side(),
                    amount,
                    Some(leg.to_string()),
                )
            })
            .collect())
    }
}
