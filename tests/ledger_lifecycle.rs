//! Account and transaction lifecycles through the `Ledger`: create, update, delete, and the
//! running balances they leave behind

use accounting_core::{
    utils::MemoryStorage, AccountStore, AccountType, Ledger, LedgerError, LedgerResult,
    TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn date(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, month, day).unwrap()
}

/// A ledger with cash, bank, revenue and rent accounts
async fn ledger_with_accounts() -> LedgerResult<Ledger<MemoryStorage>> {
    let mut ledger = Ledger::new(MemoryStorage::new());
    ledger
        .create_account("cash", "Cash", AccountType::Asset, None)
        .await?;
    ledger
        .create_account("bank", "Bank", AccountType::Asset, None)
        .await?;
    ledger
        .create_account("revenue", "Revenue", AccountType::Income, None)
        .await?;
    ledger
        .create_account("rent", "Rent", AccountType::Expense, None)
        .await?;
    Ok(ledger)
}

async fn balance(ledger: &Ledger<MemoryStorage>, account_id: &str) -> BigDecimal {
    ledger.get_account_balance(account_id, None).await.unwrap()
}

#[tokio::test]
async fn test_updated_account_keeps_its_balance_and_type() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let sale = TransactionBuilder::new("sale", date(1, 5), "Cash sale")
        .debit("cash", BigDecimal::from(500), None)
        .credit("revenue", BigDecimal::from(500), None)
        .build()?;
    ledger.record_transaction(sale).await?;

    let mut cash = ledger.get_account("cash").await?.unwrap();
    cash.name = "Petty Cash".to_string();
    ledger.update_account(&cash).await?;

    let stored = ledger.get_account("cash").await?.unwrap();
    assert_eq!(stored.name, "Petty Cash");
    assert_eq!(stored.account_type, AccountType::Asset);
    assert_eq!(stored.balance, BigDecimal::from(500));
    Ok(())
}

#[tokio::test]
async fn test_updating_or_deleting_a_missing_account_fails() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let ghost = accounting_core::Account::new("ghost", "Ghost", AccountType::Asset, None);

    assert!(matches!(
        ledger.update_account(&ghost).await,
        Err(LedgerError::AccountNotFound(id)) if id == "ghost"
    ));
    assert!(matches!(
        ledger.delete_account("ghost").await,
        Err(LedgerError::AccountNotFound(id)) if id == "ghost"
    ));
    Ok(())
}

#[tokio::test]
async fn test_deleted_account_is_gone() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;

    ledger.delete_account("rent").await?;

    assert!(ledger.get_account("rent").await?.is_none());
    let ids: Vec<String> = ledger
        .list_all_accounts()
        .await?
        .into_iter()
        .map(|account| account.id)
        .collect();
    assert_eq!(ids, ["bank", "cash", "revenue"]);
    Ok(())
}

#[tokio::test]
async fn test_account_creation_rejects_duplicates_and_missing_parents() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;

    assert!(matches!(
        ledger
            .create_account("cash", "Cash again", AccountType::Asset, None)
            .await,
        Err(LedgerError::DuplicateAccount(id)) if id == "cash"
    ));
    assert!(matches!(
        ledger
            .create_account("till", "Till", AccountType::Asset, Some("drawer".to_string()))
            .await,
        Err(LedgerError::ParentNotFound(id)) if id == "drawer"
    ));
    assert!(ledger.get_account("till").await?.is_none());
    Ok(())
}

