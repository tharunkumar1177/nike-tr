//! Routed copper: tracks (`segment`), vias, and zones, read from and written to the board
//! file in KiCad 10's form, where items name their net, as in `(net "GND")`.
//!
//! The typed fields are the ones Tron draws, connects, fills, and checks. Track arcs, via
//! padstacks, tenting, and other zone options pass through untouched in the file.

use std::collections::BTreeSet;

use uuid::Uuid;

use crate::board::{Board, MAX_COORDINATE};
use crate::copper::child_net;
use crate::design_rules::MAX_RULE;
use crate::kicad::mm_text;
use crate::kicad_value::{child_text, flag, length, number, point, points, text_arg};
use crate::sexpr::SExpr;
use crate::units::{Nm, Point};
use crate::wiring::{node_uuid, uuid_node};

/// KiCad's zone clearance for a new zone.
pub const DEFAULT_ZONE_CLEARANCE: Nm = Nm(500_000);
/// KiCad's minimum zone fill width for a new zone.
pub const DEFAULT_ZONE_MIN_THICKNESS: Nm = Nm(250_000);
/// KiCad's thermal relief gap for a new zone.
pub const DEFAULT_THERMAL_GAP: Nm = Nm(500_000);
/// KiCad's thermal spoke width for a new zone.
pub const DEFAULT_THERMAL_SPOKE: Nm = Nm(500_000);

/// One straight track: a top-level `(segment ...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub uuid: Uuid,
    pub start: Point,
    pub end: Point,
    pub width: Nm,
    pub layer: String,
    pub net: Option<String>,
    pub locked: bool,
}

impl Track {
    /// Reads a `(segment ...)` node. `None` when it isn't one or a typed field is missing.
    pub fn read(node: &SExpr) -> Option<Track> {
        if node.head() != Some("segment") {
            return None;
        }
        Some(Track {
            uuid: node_uuid(node)?,
            start: point(node.child("start")?).ok()?,
            end: point(node.child("end")?).ok()?,
            width: length(node.child("width")?, 0).ok()?,
            layer: child_text(node, "layer")?.to_owned(),
            net: child_net(node),
            locked: flag(node, "locked"),
        })
    }

    /// The `(segment ...)` node KiCad 10 writes for the track.
    pub fn to_node(&self) -> SExpr {
        let mut items = vec![
            xy_pair("start", self.start),
            xy_pair("end", self.end),
            length_node("width", self.width),
        ];
        if self.locked {
            items.push(SExpr::node("locked", vec![SExpr::atom("yes")]));
        }
        items.push(string_node("layer", &self.layer));
        items.push(net_node(self.net.as_deref()));
        items.push(uuid_node(self.uuid));
        SExpr::node("segment", items)
    }
}

/// A plated via: a top-level `(via ...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Via {
    pub uuid: Uuid,
    pub position: Point,
    /// The pad diameter.
    pub size: Nm,
    pub drill: Nm,
    /// The outermost copper layers it joins, such as `F.Cu` and `B.Cu`.
    pub layers: (String, String),
    pub net: Option<String>,
}

impl Via {
    /// Reads a `(via ...)` node. `None` when it isn't one or a typed field is missing.
    pub fn read(node: &SExpr) -> Option<Via> {
        if node.head() != Some("via") {
            return None;
        }
        let layers = node.child("layers")?;
        Some(Via {
            uuid: node_uuid(node)?,
            position: point(node.child("at")?).ok()?,
            size: length(node.child("size")?, 0).ok()?,
            drill: length(node.child("drill")?, 0).ok()?,
            layers: (
                text_arg(layers, 0)?.to_owned(),
                text_arg(layers, 1)?.to_owned(),
            ),
            net: child_net(node),
        })
    }

    /// The `(via ...)` node KiCad 10 writes for a through via.
    pub fn to_node(&self) -> SExpr {
        let layers = vec![
            SExpr::string(self.layers.0.as_str()),
            SExpr::string(self.layers.1.as_str()),
        ];
        SExpr::node(
            "via",
            vec![
                xy_pair("at", self.position),
                length_node("size", self.size),
                length_node("drill", self.drill),
                SExpr::node("layers", layers),
                net_node(self.net.as_deref()),
                uuid_node(self.uuid),
            ],
        )
    }
}

/// How a zone joins pads of its own net.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadConnection {
    /// Thermal reliefs: spokes across a gap. KiCad's default.
    Thermal,
    /// Solid copper.
    Solid,
    /// Thermal reliefs for through-hole pads, solid copper for the rest.
    ThermalThroughHoleOnly,
    /// Not joined.
    None,
}

