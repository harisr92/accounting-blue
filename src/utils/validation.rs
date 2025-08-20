//! Validation utilities

use crate::traits::*;
use crate::types::*;
use bigdecimal::BigDecimal;

/// Validate that an amount is positive
pub fn validate_positive_amount(amount: &BigDecimal) -> LedgerResult<()> {
    if *amount <= BigDecimal::from(0) {
        Err(LedgerError::Validation(
            "Amount must be positive".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Validate that an account ID is valid
pub fn validate_account_id(account_id: &str) -> LedgerResult<()> {
    if account_id.trim().is_empty() {
        return Err(LedgerError::Validation(
            "Account ID cannot be empty".to_string(),
        ));
    }

    if account_id.len() > 50 {
        return Err(LedgerError::Validation(
            "Account ID cannot exceed 50 characters".to_string(),
        ));
    }

    // Check for valid characters (alphanumeric, dashes, underscores)
    if !account_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(LedgerError::Validation(
            "Account ID can only contain alphanumeric characters, dashes, and underscores"
                .to_string(),
        ));
    }

    Ok(())
}

/// Validate that an account name is valid
pub fn validate_account_name(name: &str) -> LedgerResult<()> {
    if name.trim().is_empty() {
        return Err(LedgerError::Validation(
            "Account name cannot be empty".to_string(),
        ));
    }

    if name.len() > 100 {
        return Err(LedgerError::Validation(
            "Account name cannot exceed 100 characters".to_string(),
        ));
    }

    Ok(())
}

/// Validate that a transaction description is valid
pub fn validate_transaction_description(description: &str) -> LedgerResult<()> {
    if description.trim().is_empty() {
        return Err(LedgerError::Validation(
            "Transaction description cannot be empty".to_string(),
        ));
    }

    if description.len() > 500 {
        return Err(LedgerError::Validation(
            "Transaction description cannot exceed 500 characters".to_string(),
        ));
    }

    Ok(())
}

/// Enhanced transaction validator with detailed checks
pub struct EnhancedTransactionValidator;

impl TransactionValidator for EnhancedTransactionValidator {
    fn validate_transaction(&self, transaction: &Transaction) -> LedgerResult<()> {
        // Basic validation
        transaction.validate()?;

        // Enhanced validations
        validate_transaction_description(&transaction.description)?;

        // Validate each entry
        for entry in &transaction.entries {
            validate_account_id(&entry.account_id)?;
            validate_positive_amount(&entry.amount)?;
        }

        // Check for duplicate accounts (same account cannot appear twice with same entry type)
        let mut account_entry_combinations = std::collections::HashSet::new();
        for entry in &transaction.entries {
            let combination = (&entry.account_id, &entry.entry_type);
            if !account_entry_combinations.insert(combination) {
                return Err(LedgerError::Validation(format!(
                    "Account '{}' appears multiple times with the same entry type in transaction",
                    entry.account_id
                )));
            }
        }

        Ok(())
    }

    fn validate_account_references(&self, _transaction: &Transaction) -> LedgerResult<()> {
        // This would typically check if accounts exist in storage
        // For this basic implementation, we assume all accounts exist
        Ok(())
    }
}

/// Enhanced account validator with detailed checks
pub struct EnhancedAccountValidator;

impl AccountValidator for EnhancedAccountValidator {
    fn validate_account(&self, account: &Account) -> LedgerResult<()> {
        validate_account_id(&account.id)?;
        validate_account_name(&account.name)?;

        // Additional validations can be added here
        Ok(())
    }

    fn validate_account_deletion(&self, _account_id: &str) -> LedgerResult<()> {
        // This would typically check if account has any transactions
        // For this basic implementation, we allow deletion
        Ok(())
    }
}
