//! Typed readers for the values inside KiCad S-expression trees.
//!
//! Readers return a plain message on failure; callers attach the file path or the item
//! context. Lengths are read from millimeters and angles from degrees.

use crate::sexpr::SExpr;
use crate::units::{Angle, Nm, Point};

/// The children of `node` whose head is `head`.
pub fn children<'a>(node: &'a SExpr, head: &str) -> impl Iterator<Item = &'a SExpr> {
    node.args()
        .iter()
        .filter(move |item| item.head() == Some(head))
}

/// The child of `node` whose head is `head`, or a message naming both.
pub fn required<'a>(node: &'a SExpr, head: &str) -> Result<&'a SExpr, String> {
    let parent = node.head().unwrap_or("?");
    node.child(head)
        .ok_or_else(|| format!("({parent}) has no ({head})"))
}

/// The text of argument `index` when it is an atom or a string.
pub fn text_arg(node: &SExpr, index: usize) -> Option<&str> {
    node.args().get(index).and_then(SExpr::text)
}

/// The first argument of child `head`, such as `Resistor` in `(descr "Resistor")`.
pub fn child_text<'a>(node: &'a SExpr, head: &str) -> Option<&'a str> {
    node.child(head).and_then(|child| text_arg(child, 0))
}

/// The value of `(property "key" "value" ...)`.
pub fn property<'a>(node: &'a SExpr, key: &str) -> Option<&'a str> {
    children(node, "property")
        .find(|item| text_arg(item, 0) == Some(key))
        .and_then(|item| text_arg(item, 1))
}

/// Whether `node` holds `(head)`, `(head yes)`, or the older bare `head` atom.
pub fn flag(node: &SExpr, head: &str) -> bool {
    node.args().iter().any(|item| match item {
        SExpr::Atom(atom) => atom == head,
        SExpr::List(_) => item.head() == Some(head) && text_arg(item, 0) != Some("no"),
        SExpr::Str(_) => false,
    })
}

pub fn number(node: &SExpr, index: usize) -> Result<f64, String> {
    let head = node.head().unwrap_or("?");
    let Some(text) = text_arg(node, index) else {
        return Err(format!("({head}) is missing value {}", index + 1));
    };
    match text.parse::<f64>() {
        Ok(value) => Ok(value),
        Err(_) => Err(format!("({head}) has a non-numeric value {text:?}")),
    }
}

/// Argument `index` read as millimeters.
pub fn length(node: &SExpr, index: usize) -> Result<Nm, String> {
    let value = number(node, index)?;
    let head = node.head().unwrap_or("?");
    Nm::from_mm(value).map_err(|err| format!("({head}) length {value}: {err}"))
}

/// Argument `index` read as degrees.
pub fn angle(node: &SExpr, index: usize) -> Result<Angle, String> {
    let value = number(node, index)?;
    let head = node.head().unwrap_or("?");
    Angle::from_degrees(value).map_err(|err| format!("({head}) angle {value}: {err}"))
}

/// `(head x y ...)` read as a point in millimeters.
pub fn point(node: &SExpr) -> Result<Point, String> {
    Ok(Point::new(length(node, 0)?, length(node, 1)?))
}

/// The vertices of `(pts (xy x y) ...)`. An `(arc (start) (mid) (end))` vertex contributes
/// its three points.
pub fn points(node: &SExpr) -> Result<Vec<Point>, String> {
    let mut points = Vec::new();
    for item in required(node, "pts")?.args() {
        match item.head() {
            Some("xy") => points.push(point(item)?),
            Some("arc") => {
                for head in ["start", "mid", "end"] {
                    points.push(point(required(item, head)?)?);
                }
            }
            _ => {}
        }
    }
    Ok(points)
}

/// The stroke width of a graphic item, from `(stroke (width w))` or the older `(width w)`.
pub fn stroke_width(node: &SExpr) -> Result<Option<Nm>, String> {
    let width = node
        .child("stroke")
        .and_then(|stroke| stroke.child("width"))
        .or_else(|| node.child("width"));
    match width {
        Some(width) => length(width, 0).map(Some),
        None => Ok(None),
    }
}
