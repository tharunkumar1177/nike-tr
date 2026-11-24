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
    /// The footprint's `(footprint ...)` definition as KiCad text, exactly as the library file
    /// holds it. [`Index::footprint_definition`](crate::Index::footprint_definition) turns it
    /// into the form a board embeds.
    pub source: String,
}

/// Reads every footprint in the `.pretty` directory `dir`, sorted by name.
pub fn read_footprint_library(dir: &Path) -> Result<Vec<FootprintRecord>, LibraryError> {
    let lib = library_name(dir)?;
    let mut records = Vec::new();
    for file in library_files(dir, "kicad_mod")? {
        let root = parse_file(&file)?;
        let record =
            footprint(&lib, &root).map_err(|message| LibraryError::invalid(&file, message))?;
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
        source: root.to_pretty_string().trim_end().to_owned(),
    })
}

/// The library-file nodes a board footprint does not carry: they describe the `.kicad_mod`
/// file, not the part.
const FILE_ONLY_NODES: [&str; 3] = ["version", "generator", "generator_version"];

/// Turns a stored library footprint into the standalone form a `.kicad_pcb` embeds: a
/// `(footprint "<lib>:<name>" ...)` list without the library file's version and generator.
/// Legacy `(module ...)` definitions are renamed to `(footprint ...)`.
pub fn board_definition(lib: &str, root: &SExpr) -> Result<SExpr, String> {
    if !matches!(root.head(), Some("footprint" | "module")) {
        return Err("a definition is not a (footprint) list".to_owned());
    }
    let Some(name) = text_arg(root, 0) else {
        return Err("a definition has no name".to_owned());
    };
    let mut items = vec![
        SExpr::atom("footprint"),
        SExpr::string(format!("{lib}:{name}")),
    ];
    for item in root.args().iter().skip(1) {
        let file_only = item
            .head()
            .is_some_and(|head| FILE_ONLY_NODES.contains(&head));
        if !file_only {
            items.push(item.clone());
        }
    }
    Ok(SExpr::List(items))
}

/// The shapes of a footprint placed on a board, in the footprint's own frame: offsets as the
/// board file stores them, already mirrored for a back-side footprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedGeometry {
    /// Every pad that reads, in file order. Pad angles include the footprint's rotation.
    pub pads: Vec<Pad>,
    /// The smallest axis-aligned box `(min, max)` around the courtyard, or around the pads
    /// and graphics when the footprint has no courtyard. `None` when nothing has extent.
    pub extent: Option<(Xy, Xy)>,
}

/// Reads the pads and extent of a `(footprint ...)` node from a board. Pads and graphics
/// that don't read are left out rather than failing the footprint.
pub fn placed_geometry(node: &SExpr) -> PlacedGeometry {
    let pads: Vec<Pad> = children_named(node, "pad")
        .filter_map(|item| read_pad(item).ok())
        .collect();
    let mut courtyard = Extent::default();
    let mut outline = Extent::default();
    for item in node.args() {
        let Ok(Some(shape)) = read_shape(item) else {
            continue;
        };
        let target = match child_text(item, "layer") {
            Some("F.CrtYd" | "B.CrtYd") => &mut courtyard,
            _ => &mut outline,
        };
        target.add_shape(&shape);
    }
    for pad in &pads {
        let half = pad.size.0.max(pad.size.1) / 2;
        outline.add(Xy(pad.center.0 - half, pad.center.1 - half));
        outline.add(Xy(pad.center.0 + half, pad.center.1 + half));
    }
    PlacedGeometry {
        pads,
        extent: courtyard.bounds.or(outline.bounds),
    }
}

/// The direct children of `node` whose head is `head`.
fn children_named<'a>(node: &'a SExpr, head: &'a str) -> impl Iterator<Item = &'a SExpr> {
    node.args()
        .iter()
        .filter(move |item| item.head() == Some(head))
}

/// A growing axis-aligned box of points.
#[derive(Debug, Default)]
struct Extent {
    /// `(min, max)`, or `None` until a point is added.
    bounds: Option<(Xy, Xy)>,
}

impl Extent {
    /// Grows the box to include `point`.
    fn add(&mut self, point: Xy) {
        self.bounds = Some(match self.bounds {
            None => (point, point),
            Some((min, max)) => (
                Xy(min.0.min(point.0), min.1.min(point.1)),
                Xy(max.0.max(point.0), max.1.max(point.1)),
            ),
        });
    }

