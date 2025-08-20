//! Transaction processing and management

use bigdecimal::BigDecimal;
use chrono::NaiveDate;

use crate::traits::*;
use crate::types::*;

/// Parameters for creating an invoice with GST
pub struct InvoiceWithGstParams {
    pub id: String,
    pub date: NaiveDate,
    pub description: String,
    pub receivables_account_id: String,
    pub revenue_account_id: String,
    pub gst_payable_account_id: String,
    pub base_amount: BigDecimal,
    pub gst_amount: BigDecimal,
}

/// Parameters for creating a bill payment with GST
pub struct BillPaymentWithGstParams {
    pub id: String,
    pub date: NaiveDate,
    pub description: String,
    pub expense_account_id: String,
    pub gst_recoverable_account_id: String,
    pub cash_or_payables_account_id: String,
    pub base_amount: BigDecimal,
    pub gst_amount: BigDecimal,
}

/// Transaction manager for handling transaction operations
pub struct TransactionManager<S: LedgerStorage> {
    storage: S,
    validator: Box<dyn TransactionValidator>,
}

impl<S: LedgerStorage> TransactionManager<S> {
    /// Create a new transaction manager
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            validator: Box::new(DefaultTransactionValidator),
        }
    }

    /// Create a new transaction manager with custom validator
    pub fn with_validator(storage: S, validator: Box<dyn TransactionValidator>) -> Self {
        Self { storage, validator }
    }

    /// Record a new transaction
    pub async fn record_transaction(&mut self, mut transaction: Transaction) -> LedgerResult<()> {
        // Validate the transaction
        self.validator.validate_transaction(&transaction)?;
        self.validator.validate_account_references(&transaction)?;

        // Verify all referenced accounts exist
        for entry in &transaction.entries {
            if self.storage.get_account(&entry.account_id).await?.is_none() {
                return Err(LedgerError::AccountNotFound(entry.account_id.clone()));
            }
        }

        // Update the transaction timestamp
        transaction.updated_at = chrono::Utc::now().naive_utc();

        // Save the transaction
        self.storage.save_transaction(&transaction).await?;

        // Update account balances
        for entry in &transaction.entries {
            if let Some(mut account) = self.storage.get_account(&entry.account_id).await? {
                account.apply_entry(entry.entry_type.clone(), &entry.amount);
                self.storage.update_account(&account).await?;
            }
        }

        Ok(())
    }

    /// Get a transaction by ID
    pub async fn get_transaction(&self, transaction_id: &str) -> LedgerResult<Option<Transaction>> {
        self.storage.get_transaction(transaction_id).await
    }

    /// Get a transaction by ID, returning an error if not found
    pub async fn get_transaction_required(
        &self,
        transaction_id: &str,
    ) -> LedgerResult<Transaction> {
        self.storage
            .get_transaction(transaction_id)
            .await?
            .ok_or_else(|| LedgerError::TransactionNotFound(transaction_id.to_string()))
    }

    /// Get transactions for a specific account
    pub async fn get_account_transactions(
        &self,
        account_id: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        self.storage
            .get_account_transactions(account_id, start_date, end_date)
            .await
    }

    /// Get all transactions within a date range
    pub async fn get_transactions(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> LedgerResult<Vec<Transaction>> {
        self.storage.get_transactions(start_date, end_date).await
    }

    /// Update a transaction (requires reversing old entries and applying new ones)
    pub async fn update_transaction(&mut self, transaction: &Transaction) -> LedgerResult<()> {
        // Get the existing transaction
        let old_transaction = self.get_transaction_required(&transaction.id).await?;

        // Validate the new transaction
        self.validator.validate_transaction(transaction)?;
        self.validator.validate_account_references(transaction)?;

        // Reverse the effects of the old transaction
        for entry in &old_transaction.entries {
            if let Some(mut account) = self.storage.get_account(&entry.account_id).await? {
                // Reverse the entry by applying the opposite
                let reverse_type = match entry.entry_type {
                    EntryType::Debit => EntryType::Credit,
                    EntryType::Credit => EntryType::Debit,
                };
                account.apply_entry(reverse_type, &entry.amount);
                self.storage.update_account(&account).await?;
            }
        }

        // Apply the effects of the new transaction
        for entry in &transaction.entries {
            if let Some(mut account) = self.storage.get_account(&entry.account_id).await? {
                account.apply_entry(entry.entry_type.clone(), &entry.amount);
                self.storage.update_account(&account).await?;
            }
        }

        // Update the transaction in storage
        self.storage.update_transaction(transaction).await
    }

    /// Delete a transaction (reverses its effects on account balances)
    pub async fn delete_transaction(&mut self, transaction_id: &str) -> LedgerResult<()> {
        // Get the transaction to be deleted
        let transaction = self.get_transaction_required(transaction_id).await?;

        // Reverse the effects on account balances
        for entry in &transaction.entries {
            if let Some(mut account) = self.storage.get_account(&entry.account_id).await? {
                // Reverse the entry by applying the opposite
                let reverse_type = match entry.entry_type {
                    EntryType::Debit => EntryType::Credit,
                    EntryType::Credit => EntryType::Debit,
                };
                account.apply_entry(reverse_type, &entry.amount);
                self.storage.update_account(&account).await?;
            }
        }

        // Delete the transaction from storage
        self.storage.delete_transaction(transaction_id).await
    }
}

