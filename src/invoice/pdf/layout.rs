//! Where each part of the invoice goes on the page
//!
//! Every block takes the y position it starts at and returns its ops with the y position it ends
//! at, so blocks stack top to bottom. Positions are millimetres from the top-left corner.

use super::draw::{
    grey_text, image, page_setup, rule, text, Align, Image, PAGE_HEIGHT, PAGE_WIDTH,
};
use super::fonts::{Face, Fonts};
use super::qr::qr_ops;
use super::text::{fit, wrap};
use super::{PdfError, PdfOptions};
use crate::invoice::payment::PaymentDetails;
use crate::invoice::print::{InvoiceParty, InvoicePrint, PrintRow, RowCapacity};
use printpdf::Op;

/// What the GSTIN line of an unregistered buyer says
const UNREGISTERED_GSTIN_LINE: &str = "GSTIN: Unregistered";

/// Printed in place of the name of a buyer who gave none (allowed below the Rule 46 threshold)
const WALK_IN_BUYER_LABEL: &str = "Walk-in customer";

const MARGIN: f32 = 12.0;
const LEFT: f32 = MARGIN;
const RIGHT: f32 = PAGE_WIDTH - MARGIN;
/// Lowest point the body may reach, above the page-number strip
const BODY_BOTTOM: f32 = PAGE_HEIGHT - MARGIN - 8.0;
const ROW_HEIGHT: f32 = 6.0;
/// Baseline offset of text within a table row
const ROW_BASELINE: f32 = 4.2;
const LINE_HEIGHT: f32 = 4.2;
const BLOCK_GAP: f32 = 3.0;
const CELL_PADDING: f32 = 1.2;

const BODY_SIZE: f32 = 8.0;
const SMALL_SIZE: f32 = 7.0;
const PARTY_SIZE: f32 = 10.0;
const NAME_SIZE: f32 = 13.0;
const TITLE_SIZE: f32 = 16.0;
const TOTAL_SIZE: f32 = 10.0;
const MUTED: f32 = 0.45;

/// The box a logo is scaled to fit, top-left at the margins
const LOGO_WIDTH: f32 = 30.0;
const LOGO_HEIGHT: f32 = 18.0;
/// Space between the logo and the seller block
const LOGO_GAP: f32 = 4.0;
const SELLER_WIDTH: f32 = 82.0;
const WORDS_WIDTH: f32 = 105.0;
const TOTALS_LABEL_X: f32 = 128.0;
const SIGNATURE_HEIGHT: f32 = 22.0;
const SIGNATURE_WIDTH: f32 = 50.0;
/// Side of the UPI QR code
const QR_SIZE: f32 = 26.0;
/// Width the QR code and its caption take at the right of the payment block
const QR_COLUMN: f32 = 40.0;
/// Modules of blank margin (quiet zone) a scanner needs around a QR code, kept clear of the
/// caption
const QR_QUIET_MODULES: f32 = 4.0;
/// Caption under the UPI QR code
const QR_CAPTION: &str = "Scan to pay with any UPI app";
/// Width each line of the invoice details wraps to, right-aligned at the margin
const DETAILS_WIDTH: f32 = 80.0;
/// Space between the Bill To and Ship To columns
const COLUMN_GAP: f32 = 6.0;
/// Slack allowed when checking a block ends above the page foot, for rounding in the sums
const FIT_TOLERANCE: f32 = 0.01;

/// How a cell too wide for its column is made to fit
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overflow {
    /// Cut short with an ellipsis: for free text such as the description
    Ellipsis,
    /// Set in a smaller size, so a number is never printed cut short
    Shrink,
}

/// A table column: its heading, width in millimetres, alignment and how it fits wide cells
#[derive(Clone, Copy)]
struct Column {
    heading: &'static str,
    width: f32,
    align: Align,
    overflow: Overflow,
}

/// Label of the totals line before the invoice discount
const SUBTOTAL_LABEL: &str = "Subtotal";

