//! Financial reports and GST posting patterns, end to end through the `Ledger`

use accounting_core::{
    patterns, utils::MemoryStorage, AccountType, BillPaymentWithGstParams, InvoiceWithGstParams,
    Ledger, LedgerError, LedgerResult, TransactionBuilder,
};
use bigdecimal::BigDecimal;
use chrono::NaiveDate;

fn date(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, month, day).unwrap()
}

/// Sorted account ids of a report section
fn section_ids(balances: &[accounting_core::AccountBalance]) -> Vec<String> {
    let mut ids: Vec<String> = balances.iter().map(|b| b.account.id.clone()).collect();
    ids.sort();
    ids
}

/// A small business's January: owner investment, a loan, an equipment purchase, a sale and rent,
/// plus a February sale outside the period. Account ids carry the keywords the cash flow
/// classifier reads.
async fn january_books() -> LedgerResult<Ledger<MemoryStorage>> {
    let mut ledger = Ledger::new(MemoryStorage::new());
    for (id, name, account_type) in [
        ("cash", "Cash", AccountType::Asset),
        ("equipment", "Equipment", AccountType::Asset),
        ("loan_payable", "Bank Loan", AccountType::Liability),
        ("owners_equity", "Owner's Equity", AccountType::Equity),
        ("sales", "Sales", AccountType::Income),
        ("rent", "Rent", AccountType::Expense),
    ] {
        ledger.create_account(id, name, account_type, None).await?;
    }

    let january = [
        patterns::create_owner_investment(
            "invest",
            date(1, 1),
            "Owner investment",
            "cash",
            "owners_equity",
            BigDecimal::from(50_000),
        )?,
        patterns::create_loan_received(
            "loan",
            date(1, 2),
            "Bank loan",
            "cash",
            "loan_payable",
            BigDecimal::from(20_000),
        )?,
        patterns::create_asset_purchase(
            "equip",
            date(1, 3),
            "Bought equipment",
            "equipment",
            "cash",
            BigDecimal::from(15_000),
        )?,
        patterns::create_sales_transaction(
            "sale-jan",
            date(1, 10),
            "January sales",
            "cash",
            "sales",
            BigDecimal::from(8_000),
        )?,
        patterns::create_expense_payment(
            "rent-jan",
            date(1, 20),
            "January rent",
            "rent",
            "cash",
            BigDecimal::from(2_000),
        )?,
        patterns::create_sales_transaction(
            "sale-feb",
            date(2, 5),
            "February sales",
            "cash",
            "sales",
            BigDecimal::from(1_000),
        )?,
    ];
    for transaction in january {
        ledger.record_transaction(transaction).await?;
    }
    Ok(ledger)
}

#[tokio::test]
async fn test_balances_are_grouped_by_account_type() -> LedgerResult<()> {
    let ledger = january_books().await?;

    let by_type = ledger.get_account_balances_by_type(date(1, 31)).await?;

    assert_eq!(
        section_ids(&by_type[&AccountType::Asset]),
        ["cash", "equipment"]
    );
    assert_eq!(
        section_ids(&by_type[&AccountType::Liability]),
        ["loan_payable"]
    );
    assert_eq!(
        section_ids(&by_type[&AccountType::Equity]),
        ["owners_equity"]
    );
    assert_eq!(section_ids(&by_type[&AccountType::Income]), ["sales"]);
    assert_eq!(section_ids(&by_type[&AccountType::Expense]), ["rent"]);

    let cash = by_type[&AccountType::Asset]
        .iter()
        .find(|b| b.account.id == "cash")
        .unwrap();
    // 50,000 + 20,000 - 15,000 + 8,000 - 2,000; the February sale is after the date
    assert_eq!(cash.debit_balance, Some(BigDecimal::from(61_000)));
    assert_eq!(cash.credit_balance, None);
    Ok(())
}

#[tokio::test]
async fn test_income_statement_nets_revenue_against_expenses() -> LedgerResult<()> {
    let ledger = january_books().await?;

    let statement = ledger
        .generate_income_statement(date(1, 1), date(1, 31))
        .await?;

    assert_eq!(section_ids(&statement.revenue), ["sales"]);
    assert_eq!(section_ids(&statement.expenses), ["rent"]);
    assert_eq!(statement.total_revenue, BigDecimal::from(8_000));
    assert_eq!(statement.total_expenses, BigDecimal::from(2_000));
    assert_eq!(statement.net_income, BigDecimal::from(6_000));
    Ok(())
}

#[tokio::test]
async fn test_balance_sheet_carries_net_income_into_equity() -> LedgerResult<()> {
    let ledger = january_books().await?;

    let sheet = ledger.generate_balance_sheet(date(1, 31)).await?;

    assert_eq!(sheet.total_assets, BigDecimal::from(76_000));
    assert_eq!(sheet.total_liabilities, BigDecimal::from(20_000));
    assert_eq!(sheet.total_equity, BigDecimal::from(56_000));
    assert!(sheet.equity.iter().any(|b| b.account.id == "net_income"));
    assert!(sheet.is_balanced);
    Ok(())
}

