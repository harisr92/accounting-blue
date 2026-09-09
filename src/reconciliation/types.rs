//! Core data structures for reconciliation
//!
//! Two record kinds flow into the engine: [`LedgerTransaction`] (internal, projected from a
//! double-entry [`Transaction`] onto a single account) and [`ExternalTransaction`] (a row from a
//! bank statement, payment gateway, UPI provider or card issuer). Both keep a **positive** amount
//! and carry direction in [`EntryType`], exactly like [`Entry`](crate::types::Entry).

use bigdecimal::BigDecimal;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{EntryType, Transaction};

/// Where a set of external transactions came from
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalSource {
    /// A statement downloaded from a bank
    BankStatement {
        /// Name of the bank
        bank_name: String,
        /// Account number the statement covers
        account_number: String,
    },
    /// A settlement report from a payment gateway
    PaymentGateway {
        /// Gateway provider name (Razorpay, Stripe, ...)
        provider: String,
        /// Merchant identifier with that provider
        merchant_id: String,
    },
    /// A UPI transaction feed
    Upi {
        /// UPI provider or PSP name
        provider: String,
    },
    /// A credit card statement
    CreditCard {
        /// Card issuer name
        issuer: String,
        /// Last four digits of the card
        last_four: String,
    },
}

/// A transaction as recorded by an external system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalTransaction {
    /// Identifier assigned by the external system (or by the importer)
    pub id: String,
    /// Date the external system booked the transaction
    pub date: NaiveDate,
    /// Absolute amount; direction lives in `entry_type`
    pub amount: BigDecimal,
    /// Narration / description text
    pub description: String,
    /// Reference number (UTR, RRN, cheque number, ...) if the source provides one
    pub reference: Option<String>,
    /// Direction, expressed from the *ledger's* point of view
    pub entry_type: EntryType,
    /// Where this row came from
    pub source: ExternalSource,
}

impl ExternalTransaction {
    /// Create a new external transaction
    pub fn new(
        id: String,
        date: NaiveDate,
        amount: BigDecimal,
        description: String,
        entry_type: EntryType,
        source: ExternalSource,
    ) -> Self {
        Self {
            id,
            date,
            amount,
            description,
            reference: None,
            entry_type,
            source,
        }
    }

    /// Attach a reference number
    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }
}

/// One account's side of an internal ledger transaction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerTransaction {
    /// Identifier of the originating [`Transaction`]
    pub id: String,
    /// Date the transaction was booked
    pub date: NaiveDate,
    /// Absolute amount of this account's net movement
    pub amount: BigDecimal,
    /// Description of the transaction
    pub description: String,
    /// Reference number (invoice number, cheque number, ...)
    pub reference: Option<String>,
    /// Direction of this account's net movement
    pub entry_type: EntryType,
    /// Account this leg belongs to
    pub account_id: String,
}

impl LedgerTransaction {
    /// Create a new ledger transaction leg
    pub fn new(
        id: String,
        date: NaiveDate,
        amount: BigDecimal,
        description: String,
        entry_type: EntryType,
        account_id: String,
    ) -> Self {
        Self {
            id,
            date,
            amount,
            description,
            reference: None,
            entry_type,
            account_id,
        }
    }

