//! Error types
//!
//! Each module has its own error enum; [`Error`] wraps all of them so an application can use a
//! single `?`-friendly type across ledger, GST, invoice and reconciliation calls.

use bigdecimal::BigDecimal;

use crate::invoice::InvoiceError;
use crate::reconciliation::ReconciliationError;
use crate::tax::GstError;

/// Boxed error from a storage backend, kept as the `source` of a storage failure
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Any error raised by this crate
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Ledger, account or transaction failure
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    /// GST rate or calculation failure
    #[error(transparent)]
    Gst(#[from] GstError),
    /// GST invoice failure
    #[error(transparent)]
    Invoice(#[from] InvoiceError),
    /// Reconciliation storage or import failure
    #[error(transparent)]
    Reconciliation(#[from] ReconciliationError),
}

/// Result type using the crate-level [`Error`]
pub type Result<T> = std::result::Result<T, Error>;

/// Why a text field was rejected
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FieldError {
    /// The field is empty or only whitespace
    #[error("cannot be empty")]
    Empty,
    /// The field is longer than allowed
    #[error("cannot exceed {max} characters")]
    TooLong {
        /// Maximum length in bytes
        max: usize,
    },
    /// The field contains characters outside the allowed set
    #[error("can only contain alphanumeric characters, dashes, and underscores")]
    InvalidCharacters,
}

/// Why pagination parameters were rejected
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PaginationError {
    /// Pages are numbered from 1
    #[error("page must be 1 or greater")]
    PageOutOfRange,
    /// Page size is outside the accepted range
    #[error("page size must be between 1 and {max}")]
    PageSizeOutOfRange {
        /// Largest accepted page size
        max: u32,
    },
}

/// Errors raised by the ledger and its storage
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// The storage backend failed
    #[error("storage error: {0}")]
    Storage(#[source] BoxError),
    /// No account with this id exists
    #[error("account not found: {0}")]
    AccountNotFound(String),
    /// No transaction with this id exists
    #[error("transaction not found: {0}")]
    TransactionNotFound(String),
    /// An account with this id already exists
    #[error("account with ID '{0}' already exists")]
    DuplicateAccount(String),
    /// The parent account named on a new account does not exist
    #[error("parent account '{0}' does not exist")]
    ParentNotFound(String),
    /// Following parent links from this account leads back to it
    #[error("account hierarchy has a cycle through '{0}'")]
    AccountCycle(String),
    /// A transaction needs a debit and a credit leg at least
    #[error("transaction must have at least two entries for double-entry bookkeeping")]
    TooFewEntries,
    /// Debits and credits differ
    #[error("transaction is not balanced: debits = {debits}, credits = {credits}")]
    Unbalanced {
        /// Sum of debit entries
        debits: BigDecimal,
        /// Sum of credit entries
        credits: BigDecimal,
    },
    /// An entry amount is zero or negative
    #[error("entry amounts must be positive")]
    NonPositiveAmount,
    /// The same account appears twice on the same side of a transaction
    #[error("account '{0}' appears multiple times with the same entry type in transaction")]
    DuplicateEntry(String),
    /// A text field failed validation
    #[error("{field} {error}")]
    InvalidField {
        /// Which field was rejected
        field: &'static str,
        /// Why it was rejected
        error: FieldError,
    },
    /// Pagination parameters were rejected
    #[error("invalid pagination: {0}")]
    InvalidPagination(#[from] PaginationError),
}

impl LedgerError {
    /// Wrap a backend error as a [`LedgerError::Storage`]
    pub fn storage(error: impl Into<BoxError>) -> Self {
        Self::Storage(error.into())
    }
}

/// Result type for ledger operations
pub type LedgerResult<T> = std::result::Result<T, LedgerError>;
