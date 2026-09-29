//! Compliance checks for a GST invoice
//!
//! [`GstInvoice::new`] rejects malformed input, but an invoice's fields are public and can be
//! edited afterwards, and some rules depend on things construction can't see: the date the
//! invoice is checked on and the HSN/SAC master. [`validate_invoice`] runs every
//! rule and returns an [`InvoiceValidationReport`] listing each [`ComplianceIssue`] found, so a
//! caller sees all problems at once rather than the first.
//!
//! Issues are [`Severity::Error`] when the invoice must not be issued or posted, and
//! [`Severity::Warning`] when it is valid but worth a second look.

use super::hsn_lookup::{is_valid_hsn_sac, HsnMaster};
use super::types::{invoice_number_error, GstInvoice, GstLineItem};
use super::types::{InvoiceNumberError, LineItemError};
use bigdecimal::{BigDecimal, Zero};
use chrono::NaiveDate;

/// How serious a [`ComplianceIssue`] is
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The invoice must not be issued or posted
    Error,
    /// The invoice is valid, but something looks unusual
    Warning,
}

/// One rule an invoice breaks
///
/// `line` fields are indices into [`GstInvoice::line_items`]; messages number lines from 1.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ComplianceIssue {
    /// The invoice number breaks Rule 46
    #[error("invoice number {value:?}: {reason}")]
    InvalidInvoiceNumber {
        /// The invoice number
        value: String,
        /// The first rule it breaks
        reason: InvoiceNumberError,
    },
    /// The invoice has no line items
    #[error("an invoice needs at least one line item")]
    NoLineItems,
    /// A line's HSN/SAC code is missing or not 4, 6 or 8 digits
    #[error("line {}: HSN/SAC code {code:?} must be 4, 6 or 8 digits", .line + 1)]
    InvalidHsnSac {
        /// Index of the line
        line: usize,
        /// The code as given
        code: String,
    },
    /// A line's description, quantity, price or rate is invalid
    #[error("line {}: {reason}", .line + 1)]
    InvalidLineItem {
        /// Index of the line
        line: usize,
        /// The first rule the line breaks
        reason: LineItemError,
    },
    /// The invoice is dated after the day it is checked
    #[error("invoice date {invoice_date} is after {as_of}")]
    FutureDate {
        /// Date on the invoice
        invoice_date: NaiveDate,
        /// Date the invoice was checked against
        as_of: NaiveDate,
    },
    /// Seller and buyer have the same GSTIN
    #[error("seller and buyer have the same GSTIN {0}")]
    SameSellerAndBuyer(String),
    /// A line's HSN/SAC code is well-formed but not in the master data
    #[error("line {}: HSN/SAC code {code} is not in the master data", .line + 1)]
    UnknownHsnSac {
        /// Index of the line
        line: usize,
        /// The code as given
        code: String,
    },
    /// A line's rate differs from the default rate for its HSN/SAC code
    #[error("line {}: rate {rate}% differs from the {default}% default for {code}", .line + 1)]
    RateDiffersFromHsnDefault {
        /// Index of the line
        line: usize,
        /// The line's HSN/SAC code
        code: String,
        /// Rate charged on the line
        rate: BigDecimal,
        /// Default rate in the master
        default: BigDecimal,
    },
    /// A line has a unit price of zero
    #[error("line {}: unit price is zero", .line + 1)]
    ZeroValueLine {
        /// Index of the line
        line: usize,
    },
}

impl ComplianceIssue {
    /// How serious this issue is
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Self::InvalidInvoiceNumber { .. }
            | Self::NoLineItems
            | Self::InvalidHsnSac { .. }
            | Self::InvalidLineItem { .. }
            | Self::FutureDate { .. }
            | Self::SameSellerAndBuyer(_) => Severity::Error,
            Self::UnknownHsnSac { .. }
            | Self::RateDiffersFromHsnDefault { .. }
            | Self::ZeroValueLine { .. } => Severity::Warning,
        }
    }
}

