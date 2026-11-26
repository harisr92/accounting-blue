use crate::document::discount::Discount;
use crate::gst::invoice::note::GstCreditNote;
use crate::gst::invoice::posting::*;
use crate::gst::invoice::seller::{CompositionRate, SellerRegistration};
use crate::gst::invoice::types::{
    GstBreakdown, GstInvoice, GstLineItem, Gstin, LineItemError, Recipient, StateCode,
};
use crate::gst::invoice::validation::ComplianceIssue;
use crate::gst::tax::{round_to_paise, CGST_CODE, IGST_CODE, SGST_CODE};
use crate::ledger::TransactionBuilder;
use crate::types::EntryType;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;

const SELLER: &str = "27AAPFU0939F1ZV";
const BUYER_SAME_STATE: &str = "27AAPFU0939F2ZU";
const BUYER_OTHER_STATE: &str = "29AAPFU0939F1ZR";

fn invoice(buyer: &str, rate: i32) -> GstInvoice {
    let item = GstLineItem::new(
        "998314",
        "IT consulting",
        BigDecimal::from(2),
        BigDecimal::from(500),
        BigDecimal::from(rate),
    )
    .unwrap();
    GstInvoice::new(
        "INV-001",
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        Gstin::parse(buyer).unwrap(),
        vec![item],
    )
    .unwrap()
}

fn accounts() -> InvoiceAccounts {
    InvoiceAccounts::new("ar", "sales", "cgst_out", "sgst_out", "igst_out")
}

#[test]
fn test_intra_state_posting_splits_cgst_and_sgst() {
    let breakdown = invoice(BUYER_SAME_STATE, 18).breakdown().unwrap();

    assert_eq!(
        posting_legs(&breakdown),
        vec![
            (PostingLeg::Receivable, BigDecimal::from(1180)),
            (PostingLeg::Revenue, BigDecimal::from(1000)),
            (PostingLeg::output_tax(CGST_CODE), BigDecimal::from(90)),
            (PostingLeg::output_tax(SGST_CODE), BigDecimal::from(90)),
        ]
    );
}

#[test]
fn test_inter_state_posting_charges_igst() {
    let breakdown = invoice(BUYER_OTHER_STATE, 18).breakdown().unwrap();

    assert_eq!(
        posting_legs(&breakdown),
        vec![
            (PostingLeg::Receivable, BigDecimal::from(1180)),
            (PostingLeg::Revenue, BigDecimal::from(1000)),
            (PostingLeg::output_tax(IGST_CODE), BigDecimal::from(180)),
        ]
    );
}

#[test]
fn test_zero_rated_posting_has_no_tax_legs() {
    let breakdown = invoice(BUYER_SAME_STATE, 0).breakdown().unwrap();

    assert_eq!(
        posting_legs(&breakdown),
        vec![
            (PostingLeg::Receivable, BigDecimal::from(1000)),
            (PostingLeg::Revenue, BigDecimal::from(1000)),
        ]
    );
    assert!(posting_legs(&GstBreakdown::default()).is_empty());
}

#[test]
fn test_only_the_receivable_leg_is_a_debit() {
    let debits: Vec<_> = gst_legs()
        .into_iter()
        .filter(|leg| leg.side() == EntryType::Debit)
        .collect();
    assert_eq!(debits, vec![PostingLeg::Receivable]);
}

#[test]
fn test_entries_post_to_the_named_accounts_and_balance() {
    let entries = invoice(BUYER_SAME_STATE, 18)
        .to_entries(&accounts())
        .unwrap();

    let posted: Vec<_> = entries
        .iter()
        .map(|e| (e.account_id.as_str(), e.entry_type, e.amount.clone()))
        .collect();
    assert_eq!(
        posted,
        vec![
            ("ar", EntryType::Debit, BigDecimal::from(1180)),
            ("sales", EntryType::Credit, BigDecimal::from(1000)),
            ("cgst_out", EntryType::Credit, BigDecimal::from(90)),
            ("sgst_out", EntryType::Credit, BigDecimal::from(90)),
        ]
    );
    assert_eq!(entries[2].description.as_deref(), Some("CGST output"));
}

#[test]
fn test_every_leg_maps_to_its_own_account() {
    let accounts = accounts();
    let ids: Vec<_> = gst_legs()
        .into_iter()
        .map(|leg| accounts.account_for(&leg).unwrap())
        .collect();
    assert_eq!(ids, vec!["ar", "sales", "cgst_out", "sgst_out", "igst_out"]);
}