/// The item table's columns, left to right, spanning the width between the margins
const COLUMNS: [Column; 9] = [
    Column {
        heading: "#",
        width: 8.0,
        align: Align::Center,
        overflow: Overflow::Shrink,
    },
    Column {
        heading: "HSN/SAC",
        width: 17.0,
        align: Align::Left,
        overflow: Overflow::Ellipsis,
    },
    Column {
        heading: "Description",
        width: 45.0,
        align: Align::Left,
        overflow: Overflow::Ellipsis,
    },
    Column {
        heading: "Qty",
        width: 16.0,
        align: Align::Right,
        overflow: Overflow::Shrink,
    },
    Column {
        heading: "Rate",
        width: 21.0,
        align: Align::Right,
        overflow: Overflow::Shrink,
    },
    Column {
        heading: "Taxable Value",
        width: 24.0,
        align: Align::Right,
        overflow: Overflow::Shrink,
    },
    Column {
        heading: "GST",
        width: 11.0,
        align: Align::Right,
        overflow: Overflow::Shrink,
    },
    Column {
        heading: "Tax",
        width: 20.0,
        align: Align::Right,
        overflow: Overflow::Shrink,
    },
    Column {
        heading: "Amount",
        width: 24.0,
        align: Align::Right,
        overflow: Overflow::Shrink,
    },
];

/// Width the discount column takes from the description when a line has a discount
const DISCOUNT_WIDTH: f32 = 16.0;

/// The item table's columns when a line has a discount: [`COLUMNS`] with a discount column after
/// the rate, narrowing the description so the table keeps its width
const COLUMNS_WITH_DISCOUNT: [Column; 10] = [
    COLUMNS[0],
    COLUMNS[1],
    Column {
        width: COLUMNS[2].width - DISCOUNT_WIDTH,
        ..COLUMNS[2]
    },
    COLUMNS[3],
    COLUMNS[4],
    Column {
        heading: "Disc.",
        width: DISCOUNT_WIDTH,
        align: Align::Right,
        overflow: Overflow::Shrink,
    },
    COLUMNS[5],
    COLUMNS[6],
    COLUMNS[7],
    COLUMNS[8],
];

/// Ops for a block and the y position just below it
type Block = (Vec<Op>, f32);

/// Lays one invoice out in the faces and options it is rendered with
pub(super) struct Layout<'a> {
    fonts: &'a Fonts,
    print: &'a InvoicePrint,
    options: &'a PdfOptions,
    logo: Option<&'a Image>,
    qr: Option<&'a [Vec<bool>]>,
}

impl<'a> Layout<'a> {
    pub(super) fn new(
        fonts: &'a Fonts,
        print: &'a InvoicePrint,
        options: &'a PdfOptions,
        logo: Option<&'a Image>,
        qr: Option<&'a [Vec<bool>]>,
    ) -> Self {
        Self {
            fonts,
            print,
            options,
            logo,
            qr,
        }
    }

    /// How many item rows fit on each page, and the row slots the totals and footer need
    pub(super) fn capacity(&self) -> RowCapacity {
        let rows_below =
            |top: f32| row_count(BODY_BOTTOM - top - ROW_HEIGHT, f32::floor, ROW_HEIGHT);
        let (_, totals_bottom) = self.totals(BLOCK_GAP);
        let (_, closing_bottom) = self.closing(totals_bottom);
        RowCapacity {
            first_page: rows_below(self.header().1),
            other_pages: rows_below(self.continuation_header().1),
            totals: row_count(closing_bottom, f32::ceil, ROW_HEIGHT),
        }
    }

    /// Check the closing column (notes and terms) fits below the totals on a page of its own
    ///
    /// The totals, notes, terms and signature always go on one page, so a closing column taller
    /// than the room left there would run over the page foot.
    pub(super) fn check_terms_fit(&self) -> Result<(), PdfError> {
        let top = self.continuation_header().1 + BLOCK_GAP;
        let (_, totals_bottom) = self.totals(top);
        let (_, bottom) = self.closing_column(totals_bottom);
        if bottom <= BODY_BOTTOM + FIT_TOLERANCE {
            return Ok(());
        }
        let lines = self.wrapped(WORDS_WIDTH, &self.print.notes).len()
            + self.wrapped(WORDS_WIDTH, &self.options.terms).len();
        let over = row_count(bottom - BODY_BOTTOM - FIT_TOLERANCE, f32::ceil, LINE_HEIGHT);
        Err(PdfError::TermsTooLong {
            lines,
            max_lines: lines.saturating_sub(over),
        })
    }

