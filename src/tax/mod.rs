//! Tax calculation module

pub mod gst;

pub use gst::{
    percent_of, round_to_paise, GstCalculation, GstCalculator, GstCategory, GstError, GstRate,
    PAISE_SCALE,
};

#[cfg(test)]
mod tests;
