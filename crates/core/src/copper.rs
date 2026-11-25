//! The copper of a board as shapes on layers: pads, tracks, vias, and zone fills, each with
//! the net it carries. Connectivity, zone fill, and DRC all read the board through this
//! view.

use uuid::Uuid;

use crate::board::Board;
use crate::footprint::Footprint;
use crate::geometry::{Core, Shape, rectangle, rotate};
use crate::kicad::read_at;
use crate::kicad_value::{length, number, text_arg};
use crate::route::{Track, Via, Zone};
use crate::sexpr::SExpr;
use crate::units::{Nm, Point};

/// The bit of `B.Cu` in a [`LayerSet`].
const BACK_BIT: u32 = 31;

/// A set of copper layers as bits by stack position: `F.Cu` is bit 0, `In1.Cu` to `In30.Cu`
/// are bits 1 to 30, and `B.Cu` is bit 31, so a span of bits is a span of the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct LayerSet(pub u32);

impl LayerSet {
    pub const EMPTY: LayerSet = LayerSet(0);

    /// The stack position of the copper layer `name`.
    pub fn position(name: &str) -> Option<u32> {
        match name {
            "F.Cu" => Some(0),
            "B.Cu" => Some(BACK_BIT),
            _ => {
                let inner = name.strip_prefix("In")?.strip_suffix(".Cu")?;
                let number: u32 = inner.parse().ok()?;
                (1..BACK_BIT).contains(&number).then_some(number)
            }
        }
    }

    /// The copper layers among `names`, with KiCad's wildcards: `*.Cu` is every copper
    /// layer and `F&B.Cu` the two outer ones. Other names are ignored.
    pub fn named<'a>(names: impl IntoIterator<Item = &'a str>) -> LayerSet {
        let mut bits = 0;
        for name in names {
            bits |= match name {
                "*.Cu" => u32::MAX,
                "F&B.Cu" => 1 | (1 << BACK_BIT),
                _ => LayerSet::position(name).map_or(0, |position| 1 << position),
            };
        }
        LayerSet(bits)
    }

    /// Every layer from `a` to `b` in the stack, both included. Empty when either isn't a
    /// copper layer.
    pub fn span(a: &str, b: &str) -> LayerSet {
        let (Some(a), Some(b)) = (LayerSet::position(a), LayerSet::position(b)) else {
            return LayerSet::EMPTY;
        };
        let (low, high) = (a.min(b), a.max(b));
        let width = high - low + 1;
        let bits = if width == 32 { u32::MAX } else { (1 << width) - 1 };
        LayerSet(bits << low)
    }

    pub fn overlaps(self, other: LayerSet) -> bool {
        self.0 & other.0 != 0
    }

    pub fn contains(self, name: &str) -> bool {
        LayerSet::position(name).is_some_and(|position| self.0 & (1 << position) != 0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// What kind of pad a `(pad ...)` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadKind {
    Smd,
    ThroughHole,
    NonPlated,
    Connect,
}

/// A pad's copper on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct PadCopper {
    pub footprint: Uuid,
    pub reference: String,
    pub number: String,
    /// The pad's position among the footprint's readable pads, in file order.
    pub index: usize,
    pub kind: PadKind,
    pub net: Option<String>,
    pub layers: LayerSet,
    pub center: Point,
    /// Width and height before rotation.
    pub size: (Nm, Nm),
    /// The pad orientation on the board in tenths of a degree.
    pub rotation: i32,
    pub shape: Shape,
    /// Hole width and height, when the pad has one.
    pub drill: Option<(Nm, Nm)>,
}

/// Which item a piece of copper belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemId {
    Pad { footprint: Uuid, index: usize },
    Track(Uuid),
    Via(Uuid),
    /// One filled polygon of a zone on one layer.
    Zone { zone: Uuid, fill: usize },
}

/// One piece of copper.
#[derive(Debug, Clone, PartialEq)]
pub struct CopperItem {
    pub id: ItemId,
    pub net: Option<String>,
    pub layers: LayerSet,
    pub shape: Shape,
    /// Where ratsnest lines may end on it: a pad's center, a track's ends, or a via's
    /// center. Zone fills have none.
    pub anchors: Vec<Point>,
}

impl CopperItem {
    /// Whether the two pieces share a layer and touch.
    pub fn touches(&self, other: &CopperItem) -> bool {
        self.layers.overlaps(other.layers) && self.shape.touches(&other.shape)
    }
}

