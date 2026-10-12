//! Reads KiCad symbol libraries. A library is either a `.kicad_symdir` directory holding one
//! `.kicad_sym` file per symbol, or a single `.kicad_sym` file holding every symbol.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tron_core::sexpr::SExpr;
use tron_core::units::{Angle, Nm, Point};

use crate::error::LibraryError;
use crate::kicad::{
    angle, child_text, children, flag, length, library_files, library_name, parse_file, point,
    points, property, required, stroke_width, text_arg,
};
use crate::preview::{Fill, Pin, Shape, SymbolPreview, Xy};

/// The width KiCad draws a symbol stroke whose width is 0: 6 mils.
const DEFAULT_LINE_WIDTH: Nm = Nm(152_400);
/// The length KiCad gives a pin that declares none: 100 mils.
const DEFAULT_PIN_LENGTH: Nm = Nm(2_540_000);

/// One symbol as stored in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    pub lib: String,
    pub name: String,
    pub description: String,
    pub keywords: String,
    /// A URL, or empty.
    pub datasheet: String,
    /// The reference designator prefix, such as `R` or `U`.
    pub reference: String,
    /// The default footprint as `Library:Footprint`, or empty.
    pub footprint: String,
    /// Space-separated footprint name patterns that fit the symbol.
    pub footprint_filters: String,
    /// The symbol this one takes its graphics and pins from.
    pub extends: Option<String>,
    pub unit_count: u32,
    /// The number of distinct pin numbers across every unit.
    pub pin_count: u32,
    pub power: bool,
    pub preview: SymbolPreview,
}

struct Definition<'a> {
    path: &'a Path,
    node: &'a SExpr,
}

struct Unit<'a> {
    number: u32,
    style: u32,
    node: &'a SExpr,
}

/// Reads every symbol in the library at `path`, sorted by name.
pub fn read_symbol_library(path: &Path) -> Result<Vec<SymbolRecord>, LibraryError> {
    let lib = library_name(path)?;
    let files = if path.is_dir() {
        library_files(path, "kicad_sym")?
    } else {
        vec![path.to_path_buf()]
    };
    let mut documents = Vec::with_capacity(files.len());
    for file in files {
        let root = parse_file(&file)?;
        if root.head() != Some("kicad_symbol_lib") {
            return Err(LibraryError::invalid(
                &file,
                "is not a KiCad symbol library",
            ));
        }
        documents.push((file, root));
    }

    let mut definitions = BTreeMap::new();
    for (file, root) in &documents {
        for node in children(root, "symbol") {
            let Some(name) = text_arg(node, 0) else {
                return Err(LibraryError::invalid(file, "has a symbol without a name"));
            };
            let definition = Definition { path: file, node };
            if definitions.insert(name, definition).is_some() {
                let message = format!("defines symbol {name:?}, which {lib} already has");
                return Err(LibraryError::invalid(file, message));
            }
        }
    }

    let mut records = Vec::with_capacity(definitions.len());
    for (name, definition) in &definitions {
        match symbol(&lib, name, &definitions) {
            Ok(record) => records.push(record),
            Err(message) => {
                let message = format!("symbol {name:?}: {message}");
                return Err(LibraryError::invalid(definition.path, message));
            }
        }
    }
    Ok(records)
}

fn symbol(
    lib: &str,
    name: &str,
    definitions: &BTreeMap<&str, Definition<'_>>,
) -> Result<SymbolRecord, String> {
    let chain = ancestry(name, definitions)?;
    let own = chain[0];
    let base = chain[chain.len() - 1];
    let text = |key: &str| {
        chain
            .iter()
            .find_map(|definition| property(definition.node, key))
            .unwrap_or_default()
            .to_owned()
    };

    let units = units(base.node)?;
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

    let mut datasheet = text("Datasheet");
    if datasheet == "~" {
        datasheet.clear();
    }
    Ok(SymbolRecord {
        lib: lib.to_owned(),
        name: name.to_owned(),
        description: text("Description"),
        keywords: text("ki_keywords"),
        datasheet,
        reference: text("Reference"),
        footprint: text("Footprint"),
        footprint_filters: text("ki_fp_filters"),
        extends: child_text(own.node, "extends").map(str::to_owned),
        unit_count,
        pin_count: u32::try_from(pin_numbers.len()).unwrap_or(u32::MAX),
        power: chain
            .iter()
            .any(|definition| flag(definition.node, "power")),
        preview: SymbolPreview {
            shapes,
            pins,
            pin_numbers_visible: !hidden(base.node, "pin_numbers"),
            pin_names_visible: !hidden(base.node, "pin_names"),
        },
    })
}

/// The symbol followed by each symbol it extends, ending with the one that has the graphics.
fn ancestry<'m, 'a>(
    name: &str,
    definitions: &'m BTreeMap<&'a str, Definition<'a>>,
) -> Result<Vec<&'m Definition<'a>>, String> {
    let mut chain = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current = name;
    loop {
        if !visited.insert(current) {
            return Err(format!("its extends chain loops back to {current:?}"));
        }
        let Some(definition) = definitions.get(current) else {
            return Err(format!(
                "it extends {current:?}, which is not in the library"
            ));
        };
        chain.push(definition);
        match child_text(definition.node, "extends") {
            Some(parent) => current = parent,
            None => return Ok(chain),
        }
    }
}

/// The unit drawings of a symbol, named `<symbol>_<unit>_<style>`. Unit 0 is shared by every
/// unit and style 0 by every body style.
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

fn unit_and_style(name: &str) -> Option<(u32, u32)> {
    let mut parts = name.rsplitn(3, '_');
    let style = parts.next()?.parse().ok()?;
    let unit = parts.next()?.parse().ok()?;
    parts.next()?;
    Some((unit, style))
}

fn hidden(node: &SExpr, head: &str) -> bool {
    node.child(head).is_some_and(|child| flag(child, "hide"))
}

/// Symbol files put Y up; the index stores Y down like the schematic.
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
}