    /// Attach a reference number
    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    /// Project a double-entry [`Transaction`] onto a single account's leg.
    ///
    /// Reconciling a bank statement means comparing it against the bank account's side of each
    /// transaction, so the account's debit and credit entries are netted into one signed movement.
    /// Returns `None` when the transaction has no entry for `account_id`. A leg that nets to zero
    /// is reported as a zero-amount [`EntryType::Debit`].
    ///
    /// ```
    /// use accounting_core::reconciliation::LedgerTransaction;
    /// use accounting_core::{Entry, EntryType, Transaction};
    /// use bigdecimal::BigDecimal;
    /// use chrono::NaiveDate;
    ///
    /// let date = NaiveDate::from_ymd_opt(2024, 11, 15).unwrap();
    /// let mut txn = Transaction::new("t1".into(), date, "Customer payment".into(), None);
    /// txn.add_entry(Entry::debit("bank".into(), BigDecimal::from(1000), None));
    /// txn.add_entry(Entry::credit("sales".into(), BigDecimal::from(1000), None));
    ///
    /// let leg = LedgerTransaction::from_transaction(&txn, "bank").unwrap();
    /// assert_eq!(leg.amount, BigDecimal::from(1000));
    /// assert_eq!(leg.entry_type, EntryType::Debit);
    /// assert!(LedgerTransaction::from_transaction(&txn, "petty-cash").is_none());
    /// ```
    pub fn from_transaction(transaction: &Transaction, account_id: &str) -> Option<Self> {
        let mut net = BigDecimal::from(0);
        let mut seen = false;

        for entry in &transaction.entries {
            if entry.account_id != account_id {
                continue;
            }
            seen = true;
            match entry.entry_type {
                EntryType::Debit => net += &entry.amount,
                EntryType::Credit => net -= &entry.amount,
            }
        }

        if !seen {
            return None;
        }

        let entry_type = if net < *crate::ZERO {
            EntryType::Credit
        } else {
            EntryType::Debit
        };

        Some(Self {
            id: transaction.id.clone(),
            date: transaction.date,
            amount: net.abs(),
            description: transaction.description.clone(),
            reference: transaction.reference.clone(),
            entry_type,
            account_id: account_id.to_string(),
        })
    }
}

/// A specific way in which two transactions fail to agree
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MatchDifference {
    /// The two records are dated differently
    DateDifference {
        /// Date on the ledger side
        ledger_date: NaiveDate,
        /// Date on the external side
        external_date: NaiveDate,
        /// Absolute gap in days
        days_diff: i64,
    },
    /// The two records carry different amounts
    AmountDifference {
        /// Amount on the ledger side
        ledger_amount: BigDecimal,
        /// Amount on the external side
        external_amount: BigDecimal,
        /// Absolute gap
        difference: BigDecimal,
    },
    /// The descriptions are not similar enough
    DescriptionDifference {
        /// Description on the ledger side
        ledger_description: String,
        /// Description on the external side
        external_description: String,
        /// Similarity in `0.0..=1.0`
        similarity: f64,
    },
    /// Both sides carry a reference number and they disagree
    ReferenceMismatch {
        /// Reference on the ledger side
        ledger_reference: Option<String>,
        /// Reference on the external side
        external_reference: Option<String>,
    },
    /// The records move in opposite directions
    EntryTypeMismatch {
        /// Direction on the ledger side
        ledger_entry_type: EntryType,
        /// Direction on the external side
        external_entry_type: EntryType,
    },
}

impl std::fmt::Display for MatchDifference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DateDifference {
                ledger_date,
                external_date,
                days_diff,
            } => write!(
                f,
                "dated {ledger_date} in the ledger but {external_date} externally, {days_diff} day(s) apart"
            ),
            Self::AmountDifference {
                ledger_amount,
                external_amount,
                difference,
            } => write!(
                f,
                "amount {ledger_amount} in the ledger but {external_amount} externally, off by {difference}"
            ),
            Self::DescriptionDifference {
                ledger_description,
                external_description,
                similarity,
            } => write!(
                f,
                "descriptions are {:.0}% alike: \"{ledger_description}\" vs \"{external_description}\"",
                similarity * 100.0
            ),
            Self::ReferenceMismatch {
                ledger_reference,
                external_reference,
            } => write!(
                f,
                "reference {} in the ledger but {} externally",
                ledger_reference.as_deref().unwrap_or("(none)"),
                external_reference.as_deref().unwrap_or("(none)")
            ),
            Self::EntryTypeMismatch {
                ledger_entry_type,
                external_entry_type,
            } => write!(
                f,
                "recorded as a {ledger_entry_type:?} in the ledger but a {external_entry_type:?} externally"
            ),
        }
    }
}

/// A scored candidate for an unmatched transaction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartialMatch {
    /// Identifier of the counterpart on the other side
    pub counterpart_id: String,
    /// Score in `0.0..=1.0`
    pub match_score: f64,
    /// Why the two do not agree
    pub differences: Vec<MatchDifference>,
}

