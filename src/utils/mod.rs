//! Reference storage implementations, validation rules and, with the `india` feature, Indian
//! amount formatting

#[cfg(feature = "india")]
pub mod formatting;
pub mod memory_storage;
pub mod validation;

#[cfg(feature = "india")]
pub use formatting::{amount_in_words, format_inr};
pub use memory_storage::{MemoryReconciliationStorage, MemoryStorage};
pub use validation::{
    DefaultAccountValidator, DefaultTransactionValidator, EnhancedAccountValidator,
    EnhancedTransactionValidator,
};

#[cfg(test)]
mod tests;
