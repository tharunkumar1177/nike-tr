//! Symbol preview geometry: the shapes and pins of a KiCad symbol definition, extracted so
//! the library browser and the schematic canvas can draw a part without parsing KiCad text
//! at render time.
//!
//! Every coordinate and length is an `i64` count of nanometers in a Y-down space, the same
//! as schematic and board geometry. KiCad symbol files put Y up, so extraction negates Y.
//! Points serialize as `[x, y]` to keep the library index small.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::kicad_value::{
    angle, child_text, children, flag, length, point, points, required, stroke_width, text_arg,
};
use crate::sexpr::SExpr;
use crate::units::{Angle, Nm, Point};

/// The width KiCad draws a symbol stroke whose width is 0: 6 mils.
const DEFAULT_LINE_WIDTH: Nm = Nm(152_400);
/// The length KiCad gives a pin that declares none: 100 mils.
const DEFAULT_PIN_LENGTH: Nm = Nm(2_540_000);

/// A point in nanometers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Xy(
    #[ts(type = "number")] pub i64,
    #[ts(type = "number")] pub i64,
);

impl From<Point> for Xy {
    fn from(point: Point) -> Xy {
        Xy(point.x.0, point.y.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Fill {
    None,
    /// Filled with the stroke color.
    Outline,
    /// Filled with the symbol body color.
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Shape {
    /// Connected line segments. The path is closed only if its last point repeats the first.
    Polyline {
        points: Vec<Xy>,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    /// A closed polygon.
    Polygon {
        points: Vec<Xy>,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    Rect {
        start: Xy,
        end: Xy,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    Circle {
        center: Xy,
        #[ts(type = "number")]
        radius: i64,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    /// The circular arc from `start` through `mid` to `end`.
    Arc {
        start: Xy,
        mid: Xy,
        end: Xy,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    /// A cubic Bézier curve given by its four control points.
    Bezier {
        points: Vec<Xy>,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
}

/// The unit 1 drawing of a symbol in its normal body style, with unit 0 graphics shared by all
/// units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SymbolPreview {
    pub shapes: Vec<Shape>,
    /// Visible pins only.
    pub pins: Vec<Pin>,
    pub pin_numbers_visible: bool,
    pub pin_names_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Pin {
    /// Where a wire connects.
    pub connection: Xy,
    /// Where the pin line meets the symbol body.
    pub body: Xy,
    pub number: String,
    pub name: String,
    /// The KiCad electrical type, such as `input`, `passive`, or `power_in`.
    pub electrical: String,
}

/// The geometry a symbol definition contributes to previews and the library index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolGraphics {
    pub preview: SymbolPreview,
    /// The highest unit number, at least 1.
    pub unit_count: u32,
    /// The number of distinct pin numbers across every unit.
    pub pin_count: u32,
}

/// Extracts the geometry of a KiCad symbol definition: either its own `(symbol ...)` node
/// from a library, or the flattened form embedded in a schematic's `(lib_symbols)`. Hidden
/// pins are counted but not drawn. Fails when a unit drawing is not named
/// `<symbol>_<unit>_<style>` or a graphic value is malformed.
pub fn symbol_graphics(definition: &SExpr) -> Result<SymbolGraphics, String> {
    let units = units(definition)?;
    let unit_count = units
        .iter()
        .map(|unit| unit.number)
        .max()
        .unwrap_or(1)
        .max(1);
    let preview_unit = units
        .iter()
        .map(|unit| unit.number)
        .filter(|&number| number > 0)
        .min()
        .unwrap_or(1);

    let mut shapes = Vec::new();
    let mut pins = Vec::new();
    let mut pin_numbers = BTreeSet::new();
    for unit in units.iter().filter(|unit| unit.style <= 1) {
        let shown = unit.number == 0 || unit.number == preview_unit;
        for item in unit.node.args() {
            match item.head() {
                Some("pin") => {
                    let (pin, hidden) = read_pin(item)?;
                    if !pin.number.is_empty() {
                        pin_numbers.insert(pin.number.clone());
                    }
                    if shown && !hidden {
                        pins.push(pin);
                    }
                }
                _ if shown => shapes.extend(read_shape(item)?),
                _ => {}
            }
        }
    }

    Ok(SymbolGraphics {
        preview: SymbolPreview {
            shapes,
            pins,
            pin_numbers_visible: !hidden(definition, "pin_numbers"),
            pin_names_visible: !hidden(definition, "pin_names"),
        },
        unit_count,
        pin_count: u32::try_from(pin_numbers.len()).unwrap_or(u32::MAX),
    })
}

/// The preview geometry of a symbol definition, such as one embedded in `(lib_symbols)`.
pub fn symbol_preview(definition: &SExpr) -> Result<SymbolPreview, String> {
    Ok(symbol_graphics(definition)?.preview)
}

/// The pin numbers of every unit drawing in a `(symbol ...)` definition, in file order. A
/// placed symbol gets one `(pin ...)` entry per number. KiCad writes `~` for no number,
/// which reads as empty.
pub fn definition_pin_numbers(definition: &SExpr) -> Vec<String> {
    let mut numbers = Vec::new();
    for unit in children(definition, "symbol") {
        for pin in children(unit, "pin") {
            numbers.push(pin_text(pin, "number"));
        }
    }
    numbers
}

/// Splits a unit drawing name into its unit and body style numbers: `R_1_1` is unit 1,
/// style 1. Returns `None` when the name does not end in `_<unit>_<style>`.
pub fn unit_and_style(name: &str) -> Option<(u32, u32)> {
    let mut parts = name.rsplitn(3, '_');
    let style = parts.next()?.parse().ok()?;
    let unit = parts.next()?.parse().ok()?;
    parts.next()?;
    Some((unit, style))
}

struct Unit<'a> {
    number: u32,
    style: u32,
    node: &'a SExpr,
}

/// The unit drawings of a definition, named `<symbol>_<unit>_<style>`. Unit 0 is shared by
/// every unit and style 0 by every body style.
fn units(base: &SExpr) -> Result<Vec<Unit<'_>>, String> {
    let mut units = Vec::new();
    for node in children(base, "symbol") {
        let name = text_arg(node, 0).unwrap_or_default();
        let Some((number, style)) = unit_and_style(name) else {
            return Err(format!(
                "unit {name:?} is not named <symbol>_<unit>_<style>"
            ));
        };
        units.push(Unit {
            number,
            style,
            node,
        });
    }
    Ok(units)
}

fn hidden(node: &SExpr, head: &str) -> bool {
    node.child(head).is_some_and(|child| flag(child, "hide"))
}

/// Symbol files put Y up; previews store Y down like the schematic.
fn flip(point: Point) -> Xy {
    Xy(point.x.0, -point.y.0)
}

fn flipped(node: &SExpr, head: &str) -> Result<Xy, String> {
    Ok(flip(point(required(node, head)?)?))
}

fn flipped_points(node: &SExpr) -> Result<Vec<Xy>, String> {
    Ok(points(node)?.into_iter().map(flip).collect())
}

fn read_shape(item: &SExpr) -> Result<Option<Shape>, String> {
    let width = match stroke_width(item)? {
        Some(width) if width.0 > 0 => width.0,
        _ => DEFAULT_LINE_WIDTH.0,
    };
    let fill = match item.child("fill").and_then(|fill| child_text(fill, "type")) {
        None | Some("none") => Fill::None,
        Some("background") => Fill::Background,
        Some(_) => Fill::Outline,
    };
    let shape = match item.head() {
        Some("rectangle") => Shape::Rect {
            start: flipped(item, "start")?,
            end: flipped(item, "end")?,
            width,
            fill,
        },
        Some("polyline") => Shape::Polyline {
            points: flipped_points(item)?,
            width,
            fill,
        },
        Some("circle") => Shape::Circle {
            center: flipped(item, "center")?,
            radius: length(required(item, "radius")?, 0)?.0,
            width,
            fill,
        },
        Some("arc") => Shape::Arc {
            start: flipped(item, "start")?,
            mid: flipped(item, "mid")?,
            end: flipped(item, "end")?,
            width,
            fill,
        },
        Some("bezier") => Shape::Bezier {
            points: flipped_points(item)?,
            width,
            fill,
        },
        _ => return Ok(None),
    };
    Ok(Some(shape))
}

/// Reads a pin and whether it is hidden.
fn read_pin(item: &SExpr) -> Result<(Pin, bool), String> {
    let at = required(item, "at")?;
    let connection = point(at)?;
    let direction = if at.args().len() > 2 {
        angle(at, 2)?
    } else {
        Angle::ZERO
    };
    let pin_length = match item.child("length") {
        Some(node) => length(node, 0)?,
        None => DEFAULT_PIN_LENGTH,
    };
    let (dx, dy) = pin_offset(direction, pin_length);
    let pin = Pin {
        connection: flip(connection),
        body: flip(connection.offset(dx, dy)),
        number: pin_text(item, "number"),
        name: pin_text(item, "name"),
        electrical: text_arg(item, 0).unwrap_or("unspecified").to_owned(),
    };
    Ok((pin, flag(item, "hide")))
}

/// A pin name or number, where KiCad writes `~` for none.
fn pin_text(item: &SExpr, head: &str) -> String {
    match child_text(item, head) {
        None | Some("~") => String::new(),
        Some(text) => text.to_owned(),
    }
}

/// The offset from a pin's connection point to the body, in the Y-up space of symbol files.
fn pin_offset(direction: Angle, length: Nm) -> (Nm, Nm) {
    match direction.decidegrees() {
        0 => (length, Nm::ZERO),
        900 => (Nm::ZERO, length),
        1800 => (-length, Nm::ZERO),
        2700 => (Nm::ZERO, -length),
        decidegrees => {
            let radians = (f64::from(decidegrees) / 10.0).to_radians();
            let length = length.0 as f64;
            let dx = (length * radians.cos()).round() as i64;
            let dy = (length * radians.sin()).round() as i64;
            (Nm(dx), Nm(dy))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    #[test]
    fn unit_names_split_into_unit_and_style() {
        assert_eq!(unit_and_style("R_1_1"), Some((1, 1)));
        assert_eq!(unit_and_style("Conn_01x02_0_2"), Some((0, 2)));
        assert_eq!(unit_and_style("R_1"), None);
        assert_eq!(unit_and_style("R_a_1"), None);
    }

    #[test]
    fn pins_point_from_the_connection_toward_the_body() {
        let length = Nm(2_540_000);
        let left = Angle::from_decidegrees(1800);
        assert_eq!(pin_offset(left, length), (Nm(-2_540_000), Nm::ZERO));
        let down = Angle::from_decidegrees(2700);
        assert_eq!(pin_offset(down, length), (Nm::ZERO, Nm(-2_540_000)));
        let diagonal = pin_offset(Angle::from_decidegrees(450), Nm(1_000_000));
        assert_eq!(diagonal, (Nm(707_107), Nm(707_107)));
    }

    fn r_definition() -> SExpr {
        sexpr::parse(
            "(symbol \"Device:R\" (pin_numbers (hide yes)) (pin_names (offset 0)) \
            (property \"Reference\" \"R\" (at 2.032 0 90)) \
            (symbol \"R_0_1\" (rectangle (start -1.016 -2.54) (end 1.016 2.54) \
            (stroke (width 0.254) (type default)) (fill (type none)))) \
            (symbol \"R_1_1\" \
            (pin passive line (at 0 3.81 270) (length 1.27) (name \"\") (number \"1\")) \
            (pin passive line (at 0 -3.81 90) (length 1.27) (name \"\") (number \"2\"))))",
        )
        .unwrap()
    }

    #[test]
    fn graphics_come_y_down_from_unit_one_and_shared_graphics() {
        let graphics = symbol_graphics(&r_definition()).unwrap();
        assert_eq!(
            graphics.preview.shapes,
            [Shape::Rect {
                start: Xy(-1_016_000, 2_540_000),
                end: Xy(1_016_000, -2_540_000),
                width: 254_000,
                fill: Fill::None,
            }]
        );
        assert_eq!(
            graphics.preview.pins,
            [
                Pin {
                    connection: Xy(0, -3_810_000),
                    body: Xy(0, -2_540_000),
                    number: "1".to_owned(),
                    name: String::new(),
                    electrical: "passive".to_owned(),
                },
                Pin {
                    connection: Xy(0, 3_810_000),
                    body: Xy(0, 2_540_000),
                    number: "2".to_owned(),
                    name: String::new(),
                    electrical: "passive".to_owned(),
                },
            ]
        );
        assert!(!graphics.preview.pin_numbers_visible);
        assert!(graphics.preview.pin_names_visible);
        assert_eq!((graphics.unit_count, graphics.pin_count), (1, 2));
        assert_eq!(symbol_preview(&r_definition()).unwrap(), graphics.preview);
    }

    #[test]
    fn pin_numbers_cover_every_unit_in_file_order() {
        assert_eq!(definition_pin_numbers(&r_definition()), ["1", "2"]);
        let power = sexpr::parse(
            "(symbol \"P\" (symbol \"P_0_1\" (pin power_in line (at 0 0) (name \"\") \
            (number \"~\"))))",
        )
        .unwrap();
        assert_eq!(definition_pin_numbers(&power), [""]);
    }

    #[test]
    fn malformed_unit_names_are_rejected() {
        let broken = sexpr::parse("(symbol \"S\" (symbol \"S_1\" ()))").unwrap();
        let error = symbol_graphics(&broken).unwrap_err();
        assert!(error.contains("<symbol>_<unit>_<style>"), "{error}");
    }
}