/// One `(filled_polygon ...)` of a zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneFill {
    pub layer: String,
    pub points: Vec<Point>,
}

/// A copper zone or rule area: a top-level `(zone ...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub uuid: Uuid,
    pub net: Option<String>,
    pub name: Option<String>,
    pub layers: Vec<String>,
    /// The outline the user drew.
    pub outline: Vec<Point>,
    pub priority: u32,
    pub connection: PadConnection,
    pub clearance: Nm,
    pub min_thickness: Nm,
    pub thermal_gap: Nm,
    pub thermal_spoke: Nm,
    /// A rule area (keepout) rather than copper.
    pub keepout: bool,
    pub fills: Vec<ZoneFill>,
}

impl Zone {
    /// Reads a `(zone ...)` node. `None` when it isn't one or it has no UUID or outline.
    /// Settings that are missing take KiCad's defaults.
    pub fn read(node: &SExpr) -> Option<Zone> {
        if node.head() != Some("zone") {
            return None;
        }
        let layers = match node.child("layers") {
            Some(layers) => layers.args().iter().filter_map(SExpr::text).collect(),
            None => vec![child_text(node, "layer")?],
        };
        let connect = node.child("connect_pads");
        let connection = match connect.and_then(|node| text_arg(node, 0)) {
            Some("yes") => PadConnection::Solid,
            Some("no") => PadConnection::None,
            Some("thru_hole_only") => PadConnection::ThermalThroughHoleOnly,
            _ => PadConnection::Thermal,
        };
        let fill = node.child("fill");
        let setting = |parent: Option<&SExpr>, head: &str, default: Nm| {
            parent
                .and_then(|parent| parent.child(head))
                .and_then(|child| length(child, 0).ok())
                .unwrap_or(default)
        };
        let fills = node
            .args()
            .iter()
            .filter(|item| item.head() == Some("filled_polygon"))
            .filter_map(read_fill)
            .collect();
        Some(Zone {
            uuid: node_uuid(node)?,
            net: child_net(node),
            name: child_text(node, "name").map(str::to_owned),
            layers: layers.into_iter().map(str::to_owned).collect(),
            outline: points(node.child("polygon")?).ok()?,
            priority: node
                .child("priority")
                .and_then(|priority| number(priority, 0).ok())
                .map_or(0, |priority| priority.max(0.0) as u32),
            connection,
            clearance: setting(connect, "clearance", DEFAULT_ZONE_CLEARANCE),
            min_thickness: setting(Some(node), "min_thickness", DEFAULT_ZONE_MIN_THICKNESS),
            thermal_gap: setting(fill, "thermal_gap", DEFAULT_THERMAL_GAP),
            thermal_spoke: setting(fill, "thermal_bridge_width", DEFAULT_THERMAL_SPOKE),
            keepout: node.child("keepout").is_some(),
            fills,
        })
    }
}

/// Reads one `(filled_polygon (layer "F.Cu") (pts ...))`.
fn read_fill(node: &SExpr) -> Option<ZoneFill> {
    Some(ZoneFill {
        layer: child_text(node, "layer")?.to_owned(),
        points: points(node).ok()?,
    })
}

impl Board {
    /// Every readable track, in file order.
    pub fn tracks(&self) -> Vec<Track> {
        self.items().iter().filter_map(Track::read).collect()
    }

    /// Every readable via, in file order.
    pub fn vias(&self) -> Vec<Via> {
        self.items().iter().filter_map(Via::read).collect()
    }

    /// Every readable zone, rule areas included, in file order.
    pub fn zones(&self) -> Vec<Zone> {
        self.items().iter().filter_map(Zone::read).collect()
    }

    /// The child index a new track or via goes at: after the last footprint, graphic,
    /// track, or via, before zones and groups, where KiCad writes them.
    pub(crate) fn route_insert_index(&self) -> usize {
        let items = self.items();
        let before = items
            .iter()
            .position(|item| matches!(item.head(), Some("zone" | "group" | "generated")));
        let index = before.unwrap_or(items.len());
        let after_footprints = self.footprint_insert_index() - 1;
        index.max(after_footprints.min(items.len())) + 1
    }
}

/// The most segments, and separately the most vias, one route can have.
pub const MAX_ROUTE_ITEMS: usize = 1024;

/// One straight piece of a route the user drew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSegment {
    pub start: Point,
    pub end: Point,
    pub layer: String,
}

/// A finished interactive route: connected segments and the vias where it changes layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRequest {
    pub net: Option<String>,
    pub width: Nm,
    pub via_diameter: Nm,
    pub via_drill: Nm,
    pub segments: Vec<RouteSegment>,
    pub vias: Vec<Point>,
}