#[test]
fn test_non_compliant_invoice_is_not_posted() {
    // Line A is zero-rated, line B has a negative price: without the guard the credits for
    // CGST and SGST would be negative
    let mut invoice = invoice(BUYER_SAME_STATE, 0);
    let mut negative = invoice.line_items[0].clone();
    negative.unit_price = BigDecimal::from(-100);
    negative.gst_rate = BigDecimal::from(18);
    invoice.line_items.push(negative);

    let Err(PostingError::NotCompliant(errors)) = invoice.to_entries(&accounts()) else {
        panic!("a negative price must block the posting");
    };
    assert_eq!(
        errors,
        vec![ComplianceIssue::InvalidLineItem {
            line: 1,
            reason: LineItemError::NegativeUnitPrice,
        }]
    );
}

#[test]
fn test_same_seller_and_buyer_invoice_is_not_posted() {
    let result = invoice(SELLER, 18).to_entries(&accounts());
    assert!(matches!(result, Err(PostingError::NotCompliant(errors))
        if errors == [ComplianceIssue::SameSellerAndBuyer(SELLER.to_string())]));
}

#[test]
fn test_zero_value_invoice_has_nothing_to_post() {
    let mut invoice = invoice(BUYER_SAME_STATE, 18);
    invoice.line_items[0].unit_price = BigDecimal::from(0);

    assert!(matches!(
        invoice.to_entries(&accounts()),
        Err(PostingError::NothingToPost)
    ));
}

#[test]
fn test_warnings_do_not_block_the_posting() {
    // HSN 0000 is well-formed but unknown to the master: a warning, not an error
    let mut invoice = invoice(BUYER_OTHER_STATE, 18);
    invoice.line_items[0].hsn_sac = "0000".to_string();

    assert_eq!(invoice.to_entries(&accounts()).unwrap().len(), 3);
}

#[test]
fn test_fractional_quantities_post_balanced_paise() {
    // 2.5 units at 33.33 is 83.325 taxable
    let mut invoice = invoice(BUYER_SAME_STATE, 5);
    invoice.line_items[0].quantity = BigDecimal::from_str("2.5").unwrap();
    invoice.line_items[0].unit_price = BigDecimal::from_str("33.33").unwrap();

    let entries = invoice.to_entries(&accounts()).unwrap();
    for entry in &entries {
        assert_eq!(
            entry.amount,
            round_to_paise(&entry.amount),
            "{}",
            entry.amount
        );
    }
    let amounts: Vec<_> = entries.iter().map(|e| e.amount.to_string()).collect();
    assert_eq!(amounts, ["87.49", "83.33", "2.08", "2.08"]);

    let transaction = entries
        .into_iter()
        .fold(
            TransactionBuilder::new("inv", invoice.invoice_date, "Invoice"),
            TransactionBuilder::entry,
        )
        .build()
        .unwrap();
    assert!(transaction.is_balanced());
}

#[test]
fn test_unregistered_inter_state_invoice_posts_igst() {
    let item = GstLineItem::new(
        "998314",
        "IT consulting",
        BigDecimal::from(1),
        BigDecimal::from(150_000),
        BigDecimal::from(18),
    )
    .unwrap();
    let invoice = GstInvoice::new(
        "INV-B2C-001",
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        Recipient::unregistered(StateCode::parse("29").unwrap()),
        vec![item],
    )
    .unwrap();

    let entries = invoice.to_entries(&accounts()).unwrap();
    let posted: Vec<_> = entries
        .iter()
        .map(|e| (e.account_id.as_str(), e.entry_type, e.amount.clone()))
        .collect();
    assert_eq!(
        posted,
        vec![
            ("ar", EntryType::Debit, BigDecimal::from(177_000)),
            ("sales", EntryType::Credit, BigDecimal::from(150_000)),
            ("igst_out", EntryType::Credit, BigDecimal::from(27_000)),
        ]
    );
}

#[test]
fn test_exempt_invoice_posts_sales_without_tax_legs() {
    let item = GstLineItem::exempt(
        "4901",
        "Printed books",
        BigDecimal::from(4),
        BigDecimal::from(250),
    )
    .unwrap();
    let invoice = GstInvoice::new(
        "INV-EX-001",
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
        Gstin::parse(SELLER).unwrap(),
        Gstin::parse(BUYER_OTHER_STATE).unwrap(),
        vec![item],
    )
    .unwrap();

    let entries = invoice.to_entries(&accounts()).unwrap();
    let posted: Vec<_> = entries
        .iter()
        .map(|e| (e.account_id.as_str(), e.entry_type, e.amount.clone()))
        .collect();
    assert_eq!(
        posted,
        vec![
            ("ar", EntryType::Debit, BigDecimal::from(1000)),
            ("sales", EntryType::Credit, BigDecimal::from(1000)),
        ]
    );
}

