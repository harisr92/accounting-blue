use crate::gst::invoice::seller::*;
use crate::gst::invoice::types::{Gstin, InvoiceError, StateCode};
use bigdecimal::BigDecimal;

const SELLER: &str = "27AAPFU0939F1ZV";

fn gstin() -> Gstin {
    Gstin::parse(SELLER).unwrap()
}

fn composition() -> SellerRegistration {
    SellerRegistration::Composition {
        gstin: gstin(),
        rate: CompositionRate::TraderOrManufacturer,
    }
}

fn unregistered() -> SellerRegistration {
    SellerRegistration::Unregistered {
        state: StateCode::parse("29").unwrap(),
    }
}

#[test]
fn test_only_a_regular_seller_charges_tax() {
    assert!(SellerRegistration::from(gstin()).charges_tax());
    assert!(!composition().charges_tax());
    assert!(!unregistered().charges_tax());
}

#[test]
fn test_registered_sellers_take_their_state_from_the_gstin() {
    assert_eq!(SellerRegistration::Regular(gstin()).state(), "27");
    assert_eq!(composition().state(), "27");
    assert_eq!(composition().gstin(), Some(&gstin()));
}

#[test]
fn test_unregistered_seller_has_no_gstin_and_names_its_state() {
    assert_eq!(unregistered().gstin(), None);
    assert_eq!(unregistered().state(), "29");
}

#[test]
fn test_bills_of_supply_carry_their_declaration() {
    assert_eq!(SellerRegistration::Regular(gstin()).declaration(), None);
    assert_eq!(composition().declaration(), Some(COMPOSITION_DECLARATION));
    assert_eq!(unregistered().declaration(), Some(UNREGISTERED_DECLARATION));
}

#[test]
fn test_only_a_composition_seller_has_a_composition_rate() {
    assert_eq!(
        composition().composition_rate(),
        Some(CompositionRate::TraderOrManufacturer)
    );
    assert_eq!(
        SellerRegistration::Regular(gstin()).composition_rate(),
        None
    );
    assert_eq!(unregistered().composition_rate(), None);
}

#[test]
fn test_composition_rates_follow_the_scheme() {
    let percents: Vec<_> = CompositionRate::ALL
        .iter()
        .map(|rate| rate.percent())
        .collect();
    assert_eq!(percents, [1, 5, 6].map(BigDecimal::from));
    assert_eq!(CompositionRate::Services.to_string(), "6%");
}

#[test]
fn test_composition_rate_is_read_from_its_percentage() {
    let rate = CompositionRate::try_from(&BigDecimal::from(5)).unwrap();
    assert_eq!(rate, CompositionRate::Restaurant);
    assert!(matches!(
        CompositionRate::try_from(&BigDecimal::from(2)),
        Err(InvoiceError::InvalidCompositionRate(rate)) if rate == 2
    ));
}

#[test]
fn test_registration_round_trips_through_json() {
    for seller in [
        SellerRegistration::Regular(gstin()),
        composition(),
        unregistered(),
    ] {
        let json = serde_json::to_value(&seller).unwrap();
        let back: SellerRegistration = serde_json::from_value(json).unwrap();
        assert_eq!(back, seller);
    }
    assert_eq!(
        serde_json::to_value(composition()).unwrap(),
        serde_json::json!({ "composition": { "gstin": SELLER, "rate": "trader_or_manufacturer" } })
    );
}
