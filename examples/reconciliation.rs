//! Bank reconciliation example
//!
//! Books four transactions against a bank account, then reconciles that account's side of the
//! ledger against a statement that agrees with it only partly: one row settles a day late, one
//! row is short by a rupee, and the two sides each carry a transaction the other has never seen.

use accounting_core::reconciliation::{
    ExternalSource, ExternalTransaction, LedgerTransaction, ReconciliationConfig,
    ReconciliationEngine, ReconciliationStatus,
};
use accounting_core::utils::MemoryStorage;
use accounting_core::{AccountType, Entry, EntryType, Ledger, Transaction};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

const BANK: &str = "bank";

fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 11, day).unwrap()
}

fn rupees(amount: &str) -> BigDecimal {
    BigDecimal::from_str(amount).expect("valid amount")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏦 Accounting Core - Bank Reconciliation Example\n");

    let mut ledger = Ledger::new(MemoryStorage::new());
    ledger
        .create_account(
            BANK.to_string(),
            "Bank Account".to_string(),
            AccountType::Asset,
            None,
        )
        .await?;
    ledger
        .create_account(
            "sales".to_string(),
            "Sales".to_string(),
            AccountType::Income,
            None,
        )
        .await?;
    ledger
        .create_account(
            "rent".to_string(),
            "Rent Expense".to_string(),
            AccountType::Expense,
            None,
        )
        .await?;

    // 1. Book the internal side
    println!("📒 Recording ledger transactions...");
    let bookings = [
        (
            "txn-001",
            15,
            "1000",
            "Payment from Acme Ltd",
            "sales",
            true,
            Some("UTR-88421"),
        ),
        (
            "txn-002",
            16,
            "250",
            "Office rent November",
            "rent",
            false,
            None,
        ),
        (
            "txn-003",
            18,
            "4300",
            "Payment from Globex Inc",
            "sales",
            true,
            None,
        ),
        (
            "txn-004",
            20,
            "615",
            "Payment from Initech",
            "sales",
            true,
            None,
        ),
    ];

    for (id, date, amount, description, counterpart, incoming, reference) in bookings {
        let mut transaction = Transaction::new(
            id.to_string(),
            day(date),
            description.to_string(),
            reference.map(str::to_string),
        );
        let amount = rupees(amount);
        if incoming {
            transaction.add_entry(Entry::debit(BANK.to_string(), amount.clone(), None));
            transaction.add_entry(Entry::credit(counterpart.to_string(), amount, None));
        } else {
            transaction.add_entry(Entry::debit(counterpart.to_string(), amount.clone(), None));
            transaction.add_entry(Entry::credit(BANK.to_string(), amount, None));
        }
        println!("  ✓ {id}  {description}");
        ledger.record_transaction(transaction).await?;
    }
    println!();

    // 2. Project the bank account's leg out of each double-entry transaction
    let ledger_transactions: Vec<LedgerTransaction> = ledger
        .get_all_account_transactions(BANK, Some(day(1)), Some(day(30)))
        .await?
        .iter()
        .filter_map(|transaction| LedgerTransaction::from_transaction(transaction, BANK))
        .collect();

    // 3. The statement as the bank sees it, with directions flipped to the ledger's point of view
    let source = ExternalSource::BankStatement {
        bank_name: "SBI".to_string(),
        account_number: "12345678901".to_string(),
    };
    let external_transactions = vec![
        // Agrees exactly
        ExternalTransaction::new(
            "stmt-01".to_string(),
            day(15),
            rupees("1000"),
            "NEFT/ACME LTD/UTR-88421".to_string(),
            EntryType::Debit,
            source.clone(),
        )
        .with_reference("UTR-88421"),
        // Settled a day after it was booked. Money leaving the bank is a credit in the ledger's
        // bank account, whichever way round the statement itself prints it.
        ExternalTransaction::new(
            "stmt-02".to_string(),
            day(17),
            rupees("250"),
            "Office rent November".to_string(),
            EntryType::Credit,
            source.clone(),
        ),
        // A rupee short of the ledger
        ExternalTransaction::new(
            "stmt-03".to_string(),
            day(18),
            rupees("4299"),
            "Payment from Globex Inc".to_string(),
            EntryType::Debit,
            source.clone(),
        ),
        // Never made it into the books
        ExternalTransaction::new(
            "stmt-04".to_string(),
            day(21),
            rupees("120"),
            "Quarterly account maintenance fee".to_string(),
            EntryType::Credit,
            source.clone(),
        ),
    ];

    // 4. Reconcile
    let engine = ReconciliationEngine::new(ReconciliationConfig::default());
    let report = engine.reconcile(ledger_transactions, external_transactions, source);

    println!("📋 Reconciliation report {}", report.id);
    println!(
        "  Period            : {} to {}",
        report.period_start, report.period_end
    );
    println!(
        "  Ledger / statement : {} / {}",
        report.total_ledger_transactions, report.total_external_transactions
    );
    println!("  Matched            : {}", report.matched_count);
    println!("  Partial matches    : {}", report.partial_match_count);
    println!("  Ledger only        : {}", report.unmatched_ledger_count);
    println!("  Statement only     : {}", report.unmatched_external_count);
    println!(
        "  Match rate         : {:.1}%",
        report.summary.match_rate * 100.0
    );
    println!(
        "  Confidence         : {:.1}%",
        report.summary.confidence_score * 100.0
    );
    println!("  Balance difference : {}", report.summary.difference);
    println!();

    println!("🔍 Needs a person's attention:");
    for item in report.needs_review() {
        match item {
            ReconciliationStatus::PartialMatch {
                ledger_id,
                external_id,
                match_score,
                differences,
                auto_resolvable,
            } => {
                let flag = if *auto_resolvable {
                    "auto-resolvable"
                } else {
                    "review"
                };
                println!(
                    "  ~ {ledger_id} <-> {external_id}  ({:.0}%, {flag})",
                    match_score * 100.0
                );
                for difference in differences {
                    println!("      - {difference}");
                }
            }
            ReconciliationStatus::UnmatchedLedger {
                ledger_id,
                possible_matches,
            } => {
                println!("  ! {ledger_id} is in the ledger only");
                for candidate in possible_matches {
                    println!(
                        "      ? closest statement row {} at {:.0}%",
                        candidate.counterpart_id,
                        candidate.match_score * 100.0
                    );
                }
            }
            ReconciliationStatus::UnmatchedExternal {
                external_id,
                possible_matches,
            } => {
                println!("  ! {external_id} is on the statement only");
                for candidate in possible_matches {
                    println!(
                        "      ? closest ledger row {} at {:.0}%",
                        candidate.counterpart_id,
                        candidate.match_score * 100.0
                    );
                }
            }
            ReconciliationStatus::Matched { .. } => unreachable!("filtered out by needs_review"),
        }
    }

    Ok(())
}