    /// Grows the box to include `shape`. Arcs and curves count by their defining points,
    /// which is close enough for picking and fitting.
    fn add_shape(&mut self, shape: &Shape) {
        match shape {
            Shape::Polyline { points, .. }
            | Shape::Polygon { points, .. }
            | Shape::Bezier { points, .. } => points.iter().for_each(|point| self.add(*point)),
            Shape::Rect { start, end, .. } => {
                self.add(*start);
                self.add(*end);
            }
            Shape::Circle { center, radius, .. } => {
                self.add(Xy(center.0 - radius, center.1 - radius));
                self.add(Xy(center.0 + radius, center.1 + radius));
            }
            Shape::Arc { start, mid, end, .. } => {
                self.add(*start);
                self.add(*mid);
                self.add(*end);
            }
        }
    }
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
    let pad_number = text_arg(item, 0).unwrap_or_default().to_owned();
    let kind = match text_arg(item, 1) {
        Some("smd") => PadKind::Smd,
        Some("thru_hole") => PadKind::ThroughHole,
        Some("np_thru_hole") => PadKind::NonPlatedHole,
        Some("connect") => PadKind::Connect,
        other => return Err(format!("pad {pad_number:?} has an unknown type {other:?}")),
    };
    let shape = match text_arg(item, 2) {
        Some("rect") => PadShape::Rect,
        Some("roundrect") => PadShape::RoundRect,
        Some("circle") => PadShape::Circle,
        Some("oval") => PadShape::Oval,
        Some("trapezoid") => PadShape::Trapezoid,
        Some("custom") => PadShape::Custom,
        other => return Err(format!("pad {pad_number:?} has an unknown shape {other:?}")),
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
        number: pad_number,
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
    fn placed_footprints_read_pads_and_the_courtyard_extent() {
        let node = parse(
            "(footprint \"R:R\" (layer \"B.Cu\") (at 10 20 90) \
            (fp_rect (start -1.5 -0.7) (end 1.5 0.7) (stroke (width 0.05)) (layer \"B.CrtYd\")) \
            (fp_line (start -3 0) (end 3 0) (stroke (width 0.1)) (layer \"B.SilkS\")) \
            (pad \"1\" smd roundrect (at -0.825 0 270) (size 0.8 0.95) \
            (roundrect_rratio 0.25)) \
            (pad \"2\" smd mystery (at 0.825 0 270) (size 0.8 0.95)) \
            (pad \"\" np_thru_hole circle (at 0 0) (size 1 1) (drill 1)))",
        )
        .unwrap();
        let geometry = placed_geometry(&node);
        assert_eq!(geometry.pads.len(), 2);
        assert_eq!(geometry.pads[0].number, "1");
        assert_eq!(geometry.pads[1].number, "");
        assert_eq!(geometry.pads[0].rotation, 2700);
        assert_eq!(geometry.pads[0].corner_radius, 200_000);
        assert_eq!(geometry.pads[1].drill, Some(Xy(1_000_000, 1_000_000)));
        let extent = (Xy(-1_500_000, -700_000), Xy(1_500_000, 700_000));
        assert_eq!(geometry.extent, Some(extent));
    }

    #[test]
    fn placed_footprints_without_a_courtyard_span_their_pads_and_graphics() {
        let node = parse(
            "(footprint \"R:R\" (layer \"F.Cu\") (at 0 0) \
            (fp_circle (center 0 -2) (end 0.5 -2) (stroke (width 0.1)) (layer \"F.SilkS\")) \
            (pad \"1\" smd rect (at 1 0) (size 1 2)))",
        )
        .unwrap();
        let extent = (Xy(-500_000, -2_500_000), Xy(2_000_000, 1_000_000));
        assert_eq!(placed_geometry(&node).extent, Some(extent));
        let bare = parse("(footprint \"R:R\" (layer \"F.Cu\") (at 0 0))").unwrap();
        assert_eq!(placed_geometry(&bare).extent, None);
    }

    #[test]
    fn unknown_pad_types_are_rejected() {
        let pad = parse("(pad \"1\" mystery rect (at 0 0) (size 1 1))").unwrap();
        assert!(read_pad(&pad).unwrap_err().contains("unknown type"));
    }
}