/// Outcome for a single transaction, or a single pairing of transactions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReconciliationStatus {
    /// The two records agree closely enough to be treated as the same transaction
    Matched {
        /// Ledger transaction identifier
        ledger_id: String,
        /// External transaction identifier
        external_id: String,
        /// Score in `0.0..=1.0`
        match_score: f64,
    },
    /// Present in the ledger only
    UnmatchedLedger {
        /// Ledger transaction identifier
        ledger_id: String,
        /// Best external candidates, highest score first
        possible_matches: Vec<PartialMatch>,
    },
    /// Present in the external source only
    UnmatchedExternal {
        /// External transaction identifier
        external_id: String,
        /// Best ledger candidates, highest score first
        possible_matches: Vec<PartialMatch>,
    },
    /// Almost certainly the same transaction, but with differences worth a human's attention
    PartialMatch {
        /// Ledger transaction identifier
        ledger_id: String,
        /// External transaction identifier
        external_id: String,
        /// Score in `0.0..=1.0`
        match_score: f64,
        /// Why the two do not agree
        differences: Vec<MatchDifference>,
        /// Whether the score is high enough to accept without review
        auto_resolvable: bool,
    },
}

/// Aggregate figures for a reconciliation run
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationSummary {
    /// Net movement across the ledger transactions, credit-positive
    pub ledger_balance: BigDecimal,
    /// Net movement across the external transactions, credit-positive
    pub external_balance: BigDecimal,
    /// `ledger_balance - external_balance`
    pub difference: BigDecimal,
    /// Fraction of transactions fully matched, in `0.0..=1.0`
    pub match_rate: f64,
    /// Combined match rate and balance agreement, in `0.0..=1.0`
    pub confidence_score: f64,
}

/// The full result of a reconciliation run
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationReport {
    /// Identifier for this run
    pub id: Uuid,
    /// When the run happened (UTC)
    pub created_at: NaiveDateTime,
    /// Earliest date across both sides
    pub period_start: NaiveDate,
    /// Latest date across both sides
    pub period_end: NaiveDate,
    /// Where the external transactions came from
    pub external_source: ExternalSource,
    /// Distinct ledger accounts covered by this run, sorted
    pub account_ids: Vec<String>,
    /// How many ledger transactions went in
    pub total_ledger_transactions: usize,
    /// How many external transactions went in
    pub total_external_transactions: usize,
    /// Count of [`ReconciliationStatus::Matched`] items
    pub matched_count: usize,
    /// Count of [`ReconciliationStatus::UnmatchedLedger`] items
    pub unmatched_ledger_count: usize,
    /// Count of [`ReconciliationStatus::UnmatchedExternal`] items
    pub unmatched_external_count: usize,
    /// Count of [`ReconciliationStatus::PartialMatch`] items
    pub partial_match_count: usize,
    /// One entry per matched pair and per unmatched transaction
    pub reconciliation_items: Vec<ReconciliationStatus>,
    /// Aggregate figures
    pub summary: ReconciliationSummary,
}

impl ReconciliationReport {
    /// Items that need a human decision: partial matches and unmatched transactions
    pub fn needs_review(&self) -> impl Iterator<Item = &ReconciliationStatus> {
        self.reconciliation_items
            .iter()
            .filter(|item| !matches!(item, ReconciliationStatus::Matched { .. }))
    }

    /// Whether every transaction on both sides was fully matched
    pub fn is_fully_reconciled(&self) -> bool {
        self.unmatched_ledger_count == 0
            && self.unmatched_external_count == 0
            && self.partial_match_count == 0
    }
}

/// Errors raised by reconciliation storage and importers
#[derive(Debug, thiserror::Error)]
pub enum ReconciliationError {
    /// The storage backend failed
    #[error("Storage error: {0}")]
    Storage(String),
    /// External data could not be understood
    #[error("Invalid transaction data: {0}")]
    InvalidData(String),
    /// The requested reconciliation report does not exist
    #[error("Reconciliation not found")]
    NotFound,
}

