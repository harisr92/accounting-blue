//! Pure balance calculations over accounts and transactions
//!
//! Nothing here touches storage: callers load the accounts and transactions, and these functions
//! derive balances from them.

use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::collections::HashMap;

use crate::types::{Account, AccountBalance, AccountType, EntryType, Transaction, TrialBalance};

/// Balance of `account` from the entries that post to it, measured on its normal side
pub fn account_balance<'a>(
    account: &Account,
    transactions: impl IntoIterator<Item = &'a Transaction>,
) -> BigDecimal {
    transactions
        .into_iter()
        .flat_map(|transaction| &transaction.entries)
        .filter(|entry| entry.account_id == account.id)
        .map(|entry| {
            account
                .account_type
                .balance_effect(entry.entry_type, &entry.amount)
        })
        .sum()
}

/// Debit-positive net movement per account id
fn net_by_account<'a>(
    transactions: impl IntoIterator<Item = &'a Transaction>,
) -> HashMap<&'a str, BigDecimal> {
    transactions
        .into_iter()
        .flat_map(|transaction| &transaction.entries)
        .fold(HashMap::new(), |mut net, entry| {
            *net.entry(entry.account_id.as_str()).or_default() +=
                entry.entry_type.signed(&entry.amount);
            net
        })
}

/// Trial balance as of a date, from every account and the transactions dated up to then
///
/// Transactions dated after `as_of_date` are ignored, so the full history may be passed in.
pub fn trial_balance(
    as_of_date: NaiveDate,
    accounts: impl IntoIterator<Item = Account>,
    transactions: &[Transaction],
) -> TrialBalance {
    let net = net_by_account(transactions.iter().filter(|t| t.date <= as_of_date));
    let zero = BigDecimal::from(0);

    let balances: HashMap<String, AccountBalance> = accounts
        .into_iter()
        .map(|account| {
            let movement = net.get(account.id.as_str()).unwrap_or(&zero);
            let balance = account
                .account_type
                .balance_effect(EntryType::Debit, movement);
            (
                account.id.clone(),
                AccountBalance::from_balance(account, &balance),
            )
        })
        .collect();

    let total_debits: BigDecimal = balances
        .values()
        .filter_map(|b| b.debit_balance.as_ref())
        .sum();
    let total_credits: BigDecimal = balances
        .values()
        .filter_map(|b| b.credit_balance.as_ref())
        .sum();

    TrialBalance {
        as_of_date,
        balances,
        is_balanced: total_debits == total_credits,
        total_debits,
        total_credits,
    }
}

/// Group account balances by account type
pub fn group_by_type(
    balances: impl IntoIterator<Item = AccountBalance>,
) -> HashMap<AccountType, Vec<AccountBalance>> {
    balances
        .into_iter()
        .fold(HashMap::new(), |mut grouped, balance| {
            grouped
                .entry(balance.account.account_type)
                .or_insert_with(Vec::new)
                .push(balance);
            grouped
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, d).unwrap()
    }

    fn sale(id: &str, date: NaiveDate, amount: i32) -> Transaction {
        let mut txn = Transaction::new(id, date, "Sale", None);
        txn.add_entry(Entry::debit("cash", BigDecimal::from(amount), None));
        txn.add_entry(Entry::credit("sales", BigDecimal::from(amount), None));
        txn
    }

    #[test]
    fn test_account_balance_uses_the_normal_side() {
        let txns = [sale("t1", day(1), 100), sale("t2", day(2), 50)];
        let cash = Account::new("cash", "Cash", AccountType::Asset, None);
        let sales = Account::new("sales", "Sales", AccountType::Income, None);

        assert_eq!(account_balance(&cash, &txns), BigDecimal::from(150));
        assert_eq!(account_balance(&sales, &txns), BigDecimal::from(150));
    }

    #[test]
    fn test_trial_balance_ignores_later_transactions() {
        let txns = [sale("t1", day(1), 100), sale("t2", day(5), 50)];
        let accounts = [
            Account::new("cash", "Cash", AccountType::Asset, None),
            Account::new("sales", "Sales", AccountType::Income, None),
            Account::new("idle", "Idle", AccountType::Expense, None),
        ];

        let tb = trial_balance(day(3), accounts, &txns);

        assert!(tb.is_balanced);
        assert_eq!(tb.total_debits, BigDecimal::from(100));
        assert_eq!(tb.total_credits, BigDecimal::from(100));
        assert_eq!(tb.balances["idle"].balance_amount(), BigDecimal::from(0));
    }
}