fn credit_note(buyer: &str) -> GstCreditNote {
    let invoice = invoice(buyer, 18);
    let on = NaiveDate::from_ymd_opt(2024, 11, 20).unwrap();
    GstCreditNote::new(&invoice, "CN-001", on, invoice.line_items.clone()).unwrap()
}

fn posted(entries: &[crate::types::Entry]) -> Vec<(&str, EntryType, BigDecimal)> {
    entries
        .iter()
        .map(|e| (e.account_id.as_str(), e.entry_type, e.amount.clone()))
        .collect()
}

#[test]
fn test_credit_note_posts_the_reverse_of_its_invoice() {
    let entries = credit_note(BUYER_SAME_STATE)
        .to_entries(&accounts())
        .unwrap();

    assert_eq!(
        posted(&entries),
        vec![
            ("ar", EntryType::Credit, BigDecimal::from(1180)),
            ("sales", EntryType::Debit, BigDecimal::from(1000)),
            ("cgst_out", EntryType::Debit, BigDecimal::from(90)),
            ("sgst_out", EntryType::Debit, BigDecimal::from(90)),
        ]
    );
    let invoice_entries = invoice(BUYER_SAME_STATE, 18)
        .to_entries(&accounts())
        .unwrap();
    let reversed: Vec<_> = posted(&invoice_entries)
        .into_iter()
        .map(|(account, side, amount)| (account, side.opposite(), amount))
        .collect();
    assert_eq!(posted(&entries), reversed);
}

#[test]
fn test_credit_note_entries_balance_in_a_transaction() {
    let entries = credit_note(BUYER_OTHER_STATE)
        .to_entries(&accounts())
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[2].account_id, "igst_out");

    let on = NaiveDate::from_ymd_opt(2024, 11, 20).unwrap();
    let transaction = entries
        .into_iter()
        .fold(
            TransactionBuilder::new("cn", on, "Credit note"),
            TransactionBuilder::entry,
        )
        .build()
        .unwrap();
    assert!(transaction.is_balanced());
}

#[test]
fn test_non_compliant_credit_note_is_not_posted() {
    let mut note = credit_note(BUYER_SAME_STATE);
    note.note_date = NaiveDate::from_ymd_opt(2025, 12, 1).unwrap();

    let Err(PostingError::NotCompliant(issues)) = note.to_entries(&accounts()) else {
        panic!("a late credit note must not post");
    };
    assert!(matches!(
        issues.as_slice(),
        [ComplianceIssue::CreditNoteTooLate { .. }]
    ));
}

#[test]
fn test_discounted_invoice_posts_its_discounted_sales() {
    let invoice = invoice(BUYER_SAME_STATE, 18)
        .with_discount(Discount::Amount(BigDecimal::from(100)))
        .unwrap();

    assert_eq!(
        posting_legs(&invoice.breakdown().unwrap()),
        vec![
            (PostingLeg::Receivable, BigDecimal::from(1062)),
            (PostingLeg::Revenue, BigDecimal::from(900)),
            (PostingLeg::output_tax(CGST_CODE), BigDecimal::from(81)),
            (PostingLeg::output_tax(SGST_CODE), BigDecimal::from(81)),
        ]
    );
}

fn composition_seller() -> SellerRegistration {
    SellerRegistration::Composition {
        gstin: Gstin::parse(SELLER).unwrap(),
        rate: CompositionRate::TraderOrManufacturer,
    }
}

fn unregistered_seller() -> SellerRegistration {
    SellerRegistration::Unregistered {
        state: StateCode::parse("27").unwrap(),
    }
}

/// A bill of supply from `seller` for 3 units at 333.33 less a 10% discount: 899.99 of turnover
fn bill_of_supply(seller: SellerRegistration) -> GstInvoice {
    let item = GstLineItem::new(
        "998314",
        "IT consulting",
        BigDecimal::from(3),
        BigDecimal::from_str("333.33").unwrap(),
        BigDecimal::from(0),
    )
    .unwrap()
    .with_discount(Discount::Percent(BigDecimal::from(10)))
    .unwrap();
    GstInvoice::new(
        "BOS-001",
        NaiveDate::from_ymd_opt(2024, 11, 15).unwrap(),
        seller,
        Gstin::parse(BUYER_SAME_STATE).unwrap(),
        vec![item],
    )
    .unwrap()
}