/// The track and via nodes for `request`, with UUIDs from `fresh`. Fails with a message
/// for the user when the route isn't connected, has a segment that isn't horizontal,
/// vertical, or diagonal, uses a layer the board lacks, changes layer without a via, puts
/// a via off the route, leaves KiCad's coordinate range, or names a net the board doesn't
/// have.
pub fn route_nodes(
    board: &Board,
    request: &RouteRequest,
    fresh: &mut impl FnMut() -> Uuid,
) -> Result<Vec<SExpr>, String> {
    check_route(board, request)?;
    let mut nodes = Vec::with_capacity(request.segments.len() + request.vias.len());
    for segment in &request.segments {
        let track = Track {
            uuid: fresh(),
            start: segment.start,
            end: segment.end,
            width: request.width,
            layer: segment.layer.clone(),
            net: request.net.clone(),
            locked: false,
        };
        nodes.push(track.to_node());
    }
    for &position in &request.vias {
        let via = Via {
            uuid: fresh(),
            position,
            size: request.via_diameter,
            drill: request.via_drill,
            layers: ("F.Cu".to_owned(), "B.Cu".to_owned()),
            net: request.net.clone(),
        };
        nodes.push(via.to_node());
    }
    Ok(nodes)
}

/// The checks [`route_nodes`] makes.
fn check_route(board: &Board, request: &RouteRequest) -> Result<(), String> {
    let segments = &request.segments;
    if segments.is_empty() {
        return Err("A route needs at least one segment.".to_owned());
    }
    if segments.len() > MAX_ROUTE_ITEMS || request.vias.len() > MAX_ROUTE_ITEMS {
        return Err(format!(
            "A route can have at most {MAX_ROUTE_ITEMS} segments and {MAX_ROUTE_ITEMS} vias."
        ));
    }
    if request.width.0 <= 0 || request.width > MAX_RULE {
        return Err("The track width must be greater than 0 and at most 25 mm.".to_owned());
    }
    if !request.vias.is_empty() {
        let size = request.via_diameter;
        if request.via_drill.0 <= 0 || request.via_drill >= size || size > MAX_RULE {
            return Err("The via drill must be positive and smaller than the via.".to_owned());
        }
    }
    let copper = board.copper_layer_names();
    for (index, segment) in segments.iter().enumerate() {
        if !copper.contains(&segment.layer) {
            return Err(format!("The board has no {} layer.", segment.layer));
        }
        if !in_range(segment.start) || !in_range(segment.end) {
            return Err("The route leaves the area KiCad boards can hold.".to_owned());
        }
        let dx = (segment.end.x - segment.start.x).abs();
        let dy = (segment.end.y - segment.start.y).abs();
        if dx == Nm::ZERO && dy == Nm::ZERO {
            return Err("A route segment has no length.".to_owned());
        }
        if dx != Nm::ZERO && dy != Nm::ZERO && dx != dy {
            return Err("Route segments must be horizontal, vertical, or diagonal.".to_owned());
        }
        let Some(next) = segments.get(index + 1) else {
            continue;
        };
        if next.start != segment.end {
            return Err("The route's segments must join end to start.".to_owned());
        }
        if next.layer != segment.layer && !request.vias.contains(&segment.end) {
            return Err("The route changes layer without a via.".to_owned());
        }
    }
    let on_route = |point: &Point| {
        segments
            .iter()
            .any(|segment| segment.start == *point || segment.end == *point)
    };
    if !request.vias.iter().all(on_route) {
        return Err("Every via must sit on a corner or end of the route.".to_owned());
    }
    if let Some(net) = &request.net
        && !board_nets(board).contains(net)
    {
        return Err(format!("The board has no net named {net}."));
    }
    Ok(())
}

/// Whether `point` lies in KiCad's 32-bit nanometer range.
fn in_range(point: Point) -> bool {
    point.x.abs() <= MAX_COORDINATE && point.y.abs() <= MAX_COORDINATE
}

/// The name of every net a pad, track, via, or zone on the board carries.
pub fn board_nets(board: &Board) -> BTreeSet<String> {
    let mut nets = BTreeSet::new();
    for footprint in board.footprints() {
        nets.extend(footprint.pads.into_iter().filter_map(|pad| pad.net));
    }
    nets.extend(board.tracks().into_iter().filter_map(|track| track.net));
    nets.extend(board.vias().into_iter().filter_map(|via| via.net));
    nets.extend(board.zones().into_iter().filter_map(|zone| zone.net));
    nets
}

