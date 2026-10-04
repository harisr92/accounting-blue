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
//!
//! [`validate_credit_note`] runs the same rules over a [`CreditNote`], which is a
//! [`GstDocument`] too, plus the rules only a note has.

use super::hsn_lookup::{is_valid_hsn_sac, HsnMaster};
use super::note::CreditNote;
use super::types::{invoice_number_error, GstDocument, GstInvoice, GstLineItem};
use super::types::{InvoiceNumberError, LineItemError, Recipient, SupplyKind};
use crate::tax::round_to_paise;
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
    /// A line's taxable value rounds to zero paise, so it adds nothing to the invoice
    #[error("line {}: taxable value rounds to zero", .line + 1)]
    ZeroValueLine {
        /// Index of the line
        line: usize,
    },
    /// A credit note is dated before the invoice it corrects
    #[error("credit note dated {note_date} is before its invoice dated {original_date}")]
    NoteBeforeOriginal {
        /// Date of the note
        note_date: NaiveDate,
        /// Date of the invoice it corrects
        original_date: NaiveDate,
    },
    /// A credit note's original supply kind can't apply to its buyer, such as B2B for an
    /// unregistered buyer
    #[error("a {kind} invoice can't have been issued to {buyer}")]
    OriginalKindMismatch {
        /// The supply kind the note records for its invoice
        kind: SupplyKind,
        /// The note's buyer
        buyer: Recipient,
    },
    /// A credit note credits more than the invoice it corrects was worth
    #[error("credit note total {credited} is more than its invoice's value {invoice_value}")]
    CreditExceedsInvoice {
        /// The note's total, tax included
        credited: BigDecimal,
        /// The invoice's value, tax included
        invoice_value: BigDecimal,
    },
    /// A credit note is issued after the Section 34(2) deadline for its invoice
    #[error("credit note dated {note_date} is after the {deadline} deadline for its invoice")]
    CreditNoteTooLate {
        /// Date of the note
        note_date: NaiveDate,
        /// Last day it could be issued
        deadline: NaiveDate,
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
            | Self::SameSellerAndBuyer(_)
            | Self::NoteBeforeOriginal { .. }
            | Self::OriginalKindMismatch { .. }
            | Self::CreditExceedsInvoice { .. }
            | Self::CreditNoteTooLate { .. } => Severity::Error,
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

/// A rule whose issues are errors: every issue it finds on a document checked on a given date
type ErrorRule = fn(&dyn GstDocument, NaiveDate) -> Vec<ComplianceIssue>;

/// A rule whose issues are warnings: every issue it finds on a document, given the HSN/SAC master
type WarningRule = fn(&dyn GstDocument, &HsnMaster) -> Vec<ComplianceIssue>;

/// A rule only credit notes have, whose issues are errors
type NoteRule = fn(&CreditNote) -> Vec<ComplianceIssue>;

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

/// The rules a credit note must meet on top of [`ERROR_RULES`], reported after them
const NOTE_RULES: &[NoteRule] = &[
    note_date_rule,
    note_kind_rule,
    note_value_rule,
    note_deadline_rule,
];

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
    report(compliance_errors(invoice, as_of), invoice, master)
}

/// Check a credit note against every compliance rule
///
/// The note meets the same rules as an invoice, as [`validate_invoice`] describes, and four of
/// its own: it is not dated before the invoice it corrects, its buyer is one the invoice's supply
/// kind can apply to, its total is no more than the invoice's value, and it is issued by
/// [`CreditNote::deadline`] (Section 34(2)). All four are errors.
///
/// # Example
///
/// ```
/// use accounting_core::invoice::{validate_credit_note, ComplianceIssue, CreditNote, GstInvoice,
///     GstLineItem, Gstin, HsnMaster};
/// use bigdecimal::BigDecimal;
/// use chrono::NaiveDate;
///
/// # fn main() -> Result<(), accounting_core::Error> {
/// let item = GstLineItem::new("998314", "IT consulting", BigDecimal::from(1),
///     BigDecimal::from(1000), BigDecimal::from(18))?;
/// let invoice = GstInvoice::new("INV-001", NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
///     Gstin::parse("27AAPFU0939F1ZV")?, Gstin::parse("27AAPFU0939F2ZU")?, vec![item.clone()])?;
///
/// // The deadline for an invoice of FY 2024-25 is 30 November 2025
/// let late = NaiveDate::from_ymd_opt(2025, 12, 1).unwrap();
/// let note = CreditNote::new(&invoice, "CN-001", late, vec![item])?;
/// let report = validate_credit_note(&note, late, HsnMaster::global());
/// assert!(matches!(report.issues(), [ComplianceIssue::CreditNoteTooLate { .. }]));
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn validate_credit_note(
    note: &CreditNote,
    as_of: NaiveDate,
    master: &HsnMaster,
) -> InvoiceValidationReport {
    report(credit_note_errors(note, as_of), note, master)
}

/// A report of `errors` followed by the warnings for `document`
fn report(
    errors: Vec<ComplianceIssue>,
    document: &dyn GstDocument,
    master: &HsnMaster,
) -> InvoiceValidationReport {
    let warnings = WARNING_RULES.iter().flat_map(|rule| rule(document, master));
    InvoiceValidationReport {
        issues: errors.into_iter().chain(warnings).collect(),
    }
}

/// Only the error-severity issues, which need no HSN/SAC master
pub(crate) fn compliance_errors(
    document: &dyn GstDocument,
    as_of: NaiveDate,
) -> Vec<ComplianceIssue> {
    ERROR_RULES
        .iter()
        .flat_map(|rule| rule(document, as_of))
        .collect()
}