    /// Ops for page `number` of `count`, showing `rows`, with the totals on the last page
    pub(super) fn page(&self, number: usize, count: usize, rows: &[PrintRow]) -> Vec<Op> {
        let (head, top) = if number == 1 {
            self.header()
        } else {
            self.continuation_header()
        };
        let (table, below_table) = if rows.is_empty() {
            (Vec::new(), top)
        } else {
            self.table(top, rows)
        };
        let (closing, _) = if number == count {
            let (totals, bottom) = self.totals(below_table + BLOCK_GAP);
            let (footer, bottom) = self.closing(bottom);
            ([totals, footer].concat(), bottom)
        } else {
            (Vec::new(), below_table)
        };
        [
            page_setup(),
            head,
            table,
            closing,
            self.page_strip(number, count),
        ]
        .concat()
    }

    /// Logo if given, seller, invoice details, then the buyer, on the first page
    fn header(&self) -> Block {
        let (logo, logo_height) = self.logo.map_or((Vec::new(), 0.0), |logo| {
            image(logo, (LEFT, MARGIN), (LOGO_WIDTH, LOGO_HEIGHT))
        });
        let (seller, seller_bottom) = self.seller(MARGIN + 5.0);
        let (details, details_bottom) = self.details(MARGIN + 6.0);
        let top_bottom = (MARGIN + logo_height)
            .max(seller_bottom)
            .max(details_bottom)
            + BLOCK_GAP;
        let (buyer, bottom) = self.buyer(top_bottom + 5.0);
        let ops = [
            logo,
            seller,
            details,
            vec![
                rule(LEFT, RIGHT, top_bottom),
                rule(LEFT, RIGHT, bottom + BLOCK_GAP),
            ],
            buyer,
        ]
        .concat();
        (ops, bottom + BLOCK_GAP + 2.0)
    }

    /// Seller name in large type, then its address and GSTIN
    fn seller(&self, y: f32) -> Block {
        let seller = &self.print.seller;
        let x = self.seller_x();
        let name = self.fit(&self.fonts.bold, NAME_SIZE, &seller.name, SELLER_WIDTH);
        let name_ops = text(&self.fonts.bold, NAME_SIZE, x, y, Align::Left, &name);
        let (lines, bottom) = self.lines(x, y + 5.5, SELLER_WIDTH, &party_lines(seller, None));
        ([name_ops, lines].concat(), bottom)
    }

    /// Left edge of the seller block: right of the logo, or at the margin when there is none
    fn seller_x(&self) -> f32 {
        if self.logo.is_some() {
            LEFT + LOGO_WIDTH + LOGO_GAP
        } else {
            LEFT
        }
    }

