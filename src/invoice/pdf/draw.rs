//! Drawing primitives in millimetres measured from the top-left corner of an A4 page

use super::fonts::Face;
use printpdf::{
    Color, Greyscale, Line, LinePoint, Mm, Op, Point, Pt, TextItem, XObjectId, XObjectTransform,
};

/// A4 width in millimetres
pub(super) const PAGE_WIDTH: f32 = 210.0;
/// A4 height in millimetres
pub(super) const PAGE_HEIGHT: f32 = 297.0;
/// Thickness of rules, in points
const RULE_WIDTH_PT: f32 = 0.5;
/// Ink for text and rules
const BLACK: f32 = 0.0;
/// Millimetres in an inch, to turn a scale in millimetres per pixel into a DPI
const MM_PER_INCH: f32 = 25.4;

/// An image added to the document, with its size in pixels
pub(super) struct Image {
    /// Handle that draws the image
    pub(super) id: XObjectId,
    /// Width in pixels
    pub(super) width_px: usize,
    /// Height in pixels
    pub(super) height_px: usize,
}

/// How text sits relative to its x position
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Align {
    /// Starts at x
    Left,
    /// Centred on x
    Center,
    /// Ends at x
    Right,
}

/// A PDF point for a position measured from the top-left corner
fn point(x: f32, y: f32) -> Point {
    Point::new(Mm(x), Mm(PAGE_HEIGHT - y))
}

/// Ops that reset the pen at the start of a page
pub(super) fn page_setup() -> Vec<Op> {
    vec![
        Op::SetOutlineThickness {
            pt: Pt(RULE_WIDTH_PT),
        },
        Op::SetOutlineColor { col: grey(BLACK) },
        Op::SetFillColor { col: grey(BLACK) },
    ]
}

/// A grey level from 0 (black) to 1 (white)
fn grey(level: f32) -> Color {
    Color::Greyscale(Greyscale::new(level, None))
}

/// `text` in `face` at `size` points, on the baseline `y`, aligned to `x`
pub(super) fn text(face: &Face, size: f32, x: f32, y: f32, align: Align, text: &str) -> Vec<Op> {
    let x = match align {
        Align::Left => x,
        Align::Center => x - face.width_mm(text, size) / 2.0,
        Align::Right => x - face.width_mm(text, size),
    };
    vec![
        Op::StartTextSection,
        Op::SetFont {
            font: face.handle.clone(),
            size: Pt(size),
        },
        Op::SetTextCursor { pos: point(x, y) },
        Op::ShowText {
            items: vec![TextItem::Text(text.to_string())],
        },
        Op::EndTextSection,
    ]
}

/// Like [`text`], in grey
pub(super) fn grey_text(
    face: &Face,
    size: f32,
    level: f32,
    (x, y): (f32, f32),
    align: Align,
    content: &str,
) -> Vec<Op> {
    std::iter::once(Op::SetFillColor { col: grey(level) })
        .chain(text(face, size, x, y, align, content))
        .chain(std::iter::once(Op::SetFillColor { col: grey(BLACK) }))
        .collect()
}

/// A horizontal rule from `x1` to `x2` at `y`
pub(super) fn rule(x1: f32, x2: f32, y: f32) -> Op {
    Op::DrawLine {
        line: Line {
            points: [x1, x2]
                .into_iter()
                .map(|x| LinePoint {
                    p: point(x, y),
                    bezier: false,
                })
                .collect(),
            is_closed: false,
        },
    }
}

/// `image` scaled to fit a `width` by `height` box whose top-left corner is at (`x`, `y`),
/// keeping its aspect ratio, and the height it is drawn at; nothing for an empty image
pub(super) fn image(
    image: &Image,
    (x, y): (f32, f32),
    (width, height): (f32, f32),
) -> (Vec<Op>, f32) {
    if image.width_px == 0 || image.height_px == 0 {
        return (Vec::new(), 0.0);
    }
    let mm_per_px = (width / image.width_px as f32).min(height / image.height_px as f32);
    let drawn_height = mm_per_px * image.height_px as f32;
    let lower_left = point(x, y + drawn_height);
    let op = Op::UseXobject {
        id: image.id.clone(),
        transform: XObjectTransform {
            translate_x: Some(lower_left.x),
            translate_y: Some(lower_left.y),
            dpi: Some(MM_PER_INCH / mm_per_px),
            ..XObjectTransform::default()
        },
    };
    (vec![op], drawn_height)
}
