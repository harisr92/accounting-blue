//! How GST returns write their values in the GST portal's JSON: amounts rounded to paise and
//! written as JSON numbers, shared by GSTR-1 and GSTR-3B

use crate::gst::tax::round_to_paise;
use bigdecimal::{BigDecimal, ToPrimitive};
use serde::ser::Error as _;
use serde::Serializer;

/// An amount rounded to paise, written as a JSON number
pub(super) fn portal_amount<S: Serializer>(
    amount: &BigDecimal,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    portal_number(&round_to_paise(amount), serializer)
}

/// A decimal written as a JSON number
pub(super) fn portal_number<S: Serializer>(
    value: &BigDecimal,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let number = value
        .to_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| S::Error::custom(format!("{value} does not fit a JSON number")))?;
    serializer.serialize_f64(number)
}
