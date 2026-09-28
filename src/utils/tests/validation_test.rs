use crate::error::{FieldError, LedgerError};
use crate::traits::{AccountValidator, TransactionValidator};
use crate::types::{Account, AccountType, Entry, Transaction};
use crate::utils::validation::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

#[test]
fn test_account_id_rules() {
    assert!(validate_account_id("cash-01_a").is_ok());
    assert!(matches!(
        validate_account_id("  "),
        Err(LedgerError::InvalidField {
            error: FieldError::Empty,
            ..
        })
    ));
    assert!(matches!(
        validate_account_id(&"a".repeat(51)),
        Err(LedgerError::InvalidField {
            error: FieldError::TooLong { max: 50 },
            ..
        })
    ));
    assert!(matches!(
        validate_account_id("cash!"),
        Err(LedgerError::InvalidField {
            error: FieldError::InvalidCharacters,
            ..
        })
    ));
}

#[test]
fn test_default_account_validator_only_requires_text() {
    let account = Account::new("any id!", "Name", AccountType::Asset, None);
    assert!(DefaultAccountValidator.validate_account(&account).is_ok());
    assert!(EnhancedAccountValidator.validate_account(&account).is_err());
}

#[test]
fn test_enhanced_transaction_rejects_duplicate_entries() {
    let date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let mut txn = Transaction::new("t1", date, "Split", None);
    txn.add_entry(Entry::debit("cash", BigDecimal::from(50), None));
    txn.add_entry(Entry::debit("cash", BigDecimal::from(50), None));
    txn.add_entry(Entry::credit("sales", BigDecimal::from(100), None));

    assert!(DefaultTransactionValidator
        .validate_transaction(&txn)
        .is_ok());
    assert!(matches!(
        EnhancedTransactionValidator.validate_transaction(&txn),
        Err(LedgerError::DuplicateEntry(id)) if id == "cash"
    ));
}