/// `(head x y)` in millimeters.
fn xy_pair(head: &str, at: Point) -> SExpr {
    let x = SExpr::atom(mm_text(at.x));
    let y = SExpr::atom(mm_text(at.y));
    SExpr::node(head, vec![x, y])
}

/// `(head value)` in millimeters.
fn length_node(head: &str, value: Nm) -> SExpr {
    SExpr::node(head, vec![SExpr::atom(mm_text(value))])
}

/// `(head "text")`.
fn string_node(head: &str, text: &str) -> SExpr {
    SExpr::node(head, vec![SExpr::string(text)])
}

/// `(net "Name")`, or `(net "")` for an item without a net, as KiCad 10 writes it.
fn net_node(net: Option<&str>) -> SExpr {
    string_node("net", net.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr::parse;

    fn at(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn tracks_and_vias_round_trip_in_kicad_10_form() {
        let text = "(segment (start 1 2.5) (end 3 2.5) (width 0.2) (layer \"F.Cu\") \
                    (net \"GND\") (uuid \"00000000-0000-4000-8000-000000000001\"))";
        let node = parse(text).unwrap();
        let track = Track::read(&node).unwrap();
        assert_eq!(track.start, at(1_000_000, 2_500_000));
        assert_eq!(track.width, Nm(200_000));
        assert_eq!(track.net.as_deref(), Some("GND"));
        assert!(!track.locked);
        assert_eq!(track.to_node(), node);

        let text = "(via (at 3 2.5) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") \
                    (net \"\") (uuid \"00000000-0000-4000-8000-000000000002\"))";
        let node = parse(text).unwrap();
        let via = Via::read(&node).unwrap();
        assert_eq!(via.size, Nm(600_000));
        assert_eq!(via.net, None);
        assert_eq!(via.layers.1, "B.Cu");
        assert_eq!(via.to_node(), node);
    }

    #[test]
    fn zones_read_their_settings_and_fills() {
        let text = "(zone (net \"GND\") (layer \"B.Cu\") \
                    (uuid \"00000000-0000-4000-8000-000000000003\") (hatch edge 0.5) \
                    (priority 2) (connect_pads (clearance 0.3)) (min_thickness 0.2) \
                    (fill yes (thermal_gap 0.4) (thermal_bridge_width 0.35)) \
                    (polygon (pts (xy 0 0) (xy 10 0) (xy 10 10) (xy 0 10))) \
                    (filled_polygon (layer \"B.Cu\") (pts (xy 1 1) (xy 9 1) (xy 9 9))))";
        let zone = Zone::read(&parse(text).unwrap()).unwrap();
        assert_eq!(zone.layers, ["B.Cu"]);
        assert_eq!(zone.priority, 2);
        assert_eq!(zone.connection, PadConnection::Thermal);
        assert_eq!(zone.clearance, Nm(300_000));
        assert_eq!(zone.min_thickness, Nm(200_000));
        assert_eq!(zone.thermal_gap, Nm(400_000));
        assert_eq!(zone.thermal_spoke, Nm(350_000));
        assert_eq!(zone.outline.len(), 4);
        assert_eq!(zone.fills.len(), 1);
        assert_eq!(zone.fills[0].points[2], at(9_000_000, 9_000_000));

        let bare = "(zone (net \"A\") (layers \"F.Cu\" \"B.Cu\") \
                    (uuid \"00000000-0000-4000-8000-000000000004\") (connect_pads yes) \
                    (polygon (pts (xy 0 0) (xy 1 0) (xy 1 1))))";
        let zone = Zone::read(&parse(bare).unwrap()).unwrap();
        assert_eq!(zone.layers, ["F.Cu", "B.Cu"]);
        assert_eq!(zone.connection, PadConnection::Solid);
        assert_eq!(zone.clearance, DEFAULT_ZONE_CLEARANCE);
        assert_eq!(zone.thermal_spoke, DEFAULT_THERMAL_SPOKE);
        assert!(zone.fills.is_empty());
    }

    #[test]
    fn routes_go_after_footprints_and_before_zones() {
        let board = Board::parse(
            "(kicad_pcb (version 20260206) (layers) \
             (footprint \"R\" (layer \"F.Cu\") (uuid \"00000000-0000-4000-8000-000000000005\") \
             (at 0 0)) \
             (zone (net \"A\") (layer \"F.Cu\") (uuid \"00000000-0000-4000-8000-000000000006\") \
             (polygon (pts (xy 0 0) (xy 1 0) (xy 1 1)))))",
        )
        .unwrap();
        assert_eq!(board.route_insert_index(), 4);
        assert_eq!(board.zones().len(), 1);
    }
}
