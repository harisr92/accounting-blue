//! GST state codes and their names
//!
//! [`GST_STATES`] lists every state and union territory code the GST portal uses, with `97`
//! (other territory) and `99` (centre jurisdiction). An invoice prints a place of supply as
//! [`state_label`], such as `29 - Karnataka`, rather than the bare code.
//!
//! # Example
//!
//! ```
//! use accounting_core::invoice::{state_label, state_name};
//!
//! assert_eq!(state_name("29"), Some("Karnataka"));
//! assert_eq!(state_label("29"), "29 - Karnataka");
//! assert_eq!(state_label("00"), "00");
//! ```

/// Every GST state code with its name, as the GST portal lists them, in code order
pub const GST_STATES: &[(&str, &str)] = &[
    ("01", "Jammu and Kashmir"),
    ("02", "Himachal Pradesh"),
    ("03", "Punjab"),
    ("04", "Chandigarh"),
    ("05", "Uttarakhand"),
    ("06", "Haryana"),
    ("07", "Delhi"),
    ("08", "Rajasthan"),
    ("09", "Uttar Pradesh"),
    ("10", "Bihar"),
    ("11", "Sikkim"),
    ("12", "Arunachal Pradesh"),
    ("13", "Nagaland"),
    ("14", "Manipur"),
    ("15", "Mizoram"),
    ("16", "Tripura"),
    ("17", "Meghalaya"),
    ("18", "Assam"),
    ("19", "West Bengal"),
    ("20", "Jharkhand"),
    ("21", "Odisha"),
    ("22", "Chhattisgarh"),
    ("23", "Madhya Pradesh"),
    ("24", "Gujarat"),
    ("25", "Daman and Diu"),
    ("26", "Dadra and Nagar Haveli and Daman and Diu"),
    ("27", "Maharashtra"),
    ("28", "Andhra Pradesh (Old)"),
    ("29", "Karnataka"),
    ("30", "Goa"),
    ("31", "Lakshadweep"),
    ("32", "Kerala"),
    ("33", "Tamil Nadu"),
    ("34", "Puducherry"),
    ("35", "Andaman and Nicobar Islands"),
    ("36", "Telangana"),
    ("37", "Andhra Pradesh"),
    ("38", "Ladakh"),
    ("97", "Other Territory"),
    ("99", "Centre Jurisdiction"),
];

/// The name of the state or territory with GST code `code`, if there is one
#[must_use]
pub fn state_name(code: impl Into<String>) -> Option<&'static str> {
    let code = code.into();
    GST_STATES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, name)| *name)
}

/// `code` with its name, as in `29 - Karnataka`; the bare code when it names no state
#[must_use]
pub fn state_label(code: impl Into<String>) -> String {
    let code = code.into();
    match state_name(code.as_str()) {
        Some(name) => format!("{code} - {name}"),
        None => code,
    }
}