/// The pads of every readable footprint, in file order.
pub fn board_pads(board: &Board) -> Vec<PadCopper> {
    let mut pads = Vec::new();
    for (footprint, node) in board.footprint_nodes() {
        let readable = node
            .args()
            .iter()
            .filter_map(|item| read_pad(&footprint, item));
        for (index, mut pad) in readable.enumerate() {
            pad.index = index;
            pads.push(pad);
        }
    }
    pads
}

/// Every piece of copper on the board: pads, tracks, vias, then zone fills.
pub fn copper_items(board: &Board) -> Vec<CopperItem> {
    let mut items: Vec<CopperItem> = board_pads(board).into_iter().map(pad_item).collect();
    items.extend(board.tracks().iter().map(track_item));
    items.extend(board.vias().iter().map(via_item));
    for zone in board.zones() {
        items.extend(zone_items(&zone));
    }
    items
}

/// The copper of a pad.
pub fn pad_item(pad: PadCopper) -> CopperItem {
    CopperItem {
        id: ItemId::Pad {
            footprint: pad.footprint,
            index: pad.index,
        },
        net: pad.net,
        layers: pad.layers,
        shape: pad.shape,
        anchors: vec![pad.center],
    }
}

/// The copper of a track.
pub fn track_item(track: &Track) -> CopperItem {
    CopperItem {
        id: ItemId::Track(track.uuid),
        net: track.net.clone(),
        layers: LayerSet::named([track.layer.as_str()]),
        shape: Shape::stroke(track.start, track.end, track.width),
        anchors: vec![track.start, track.end],
    }
}

/// The copper of a via: its pad on every layer it spans.
pub fn via_item(via: &Via) -> CopperItem {
    CopperItem {
        id: ItemId::Via(via.uuid),
        net: via.net.clone(),
        layers: LayerSet::span(&via.layers.0, &via.layers.1),
        shape: Shape::disc(via.position, Nm(via.size.0 / 2)),
        anchors: vec![via.position],
    }
}

/// The copper of a zone's filled polygons. Rule areas have none.
pub fn zone_items(zone: &Zone) -> Vec<CopperItem> {
    if zone.keepout {
        return Vec::new();
    }
    let mut items = Vec::with_capacity(zone.fills.len());
    for (fill, polygon) in zone.fills.iter().enumerate() {
        items.push(CopperItem {
            id: ItemId::Zone {
                zone: zone.uuid,
                fill,
            },
            net: zone.net.clone(),
            layers: LayerSet::named([polygon.layer.as_str()]),
            shape: Shape::polygon(polygon.points.clone()),
            anchors: Vec::new(),
        });
    }
    items
}

/// Reads one `(pad ...)` child of a footprint as board copper. Pads whose type, shape, or
/// size don't read are left out. Trapezoid and custom pads count as their anchor
/// rectangle, and chamfered corners as square ones, which is never smaller than the pad.
fn read_pad(footprint: &Footprint, node: &SExpr) -> Option<PadCopper> {
    if node.head() != Some("pad") {
        return None;
    }
    let kind = match text_arg(node, 1)? {
        "smd" => PadKind::Smd,
        "thru_hole" => PadKind::ThroughHole,
        "np_thru_hole" => PadKind::NonPlated,
        "connect" => PadKind::Connect,
        _ => return None,
    };
    let shape_name = text_arg(node, 2)?;
    let (offset, rotation) = read_at(node)?;
    let size = node.child("size")?;
    let size = (length(size, 0).ok()?, length(size, 1).ok()?);
    let center = footprint.board_point(offset);
    let rotation = rotation.decidegrees();
    let shape = match shape_name {
        "circle" => Shape::disc(center, Nm(size.0.0 / 2)),
        "oval" => oval(center, size, rotation),
        "roundrect" => {
            let ratio = node
                .child("roundrect_rratio")
                .and_then(|ratio| number(ratio, 0).ok())
                .unwrap_or(0.25)
                .clamp(0.0, 0.5);
            let radius = (size.0.0.min(size.1.0) as f64 * ratio).round() as i64;
            let width = Nm(size.0.0 - 2 * radius);
            let height = Nm(size.1.0 - 2 * radius);
            Shape {
                core: Core::Polygon(rectangle(center, width, height, rotation)),
                radius: Nm(radius),
            }
        }
        _ => Shape::polygon(rectangle(center, size.0, size.1, rotation)),
    };
    let layers = match node.child("layers") {
        Some(layers) => LayerSet::named(layers.args().iter().filter_map(SExpr::text)),
        None => LayerSet::EMPTY,
    };
    Some(PadCopper {
        footprint: footprint.uuid,
        reference: footprint.reference.clone(),
        number: text_arg(node, 0).unwrap_or_default().to_owned(),
        index: 0,
        kind,
        net: child_net(node),
        layers,
        center,
        size,
        rotation,
        shape,
        drill: node.child("drill").and_then(drill_size),
    })
}

