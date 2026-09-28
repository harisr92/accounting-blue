//! Ready-made transactions for common business events

use bigdecimal::BigDecimal;
use chrono::NaiveDate;

use crate::error::LedgerResult;
use crate::ledger::builder::TransactionBuilder;
use crate::types::Transaction;

/// Parameters for creating an invoice with GST
#[derive(Debug, Clone)]
pub struct InvoiceWithGstParams {
    /// Transaction id
    pub id: String,
    /// Transaction date
    pub date: NaiveDate,
    /// Transaction description
    pub description: String,
    /// Account debited with the total (cash or receivables)
    pub receivables_account_id: String,
    /// Account credited with the base amount
    pub revenue_account_id: String,
    /// Account credited with the GST
    pub gst_payable_account_id: String,
    /// Amount before GST
    pub base_amount: BigDecimal,
    /// GST charged
    pub gst_amount: BigDecimal,
}

/// Parameters for creating a bill payment with GST
#[derive(Debug, Clone)]
pub struct BillPaymentWithGstParams {
    /// Transaction id
    pub id: String,
    /// Transaction date
    pub date: NaiveDate,
    /// Transaction description
    pub description: String,
    /// Account debited with the base amount
    pub expense_account_id: String,
    /// Account debited with the recoverable GST
    pub gst_recoverable_account_id: String,
    /// Account credited with the total (cash or payables)
    pub cash_or_payables_account_id: String,
    /// Amount before GST
    pub base_amount: BigDecimal,
    /// GST paid
    pub gst_amount: BigDecimal,
}

/// One side of a two-entry transaction
struct Leg {
    account_id: String,
    memo: Option<&'static str>,
}

impl Leg {
    fn plain(account_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            memo: None,
        }
    }

    fn with_memo(account_id: impl Into<String>, memo: &'static str) -> Self {
        Self {
            account_id: account_id.into(),
            memo: Some(memo),
        }
    }
}

/// Move `amount` from the credit leg to the debit leg
fn transfer(
    id: impl Into<String>,
    date: NaiveDate,
    description: impl Into<String>,
    debit: Leg,
    credit: Leg,
    amount: BigDecimal,
) -> LedgerResult<Transaction> {
    TransactionBuilder::new(id, date, description)
        .debit(debit.account_id, amount.clone(), debit.memo.map(Into::into))
        .credit(credit.account_id, amount, credit.memo.map(Into::into))
        .build()
}

/// Create a simple payment transaction (debit expense, credit cash)
///
/// # Errors
///
/// Any double-entry rule from [`Transaction::validate`], e.g. a non-positive amount.
pub fn create_expense_payment(
    id: impl Into<String>,
    date: NaiveDate,
    description: impl Into<String>,
    expense_account_id: impl Into<String>,
    cash_account_id: impl Into<String>,
    amount: BigDecimal,
) -> LedgerResult<Transaction> {
    let debit = Leg::plain(expense_account_id);
    let credit = Leg::plain(cash_account_id);
    transfer(id, date, description, debit, credit, amount)
}

/// Create a sales transaction (debit cash/receivables, credit revenue)
///
/// # Errors
///
/// Any double-entry rule from [`Transaction::validate`], e.g. a non-positive amount.
pub fn create_sales_transaction(
    id: impl Into<String>,
    date: NaiveDate,
    description: impl Into<String>,
    cash_or_receivables_account_id: impl Into<String>,
    revenue_account_id: impl Into<String>,
    amount: BigDecimal,
) -> LedgerResult<Transaction> {
    let debit = Leg::plain(cash_or_receivables_account_id);
    let credit = Leg::plain(revenue_account_id);
    transfer(id, date, description, debit, credit, amount)
}

/// Create an asset purchase transaction (debit asset, credit cash/payables)
///
/// # Errors
///
/// Any double-entry rule from [`Transaction::validate`], e.g. a non-positive amount.
pub fn create_asset_purchase(
    id: impl Into<String>,
    date: NaiveDate,
    description: impl Into<String>,
    asset_account_id: impl Into<String>,
    cash_or_payables_account_id: impl Into<String>,
    amount: BigDecimal,
) -> LedgerResult<Transaction> {
    let debit = Leg::plain(asset_account_id);
    let credit = Leg::plain(cash_or_payables_account_id);
    transfer(id, date, description, debit, credit, amount)
}

/// Create a loan transaction (debit cash, credit loan payable)
///
/// # Errors
///
/// Any double-entry rule from [`Transaction::validate`], e.g. a non-positive amount.
pub fn create_loan_received(
    id: impl Into<String>,
    date: NaiveDate,
    description: impl Into<String>,
    cash_account_id: impl Into<String>,
    loan_payable_account_id: impl Into<String>,
    amount: BigDecimal,
) -> LedgerResult<Transaction> {
    let debit = Leg::with_memo(cash_account_id, "Cash received from loan");
    let credit = Leg::with_memo(loan_payable_account_id, "Loan payable");
    transfer(id, date, description, debit, credit, amount)
}

/// Create owner investment transaction (debit cash, credit equity)
///
/// # Errors
///
/// Any double-entry rule from [`Transaction::validate`], e.g. a non-positive amount.
pub fn create_owner_investment(
    id: impl Into<String>,
    date: NaiveDate,
    description: impl Into<String>,
    cash_account_id: impl Into<String>,
    equity_account_id: impl Into<String>,
    amount: BigDecimal,
) -> LedgerResult<Transaction> {
    let debit = Leg::with_memo(cash_account_id, "Cash invested by owner");
    let credit = Leg::with_memo(equity_account_id, "Owner's equity contribution");
    transfer(id, date, description, debit, credit, amount)
}

/// Create an invoice with GST (debit the total, credit revenue and GST payable)
///
/// # Errors
///
/// Any double-entry rule from [`Transaction::validate`], e.g. a non-positive amount.
pub fn create_invoice_with_gst(params: InvoiceWithGstParams) -> LedgerResult<Transaction> {
    let total_amount = &params.base_amount + &params.gst_amount;

    TransactionBuilder::new(params.id, params.date, params.description)
        .debit(
            params.receivables_account_id,
            total_amount,
            Some("Total including GST".to_string()),
        )
        .credit(
            params.revenue_account_id,
            params.base_amount,
            Some("Revenue amount".to_string()),
        )
        .credit(
            params.gst_payable_account_id,
            params.gst_amount,
            Some("GST payable".to_string()),
        )
        .build()
}

/// Create a bill payment with GST (debit expense and GST recoverable, credit the total)
///
/// # Errors
///
/// Any double-entry rule from [`Transaction::validate`], e.g. a non-positive amount.
pub fn create_bill_payment_with_gst(params: BillPaymentWithGstParams) -> LedgerResult<Transaction> {
    let total_amount = &params.base_amount + &params.gst_amount;

    TransactionBuilder::new(params.id, params.date, params.description)
        .debit(
            params.expense_account_id,
            params.base_amount,
            Some("Expense amount".to_string()),
        )
        .debit(
            params.gst_recoverable_account_id,
            params.gst_amount,
            Some("GST recoverable".to_string()),
        )
        .credit(
            params.cash_or_payables_account_id,
            total_amount,
            Some("Total payment".to_string()),
        )
        .build()
}
