//! Financial report structures

use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::types::AccountBalance;

/// Balance Sheet structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceSheet {
    /// Date the balances are taken at
    pub as_of_date: NaiveDate,
    /// Asset account balances
    pub assets: Vec<AccountBalance>,
    /// Liability account balances
    pub liabilities: Vec<AccountBalance>,
    /// Equity account balances, plus a synthetic `net_income` line when it is non-zero
    pub equity: Vec<AccountBalance>,
    /// Sum of `assets`
    pub total_assets: BigDecimal,
    /// Sum of `liabilities`
    pub total_liabilities: BigDecimal,
    /// Sum of `equity`
    pub total_equity: BigDecimal,
    /// Whether assets equal liabilities plus equity
    pub is_balanced: bool,
}

/// Income Statement structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncomeStatement {
    /// Start of the reporting period
    pub start_date: NaiveDate,
    /// End of the reporting period
    pub end_date: NaiveDate,
    /// Income account balances
    pub revenue: Vec<AccountBalance>,
    /// Expense account balances
    pub expenses: Vec<AccountBalance>,
    /// Sum of `revenue`
    pub total_revenue: BigDecimal,
    /// Sum of `expenses`
    pub total_expenses: BigDecimal,
    /// `total_revenue - total_expenses`
    pub net_income: BigDecimal,
}

/// Cash Flow Statement structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashFlowStatement {
    /// Start of the reporting period
    pub start_date: NaiveDate,
    /// End of the reporting period
    pub end_date: NaiveDate,
    /// Day-to-day business transactions
    pub operating_activities: Vec<CashFlowItem>,
    /// Purchases of long-lived assets
    pub investing_activities: Vec<CashFlowItem>,
    /// Loans and owner's equity movements
    pub financing_activities: Vec<CashFlowItem>,
    /// Sum of `operating_activities`
    pub net_operating_cash_flow: BigDecimal,
    /// Sum of `investing_activities`
    pub net_investing_cash_flow: BigDecimal,
    /// Sum of `financing_activities`
    pub net_financing_cash_flow: BigDecimal,
    /// Sum of all three sections
    pub net_cash_flow: BigDecimal,
}

/// Cash Flow Item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashFlowItem {
    /// Description of the originating transaction
    pub description: String,
    /// Amount moved
    pub amount: BigDecimal,
}

/// Report on ledger integrity and validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerIntegrityReport {
    /// Date the checks were run for
    pub as_of_date: NaiveDate,
    /// Whether every check passed
    pub is_valid: bool,
    /// One human-readable line per failed check
    pub issues: Vec<String>,
    /// Trial balance debit total
    pub trial_balance_total_debits: BigDecimal,
    /// Trial balance credit total
    pub trial_balance_total_credits: BigDecimal,
    /// Balance sheet asset total
    pub balance_sheet_total_assets: BigDecimal,
    /// Balance sheet liabilities plus equity
    pub balance_sheet_total_liabilities_equity: BigDecimal,
}
