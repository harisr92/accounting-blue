//! Reference storage implementations, validation rules and Indian amount formatting

pub mod formatting;
pub mod memory_storage;
pub mod validation;

pub use formatting::{amount_in_words, format_inr};
pub use memory_storage::{MemoryReconciliationStorage, MemoryStorage};
pub use validation::{
    DefaultAccountValidator, DefaultTransactionValidator, EnhancedAccountValidator,
    EnhancedTransactionValidator,
};

#[cfg(test)]
mod tests;