#[tokio::test]
async fn test_cash_flow_sorts_the_period_into_activities() -> LedgerResult<()> {
    let ledger = january_books().await?;

    let flow = ledger.generate_cash_flow(date(1, 1), date(1, 31)).await?;

    let descriptions = |items: &[accounting_core::CashFlowItem]| -> Vec<String> {
        let mut names: Vec<String> = items.iter().map(|i| i.description.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(
        descriptions(&flow.operating_activities),
        ["January rent", "January sales"]
    );
    assert_eq!(
        descriptions(&flow.investing_activities),
        ["Bought equipment"]
    );
    assert_eq!(
        descriptions(&flow.financing_activities),
        ["Bank loan", "Owner investment"]
    );

    // Each item is its transaction's total debits
    assert_eq!(flow.net_operating_cash_flow, BigDecimal::from(10_000));
    assert_eq!(flow.net_investing_cash_flow, BigDecimal::from(15_000));
    assert_eq!(flow.net_financing_cash_flow, BigDecimal::from(70_000));
    assert_eq!(flow.net_cash_flow, BigDecimal::from(95_000));
    Ok(())
}

#[tokio::test]
async fn test_gst_sale_and_purchase_post_to_the_tax_accounts() -> LedgerResult<()> {
    let mut ledger = Ledger::new(MemoryStorage::new());
    let chart = ledger.setup_standard_chart_of_accounts().await?;
    let id = |key: &str| chart[key].id.clone();
    ledger
        .create_account("2200", "Output GST Payable", AccountType::Liability, None)
        .await?;
    ledger
        .create_account("1400", "Input GST Recoverable", AccountType::Asset, None)
        .await?;

    ledger
        .record_transaction(patterns::create_owner_investment(
            "invest",
            date(1, 1),
            "Owner investment",
            id("cash"),
            id("owners_equity"),
            BigDecimal::from(10_000),
        )?)
        .await?;
    ledger
        .record_transaction(patterns::create_invoice_with_gst(InvoiceWithGstParams {
            id: "inv-1".to_string(),
            date: date(1, 10),
            description: "Invoice INV-1".to_string(),
            receivables_account_id: id("accounts_receivable"),
            revenue_account_id: id("sales_revenue"),
            gst_payable_account_id: "2200".to_string(),
            base_amount: BigDecimal::from(10_000),
            gst_amount: BigDecimal::from(1_800),
        })?)
        .await?;
    ledger
        .record_transaction(patterns::create_bill_payment_with_gst(
            BillPaymentWithGstParams {
                id: "bill-1".to_string(),
                date: date(1, 20),
                description: "Office rent".to_string(),
                expense_account_id: id("rent_expense"),
                gst_recoverable_account_id: "1400".to_string(),
                cash_or_payables_account_id: id("cash"),
                base_amount: BigDecimal::from(5_000),
                gst_amount: BigDecimal::from(900),
            },
        )?)
        .await?;

    let balance = |account_id: String| {
        let ledger = &ledger;
        async move { ledger.get_account_balance(&account_id, None).await.unwrap() }
    };
    assert_eq!(
        balance(id("accounts_receivable")).await,
        BigDecimal::from(11_800)
    );
    assert_eq!(balance(id("sales_revenue")).await, BigDecimal::from(10_000));
    assert_eq!(balance("2200".to_string()).await, BigDecimal::from(1_800));
    assert_eq!(balance(id("rent_expense")).await, BigDecimal::from(5_000));
    assert_eq!(balance("1400".to_string()).await, BigDecimal::from(900));
    assert_eq!(balance(id("cash")).await, BigDecimal::from(4_100));

    let trial = ledger.get_trial_balance(date(1, 31)).await?;
    assert!(trial.is_balanced);
    assert_eq!(trial.total_debits, BigDecimal::from(21_800));

    let integrity = ledger.validate_integrity(date(1, 31)).await?;
    assert!(integrity.is_valid, "{:?}", integrity.issues);
    assert_eq!(
        integrity.balance_sheet_total_assets,
        BigDecimal::from(16_800)
    );
    Ok(())
}

#[test]
fn test_gst_patterns_reject_a_zero_tax_leg() {
    let invoice = patterns::create_invoice_with_gst(InvoiceWithGstParams {
        id: "inv-0".to_string(),
        date: date(1, 10),
        description: "Exempt supply".to_string(),
        receivables_account_id: "1200".to_string(),
        revenue_account_id: "4000".to_string(),
        gst_payable_account_id: "2200".to_string(),
        base_amount: BigDecimal::from(1_000),
        gst_amount: BigDecimal::from(0),
    });
    assert!(matches!(invoice, Err(LedgerError::NonPositiveAmount)));

    let bill = patterns::create_bill_payment_with_gst(BillPaymentWithGstParams {
        id: "bill-0".to_string(),
        date: date(1, 20),
        description: "Exempt purchase".to_string(),
        expense_account_id: "6000".to_string(),
        gst_recoverable_account_id: "1400".to_string(),
        cash_or_payables_account_id: "1000".to_string(),
        base_amount: BigDecimal::from(1_000),
        gst_amount: BigDecimal::from(0),
    });
    assert!(matches!(bill, Err(LedgerError::NonPositiveAmount)));
}

#[tokio::test]
async fn test_reports_ignore_transactions_after_the_report_date() -> LedgerResult<()> {
    let mut ledger = january_books().await?;
    let march = TransactionBuilder::new("sale-mar", date(3, 1), "March sales")
        .debit("cash", BigDecimal::from(4_000), None)
        .credit("sales", BigDecimal::from(4_000), None)
        .build()?;
    ledger.record_transaction(march).await?;

    let january = ledger.generate_balance_sheet(date(1, 31)).await?;
    let march_end = ledger.generate_balance_sheet(date(3, 31)).await?;

    assert_eq!(january.total_assets, BigDecimal::from(76_000));
    // February's 1,000 and March's 4,000 sales
    assert_eq!(march_end.total_assets, BigDecimal::from(81_000));
    assert!(march_end.is_balanced);

    let flow = ledger.generate_cash_flow(date(2, 1), date(2, 29)).await?;
    assert_eq!(flow.net_cash_flow, BigDecimal::from(1_000));
    assert!(flow.investing_activities.is_empty());
    assert!(flow.financing_activities.is_empty());
    Ok(())
}
