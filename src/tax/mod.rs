//! Tax calculation module

pub mod gst;

pub use gst::{percent_of, GstCalculation, GstCalculator, GstCategory, GstError, GstRate};
