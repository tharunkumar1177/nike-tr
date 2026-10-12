//! Reads KiCad footprint libraries: a `.pretty` directory holding one `.kicad_mod` file per
//! footprint.

use std::collections::BTreeSet;
use std::path::Path;

use tron_core::sexpr::SExpr;
use tron_core::units::{Angle, Point};

use crate::error::LibraryError;
use crate::kicad::{
    angle, child_text, length, library_files, library_name, number, parse_file, point, points,
    required, stroke_width, text_arg,
};
use crate::preview::{
    Fill, FootprintGraphic, FootprintPreview, Layer, Pad, PadKind, PadShape, Shape, Xy,
};

/// How a footprint is assembled, from its `(attr ...)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mount {
    Smd,
    ThroughHole,
    Unspecified,
}

impl Mount {
    pub fn as_str(self) -> &'static str {
        match self {
            Mount::Smd => "smd",
            Mount::ThroughHole => "through_hole",
            Mount::Unspecified => "unspecified",
        }
    }

    pub fn from_name(name: &str) -> Option<Mount> {
        match name {
            "smd" => Some(Mount::Smd),
            "through_hole" => Some(Mount::ThroughHole),
            "unspecified" => Some(Mount::Unspecified),
            _ => None,
        }
    }
}

/// One footprint as stored in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootprintRecord {
    pub lib: String,
    pub name: String,
    pub description: String,
    /// Space-separated search tags.
    pub tags: String,
    /// The number of distinct pad numbers on pads that can carry a net.
    pub pad_count: u32,
    pub mount: Mount,
    pub preview: FootprintPreview,
}

/// Reads every footprint in the `.pretty` directory `dir`, sorted by name.
pub fn read_footprint_library(dir: &Path) -> Result<Vec<FootprintRecord>, LibraryError> {
    let lib = library_name(dir)?;
    let mut records = Vec::new();
    for file in library_files(dir, "kicad_mod")? {
        let root = parse_file(&file)?;
        let record = footprint(&lib, &root)
            .map_err(|message| LibraryError::invalid(&file, message))?;
        records.push(record);
    }
    records.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(records)
}

