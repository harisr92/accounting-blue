use crate::gst::invoice::payment::upi_uri;
use crate::gst::invoice::pdf::qr::*;
use crate::gst::invoice::pdf::PdfError;
use bigdecimal::BigDecimal;
use printpdf::Op;

/// Light modules around the code, as scanners expect
const QUIET_ZONE: usize = 4;
/// Pixels per module in the test image
const SCALE: usize = 4;

/// Decode `modules` as a scanner would read them from a greyscale image
fn decode(modules: &[Vec<bool>]) -> String {
    let side = (modules.len() + 2 * QUIET_ZONE) * SCALE;
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(side, side, |x, y| {
        let (row, column) = (y / SCALE, x / SCALE);
        let dark = row
            .checked_sub(QUIET_ZONE)
            .zip(column.checked_sub(QUIET_ZONE))
            .and_then(|(row, column)| modules.get(row)?.get(column).copied())
            .unwrap_or(false);
        if dark {
            0
        } else {
            255
        }
    });
    let grids = image.detect_grids();
    assert_eq!(grids.len(), 1);
    grids[0].decode().unwrap().1
}

#[test]
fn test_qr_code_decodes_back_to_the_upi_link() {
    let uri = upi_uri(
        "acme@okhdfc",
        "Acme Services Pvt Ltd",
        &BigDecimal::from(1180),
        "INV/2025-26/042",
    );
    let modules = qr_modules(&uri).unwrap();

    assert!(modules.iter().all(|row| row.len() == modules.len()));
    assert_eq!(decode(&modules), uri);
}

#[test]
fn test_data_too_long_for_a_qr_code_is_refused() {
    let result = qr_modules(&"x".repeat(5000));
    assert!(matches!(result, Err(PdfError::QrCode { .. })), "{result:?}");
}

#[test]
fn test_qr_code_is_drawn_as_one_shape_with_a_square_per_dark_module() {
    let modules = qr_modules("upi://pay?pa=a@b").unwrap();
    let dark = modules.iter().flatten().filter(|dark| **dark).count();
    let ops = qr_ops(&modules, (10.0, 20.0), 26.0);

    assert_eq!(ops.len(), 1);
    let Op::DrawPolygon { polygon } = &ops[0] else {
        panic!("expected a polygon, got {ops:?}");
    };
    assert_eq!(polygon.rings.len(), dark);
    assert!(qr_ops(&[], (0.0, 0.0), 26.0).is_empty());
}
