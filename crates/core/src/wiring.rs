//! Wires, junctions, and net labels on the schematic, and the geometry the wiring commands
//! use to split wires and place junctions.
//!
//! Connections are exact coordinate matches. A wiring edit splits every wire that a new
//! connection point lands inside, so connected items always meet at shared endpoints.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::kicad::{at_node, mm_text, read_at};
use crate::kicad_value::{child_text, points, text_arg};
use crate::sexpr::SExpr;
use crate::units::{Angle, Nm, Point};

/// The longest net label, in characters.
pub const MAX_LABEL_CHARS: usize = 256;

/// One straight `(wire (pts (xy) (xy)) ...)` segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wire {
    pub uuid: Uuid,
    pub start: Point,
    pub end: Point,
}

impl Wire {
    pub(crate) fn read(node: &SExpr) -> Option<Wire> {
        if node.head() != Some("wire") {
            return None;
        }
        let uuid = node_uuid(node)?;
        match points(node).ok()?.as_slice() {
            [start, end] => Some(Wire {
                uuid,
                start: *start,
                end: *end,
            }),
            _ => None,
        }
    }

    /// Builds the wire node, keeping every child of `previous` other than the points and
    /// the UUID. A new wire gets KiCad's default stroke.
    pub(crate) fn node(&self, previous: Option<&SExpr>) -> SExpr {
        let pts = SExpr::node("pts", vec![xy_node(self.start), xy_node(self.end)]);
        let mut items = vec![SExpr::atom("wire"), pts];
        match previous {
            Some(previous) => {
                for child in previous.args() {
                    if !matches!(child.head(), Some("pts" | "uuid")) {
                        items.push(child.clone());
                    }
                }
            }
            None => {
                let width = SExpr::node("width", vec![SExpr::atom("0")]);
                let kind = SExpr::node("type", vec![SExpr::atom("default")]);
                items.push(SExpr::node("stroke", vec![width, kind]));
            }
        }
        items.push(uuid_node(self.uuid));
        SExpr::List(items)
    }

    pub fn ends(&self) -> [Point; 2] {
        [self.start, self.end]
    }

    pub fn moved_by(&self, dx: Nm, dy: Nm) -> Wire {
        Wire {
            uuid: self.uuid,
            start: self.start.offset(dx, dy),
            end: self.end.offset(dx, dy),
        }
    }

    /// Whether `point` lies on the segment strictly between its ends.
    pub fn contains_interior(&self, point: Point) -> bool {
        if point == self.start || point == self.end || self.start == self.end {
            return false;
        }
        let (dx, dy) = self.direction();
        let (px, py) = self.relative(point);
        let cross = dx * py - dy * px;
        let dot = px * dx + py * dy;
        cross == 0 && dot > 0 && dot < dx * dx + dy * dy
    }

    /// How far along the segment `point` lies, for ordering split points.
    fn distance_along(&self, point: Point) -> i128 {
        let (dx, dy) = self.direction();
        let (px, py) = self.relative(point);
        px * dx + py * dy
    }

    fn direction(&self) -> (i128, i128) {
        self.relative(self.end)
    }

    fn relative(&self, point: Point) -> (i128, i128) {
        let x = i128::from(point.x.0) - i128::from(self.start.x.0);
        let y = i128::from(point.y.0) - i128::from(self.start.y.0);
        (x, y)
    }
}

/// A `(junction (at x y) ...)` dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Junction {
    pub uuid: Uuid,
    pub position: Point,
}

impl Junction {
    pub(crate) fn read(node: &SExpr) -> Option<Junction> {
        if node.head() != Some("junction") {
            return None;
        }
        Some(Junction {
            uuid: node_uuid(node)?,
            position: read_at(node)?.0,
        })
    }

    /// A new junction with KiCad's defaults: the theme diameter and color.
    pub(crate) fn new_node(&self) -> SExpr {
        let x = SExpr::atom(mm_text(self.position.x));
        let y = SExpr::atom(mm_text(self.position.y));
        let color = ["0", "0", "0", "0"].into_iter().map(SExpr::atom).collect();
        SExpr::node(
            "junction",
            vec![
                SExpr::node("at", vec![x, y]),
                SExpr::node("diameter", vec![SExpr::atom("0")]),
                SExpr::node("color", color),
                uuid_node(self.uuid),
            ],
        )
    }
}