/// Only the error-severity issues of a credit note: the document rules, then its own
pub(crate) fn credit_note_errors(note: &CreditNote, as_of: NaiveDate) -> Vec<ComplianceIssue> {
    let note_issues = NOTE_RULES.iter().flat_map(|rule| rule(note));
    compliance_errors(note, as_of)
        .into_iter()
        .chain(note_issues)
        .collect()
}

/// Issues found on each line by `check`, which is given the line's index and item
fn per_line(
    document: &dyn GstDocument,
    check: impl Fn(usize, &GstLineItem) -> Option<ComplianceIssue>,
) -> Vec<ComplianceIssue> {
    document
        .line_items()
        .iter()
        .enumerate()
        .filter_map(|(line, item)| check(line, item))
        .collect()
}

fn invoice_number_rule(document: &dyn GstDocument, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    invoice_number_error(document.number())
        .map(|reason| ComplianceIssue::InvalidInvoiceNumber {
            value: document.number().to_string(),
            reason,
        })
        .into_iter()
        .collect()
}

fn line_items_present_rule(document: &dyn GstDocument, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    if document.line_items().is_empty() {
        vec![ComplianceIssue::NoLineItems]
    } else {
        Vec::new()
    }
}

/// The HSN/SAC shape rule [`GstLineItem::new`] applies to each line
fn hsn_sac_shape_rule(document: &dyn GstDocument, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    per_line(document, |line, item| {
        (!is_valid_hsn_sac(&item.hsn_sac)).then(|| ComplianceIssue::InvalidHsnSac {
            line,
            code: item.hsn_sac.clone(),
        })
    })
}

/// The other construction rules for each line: description, quantity, price and rate
fn line_fields_rule(document: &dyn GstDocument, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    per_line(document, |line, item| {
        item.check()
            .err()
            .map(|reason| ComplianceIssue::InvalidLineItem { line, reason })
    })
}

fn date_rule(document: &dyn GstDocument, as_of: NaiveDate) -> Vec<ComplianceIssue> {
    if document.date() > as_of {
        vec![ComplianceIssue::FutureDate {
            invoice_date: document.date(),
            as_of,
        }]
    } else {
        Vec::new()
    }
}

fn parties_rule(document: &dyn GstDocument, _as_of: NaiveDate) -> Vec<ComplianceIssue> {
    if document.buyer().gstin() == Some(document.seller_gstin()) {
        vec![ComplianceIssue::SameSellerAndBuyer(
            document.seller_gstin().to_string(),
        )]
    } else {
        Vec::new()
    }
}

/// Warn about well-formed codes the master doesn't know, and rates that differ from its default
///
/// A rate is only compared when the master has the line's exact code: a fallback heading's rate
/// may not apply to every tariff item under it. It is also not compared on a line that already
/// breaks a field rule, on an exempt or non-GST line (whose rate is 0 by definition), or on an
/// invoice dated before [`HsnMaster::effective_from`], when the schedule didn't apply yet. A credit
/// note goes by its original invoice's date ([`GstDocument::supply_date`]), since it reverses tax
/// at the rates that invoice charged.
fn hsn_master_rule(document: &dyn GstDocument, master: &HsnMaster) -> Vec<ComplianceIssue> {
    let schedule_applies = document.supply_date() >= master.effective_from();
    per_line(document, |line, item| {
        if !is_valid_hsn_sac(&item.hsn_sac) {
            return None;
        }
        let Some(entry) = master.lookup(&item.hsn_sac) else {
            return Some(ComplianceIssue::UnknownHsnSac {
                line,
                code: item.hsn_sac.clone(),
            });
        };
        let compare_rate = schedule_applies
            && item.treatment.is_taxable()
            && entry.code == item.hsn_sac
            && item.check().is_ok();
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

fn zero_value_rule(document: &dyn GstDocument, _master: &HsnMaster) -> Vec<ComplianceIssue> {
    per_line(document, |line, item| {
        round_to_paise(&item.taxable_value())
            .is_zero()
            .then_some(ComplianceIssue::ZeroValueLine { line })
    })
}

fn note_date_rule(note: &CreditNote) -> Vec<ComplianceIssue> {
    if note.predates_original() {
        vec![ComplianceIssue::NoteBeforeOriginal {
            note_date: note.note_date,
            original_date: note.original.date,
        }]
    } else {
        Vec::new()
    }
}

/// The note's buyer must be one its original invoice's supply kind can apply to, or GSTR-1 would
/// report it in the wrong table or drop it from Table 9B
fn note_kind_rule(note: &CreditNote) -> Vec<ComplianceIssue> {
    if note.original_kind_fits_buyer() {
        Vec::new()
    } else {
        vec![ComplianceIssue::OriginalKindMismatch {
            kind: note.original.kind,
            buyer: note.buyer.clone(),
        }]
    }
}

/// A note may credit at most its invoice's value; a note whose tax can't be computed is left to
/// [`line_fields_rule`]
fn note_value_rule(note: &CreditNote) -> Vec<ComplianceIssue> {
    match note.credit_over_value() {
        Ok(Some(credited)) => vec![ComplianceIssue::CreditExceedsInvoice {
            credited,
            invoice_value: note.original.value.clone(),
        }],
        Ok(None) | Err(_) => Vec::new(),
    }
}

fn note_deadline_rule(note: &CreditNote) -> Vec<ComplianceIssue> {
    let deadline = note.deadline();
    if note.note_date > deadline {
        vec![ComplianceIssue::CreditNoteTooLate {
            note_date: note.note_date,
            deadline,
        }]
    } else {
        Vec::new()
    }
}
