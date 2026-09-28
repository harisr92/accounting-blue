//! Validation rules and the validators built from them
//!
//! Each rule is a small function returning [`LedgerResult<()>`]. The validators are compositions
//! of those rules: [`DefaultAccountValidator`] and [`DefaultTransactionValidator`] enforce only
//! what bookkeeping requires, while the `Enhanced` validators add length and character limits
//! suitable for ids stored in a database.

use std::collections::HashSet;

use crate::error::{FieldError, LedgerError, LedgerResult};
use crate::traits::{AccountValidator, TransactionValidator};
use crate::types::{Account, Transaction};

/// Longest account id the enhanced rules accept
pub const MAX_ACCOUNT_ID_LEN: usize = 50;
/// Longest account name the enhanced rules accept
pub const MAX_ACCOUNT_NAME_LEN: usize = 100;
/// Longest transaction description the enhanced rules accept
pub const MAX_DESCRIPTION_LEN: usize = 500;

fn field_error(field: &'static str, error: FieldError) -> LedgerError {
    LedgerError::InvalidField { field, error }
}

/// Reject empty or whitespace-only text
///
/// # Errors
///
/// [`FieldError::Empty`] for `field`.
pub fn require_non_empty(field: &'static str, value: &str) -> LedgerResult<()> {
    if value.trim().is_empty() {
        Err(field_error(field, FieldError::Empty))
    } else {
        Ok(())
    }
}

/// Reject text longer than `max` bytes
///
/// # Errors
///
/// [`FieldError::TooLong`] for `field`.
pub fn require_max_len(field: &'static str, value: &str, max: usize) -> LedgerResult<()> {
    if value.len() > max {
        Err(field_error(field, FieldError::TooLong { max }))
    } else {
        Ok(())
    }
}

/// Validate that an account ID is non-empty, short, and made of alphanumerics, `-` and `_`
///
/// # Errors
///
/// [`LedgerError::InvalidField`] naming the first rule broken.
pub fn validate_account_id(account_id: &str) -> LedgerResult<()> {
    const FIELD: &str = "account ID";
    require_non_empty(FIELD, account_id)?;
    require_max_len(FIELD, account_id, MAX_ACCOUNT_ID_LEN)?;

    if account_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        Ok(())
    } else {
        Err(field_error(FIELD, FieldError::InvalidCharacters))
    }
}

/// Validate that an account name is non-empty and at most [`MAX_ACCOUNT_NAME_LEN`] bytes
///
/// # Errors
///
/// [`LedgerError::InvalidField`] naming the first rule broken.
pub fn validate_account_name(name: &str) -> LedgerResult<()> {
    const FIELD: &str = "account name";
    require_non_empty(FIELD, name)?;
    require_max_len(FIELD, name, MAX_ACCOUNT_NAME_LEN)
}

/// Validate that a transaction description is non-empty and at most [`MAX_DESCRIPTION_LEN`]
/// bytes
///
/// # Errors
///
/// [`LedgerError::InvalidField`] naming the first rule broken.
pub fn validate_transaction_description(description: &str) -> LedgerResult<()> {
    const FIELD: &str = "transaction description";
    require_non_empty(FIELD, description)?;
    require_max_len(FIELD, description, MAX_DESCRIPTION_LEN)
}

/// Reject a transaction that posts to the same account twice on the same side
///
/// # Errors
///
/// [`LedgerError::DuplicateEntry`] naming the repeated account.
pub fn validate_unique_entries(transaction: &Transaction) -> LedgerResult<()> {
    let mut seen = HashSet::new();
    transaction
        .entries
        .iter()
        .find(|entry| !seen.insert((&entry.account_id, entry.entry_type)))
        .map_or(Ok(()), |entry| {
            Err(LedgerError::DuplicateEntry(entry.account_id.clone()))
        })
}

/// Requires a non-empty account id and name
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultAccountValidator;

impl AccountValidator for DefaultAccountValidator {
    fn validate_account(&self, account: &Account) -> LedgerResult<()> {
        require_non_empty("account ID", &account.id)?;
        require_non_empty("account name", &account.name)
    }
}

/// Enforces the double-entry rules of [`Transaction::validate`]
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultTransactionValidator;

impl TransactionValidator for DefaultTransactionValidator {
    fn validate_transaction(&self, transaction: &Transaction) -> LedgerResult<()> {
        transaction.validate()
    }
}

/// Adds id format and length limits to the default account rules
#[derive(Debug, Clone, Copy, Default)]
pub struct EnhancedAccountValidator;

impl AccountValidator for EnhancedAccountValidator {
    fn validate_account(&self, account: &Account) -> LedgerResult<()> {
        validate_account_id(&account.id)?;
        validate_account_name(&account.name)
    }
}

/// Adds description, account id and duplicate-entry checks to the double-entry rules
#[derive(Debug, Clone, Copy, Default)]
pub struct EnhancedTransactionValidator;

impl TransactionValidator for EnhancedTransactionValidator {
    fn validate_transaction(&self, transaction: &Transaction) -> LedgerResult<()> {
        transaction.validate()?;
        validate_transaction_description(&transaction.description)?;
        transaction
            .entries
            .iter()
            .try_for_each(|entry| validate_account_id(&entry.account_id))?;
        validate_unique_entries(transaction)
    }
}
