use crate::returns::gstr1::Gstr1Error;
use crate::returns::period::*;
use chrono::NaiveDate;

#[test]
fn test_period_accepts_months_of_the_gst_era() {
    let period = ReturnPeriod::new(2024, 11).unwrap();
    assert_eq!(period.year(), 2024);
    assert_eq!(period.month(), 11);
    assert!(ReturnPeriod::new(FIRST_GST_YEAR, 1).is_ok());
    assert!(ReturnPeriod::new(2024, 12).is_ok());
}

#[test]
fn test_period_rejects_bad_months_and_years() {
    for (year, month) in [(2024, 0), (2024, 13), (2016, 6), (10_000, 1)] {
        assert!(matches!(
            ReturnPeriod::new(year, month),
            Err(Gstr1Error::InvalidPeriod { year: y, month: m }) if y == year && m == month
        ));
    }
}

#[test]
fn test_period_is_written_mmyyyy() {
    let period = ReturnPeriod::new(2024, 4).unwrap();
    assert_eq!(period.to_portal_string(), "042024");
    assert_eq!(period.to_string(), "042024");
    assert_eq!(serde_json::to_string(&period).unwrap(), r#""042024""#);
}

#[test]
fn test_period_contains_only_its_own_month() {
    let period = ReturnPeriod::new(2024, 11).unwrap();
    let date = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    assert!(period.contains(date(2024, 11, 1)));
    assert!(period.contains(date(2024, 11, 30)));
    assert!(!period.contains(date(2024, 10, 31)));
    assert!(!period.contains(date(2024, 12, 1)));
    assert!(!period.contains(date(2023, 11, 15)));
}

#[test]
fn test_period_containing_a_date() {
    let date = NaiveDate::from_ymd_opt(2025, 2, 14).unwrap();
    assert_eq!(
        ReturnPeriod::containing(date).unwrap(),
        ReturnPeriod::new(2025, 2).unwrap()
    );
}