fn composition_accounts() -> InvoiceAccounts {
    accounts().with_composition_tax("composition_expense", "composition_payable")
}

fn bill_credit_note(bill: &GstInvoice) -> GstCreditNote {
    let on = NaiveDate::from_ymd_opt(2024, 11, 20).unwrap();
    let mut returned = bill.line_items[0].clone();
    returned.quantity = BigDecimal::from(1);
    GstCreditNote::new(bill, "CN-001", on, vec![returned]).unwrap()
}

#[test]
fn test_unregistered_sellers_bill_posts_receivable_and_revenue_only() {
    let entries = bill_of_supply(unregistered_seller())
        .to_entries(&accounts())
        .unwrap();

    assert_eq!(
        posted(&entries),
        vec![
            (
                "ar",
                EntryType::Debit,
                BigDecimal::from_str("899.99").unwrap()
            ),
            (
                "sales",
                EntryType::Credit,
                BigDecimal::from_str("899.99").unwrap()
            ),
        ]
    );
}

#[test]
fn test_composition_tax_is_its_rate_of_the_turnover_after_discount() {
    let bill = bill_of_supply(composition_seller());
    // 1% of 899.99 is 8.9999, rounded to paise
    assert_eq!(
        bill.composition_tax().unwrap(),
        Some(BigDecimal::from_str("9.00").unwrap())
    );
    assert_eq!(
        bill_of_supply(unregistered_seller())
            .composition_tax()
            .unwrap(),
        None
    );
    assert_eq!(
        invoice(BUYER_SAME_STATE, 18).composition_tax().unwrap(),
        None
    );
}

#[test]
fn test_composition_sellers_bill_posts_its_tax_as_an_expense_and_a_payable() {
    let entries = bill_of_supply(composition_seller())
        .to_entries(&composition_accounts())
        .unwrap();

    let turnover = BigDecimal::from_str("899.99").unwrap();
    let tax = BigDecimal::from_str("9.00").unwrap();
    assert_eq!(
        posted(&entries),
        vec![
            ("ar", EntryType::Debit, turnover.clone()),
            ("sales", EntryType::Credit, turnover),
            ("composition_expense", EntryType::Debit, tax.clone()),
            ("composition_payable", EntryType::Credit, tax),
        ]
    );
    assert_eq!(
        entries[2].description.as_deref(),
        Some(COMPOSITION_TAX_MEMO)
    );
}

#[test]
fn test_composition_bill_without_its_accounts_is_not_posted() {
    let result = bill_of_supply(composition_seller()).to_entries(&accounts());
    assert!(
        matches!(
            &result,
            Err(PostingError::MissingCompositionAccounts { amount })
                if *amount == BigDecimal::from_str("9.00").unwrap()
        ),
        "{result:?}"
    );
}

#[test]
fn test_credit_notes_against_bills_of_supply_reverse_them_pro_rata() {
    let unregistered = bill_of_supply(unregistered_seller());
    let entries = bill_credit_note(&unregistered)
        .to_entries(&accounts())
        .unwrap();
    let credited = BigDecimal::from_str("300.00").unwrap();
    assert_eq!(
        posted(&entries),
        vec![
            ("ar", EntryType::Credit, credited.clone()),
            ("sales", EntryType::Debit, credited.clone()),
        ]
    );

    let composition = bill_of_supply(composition_seller());
    let entries = bill_credit_note(&composition)
        .to_entries(&composition_accounts())
        .unwrap();
    let tax = BigDecimal::from_str("3.00").unwrap();
    assert_eq!(
        posted(&entries),
        vec![
            ("ar", EntryType::Credit, credited.clone()),
            ("sales", EntryType::Debit, credited),
            ("composition_expense", EntryType::Credit, tax.clone()),
            ("composition_payable", EntryType::Debit, tax),
        ]
    );
}

#[test]
fn test_composition_entries_balance_in_a_transaction() {
    let entries = bill_of_supply(composition_seller())
        .to_entries(&composition_accounts())
        .unwrap();
    let (debits, credits) = entries.iter().fold(
        (BigDecimal::from(0), BigDecimal::from(0)),
        |(debits, credits), entry| match entry.entry_type {
            EntryType::Debit => (debits + &entry.amount, credits),
            EntryType::Credit => (debits, credits + &entry.amount),
        },
    );
    assert_eq!(debits, credits);
}
