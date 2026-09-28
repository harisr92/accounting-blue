//! Fluent construction of validated transactions

use bigdecimal::BigDecimal;
use chrono::NaiveDate;

use crate::error::LedgerResult;
use crate::types::{Entry, Transaction};

/// Transaction builder for creating complex transactions
#[derive(Debug)]
pub struct TransactionBuilder {
    transaction: Transaction,
}

impl TransactionBuilder {
    /// Start a transaction with no entries
    pub fn new(id: impl Into<String>, date: NaiveDate, description: impl Into<String>) -> Self {
        Self {
            transaction: Transaction::new(id, date, description, None),
        }
    }

    /// Set the reference for the transaction
    #[must_use]
    pub fn reference(mut self, reference: impl Into<String>) -> Self {
        self.transaction.reference = Some(reference.into());
        self
    }

    /// Add metadata to the transaction
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.transaction.metadata.insert(key.into(), value.into());
        self
    }

    /// Add a debit entry
    #[must_use]
    pub fn debit(
        self,
        account_id: impl Into<String>,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        self.entry(Entry::debit(account_id, amount, description))
    }

    /// Add a credit entry
    #[must_use]
    pub fn credit(
        self,
        account_id: impl Into<String>,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        self.entry(Entry::credit(account_id, amount, description))
    }

    /// Add a custom entry
    #[must_use]
    pub fn entry(mut self, entry: Entry) -> Self {
        self.transaction.add_entry(entry);
        self
    }

    /// Validate and return the transaction
    ///
    /// # Errors
    ///
    /// Any double-entry rule from [`Transaction::validate`].
    pub fn build(self) -> LedgerResult<Transaction> {
        self.transaction.validate()?;
        Ok(self.transaction)
    }
}