/// Every issue [`validate_invoice`] found, in rule order
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InvoiceValidationReport {
    issues: Vec<ComplianceIssue>,
}

impl InvoiceValidationReport {
    /// All issues
    #[must_use]
    pub fn issues(&self) -> &[ComplianceIssue] {
        &self.issues
    }

    /// Issues of severity [`Severity::Error`]
    pub fn errors(&self) -> impl Iterator<Item = &ComplianceIssue> {
        self.of_severity(Severity::Error)
    }

    /// Issues of severity [`Severity::Warning`]
    pub fn warnings(&self) -> impl Iterator<Item = &ComplianceIssue> {
        self.of_severity(Severity::Warning)
    }

    fn of_severity(&self, severity: Severity) -> impl Iterator<Item = &ComplianceIssue> {
        self.issues
            .iter()
            .filter(move |issue| issue.severity() == severity)
    }

    /// Whether the invoice has no errors (warnings are allowed)
    #[must_use]
    pub fn is_compliant(&self) -> bool {
        self.errors().next().is_none()
    }

    /// Consume the report, returning its issues
    #[must_use]
    pub fn into_issues(self) -> Vec<ComplianceIssue> {
        self.issues
    }
}

/// A rule whose issues are errors: every issue it finds on an invoice checked on a given date
type ErrorRule = fn(&GstInvoice, NaiveDate) -> Vec<ComplianceIssue>;

/// A rule whose issues are warnings: every issue it finds on an invoice, given the HSN/SAC master
type WarningRule = fn(&GstInvoice, &HsnMaster) -> Vec<ComplianceIssue>;

/// The rules that block an invoice, in report order
const ERROR_RULES: &[ErrorRule] = &[
    invoice_number_rule,
    line_items_present_rule,
    hsn_sac_shape_rule,
    line_fields_rule,
    date_rule,
    parties_rule,
];

/// The rules that only flag an invoice for a second look, reported after the errors
const WARNING_RULES: &[WarningRule] = &[hsn_master_rule, zero_value_rule];

/// Check an invoice against every compliance rule
///
/// `as_of` is the day the invoice is checked, usually today; an invoice dated after it is
/// flagged. `master` supplies the HSN/SAC codes and default rates the lines are compared with,
/// usually [`HsnMaster::global`]. The function is pure, so the caller supplies both.
///
/// # Example
///
/// ```
/// use accounting_core::invoice::{validate_invoice, ComplianceIssue, GstInvoice, GstLineItem, Gstin, HsnMaster};
/// use bigdecimal::BigDecimal;
/// use chrono::NaiveDate;
///
/// # fn main() -> Result<(), accounting_core::Error> {
/// let item = GstLineItem::new("998314", "IT consulting", BigDecimal::from(1),
///     BigDecimal::from(1000), BigDecimal::from(18))?;
/// let invoice = GstInvoice::new("INV-001", NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
///     Gstin::parse("27AAPFU0939F1ZV")?, Gstin::parse("27AAPFU0939F2ZU")?, vec![item])?;
/// let master = HsnMaster::global();
///
/// let checked_on = NaiveDate::from_ymd_opt(2024, 11, 30).unwrap();
/// assert!(validate_invoice(&invoice, checked_on, master).is_compliant());
///
/// let report = validate_invoice(&invoice, NaiveDate::from_ymd_opt(2024, 11, 1).unwrap(), master);
/// assert!(!report.is_compliant());
/// assert!(matches!(report.issues(), [ComplianceIssue::FutureDate { .. }]));
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn validate_invoice(
    invoice: &GstInvoice,
    as_of: NaiveDate,
    master: &HsnMaster,
) -> InvoiceValidationReport {
    let warnings = WARNING_RULES.iter().flat_map(|rule| rule(invoice, master));
    InvoiceValidationReport {
        issues: compliance_errors(invoice, as_of)
            .into_iter()
            .chain(warnings)
            .collect(),
    }
}