/// Result type for reconciliation operations
pub type ReconciliationResult<T> = Result<T, ReconciliationError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap()
    }

    fn transaction() -> Transaction {
        let mut transaction = Transaction::new(
            "txn-1".to_string(),
            date(),
            "Customer payment".to_string(),
            Some("INV-9".to_string()),
        );
        transaction.add_entry(Entry::debit("bank".into(), BigDecimal::from(1000), None));
        transaction.add_entry(Entry::credit("sales".into(), BigDecimal::from(1000), None));
        transaction
    }

    #[test]
    fn test_from_transaction_projects_the_debit_leg() {
        let leg = LedgerTransaction::from_transaction(&transaction(), "bank").unwrap();

        assert_eq!(leg.id, "txn-1");
        assert_eq!(leg.date, date());
        assert_eq!(leg.amount, BigDecimal::from(1000));
        assert_eq!(leg.entry_type, EntryType::Debit);
        assert_eq!(leg.account_id, "bank");
        assert_eq!(leg.reference.as_deref(), Some("INV-9"));
    }

    #[test]
    fn test_from_transaction_projects_the_credit_leg() {
        let leg = LedgerTransaction::from_transaction(&transaction(), "sales").unwrap();

        assert_eq!(leg.amount, BigDecimal::from(1000));
        assert_eq!(leg.entry_type, EntryType::Credit);
    }

    #[test]
    fn test_from_transaction_nets_repeated_entries() {
        let mut txn = Transaction::new(
            "txn-2".to_string(),
            date(),
            "Split settlement".to_string(),
            None,
        );
        txn.add_entry(Entry::debit("bank".into(), BigDecimal::from(1000), None));
        txn.add_entry(Entry::credit("bank".into(), BigDecimal::from(150), None));
        txn.add_entry(Entry::credit("sales".into(), BigDecimal::from(850), None));

        let leg = LedgerTransaction::from_transaction(&txn, "bank").unwrap();
        assert_eq!(leg.amount, BigDecimal::from(850));
        assert_eq!(leg.entry_type, EntryType::Debit);
    }

    #[test]
    fn test_from_transaction_without_the_account() {
        assert!(LedgerTransaction::from_transaction(&transaction(), "petty-cash").is_none());
    }

    #[test]
    fn test_from_transaction_with_a_leg_that_nets_to_zero() {
        let mut txn = Transaction::new("txn-3".to_string(), date(), "Contra".to_string(), None);
        txn.add_entry(Entry::debit("bank".into(), BigDecimal::from(500), None));
        txn.add_entry(Entry::credit("bank".into(), BigDecimal::from(500), None));

        let leg = LedgerTransaction::from_transaction(&txn, "bank").unwrap();
        assert_eq!(leg.amount, BigDecimal::from(0));
        assert_eq!(leg.entry_type, EntryType::Debit);
    }

    #[test]
    fn test_needs_review_skips_full_matches() {
        let items = vec![
            ReconciliationStatus::Matched {
                ledger_id: "txn-1".to_string(),
                external_id: "ext-1".to_string(),
                match_score: 1.0,
            },
            ReconciliationStatus::UnmatchedLedger {
                ledger_id: "txn-2".to_string(),
                possible_matches: Vec::new(),
            },
        ];

        let report = ReconciliationReport {
            id: Uuid::new_v4(),
            created_at: chrono::Utc::now().naive_utc(),
            period_start: date(),
            period_end: date(),
            external_source: ExternalSource::Upi {
                provider: "Test".to_string(),
            },
            account_ids: vec!["bank".to_string()],
            total_ledger_transactions: 2,
            total_external_transactions: 1,
            matched_count: 1,
            unmatched_ledger_count: 1,
            unmatched_external_count: 0,
            partial_match_count: 0,
            reconciliation_items: items,
            summary: ReconciliationSummary {
                ledger_balance: BigDecimal::from(0),
                external_balance: BigDecimal::from(0),
                difference: BigDecimal::from(0),
                match_rate: 0.5,
                confidence_score: 0.75,
            },
        };

        assert_eq!(report.needs_review().count(), 1);
        assert!(!report.is_fully_reconciled());
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn test_differences_render_for_humans() {
        let difference = MatchDifference::AmountDifference {
            ledger_amount: BigDecimal::from(4300),
            external_amount: BigDecimal::from(4299),
            difference: BigDecimal::from(1),
        };
        assert_eq!(
            difference.to_string(),
            "amount 4300 in the ledger but 4299 externally, off by 1"
        );

        let missing = MatchDifference::ReferenceMismatch {
            ledger_reference: Some("UTR-1".to_string()),
            external_reference: None,
        };
        assert_eq!(
            missing.to_string(),
            "reference UTR-1 in the ledger but (none) externally"
        );
    }
}