    /// Title and invoice details, right-aligned
    fn details(&self, y: f32) -> Block {
        let print = self.print;
        let supply = if print.is_inter_state {
            "Inter-state (IGST)"
        } else {
            "Intra-state (CGST + SGST)"
        };
        let lines: Vec<String> = [
            Some(format!("Invoice No: {}", print.invoice_number)),
            Some(format!("Invoice Date: {}", print.invoice_date)),
            print
                .due_date
                .as_ref()
                .map(|date| format!("Due Date: {date}")),
            print.reference.as_ref().map(|r| format!("Reference: {r}")),
            Some(format!("Place of Supply: {}", print.place_of_supply)),
            Some(format!("Supply: {supply}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        let lines = self.wrapped(DETAILS_WIDTH, &lines);
        let title = text(
            &self.fonts.bold,
            TITLE_SIZE,
            RIGHT,
            y,
            Align::Right,
            &print.title,
        );
        let detail_ops = lines.iter().enumerate().flat_map(|(index, line)| {
            let baseline = y + 6.0 + LINE_HEIGHT * index as f32;
            text(
                &self.fonts.regular,
                BODY_SIZE,
                RIGHT,
                baseline,
                Align::Right,
                line,
            )
        });
        let bottom = y + 6.0 + LINE_HEIGHT * (lines.len() - 1) as f32;
        (title.into_iter().chain(detail_ops).collect(), bottom)
    }

    /// "Bill To" with the buyer's name, or [`WALK_IN_BUYER_LABEL`] when it has none, address,
    /// state and GSTIN; beside it "Ship To" with the delivery address when there is one
    fn buyer(&self, y: f32) -> Block {
        let print = self.print;
        let buyer = &print.buyer;
        let shown_name = if buyer.has_name() {
            buyer.name.as_str()
        } else {
            WALK_IN_BUYER_LABEL
        };
        let bill_lines = party_lines(buyer, Some(&print.buyer_state));
        let Some(ship_to) = &print.ship_to else {
            return self.party_block((LEFT, y), RIGHT - LEFT, "BILL TO", shown_name, &bill_lines);
        };
        let width = (RIGHT - LEFT - COLUMN_GAP) / 2.0;
        let (bill, bill_bottom) =
            self.party_block((LEFT, y), width, "BILL TO", shown_name, &bill_lines);
        let (ship, ship_bottom) = self.party_block(
            (LEFT + width + COLUMN_GAP, y),
            width,
            "SHIP TO",
            &ship_to.name,
            &ship_to_lines(ship_to),
        );
        ([bill, ship].concat(), bill_bottom.max(ship_bottom))
    }

    /// A party block `width` wide at `(x, y)`: a small grey `label`, the name in bold, then
    /// `lines`
    fn party_block(
        &self,
        (x, y): (f32, f32),
        width: f32,
        label: &str,
        name: &str,
        lines: &[String],
    ) -> Block {
        let bold = &self.fonts.bold;
        let label = grey_text(bold, SMALL_SIZE, MUTED, (x, y), Align::Left, label);
        let name = self.fit(bold, PARTY_SIZE, name, width);
        let name_ops = text(bold, PARTY_SIZE, x, y + 5.0, Align::Left, &name);
        let (lines, bottom) = self.lines(x, y + 5.0 + LINE_HEIGHT + 0.5, width, lines);
        ([label, name_ops, lines].concat(), bottom)
    }

    /// Short header on every page after the first
    fn continuation_header(&self) -> Block {
        let y = MARGIN + 5.0;
        let title = format!("{} (continued)", self.print.title);
        let number = format!("Invoice No: {}", self.print.invoice_number);
        let ops = [
            text(&self.fonts.bold, PARTY_SIZE, LEFT, y, Align::Left, &title),
            text(
                &self.fonts.regular,
                BODY_SIZE,
                RIGHT,
                y,
                Align::Right,
                &number,
            ),
            vec![rule(LEFT, RIGHT, y + BLOCK_GAP)],
        ]
        .concat();
        (ops, y + BLOCK_GAP + 2.0)
    }

    /// The item table's columns: with a discount column when any line has a discount
    fn columns(&self) -> &'static [Column] {
        if self.print.has_line_discounts() {
            &COLUMNS_WITH_DISCOUNT
        } else {
            &COLUMNS
        }
    }

    /// Column headings, then one line per row, between rules
    fn table(&self, top: f32, rows: &[PrintRow]) -> Block {
        let columns = self.columns();
        let with_discount = columns.len() == COLUMNS_WITH_DISCOUNT.len();
        let headings: Vec<String> = columns
            .iter()
            .map(|column| column.heading.to_string())
            .collect();
        let heading = self.row(columns, &self.fonts.bold, top, &headings);
        let body = rows.iter().enumerate().flat_map(|(index, row)| {
            let row_top = top + ROW_HEIGHT * (index + 1) as f32;
            self.row(
                columns,
                &self.fonts.regular,
                row_top,
                &cells(row, with_discount),
            )
        });
        let bottom = top + ROW_HEIGHT * (rows.len() + 1) as f32;
        let rules = [top, top + ROW_HEIGHT, bottom].map(|y| rule(LEFT, RIGHT, y));
        (
            heading.into_iter().chain(body).chain(rules).collect(),
            bottom,
        )
    }

    /// One table row of `cells` whose top edge is at `top`, each cut to its one of `columns`
    fn row(&self, columns: &[Column], face: &Face, top: f32, cells: &[String]) -> Vec<Op> {
        let lefts = columns.iter().scan(LEFT, |x, column| {
            let left = *x;
            *x += column.width;
            Some(left)
        });
        columns
            .iter()
            .zip(lefts)
            .zip(cells)
            .flat_map(|((column, left), cell)| {
                let inner = column.width - 2.0 * CELL_PADDING;
                let (size, content) = match column.overflow {
                    Overflow::Ellipsis => (BODY_SIZE, self.fit(face, BODY_SIZE, cell, inner)),
                    Overflow::Shrink => (shrink_to_fit(face, BODY_SIZE, cell, inner), cell.clone()),
                };
                let x = match column.align {
                    Align::Left => left + CELL_PADDING,
                    Align::Center => left + column.width / 2.0,
                    Align::Right => left + column.width - CELL_PADDING,
                };
                text(face, size, x, top + ROW_BASELINE, column.align, &content)
            })
            .collect()
    }

    /// Amount in words on the left; subtotal and discount when there is one, taxable value, tax
    /// lines and total on the right
    fn totals(&self, top: f32) -> Block {
        let print = self.print;
        let lines = summary_lines(print);
        let amounts = lines
            .iter()
            .enumerate()
            .flat_map(|(index, (label, amount))| {
                let y = top + LINE_HEIGHT * (index + 1) as f32;
                self.amount_line(&self.fonts.regular, BODY_SIZE, y, label, amount)
            });
        let rule_y = top + LINE_HEIGHT * lines.len() as f32 + 1.5;
        let total_y = rule_y + LINE_HEIGHT + 1.0;
        let total = self.amount_line(&self.fonts.bold, TOTAL_SIZE, total_y, "Total", &print.total);
        let (words, words_bottom) = self.words(top);
        let ops = amounts
            .chain(std::iter::once(rule(TOTALS_LABEL_X, RIGHT, rule_y)))
            .chain(total)
            .chain(words)
            .collect();
        (ops, total_y.max(words_bottom) + BLOCK_GAP)
    }

    /// A totals line: `label` on the left of the column, `amount` with its currency on the right
    fn amount_line(&self, face: &Face, size: f32, y: f32, label: &str, amount: &str) -> Vec<Op> {
        let amount = self.money(amount);
        [
            text(face, size, TOTALS_LABEL_X, y, Align::Left, label),
            text(face, size, RIGHT, y, Align::Right, &amount),
        ]
        .concat()
    }

    /// "Amount in words" and the total spelled out, wrapped
    fn words(&self, top: f32) -> Block {
        let label = text(
            &self.fonts.bold,
            BODY_SIZE,
            LEFT,
            top + LINE_HEIGHT,
            Align::Left,
            "Amount in words",
        );
        let words = wrap(&self.print.total_in_words, WORDS_WIDTH, |s| {
            self.fonts.regular.width_mm(s, BODY_SIZE)
        });
        let (lines, bottom) = self.lines(LEFT, top + 2.0 * LINE_HEIGHT, WORDS_WIDTH, &words);
        ([label, lines].concat(), bottom)
    }

    /// Notes and terms on the left, signature on the right, below the totals
    fn closing(&self, top: f32) -> Block {
        let (column, column_bottom) = self.closing_column(top);
        (
            [column, self.signature(top)].concat(),
            column_bottom.max(top + SIGNATURE_HEIGHT),
        )
    }

    /// The left column below the totals, top to bottom: notes, payment details, then terms
    fn closing_column(&self, top: f32) -> Block {
        let (notes, bottom) = self.section(top, "Notes", &self.print.notes, WORDS_WIDTH);
        let (payment, bottom) = self.payment(next_top(&notes, bottom));
        let (terms, bottom) = self.section(
            next_top(&payment, bottom),
            "Terms & Conditions",
            &self.options.terms,
            WORDS_WIDTH,
        );
        ([notes, payment, terms].concat(), bottom)
    }

    /// "Payment details" with the bank account and UPI ID, and the UPI QR code beside them when
    /// there is one; nothing when there are no payment details
    fn payment(&self, top: f32) -> Block {
        let Some(payment) = &self.print.payment else {
            return (Vec::new(), top);
        };
        let lines = payment_lines(payment);
        let Some(modules) = self.qr else {
            return self.section(top, "Payment details", &lines, WORDS_WIDTH);
        };
        let (details, details_bottom) =
            self.section(top, "Payment details", &lines, WORDS_WIDTH - QR_COLUMN);
        let (code, code_bottom) = self.qr_code(modules, top);
        ([details, code].concat(), details_bottom.max(code_bottom))
    }

    /// The QR code centred in the right of the payment block, with [`QR_CAPTION`] under it
    fn qr_code(&self, modules: &[Vec<bool>], top: f32) -> Block {
        let centre = LEFT + WORDS_WIDTH - QR_COLUMN / 2.0;
        let code_top = top + 1.0;
        let quiet_zone = QR_QUIET_MODULES * QR_SIZE / modules.len().max(1) as f32;
        let caption_y = code_top + QR_SIZE + quiet_zone + LINE_HEIGHT / 2.0;
        let caption = grey_text(
            &self.fonts.regular,
            SMALL_SIZE,
            MUTED,
            (centre, caption_y),
            Align::Center,
            QR_CAPTION,
        );
        let code = qr_ops(modules, (centre - QR_SIZE / 2.0, code_top), QR_SIZE);
        ([code, caption].concat(), caption_y)
    }

    /// A bold `heading` and `lines` wrapped to `width` below it, or nothing when `lines` is
    /// empty
    fn section(&self, top: f32, heading: &str, lines: &[String], width: f32) -> Block {
        if lines.is_empty() {
            return (Vec::new(), top);
        }
        let heading = text(
            &self.fonts.bold,
            BODY_SIZE,
            LEFT,
            top + LINE_HEIGHT,
            Align::Left,
            heading,
        );
        let (lines, bottom) = self.lines(LEFT, top + 2.0 * LINE_HEIGHT, width, lines);
        ([heading, lines].concat(), bottom)
    }

    /// "For" the seller, a signature line and "Authorised Signatory", on the right
    fn signature(&self, top: f32) -> Vec<Op> {
        let signer = self.fit(
            &self.fonts.bold,
            BODY_SIZE,
            &format!("For {}", self.print.seller.name),
            RIGHT - TOTALS_LABEL_X,
        );
        let line_y = top + SIGNATURE_HEIGHT - 5.0;
        [
            text(
                &self.fonts.bold,
                BODY_SIZE,
                RIGHT,
                top + LINE_HEIGHT,
                Align::Right,
                &signer,
            ),
            vec![rule(RIGHT - SIGNATURE_WIDTH, RIGHT, line_y)],
            text(
                &self.fonts.regular,
                SMALL_SIZE,
                RIGHT,
                line_y + LINE_HEIGHT,
                Align::Right,
                "Authorised Signatory",
            ),
        ]
        .concat()
    }

    /// Any footer note and the page number at the foot of every page
    fn page_strip(&self, number: usize, count: usize) -> Vec<Op> {
        let y = PAGE_HEIGHT - MARGIN;
        let page = format!("Page {number} of {count}");
        let face = &self.fonts.regular;
        [
            vec![rule(LEFT, RIGHT, y - LINE_HEIGHT)],
            grey_text(
                face,
                SMALL_SIZE,
                MUTED,
                (LEFT, y),
                Align::Left,
                self.options.footer_note.as_deref().unwrap_or_default(),
            ),
            grey_text(face, SMALL_SIZE, MUTED, (RIGHT, y), Align::Right, &page),
        ]
        .concat()
    }

    /// Body-size lines from baseline `y` down, each wrapped to `width`
    fn lines(&self, x: f32, y: f32, width: f32, lines: &[String]) -> Block {
        let face = &self.fonts.regular;
        let wrapped = self.wrapped(width, lines);
        let ops = wrapped
            .iter()
            .enumerate()
            .flat_map(|(index, line)| {
                text(
                    face,
                    BODY_SIZE,
                    x,
                    y + LINE_HEIGHT * index as f32,
                    Align::Left,
                    line,
                )
            })
            .collect();
        (
            ops,
            y + LINE_HEIGHT * wrapped.len().saturating_sub(1) as f32,
        )
    }

    /// Body-size `lines`, each wrapped to `width`
    fn wrapped(&self, width: f32, lines: &[String]) -> Vec<String> {
        let face = &self.fonts.regular;
        lines
            .iter()
            .flat_map(|line| wrap(line, width, |s| face.width_mm(s, BODY_SIZE)))
            .collect()
    }

    /// `text` cut to fit `width` in `face` at `size`
    fn fit(&self, face: &Face, size: f32, text: &str, width: f32) -> String {
        fit(text, width, |s| face.width_mm(s, size))
    }

    /// An amount with the currency label in front
    fn money(&self, amount: &str) -> String {
        match self.options.currency_label.as_str() {
            "" => amount.to_string(),
            label => format!("{label} {amount}"),
        }
    }
}

/// Address lines, then `State: ` and `state` when given, then the GSTIN, or
/// `GSTIN: Unregistered` for a party without one
fn party_lines(party: &InvoiceParty, state: Option<&str>) -> Vec<String> {
    let identity = party.gstin.as_ref().map_or_else(
        || UNREGISTERED_GSTIN_LINE.to_string(),
        |gstin| format!("GSTIN: {gstin}"),
    );
    party
        .address
        .iter()
        .cloned()
        .chain(state.map(|state| format!("State: {state}")))
        .chain(std::iter::once(identity))
        .collect()
}

/// The delivery address lines, then the GSTIN only when the delivery party has one
fn ship_to_lines(party: &InvoiceParty) -> Vec<String> {
    party
        .address
        .iter()
        .cloned()
        .chain(party.gstin.as_ref().map(|gstin| format!("GSTIN: {gstin}")))
        .collect()
}

/// The lines of the payment block: payee, bank, account number, IFSC and UPI ID, each when set
fn payment_lines(payment: &PaymentDetails) -> Vec<String> {
    [
        ("Name", &payment.payee_name),
        ("Bank", &payment.bank_name),
        ("A/c No", &payment.account_number),
        ("IFSC", &payment.ifsc),
        ("UPI", &payment.upi_id),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.as_ref().map(|value| format!("{label}: {value}")))
    .collect()
}

/// Where the block after one that ends at `bottom` starts: a gap below it, or at `bottom` when
/// the block drew nothing
fn next_top(ops: &[Op], bottom: f32) -> f32 {
    if ops.is_empty() {
        bottom
    } else {
        bottom + BLOCK_GAP
    }
}

/// The totals lines above the rule, as label and amount: the subtotal and the invoice discount
/// when there are discounts, then the taxable value, each tax and the total tax
fn summary_lines(print: &InvoicePrint) -> Vec<(&str, String)> {
    let subtotal = print
        .shows_subtotal()
        .then(|| (SUBTOTAL_LABEL, print.subtotal.clone()));
    let discount = print
        .discount
        .as_ref()
        .zip(print.discount_label.as_ref())
        .map(|(amount, label)| (label.as_str(), format!("-{amount}")));
    subtotal
        .into_iter()
        .chain(discount)
        .chain(std::iter::once((
            "Taxable Value",
            print.taxable_value.clone(),
        )))
        .chain(
            print
                .tax_lines
                .iter()
                .map(|line| (line.label.as_str(), line.amount.clone())),
        )
        .chain(std::iter::once(("Total Tax", print.total_tax.clone())))
        .collect()
}

/// A row's cells, in column order, with the discount cell when the table has that column
fn cells(row: &PrintRow, with_discount: bool) -> Vec<String> {
    let discount = with_discount.then(|| row.discount.clone().unwrap_or_default());
    [
        row.serial.to_string(),
        row.hsn_sac.clone(),
        row.description.clone(),
        row.quantity.clone(),
        row.rate.clone(),
    ]
    .into_iter()
    .chain(discount)
    .chain([
        row.taxable_value.clone(),
        row.gst_rate.clone(),
        row.tax.clone(),
        row.amount.clone(),
    ])
    .collect()
}

/// The size, at most `size`, at which `text` in `face` fits in `width` millimetres
fn shrink_to_fit(face: &Face, size: f32, text: &str, width: f32) -> f32 {
    let natural = face.width_mm(text, size);
    if natural <= width {
        size
    } else {
        // Text width is proportional to its size
        size * width / natural
    }
}

/// Rows of `pitch` millimetres in `height` millimetres, rounded by `round`
fn row_count(height: f32, round: fn(f32) -> f32, pitch: f32) -> usize {
    // Non-negative and bounded by the page height over the pitch, so the cast is exact
    round(height / pitch).max(0.0) as usize
}