/// The outline of a global label, from its `(shape ...)` node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelShape {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
}

impl LabelShape {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelShape::Input => "input",
            LabelShape::Output => "output",
            LabelShape::Bidirectional => "bidirectional",
            LabelShape::TriState => "tri_state",
            LabelShape::Passive => "passive",
        }
    }

    fn parse(text: &str) -> Option<LabelShape> {
        let shape = match text {
            "input" => LabelShape::Input,
            "output" => LabelShape::Output,
            "bidirectional" => LabelShape::Bidirectional,
            "tri_state" => LabelShape::TriState,
            "passive" => LabelShape::Passive,
            _ => return None,
        };
        Some(shape)
    }
}

/// Whether a label names a net on this sheet only or across the whole design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    Local,
    Global(LabelShape),
}

/// A `(label ...)` or `(global_label ...)`. It connects at its anchor, `position`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub uuid: Uuid,
    pub kind: LabelKind,
    pub text: String,
    pub position: Point,
    /// A multiple of 90°.
    pub rotation: Angle,
}

impl Label {
    pub(crate) fn read(node: &SExpr) -> Option<Label> {
        let kind = match node.head() {
            Some("label") => LabelKind::Local,
            Some("global_label") => {
                let shape = child_text(node, "shape").and_then(LabelShape::parse);
                LabelKind::Global(shape.unwrap_or(LabelShape::Passive))
            }
            _ => return None,
        };
        let (position, rotation) = read_at(node)?;
        Some(Label {
            uuid: node_uuid(node)?,
            kind,
            text: text_arg(node, 0)?.to_owned(),
            position,
            rotation,
        })
    }

    fn head(&self) -> &'static str {
        match self.kind {
            LabelKind::Local => "label",
            LabelKind::Global(_) => "global_label",
        }
    }

    /// The `(justify ...)` KiCad writes for the label's orientation: text runs away from
    /// the anchor, so labels turned 180° or 270° are right-justified.
    fn justify(&self) -> SExpr {
        let right = self.rotation.decidegrees() >= 1800;
        let mut args = vec![SExpr::atom(if right { "right" } else { "left" })];
        if self.kind == LabelKind::Local {
            args.push(SExpr::atom("bottom"));
        }
        SExpr::node("justify", args)
    }

    /// Builds the label node. With a `previous` node, every child Tron doesn't edit stays,
    /// and only the justification inside `(effects ...)` follows the new rotation.
    pub(crate) fn to_node(&self, previous: Option<&SExpr>) -> SExpr {
        let mut items = vec![SExpr::atom(self.head()), SExpr::string(self.text.as_str())];
        if let LabelKind::Global(shape) = self.kind {
            items.push(SExpr::node("shape", vec![SExpr::atom(shape.as_str())]));
        }
        items.push(at_node(self.position, self.rotation));
        match previous {
            Some(previous) => {
                for child in previous.args().iter().skip(1) {
                    match child.head() {
                        Some("shape" | "at" | "uuid") => {}
                        Some("effects") => items.push(self.effects(Some(child))),
                        _ => items.push(child.clone()),
                    }
                }
            }
            None => {
                if matches!(self.kind, LabelKind::Global(_)) {
                    let yes = vec![SExpr::atom("yes")];
                    items.push(SExpr::node("fields_autoplaced", yes));
                }
                items.push(self.effects(None));
            }
        }
        items.push(uuid_node(self.uuid));
        SExpr::List(items)
    }

    fn effects(&self, previous: Option<&SExpr>) -> SExpr {
        let mut args: Vec<SExpr> = match previous {
            Some(previous) => previous
                .args()
                .iter()
                .filter(|child| child.head() != Some("justify"))
                .cloned()
                .collect(),
            None => {
                let size = vec![SExpr::atom("1.27"), SExpr::atom("1.27")];
                vec![SExpr::node("font", vec![SExpr::node("size", size)])]
            }
        };
        args.push(self.justify());
        SExpr::node("effects", args)
    }
}