fn footprint(lib: &str, root: &SExpr) -> Result<FootprintRecord, String> {
    if !matches!(root.head(), Some("footprint" | "module")) {
        return Err("is not a KiCad footprint".to_owned());
    }
    let Some(name) = text_arg(root, 0) else {
        return Err("has a footprint without a name".to_owned());
    };

    let mut graphics = Vec::new();
    let mut pads = Vec::new();
    for item in root.args() {
        if item.head() == Some("pad") {
            pads.push(read_pad(item)?);
            continue;
        }
        let Some(layer) = child_text(item, "layer").and_then(graphic_layer) else {
            continue;
        };
        if let Some(shape) = read_shape(item)? {
            graphics.push(FootprintGraphic { layer, shape });
        }
    }

    let pad_count = pads
        .iter()
        .filter(|pad| pad.kind != PadKind::NonPlatedHole && !pad.number.is_empty())
        .map(|pad| pad.number.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let attributes: Vec<&str> = match root.child("attr") {
        Some(attr) => attr.args().iter().filter_map(SExpr::text).collect(),
        None => Vec::new(),
    };
    let mount = if attributes.contains(&"smd") {
        Mount::Smd
    } else if attributes.contains(&"through_hole") {
        Mount::ThroughHole
    } else {
        Mount::Unspecified
    };

    Ok(FootprintRecord {
        lib: lib.to_owned(),
        name: name.to_owned(),
        description: child_text(root, "descr").unwrap_or_default().to_owned(),
        tags: child_text(root, "tags").unwrap_or_default().to_owned(),
        pad_count: u32::try_from(pad_count).unwrap_or(u32::MAX),
        mount,
        preview: FootprintPreview { graphics, pads },
    })
}

fn graphic_layer(name: &str) -> Option<Layer> {
    match name {
        "F.SilkS" => Some(Layer::Silkscreen),
        "F.Fab" => Some(Layer::Fabrication),
        "F.CrtYd" => Some(Layer::Courtyard),
        _ => None,
    }
}

fn xy(node: &SExpr, head: &str) -> Result<Xy, String> {
    Ok(point(required(node, head)?)?.into())
}

fn distance(a: Point, b: Point) -> i64 {
    let dx = (b.x.0 - a.x.0) as f64;
    let dy = (b.y.0 - a.y.0) as f64;
    dx.hypot(dy).round() as i64
}

fn read_shape(item: &SExpr) -> Result<Option<Shape>, String> {
    let width = stroke_width(item)?.map_or(0, |width| width.0);
    let fill = match child_text(item, "fill") {
        Some("yes" | "solid") => Fill::Outline,
        _ => Fill::None,
    };
    let shape = match item.head() {
        Some("fp_line") => Shape::Polyline {
            points: vec![xy(item, "start")?, xy(item, "end")?],
            width,
            fill: Fill::None,
        },
        Some("fp_rect") => Shape::Rect {
            start: xy(item, "start")?,
            end: xy(item, "end")?,
            width,
            fill,
        },
        Some("fp_circle") => {
            let center = point(required(item, "center")?)?;
            let edge = point(required(item, "end")?)?;
            Shape::Circle {
                center: center.into(),
                radius: distance(center, edge),
                width,
                fill,
            }
        }
        Some("fp_arc") => Shape::Arc {
            start: xy(item, "start")?,
            mid: xy(item, "mid")?,
            end: xy(item, "end")?,
            width,
            fill: Fill::None,
        },
        Some("fp_poly") => Shape::Polygon {
            points: points(item)?.into_iter().map(Xy::from).collect(),
            width,
            fill,
        },
        Some("fp_curve") => Shape::Bezier {
            points: points(item)?.into_iter().map(Xy::from).collect(),
            width,
            fill: Fill::None,
        },
        _ => return Ok(None),
    };
    Ok(Some(shape))
}

fn read_pad(item: &SExpr) -> Result<Pad, String> {
    let number = text_arg(item, 0).unwrap_or_default().to_owned();
    let kind = match text_arg(item, 1) {
        Some("smd") => PadKind::Smd,
        Some("thru_hole") => PadKind::ThroughHole,
        Some("np_thru_hole") => PadKind::NonPlatedHole,
        Some("connect") => PadKind::Connect,
        other => return Err(format!("pad {number:?} has an unknown type {other:?}")),
    };
    let shape = match text_arg(item, 2) {
        Some("rect") => PadShape::Rect,
        Some("roundrect") => PadShape::RoundRect,
        Some("circle") => PadShape::Circle,
        Some("oval") => PadShape::Oval,
        Some("trapezoid") => PadShape::Trapezoid,
        Some("custom") => PadShape::Custom,
        other => return Err(format!("pad {number:?} has an unknown shape {other:?}")),
    };
    let at = required(item, "at")?;
    let rotation = if at.args().len() > 2 {
        angle(at, 2)?
    } else {
        Angle::ZERO
    };
    let size = required(item, "size")?;
    let (width, height) = (length(size, 0)?.0, length(size, 1)?.0);
    let corner_radius = match (shape, item.child("roundrect_rratio")) {
        (PadShape::RoundRect, Some(ratio)) => {
            (width.min(height) as f64 * number(ratio, 0)?).round() as i64
        }
        _ => 0,
    };
    let drill = match item.child("drill") {
        Some(node) => drill_size(node)?,
        None => None,
    };
    Ok(Pad {
        number,
        kind,
        shape,
        center: point(at)?.into(),
        size: Xy(width, height),
        rotation: rotation.decidegrees(),
        corner_radius,
        drill,
    })
}

/// Reads `(drill d)` or `(drill oval w h)`. A drill node holding only an offset has no size.
fn drill_size(node: &SExpr) -> Result<Option<Xy>, String> {
    let first = usize::from(text_arg(node, 0) == Some("oval"));
    if text_arg(node, first).is_none() {
        return Ok(None);
    }
    let width = length(node, first)?.0;
    let height = match text_arg(node, first + 1) {
        Some(_) => length(node, first + 1)?.0,
        None => width,
    };
    Ok(Some(Xy(width, height)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tron_core::sexpr::parse;

    #[test]
    fn drills_read_round_oval_and_offset_only_forms() {
        let round = parse("(drill 0.9)").unwrap();
        assert_eq!(drill_size(&round), Ok(Some(Xy(900_000, 900_000))));
        let oval = parse("(drill oval 1 2.5)").unwrap();
        assert_eq!(drill_size(&oval), Ok(Some(Xy(1_000_000, 2_500_000))));
        let offset = parse("(drill (offset 0 0.5))").unwrap();
        assert_eq!(drill_size(&offset), Ok(None));
    }

    #[test]
    fn unknown_pad_types_are_rejected() {
        let pad = parse("(pad \"1\" mystery rect (at 0 0) (size 1 1))").unwrap();
        assert!(read_pad(&pad).unwrap_err().contains("unknown type"));
    }
}