#[tokio::test]
async fn test_hierarchy_navigation_over_three_levels() -> LedgerResult<()> {
    let mut ledger = Ledger::new(MemoryStorage::new());
    ledger
        .create_account("assets", "Assets", AccountType::Asset, None)
        .await?;
    ledger
        .create_account(
            "current",
            "Current Assets",
            AccountType::Asset,
            Some("assets".to_string()),
        )
        .await?;
    ledger
        .create_account(
            "fixed",
            "Fixed Assets",
            AccountType::Asset,
            Some("assets".to_string()),
        )
        .await?;
    ledger
        .create_account(
            "cash",
            "Cash",
            AccountType::Asset,
            Some("current".to_string()),
        )
        .await?;

    let mut children: Vec<String> = ledger
        .child_accounts("assets")
        .await?
        .into_iter()
        .map(|account| account.id)
        .collect();
    children.sort();
    assert_eq!(children, ["current", "fixed"]);
    assert!(ledger.child_accounts("cash").await?.is_empty());

    let path: Vec<String> = ledger
        .account_path("cash")
        .await?
        .into_iter()
        .map(|account| account.id)
        .collect();
    assert_eq!(path, ["assets", "current", "cash"]);

    assert!(matches!(
        ledger.account_path("missing").await,
        Err(LedgerError::AccountNotFound(id)) if id == "missing"
    ));
    Ok(())
}

#[tokio::test]
async fn test_updating_a_transaction_reverses_the_old_entries() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let sale = TransactionBuilder::new("sale", date(1, 5), "Cash sale")
        .debit("cash", BigDecimal::from(1000), None)
        .credit("revenue", BigDecimal::from(1000), None)
        .build()?;
    ledger.record_transaction(sale).await?;

    // The sale was really 1200 and was paid into the bank, not cash
    let corrected = TransactionBuilder::new("sale", date(1, 5), "Bank sale")
        .debit("bank", BigDecimal::from(1200), None)
        .credit("revenue", BigDecimal::from(1200), None)
        .build()?;
    ledger.update_transaction(&corrected).await?;

    assert_eq!(balance(&ledger, "cash").await, BigDecimal::from(0));
    assert_eq!(balance(&ledger, "bank").await, BigDecimal::from(1200));
    assert_eq!(balance(&ledger, "revenue").await, BigDecimal::from(1200));

    let stored = ledger.get_transaction("sale").await?.unwrap();
    assert_eq!(stored.description, "Bank sale");
    assert!(ledger.get_trial_balance(date(1, 31)).await?.is_balanced);
    Ok(())
}

#[tokio::test]
async fn test_deleting_a_transaction_reverses_its_effect() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let sale = TransactionBuilder::new("sale", date(1, 5), "Cash sale")
        .debit("cash", BigDecimal::from(1000), None)
        .credit("revenue", BigDecimal::from(1000), None)
        .build()?;
    let rent = TransactionBuilder::new("rent-jan", date(1, 10), "January rent")
        .debit("rent", BigDecimal::from(300), None)
        .credit("cash", BigDecimal::from(300), None)
        .build()?;
    ledger.record_transaction(sale).await?;
    ledger.record_transaction(rent).await?;
    assert_eq!(balance(&ledger, "cash").await, BigDecimal::from(700));

    ledger.delete_transaction("rent-jan").await?;

    assert!(ledger.get_transaction("rent-jan").await?.is_none());
    assert_eq!(balance(&ledger, "cash").await, BigDecimal::from(1000));
    assert_eq!(balance(&ledger, "rent").await, BigDecimal::from(0));
    // The recomputed history agrees with the running balance
    assert_eq!(
        ledger
            .get_account_balance("cash", Some(date(1, 31)))
            .await?,
        BigDecimal::from(1000)
    );
    Ok(())
}

#[tokio::test]
async fn test_updating_or_deleting_a_missing_transaction_fails() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let ghost = TransactionBuilder::new("ghost", date(1, 5), "Never recorded")
        .debit("cash", BigDecimal::from(10), None)
        .credit("revenue", BigDecimal::from(10), None)
        .build()?;

    assert!(matches!(
        ledger.update_transaction(&ghost).await,
        Err(LedgerError::TransactionNotFound(id)) if id == "ghost"
    ));
    assert!(matches!(
        ledger.delete_transaction("ghost").await,
        Err(LedgerError::TransactionNotFound(id)) if id == "ghost"
    ));
    assert_eq!(balance(&ledger, "cash").await, BigDecimal::from(0));
    Ok(())
}