/// Checks a label name: one line of 1 to [`MAX_LABEL_CHARS`] characters with no
/// whitespace, since KiCad treats spaces in net names as separators in netlists.
pub fn validate_label_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Err("A label needs a name.".to_owned());
    }
    if text.chars().count() > MAX_LABEL_CHARS {
        return Err(format!(
            "A label name can have at most {MAX_LABEL_CHARS} characters."
        ));
    }
    if text.chars().any(char::is_whitespace) {
        return Err("A label name can't contain spaces.".to_owned());
    }
    Ok(())
}

/// The connectivity items of a sheet, as the wiring planner sees them.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sheet {
    pub wires: Vec<Wire>,
    pub junctions: Vec<Junction>,
    pub labels: Vec<Label>,
    /// Every pin connection point, hidden pins included.
    pub pins: Vec<Point>,
}

/// What a wiring edit asks for, before splitting and junction upkeep.
#[derive(Debug, Clone, Default)]
pub(crate) struct WiringRequest {
    /// Wires and labels to remove, by UUID.
    pub deleted: HashSet<Uuid>,
    /// Existing wires replaced by a moved copy with the same UUID.
    pub moved_wires: Vec<Wire>,
    /// Existing labels replaced by an edited copy with the same UUID.
    pub changed_labels: Vec<Label>,
    pub new_wires: Vec<Wire>,
    pub new_labels: Vec<Label>,
}

