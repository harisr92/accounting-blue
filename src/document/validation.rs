//! Checks every invoice or credit note must pass, whatever its tax regime
//!
//! A document's fields are public and can be edited after it is built, and one rule depends on
//! the date it is checked on. [`validate_document`] runs every rule and returns a
//! [`ValidationReport`] listing each [`ValidationIssue`] found, so a caller sees all problems at
//! once rather than the first.
//!
//! Issues are [`Severity::Error`] when the document must not be issued or posted, and
//! [`Severity::Warning`] when it is valid but worth a second look. The rule functions are public
//! so a regime with rules of its own, such as Indian GST, can run them alongside its own.

use crate::document::discount::DiscountError;
use crate::document::line::{check_line, DocumentLine, LineError};
use crate::document::pricing::Document;
use crate::money::round_money;
use bigdecimal::Zero;
use chrono::NaiveDate;

/// How serious a validation issue is
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The document must not be issued or posted
    Error,
    /// The document is valid but worth a second look
    Warning,
}

/// An issue a [`ValidationReport`] can hold
pub trait Issue {
    /// How serious the issue is
    fn severity(&self) -> Severity;
}

/// A rule an invoice or credit note breaks
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ValidationIssue {
    /// The document has no lines
    #[error("a document needs at least one line")]
    NoLines,
    /// A line breaks a field rule; `line` counts from 0
    #[error("line {}: {reason}", .line + 1)]
    InvalidLine {
        /// Position of the line, from 0
        line: usize,
        /// The rule it breaks
        reason: LineError,
    },
    /// The document discount is out of range or more than the subtotal
    #[error("document discount: {0}")]
    InvalidDiscount(DiscountError),
    /// The document is dated after the day it is checked on
    #[error("document date {date} is after {as_of}")]
    FutureDate {
        /// Date of the document
        date: NaiveDate,
        /// Day it was checked on
        as_of: NaiveDate,
    },
    /// A line's taxable value rounds to zero; `line` counts from 0
    #[error("line {}: taxable value rounds to zero", .line + 1)]
    ZeroValueLine {
        /// Position of the line, from 0
        line: usize,
    },
}

impl Issue for ValidationIssue {
    fn severity(&self) -> Severity {
        match self {
            Self::NoLines
            | Self::InvalidLine { .. }
            | Self::InvalidDiscount(_)
            | Self::FutureDate { .. } => Severity::Error,
            Self::ZeroValueLine { .. } => Severity::Warning,
        }
    }
}

/// Every issue found in a document
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationReport<I = ValidationIssue> {
    issues: Vec<I>,
}

impl<I> Default for ValidationReport<I> {
    fn default() -> Self {
        Self { issues: Vec::new() }
    }
}

impl<I: Issue> ValidationReport<I> {
    /// A report of `issues`
    #[must_use]
    pub fn new(issues: Vec<I>) -> Self {
        Self { issues }
    }

    /// Every issue, errors and warnings, in the order the rules found them
    #[must_use]
    pub fn issues(&self) -> &[I] {
        &self.issues
    }

    /// The issues that stop the document being issued or posted
    pub fn errors(&self) -> impl Iterator<Item = &I> {
        self.of_severity(Severity::Error)
    }

    /// The issues worth a second look
    pub fn warnings(&self) -> impl Iterator<Item = &I> {
        self.of_severity(Severity::Warning)
    }

    /// The issues of `severity`
    fn of_severity(&self, severity: Severity) -> impl Iterator<Item = &I> {
        self.issues
            .iter()
            .filter(move |issue| issue.severity() == severity)
    }

    /// Whether the document breaks no error-severity rule; warnings are allowed
    #[must_use]
    pub fn is_compliant(&self) -> bool {
        self.errors().next().is_none()
    }

    /// The issues, consuming the report
    #[must_use]
    pub fn into_issues(self) -> Vec<I> {
        self.issues
    }
}

/// Run every rule over `document` as of `as_of`
///
/// # Example
///
/// ```
/// use accounting_core::document::{validate_document, FlatRate, LineItem};
/// use accounting_core::sales::Invoice;
/// use bigdecimal::BigDecimal;
/// use chrono::NaiveDate;
///
/// # fn main() -> Result<(), accounting_core::Error> {
/// let date = |day| NaiveDate::from_ymd_opt(2025, 3, day).unwrap();
/// let line = LineItem::new("Chair", BigDecimal::from(1), BigDecimal::from(50),
///     BigDecimal::from(20))?;
/// let invoice = Invoice::new("INV-001", date(10), "Acme", vec![line], FlatRate::new("VAT"))?;
///
/// assert!(validate_document(&invoice, date(10)).is_compliant());
/// assert!(!validate_document(&invoice, date(9)).is_compliant()); // dated in the future
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn validate_document<D: Document + ?Sized>(document: &D, as_of: NaiveDate) -> ValidationReport {
    ValidationReport::new(
        [
            lines_present_rule(document),
            line_fields_rule(document),
            discount_rule(document),
            date_rule(document, as_of),
            zero_value_rule(document),
        ]
        .into_iter()
        .flatten()
        .collect(),
    )
}

/// A document needs at least one line
#[must_use]
pub fn lines_present_rule<D: Document + ?Sized>(document: &D) -> Vec<ValidationIssue> {
    if document.lines().is_empty() {
        vec![ValidationIssue::NoLines]
    } else {
        Vec::new()
    }
}

/// Each line must pass the rules it was built with: [`check_line`]
#[must_use]
pub fn line_fields_rule<D: Document + ?Sized>(document: &D) -> Vec<ValidationIssue> {
    per_line(document, |line, item| {
        check_line(item)
            .err()
            .map(|reason| ValidationIssue::InvalidLine { line, reason })
    })
}

/// The discount off the subtotal must fit it
#[must_use]
pub fn discount_rule<D: Document + ?Sized>(document: &D) -> Vec<ValidationIssue> {
    document
        .check_discount()
        .err()
        .map(ValidationIssue::InvalidDiscount)
        .into_iter()
        .collect()
}

/// A document may not be dated after the day it is checked on
#[must_use]
pub fn date_rule<D: Document + ?Sized>(document: &D, as_of: NaiveDate) -> Vec<ValidationIssue> {
    if document.date() > as_of {
        vec![ValidationIssue::FutureDate {
            date: document.date(),
            as_of,
        }]
    } else {
        Vec::new()
    }
}

/// Warn about lines whose taxable value rounds to zero, such as a zero unit price
#[must_use]
pub fn zero_value_rule<D: Document + ?Sized>(document: &D) -> Vec<ValidationIssue> {
    per_line(document, |line, item| {
        round_money(&item.taxable_value())
            .is_zero()
            .then_some(ValidationIssue::ZeroValueLine { line })
    })
}

/// The issues `check` finds on each line, numbered from 0
fn per_line<D: Document + ?Sized>(
    document: &D,
    check: impl Fn(usize, &D::Line) -> Option<ValidationIssue>,
) -> Vec<ValidationIssue> {
    document
        .lines()
        .iter()
        .enumerate()
        .filter_map(|(line, item)| check(line, item))
        .collect()
}