/// An oval pad: a segment along its long side grown by half its short side.
fn oval(center: Point, size: (Nm, Nm), rotation: i32) -> Shape {
    let (width, height) = (size.0.0, size.1.0);
    let half = (width - height).abs() / 2;
    let along = if width >= height {
        Point::new(Nm(half), Nm::ZERO)
    } else {
        Point::new(Nm::ZERO, Nm(half))
    };
    let along = rotate(along, rotation);
    let a = center.offset(along.x, along.y);
    let b = center.offset(-along.x, -along.y);
    Shape {
        core: Core::Segment(a, b),
        radius: Nm(width.min(height) / 2),
    }
}

/// The net of an item from `(net "Name")` or the older `(net 3 "Name")`; the empty name
/// counts as none.
pub(crate) fn child_net(node: &SExpr) -> Option<String> {
    let net = node.child("net")?;
    let name = match net.args() {
        [name] | [_, name] => name.text()?,
        _ => return None,
    };
    (!name.is_empty()).then(|| name.to_owned())
}

/// Reads `(drill d)` or `(drill oval w h)`. A drill node holding only an offset has no size.
fn drill_size(node: &SExpr) -> Option<(Nm, Nm)> {
    let first = usize::from(text_arg(node, 0) == Some("oval"));
    text_arg(node, first)?;
    let width = length(node, first).ok()?;
    let height = match text_arg(node, first + 1) {
        Some(_) => length(node, first + 1).ok()?,
        None => width,
    };
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_sets_follow_the_stack() {
        assert_eq!(LayerSet::named(["F.Cu"]), LayerSet(1));
        assert_eq!(LayerSet::named(["In2.Cu", "F.Mask"]), LayerSet(4));
        assert_eq!(LayerSet::named(["F&B.Cu"]), LayerSet(1 | 1 << 31));
        assert!(LayerSet::named(["*.Cu"]).contains("In5.Cu"));
        assert_eq!(LayerSet::span("In1.Cu", "In3.Cu"), LayerSet(0b1110));
        assert_eq!(LayerSet::span("B.Cu", "F.Cu"), LayerSet(u32::MAX));
        assert!(LayerSet::span("F.Cu", "Edge.Cuts").is_empty());
        assert_eq!(LayerSet::position("In31.Cu"), None);
    }

    #[test]
    fn pads_read_as_shapes_on_the_board() {
        let board = Board::parse(
            "(kicad_pcb (version 20260206) \
             (footprint \"R\" (layer \"F.Cu\") (uuid \"00000000-0000-4000-8000-000000000001\") \
             (at 10 20 90) \
             (pad \"1\" smd roundrect (at -1 0 90) (size 1 0.6) (roundrect_rratio 0.25) \
             (layers \"F.Cu\" \"F.Mask\") (net \"A\")) \
             (pad \"2\" thru_hole oval (at 1 0 90) (size 2 1) (drill 0.6) \
             (layers \"*.Cu\") (net 3 \"B\")) \
             (pad \"\" np_thru_hole circle (at 0 3) (size 1 1) (drill 1) (layers \"*.Cu\"))))",
        )
        .unwrap();
        let pads = board_pads(&board);
        assert_eq!(pads.len(), 3);
        let center = Point::new(Nm(10_000_000), Nm(21_000_000));
        assert_eq!(pads[0].center, center);
        assert_eq!(pads[0].shape.radius, Nm(150_000));
        assert_eq!(pads[0].layers, LayerSet(1));
        assert_eq!(pads[0].net.as_deref(), Some("A"));
        assert_eq!(pads[1].net.as_deref(), Some("B"));
        assert_eq!(pads[1].kind, PadKind::ThroughHole);
        assert_eq!(pads[1].drill, Some((Nm(600_000), Nm(600_000))));
        assert_eq!(pads[1].shape.radius, Nm(500_000));
        let tip = Point::new(Nm(10_000_000), Nm(18_500_000));
        assert!(pads[1].shape.contains(tip));
        assert_eq!(pads[2].index, 2);
        assert_eq!(pads[2].net, None);
    }
}