/// Transaction builder for creating complex transactions
#[derive(Debug)]
pub struct TransactionBuilder {
    transaction: Transaction,
}

impl TransactionBuilder {
    /// Create a new transaction builder
    pub fn new(id: String, date: NaiveDate, description: String) -> Self {
        Self {
            transaction: Transaction::new(id, date, description, None),
        }
    }

    /// Set the reference for the transaction
    pub fn reference(mut self, reference: String) -> Self {
        self.transaction.reference = Some(reference);
        self
    }

    /// Add metadata to the transaction
    pub fn metadata(mut self, key: String, value: String) -> Self {
        self.transaction.metadata.insert(key, value);
        self
    }

    /// Add a debit entry
    pub fn debit(
        mut self,
        account_id: String,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        self.transaction
            .add_entry(Entry::debit(account_id, amount, description));
        self
    }

    /// Add a credit entry
    pub fn credit(
        mut self,
        account_id: String,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Self {
        self.transaction
            .add_entry(Entry::credit(account_id, amount, description));
        self
    }

    /// Add a custom entry
    pub fn entry(mut self, entry: Entry) -> Self {
        self.transaction.add_entry(entry);
        self
    }

    /// Build the transaction
    pub fn build(self) -> LedgerResult<Transaction> {
        self.transaction.validate()?;
        Ok(self.transaction)
    }
}

/// Common transaction patterns
pub mod patterns {
    use super::*;

    /// Create a simple payment transaction (debit expense, credit cash)
    pub fn create_expense_payment(
        id: String,
        date: NaiveDate,
        description: String,
        expense_account_id: String,
        cash_account_id: String,
        amount: BigDecimal,
    ) -> LedgerResult<Transaction> {
        TransactionBuilder::new(id, date, description)
            .debit(expense_account_id, amount.clone(), None)
            .credit(cash_account_id, amount, None)
            .build()
    }

    /// Create a sales transaction (debit cash/receivables, credit revenue)
    pub fn create_sales_transaction(
        id: String,
        date: NaiveDate,
        description: String,
        cash_or_receivables_account_id: String,
        revenue_account_id: String,
        amount: BigDecimal,
    ) -> LedgerResult<Transaction> {
        TransactionBuilder::new(id, date, description)
            .debit(cash_or_receivables_account_id, amount.clone(), None)
            .credit(revenue_account_id, amount, None)
            .build()
    }

    /// Create an asset purchase transaction (debit asset, credit cash/payables)
    pub fn create_asset_purchase(
        id: String,
        date: NaiveDate,
        description: String,
        asset_account_id: String,
        cash_or_payables_account_id: String,
        amount: BigDecimal,
    ) -> LedgerResult<Transaction> {
        TransactionBuilder::new(id, date, description)
            .debit(asset_account_id, amount.clone(), None)
            .credit(cash_or_payables_account_id, amount, None)
            .build()
    }

    /// Create an invoice with GST
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

    /// Create a bill payment with GST
    pub fn create_bill_payment_with_gst(
        params: BillPaymentWithGstParams,
    ) -> LedgerResult<Transaction> {
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

    /// Create a loan transaction
    pub fn create_loan_received(
        id: String,
        date: NaiveDate,
        description: String,
        cash_account_id: String,
        loan_payable_account_id: String,
        amount: BigDecimal,
    ) -> LedgerResult<Transaction> {
        TransactionBuilder::new(id, date, description)
            .debit(
                cash_account_id,
                amount.clone(),
                Some("Cash received from loan".to_string()),
            )
            .credit(
                loan_payable_account_id,
                amount,
                Some("Loan payable".to_string()),
            )
            .build()
    }

    /// Create owner investment transaction
    pub fn create_owner_investment(
        id: String,
        date: NaiveDate,
        description: String,
        cash_account_id: String,
        equity_account_id: String,
        amount: BigDecimal,
    ) -> LedgerResult<Transaction> {
        TransactionBuilder::new(id, date, description)
            .debit(
                cash_account_id,
                amount.clone(),
                Some("Cash invested by owner".to_string()),
            )
            .credit(
                equity_account_id,
                amount,
                Some("Owner's equity contribution".to_string()),
            )
            .build()
    }
}
