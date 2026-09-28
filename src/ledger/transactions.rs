//! Transaction operations: recording, updating and deleting, keeping account balances in step

use crate::error::{LedgerError, LedgerResult};
use crate::ledger::accounts::require_account;
use crate::traits::{AccountStore, LedgerStorage, TransactionStore, TransactionValidator};
use crate::types::{Entry, Transaction};

/// Load a transaction, failing when it does not exist
pub(crate) async fn require_transaction<S: TransactionStore>(
    store: &S,
    transaction_id: &str,
) -> LedgerResult<Transaction> {
    store
        .get_transaction(transaction_id)
        .await?
        .ok_or_else(|| LedgerError::TransactionNotFound(transaction_id.to_string()))
}

/// Fail unless every account the entries post to exists
async fn ensure_accounts_exist<S: AccountStore>(store: &S, entries: &[Entry]) -> LedgerResult<()> {
    for entry in entries {
        require_account(store, &entry.account_id).await?;
    }
    Ok(())
}

/// Apply each entry to its account's stored balance
async fn post_entries<S: AccountStore>(
    store: &mut S,
    entries: impl IntoIterator<Item = Entry>,
) -> LedgerResult<()> {
    for entry in entries {
        let mut account = require_account(store, &entry.account_id).await?;
        account.apply_entry(entry.entry_type, &entry.amount);
        store.update_account(&account).await?;
    }
    Ok(())
}

/// Validate a transaction, save it and apply it to its accounts
pub(crate) async fn record_transaction<S: LedgerStorage>(
    store: &mut S,
    validator: &dyn TransactionValidator,
    mut transaction: Transaction,
) -> LedgerResult<()> {
    validator.validate_transaction(&transaction)?;
    ensure_accounts_exist(store, &transaction.entries).await?;

    transaction.updated_at = chrono::Utc::now().naive_utc();
    store.save_transaction(&transaction).await?;
    post_entries(store, transaction.entries).await
}

/// Replace a transaction, reversing the old entries and applying the new ones
///
/// Every account on both versions must exist; nothing is changed otherwise.
pub(crate) async fn update_transaction<S: LedgerStorage>(
    store: &mut S,
    validator: &dyn TransactionValidator,
    transaction: &Transaction,
) -> LedgerResult<()> {
    let old = require_transaction(store, &transaction.id).await?;
    validator.validate_transaction(transaction)?;
    ensure_accounts_exist(store, &old.entries).await?;
    ensure_accounts_exist(store, &transaction.entries).await?;

    post_entries(store, old.entries.iter().map(Entry::reversed)).await?;
    post_entries(store, transaction.entries.iter().cloned()).await?;
    store.update_transaction(transaction).await
}

/// Delete a transaction and reverse its effect on account balances
pub(crate) async fn delete_transaction<S: LedgerStorage>(
    store: &mut S,
    transaction_id: &str,
) -> LedgerResult<()> {
    let old = require_transaction(store, transaction_id).await?;
    ensure_accounts_exist(store, &old.entries).await?;

    post_entries(store, old.entries.iter().map(Entry::reversed)).await?;
    store.delete_transaction(transaction_id).await
}