/// The node-level outcome of a wiring edit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WiringPlan {
    /// UUIDs of nodes to remove.
    pub deleted: HashSet<Uuid>,
    /// Existing nodes rebuilt in place, by UUID.
    pub changed: Vec<(Uuid, ChangedItem)>,
    /// New nodes, in the order they are inserted.
    pub added: Vec<NewItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChangedItem {
    Wire(Wire),
    Label(Label),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NewItem {
    Wire(Wire),
    Junction(Junction),
    Label(Label),
}

impl NewItem {
    pub(crate) fn to_node(&self) -> SExpr {
        match self {
            NewItem::Wire(wire) => wire.node(None),
            NewItem::Junction(junction) => junction.new_node(),
            NewItem::Label(label) => label.to_node(None),
        }
    }
}

/// Whether the connections at `point` need a junction dot: at least two wire ends there
/// and three or more connections in all.
fn needs_junction(wires: &[Wire], pins: &[Point], point: Point) -> bool {
    let wire_ends = wires
        .iter()
        .flat_map(Wire::ends)
        .filter(|end| *end == point)
        .count();
    let pin_count = pins.iter().filter(|pin| **pin == point).count();
    wire_ends >= 2 && wire_ends + pin_count >= 3
}

/// Splits `wire` at every point in `at` that lies inside it. The first piece keeps the
/// wire's UUID; later pieces take fresh ones.
fn split(wire: Wire, at: &[Point], fresh: &mut impl FnMut() -> Uuid) -> Vec<Wire> {
    let mut cuts: Vec<Point> = at
        .iter()
        .copied()
        .filter(|point| wire.contains_interior(*point))
        .collect();
    cuts.sort_by_key(|point| wire.distance_along(*point));
    cuts.dedup();
    let mut pieces = Vec::with_capacity(cuts.len() + 1);
    let mut start = wire.start;
    for cut in cuts.into_iter().chain([wire.end]) {
        let uuid = if pieces.is_empty() { wire.uuid } else { fresh() };
        pieces.push(Wire {
            uuid,
            start,
            end: cut,
        });
        start = cut;
    }
    pieces
}

/// Sorts points and drops duplicates.
fn unique(mut points: Vec<Point>) -> Vec<Point> {
    points.sort_by_key(|point| (point.x, point.y));
    points.dedup();
    points
}

/// Turns a wiring request into node edits: wires that a new or moved connection point
/// lands inside are split there, edited wires are split wherever a connection point lies
/// inside them, and junctions are added or removed at every point the edit touched.
pub(crate) fn plan(
    sheet: &Sheet,
    request: WiringRequest,
    fresh: &mut impl FnMut() -> Uuid,
) -> WiringPlan {
    let mut replaced = request.deleted.clone();
    replaced.extend(request.moved_wires.iter().map(|wire| wire.uuid));
    replaced.extend(request.changed_labels.iter().map(|label| label.uuid));
    let moved: HashSet<Uuid> = request.moved_wires.iter().map(|wire| wire.uuid).collect();

    let mut touched: Vec<Point> = Vec::new();
    for wire in &sheet.wires {
        if replaced.contains(&wire.uuid) {
            touched.extend(wire.ends());
        }
    }

    let mut edited_wires = request.moved_wires.clone();
    edited_wires.extend(request.new_wires.iter().copied());
    let mut new_points: Vec<Point> = edited_wires.iter().flat_map(Wire::ends).collect();
    let edited_labels = request.changed_labels.iter().chain(&request.new_labels);
    new_points.extend(edited_labels.map(|label| label.position));
    touched.extend(new_points.iter().copied());

    let mut plan = WiringPlan {
        deleted: request.deleted.clone(),
        ..WiringPlan::default()
    };

    let mut final_wires: Vec<Wire> = Vec::new();
    for wire in &sheet.wires {
        if replaced.contains(&wire.uuid) {
            continue;
        }
        let pieces = split(*wire, &new_points, fresh);
        if let [first, rest @ ..] = pieces.as_slice()
            && !rest.is_empty()
        {
            plan.changed.push((wire.uuid, ChangedItem::Wire(*first)));
            for piece in rest {
                touched.push(piece.start);
                plan.added.push(NewItem::Wire(*piece));
            }
        }
        final_wires.extend(pieces);
    }

    let mut connection_points: Vec<Point> = final_wires.iter().flat_map(Wire::ends).collect();
    connection_points.extend(new_points.iter().copied());
    connection_points.extend(sheet.pins.iter().copied());
    for label in &sheet.labels {
        if !replaced.contains(&label.uuid) {
            connection_points.push(label.position);
        }
    }
    let connection_points = unique(connection_points);

    for wire in edited_wires {
        let pieces = split(wire, &connection_points, fresh);
        for (index, piece) in pieces.iter().enumerate() {
            if index > 0 {
                touched.push(piece.start);
            }
            if index == 0 && moved.contains(&wire.uuid) {
                plan.changed.push((wire.uuid, ChangedItem::Wire(*piece)));
            } else {
                plan.added.push(NewItem::Wire(*piece));
            }
        }
        final_wires.extend(pieces);
    }

    for label in request.changed_labels {
        plan.changed.push((label.uuid, ChangedItem::Label(label)));
    }
    plan.added.extend(request.new_labels.into_iter().map(NewItem::Label));

    let mut junctions_at: HashMap<Point, Vec<Uuid>> = HashMap::new();
    for junction in &sheet.junctions {
        let entry = junctions_at.entry(junction.position).or_default();
        entry.push(junction.uuid);
    }
    for point in unique(touched) {
        let existing = junctions_at.get(&point);
        if needs_junction(&final_wires, &sheet.pins, point) {
            if existing.is_none() {
                let junction = Junction {
                    uuid: fresh(),
                    position: point,
                };
                plan.added.push(NewItem::Junction(junction));
            }
        } else if let Some(uuids) = existing {
            plan.deleted.extend(uuids.iter().copied());
        }
    }
    plan
}

/// Breaks a drawn polyline into its wire segments. Every segment must be horizontal or
/// vertical. Repeated points are dropped, and consecutive collinear segments stay
/// separate, so each click is a corner the user placed.
pub fn polyline_segments(points: &[Point]) -> Result<Vec<(Point, Point)>, String> {
    let mut segments = Vec::new();
    for pair in points.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == end {
            continue;
        }
        if start.x != end.x && start.y != end.y {
            return Err("Wire segments must be horizontal or vertical.".to_owned());
        }
        segments.push((start, end));
    }
    if segments.is_empty() {
        return Err("A wire needs two different points.".to_owned());
    }
    Ok(segments)
}