/// Only the error-severity issues, which need no HSN/SAC master
pub(super) fn compliance_errors(invoice: &GstInvoice, as_of: NaiveDate) -> Vec<ComplianceIssue> {
    ERROR_RULES
        .iter()
        .flat_map(|rule| rule(invoice, as_of))
        .collect()
}

/// Issues found on each line by `check`, which is given the line's index and item
fn per_line(
    invoice: &GstInvoice,
    check: impl Fn(usize, &GstLineItem) -> Option<ComplianceIssue>,
) -> Vec<ComplianceIssue> {
    invoice
        .line_items
        .iter()
        .enumerate()
        .filter_map(|(line, item)| check(line, item))
        .collect()
}

fn invoice_number_rule(invoice: &GstInvoice, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    invoice_number_error(&invoice.invoice_number)
        .map(|reason| ComplianceIssue::InvalidInvoiceNumber {
            value: invoice.invoice_number.clone(),
            reason,
        })
        .into_iter()
        .collect()
}

fn line_items_present_rule(invoice: &GstInvoice, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    if invoice.line_items.is_empty() {
        vec![ComplianceIssue::NoLineItems]
    } else {
        Vec::new()
    }
}

/// The HSN/SAC shape rule [`GstLineItem::new`] applies to each line
fn hsn_sac_shape_rule(invoice: &GstInvoice, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    per_line(invoice, |line, item| {
        (!is_valid_hsn_sac(&item.hsn_sac)).then(|| ComplianceIssue::InvalidHsnSac {
            line,
            code: item.hsn_sac.clone(),
        })
    })
}

/// The other construction rules for each line: description, quantity, price and rate
fn line_fields_rule(invoice: &GstInvoice, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    per_line(invoice, |line, item| {
        item.check()
            .err()
            .map(|reason| ComplianceIssue::InvalidLineItem { line, reason })
    })
}

fn date_rule(invoice: &GstInvoice, as_of: NaiveDate) -> Vec<ComplianceIssue> {
    if invoice.invoice_date > as_of {
        vec![ComplianceIssue::FutureDate {
            invoice_date: invoice.invoice_date,
            as_of,
        }]
    } else {
        Vec::new()
    }
}

fn parties_rule(invoice: &GstInvoice, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    if invoice.seller_gstin == invoice.buyer_gstin {
        vec![ComplianceIssue::SameSellerAndBuyer(
            invoice.seller_gstin.to_string(),
        )]
    } else {
        Vec::new()
    }
}

/// Warn about well-formed codes the master doesn't know, and rates that differ from its default
///
/// A rate is only compared when the master has the line's exact code: a fallback heading's rate
/// may not apply to every tariff item under it. It is also not compared on a line that already
/// breaks a field rule, or on an invoice dated before [`HsnMaster::effective_from`], when the
/// schedule didn't apply yet.
fn hsn_master_rule(invoice: &GstInvoice, master: &HsnMaster) -> Vec<ComplianceIssue> {
    let schedule_applies = invoice.invoice_date >= master.effective_from();
    per_line(invoice, |line, item| {
        if !is_valid_hsn_sac(&item.hsn_sac) {
            return None;
        }
        let Some(entry) = master.lookup(&item.hsn_sac) else {
            return Some(ComplianceIssue::UnknownHsnSac {
                line,
                code: item.hsn_sac.clone(),
            });
        };
        let compare_rate = schedule_applies && entry.code == item.hsn_sac && item.check().is_ok();
        (compare_rate && entry.gst_rate != item.gst_rate).then(|| {
            ComplianceIssue::RateDiffersFromHsnDefault {
                line,
                code: item.hsn_sac.clone(),
                rate: item.gst_rate.clone(),
                default: entry.gst_rate.clone(),
            }
        })
    })
}

fn zero_value_rule(invoice: &GstInvoice, _master: &HsnMaster) -> Vec<ComplianceIssue> {
    per_line(invoice, |line, item| {
        item.unit_price
            .is_zero()
            .then_some(ComplianceIssue::ZeroValueLine { line })
    })
}