#[tokio::test]
async fn test_transaction_on_an_unknown_account_changes_nothing() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let sale = TransactionBuilder::new("sale", date(1, 5), "Cash sale")
        .debit("cash", BigDecimal::from(1000), None)
        .credit("revenue", BigDecimal::from(1000), None)
        .build()?;
    ledger.record_transaction(sale).await?;

    let stray = TransactionBuilder::new("stray", date(1, 6), "Posts to a missing account")
        .debit("cash", BigDecimal::from(50), None)
        .credit("sundry", BigDecimal::from(50), None)
        .build()?;
    assert!(matches!(
        ledger.record_transaction(stray).await,
        Err(LedgerError::AccountNotFound(id)) if id == "sundry"
    ));
    assert!(ledger.get_transaction("stray").await?.is_none());

    // An update that moves an entry to a missing account is rejected whole
    let bad_update = TransactionBuilder::new("sale", date(1, 5), "Cash sale")
        .debit("sundry", BigDecimal::from(1000), None)
        .credit("revenue", BigDecimal::from(1000), None)
        .build()?;
    assert!(matches!(
        ledger.update_transaction(&bad_update).await,
        Err(LedgerError::AccountNotFound(id)) if id == "sundry"
    ));

    assert_eq!(balance(&ledger, "cash").await, BigDecimal::from(1000));
    assert_eq!(balance(&ledger, "revenue").await, BigDecimal::from(1000));
    Ok(())
}

#[tokio::test]
async fn test_deleting_a_transaction_skips_accounts_deleted_since() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let rent = TransactionBuilder::new("rent-jan", date(1, 10), "January rent")
        .debit("rent", BigDecimal::from(300), None)
        .credit("cash", BigDecimal::from(300), None)
        .build()?;
    ledger.record_transaction(rent).await?;
    // The ledger refuses to delete a used account, but a backend can still lose one
    let mut storage = ledger.into_storage();
    storage.delete_account("rent").await?;
    let mut ledger = Ledger::new(storage);

    ledger.delete_transaction("rent-jan").await?;

    assert!(ledger.get_transaction("rent-jan").await?.is_none());
    assert_eq!(balance(&ledger, "cash").await, BigDecimal::from(0));
    Ok(())
}

#[tokio::test]
async fn test_an_account_with_transactions_cannot_be_deleted() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    let rent = TransactionBuilder::new("rent-jan", date(1, 10), "January rent")
        .debit("rent", BigDecimal::from(300), None)
        .credit("cash", BigDecimal::from(300), None)
        .build()?;
    ledger.record_transaction(rent).await?;

    assert!(matches!(
        ledger.delete_account("rent").await,
        Err(LedgerError::AccountHasTransactions(id)) if id == "rent"
    ));
    assert_eq!(balance(&ledger, "rent").await, BigDecimal::from(300));
    assert!(ledger.validate_integrity(date(1, 31)).await?.is_valid);

    // Once its transactions are gone, the account can go too
    ledger.delete_transaction("rent-jan").await?;
    ledger.delete_account("rent").await?;
    assert!(ledger.get_account("rent").await?.is_none());
    Ok(())
}

#[tokio::test]
async fn test_a_parent_account_cannot_be_deleted_before_its_children() -> LedgerResult<()> {
    let mut ledger = ledger_with_accounts().await?;
    ledger
        .create_account("till", "Till", AccountType::Asset, Some("cash".to_string()))
        .await?;

    assert!(matches!(
        ledger.delete_account("cash").await,
        Err(LedgerError::AccountHasChildren(id)) if id == "cash"
    ));
    assert!(ledger.get_account("cash").await?.is_some());

    ledger.delete_account("till").await?;
    ledger.delete_account("cash").await?;
    assert!(ledger.get_account("cash").await?.is_none());
    Ok(())
}
