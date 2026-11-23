//! Tax calculation module

pub mod gst;

pub use gst::{
    percent_of, round_to_paise, GstCalculation, GstCalculator, GstCategory, GstError, GstRate,
    GstRegime, CGST_CODE, IGST_CODE, PAISE_SCALE, SGST_CODE,
};

#[cfg(test)]
mod tests;