fn xy_node(point: Point) -> SExpr {
    let x = SExpr::atom(mm_text(point.x));
    let y = SExpr::atom(mm_text(point.y));
    SExpr::node("xy", vec![x, y])
}

fn uuid_node(uuid: Uuid) -> SExpr {
    SExpr::node("uuid", vec![SExpr::string(uuid.to_string())])
}

/// The parsed `(uuid ...)` child of a top-level schematic item.
pub(crate) fn node_uuid(node: &SExpr) -> Option<Uuid> {
    child_text(node, "uuid").and_then(|text| Uuid::parse_str(text).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point on the 1.27 mm grid.
    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x * 1_270_000), Nm(y * 1_270_000))
    }

    fn wire(n: u128, start: Point, end: Point) -> Wire {
        let uuid = Uuid::from_u128(n);
        Wire { uuid, start, end }
    }

    fn counter() -> impl FnMut() -> Uuid {
        let mut next = 1000u128;
        move || {
            next += 1;
            Uuid::from_u128(next)
        }
    }

    fn added_wires(plan: &WiringPlan) -> Vec<(Point, Point)> {
        let mut wires = Vec::new();
        for item in &plan.added {
            if let NewItem::Wire(wire) = item {
                wires.push((wire.start, wire.end));
            }
        }
        wires
    }

    fn added_junctions(plan: &WiringPlan) -> Vec<Point> {
        let mut junctions = Vec::new();
        for item in &plan.added {
            if let NewItem::Junction(junction) = item {
                junctions.push(junction.position);
            }
        }
        junctions
    }

    #[test]
    fn interior_points_exclude_the_ends() {
        let horizontal = wire(1, p(0, 0), p(10, 0));
        assert!(horizontal.contains_interior(p(5, 0)));
        assert!(!horizontal.contains_interior(p(0, 0)));
        assert!(!horizontal.contains_interior(p(10, 0)));
        assert!(!horizontal.contains_interior(p(11, 0)));
        assert!(!horizontal.contains_interior(p(5, 1)));
        let diagonal = wire(2, p(0, 0), p(4, 4));
        assert!(diagonal.contains_interior(p(2, 2)));
        assert!(!diagonal.contains_interior(p(2, 3)));
    }

    #[test]
    fn a_wire_ending_on_another_splits_it_and_adds_a_junction() {
        let sheet = Sheet {
            wires: vec![wire(1, p(0, 0), p(10, 0))],
            ..Sheet::default()
        };
        let request = WiringRequest {
            new_wires: vec![wire(2, p(4, 0), p(4, 6))],
            ..WiringRequest::default()
        };
        let plan = plan(&sheet, request, &mut counter());
        let kept = ChangedItem::Wire(wire(1, p(0, 0), p(4, 0)));
        assert_eq!(plan.changed, [(Uuid::from_u128(1), kept)]);
        assert_eq!(
            plan.added,
            [
                NewItem::Wire(wire(1001, p(4, 0), p(10, 0))),
                NewItem::Wire(wire(2, p(4, 0), p(4, 6))),
                NewItem::Junction(Junction {
                    uuid: Uuid::from_u128(1002),
                    position: p(4, 0),
                }),
            ]
        );
    }

    #[test]
    fn a_new_wire_is_split_at_pins_and_wire_ends_it_crosses() {
        let sheet = Sheet {
            wires: vec![wire(1, p(5, -4), p(5, 0))],
            pins: vec![p(8, 0)],
            ..Sheet::default()
        };
        let request = WiringRequest {
            new_wires: vec![wire(2, p(0, 0), p(10, 0))],
            ..WiringRequest::default()
        };
        let plan = plan(&sheet, request, &mut counter());
        assert_eq!(
            added_wires(&plan),
            [(p(0, 0), p(5, 0)), (p(5, 0), p(8, 0)), (p(8, 0), p(10, 0))]
        );
        assert_eq!(added_junctions(&plan), [p(5, 0), p(8, 0)]);
    }

    #[test]
    fn crossing_wires_stay_unconnected() {
        let sheet = Sheet {
            wires: vec![wire(1, p(0, 0), p(10, 0))],
            ..Sheet::default()
        };
        let request = WiringRequest {
            new_wires: vec![wire(2, p(5, -5), p(5, 5))],
            ..WiringRequest::default()
        };
        let plan = plan(&sheet, request, &mut counter());
        assert!(plan.changed.is_empty());
        assert_eq!(added_wires(&plan), [(p(5, -5), p(5, 5))]);
        assert!(added_junctions(&plan).is_empty());
    }

    #[test]
    fn deleting_a_branch_removes_the_junction_it_needed() {
        let sheet = Sheet {
            wires: vec![
                wire(1, p(0, 0), p(4, 0)),
                wire(2, p(4, 0), p(10, 0)),
                wire(3, p(4, 0), p(4, 6)),
            ],
            junctions: vec![Junction {
                uuid: Uuid::from_u128(9),
                position: p(4, 0),
            }],
            ..Sheet::default()
        };
        let request = WiringRequest {
            deleted: HashSet::from([Uuid::from_u128(3)]),
            ..WiringRequest::default()
        };
        let plan = plan(&sheet, request, &mut counter());
        let expected = HashSet::from([Uuid::from_u128(3), Uuid::from_u128(9)]);
        assert_eq!(plan.deleted, expected);
        assert!(plan.added.is_empty());
    }

    #[test]
    fn a_label_on_a_wire_splits_it_without_a_junction() {
        let sheet = Sheet {
            wires: vec![wire(1, p(0, 0), p(10, 0))],
            ..Sheet::default()
        };
        let label = Label {
            uuid: Uuid::from_u128(5),
            kind: LabelKind::Local,
            text: "OUT".to_owned(),
            position: p(6, 0),
            rotation: Angle::ZERO,
        };
        let request = WiringRequest {
            new_labels: vec![label.clone()],
            ..WiringRequest::default()
        };
        let plan = plan(&sheet, request, &mut counter());
        assert_eq!(plan.changed.len(), 1);
        let rest = NewItem::Wire(wire(1001, p(6, 0), p(10, 0)));
        assert_eq!(plan.added, [rest, NewItem::Label(label)]);
    }

    #[test]
    fn polylines_must_be_orthogonal() {
        let segments = polyline_segments(&[p(0, 0), p(4, 0), p(4, 0), p(4, 3)]);
        assert_eq!(segments, Ok(vec![(p(0, 0), p(4, 0)), (p(4, 0), p(4, 3))]));
        assert!(polyline_segments(&[p(0, 0), p(4, 3)]).is_err());
        assert!(polyline_segments(&[p(1, 1), p(1, 1)]).is_err());
        assert!(polyline_segments(&[p(1, 1)]).is_err());
    }

    #[test]
    fn label_text_is_one_word() {
        assert!(validate_label_text("VIN").is_ok());
        assert!(validate_label_text("").is_err());
        assert!(validate_label_text("A B").is_err());
        assert!(validate_label_text(&"x".repeat(MAX_LABEL_CHARS + 1)).is_err());
    }

    #[test]
    fn nodes_round_trip_their_typed_fields() {
        let wire = wire(1, p(0, 0), p(10, 0));
        assert_eq!(Wire::read(&wire.node(None)), Some(wire));
        let junction = Junction {
            uuid: Uuid::from_u128(2),
            position: p(3, 4),
        };
        assert_eq!(Junction::read(&junction.new_node()), Some(junction));
        let label = Label {
            uuid: Uuid::from_u128(3),
            kind: LabelKind::Global(LabelShape::Output),
            text: "VIN".to_owned(),
            position: p(2, 2),
            rotation: Angle::from_decidegrees(1800),
        };
        let node = label.to_node(None);
        assert_eq!(Label::read(&node), Some(label.clone()));
        let text = node.to_pretty_string();
        assert!(text.contains("(shape output)"), "{text}");
        assert!(text.contains("(justify right)"), "{text}");

        let turned = Label {
            rotation: Angle::ZERO,
            ..label
        };
        let rebuilt = turned.to_node(Some(&node)).to_pretty_string();
        assert!(rebuilt.contains("(justify left)"), "{rebuilt}");
        assert!(rebuilt.contains("(fields_autoplaced yes)"), "{rebuilt}");
    }
}
