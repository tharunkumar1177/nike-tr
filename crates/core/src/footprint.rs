//! Footprints placed on a board.
//!
//! A `(footprint ...)` node keeps its full S-expression. The fields Tron edits are typed,
//! and every other child, such as graphics, 3D models, or pad shapes, passes through edits
//! untouched.

use crate::kicad::{deg_text, mm_text, read_at};
use crate::kicad_value::{angle, child_text, children, property, text_arg};
use crate::sexpr::SExpr;
use crate::units::{Angle, Nm, Point};
use uuid::Uuid;

/// The board side a footprint is mounted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Front,
    Back,
}

impl Side {
    /// The outer copper layer of the side, which a footprint names in its `(layer ...)`.
    pub fn copper_layer(self) -> &'static str {
        match self {
            Side::Front => "F.Cu",
            Side::Back => "B.Cu",
        }
    }

    /// The side whose outer copper layer is `name`.
    fn from_layer(name: &str) -> Option<Side> {
        match name {
            "F.Cu" => Some(Side::Front),
            "B.Cu" => Some(Side::Back),
            _ => None,
        }
    }
}

/// One `(pad ...)` of a placed footprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pad {
    /// The pad number, such as `1` or `A3`. Mounting holes have an empty number.
    pub number: String,
    /// The pad center relative to the footprint origin, before the footprint's rotation.
    pub position: Point,
    /// The pad orientation on the board, the footprint's rotation included.
    pub rotation: Angle,
    /// The name of the net the pad connects to, or `None` when it is unconnected.
    pub net: Option<String>,
}

impl Pad {
    /// Reads a `(pad "<number>" <type> <shape> (at x y [angle]) ...)` node. The net is read
    /// from `(net "Name")` or the older `(net 3 "Name")`; the empty net counts as none.
    fn read(node: &SExpr) -> Option<Pad> {
        if node.head() != Some("pad") {
            return None;
        }
        let number = text_arg(node, 0)?.to_owned();
        let (position, rotation) = read_at(node)?;
        Some(Pad {
            number,
            position,
            rotation,
            net: node.child("net").and_then(net_name).map(str::to_owned),
        })
    }

    /// Rebuilds the pad node with the typed number, `(at ...)`, and `(net ...)`, keeping every
    /// other child of `previous` in place. A new net goes after `(roundrect_rratio ...)` or
    /// whatever shape option precedes `(pinfunction ...)`, where KiCad writes it.
    fn to_node(&self, previous: &SExpr) -> SExpr {
        let mut items = vec![SExpr::atom("pad"), SExpr::string(self.number.as_str())];
        let mut net = self.net.as_deref().map(net_node);
        for child in previous.args().iter().skip(1) {
            match child.head() {
                Some("at") => items.push(board_at_node(self.position, self.rotation)),
                Some("net") => items.extend(net.take()),
                Some("pinfunction" | "pintype" | "uuid") => {
                    items.extend(net.take());
                    items.push(child.clone());
                }
                _ => items.push(child.clone()),
            }
        }
        items.extend(net);
        SExpr::List(items)
    }
}

/// A footprint placed on the board: one top-level `(footprint ...)` item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footprint {
    pub uuid: Uuid,
    /// The `<library>:<footprint>` the footprint was placed from.
    pub lib_id: String,
    pub position: Point,
    pub rotation: Angle,
    pub side: Side,
    pub reference: String,
    pub value: String,
    /// The schematic symbol the footprint belongs to, as KiCad's sheet path of symbol UUIDs:
    /// `/<symbol-uuid>` for a symbol on the root sheet.
    pub path: Option<String>,
    pub pads: Vec<Pad>,
}

impl Footprint {
    /// Reads a `(footprint ...)` node. Returns `None` when the node is not a footprint or its
    /// name, `(layer ...)` on an outer copper layer, `(uuid ...)`, or `(at ...)` is missing or
    /// malformed; such nodes are left untouched by every edit. Reference and value come from
    /// `(property "Reference" ...)` or the older `(fp_text reference ...)`.
    pub fn read(node: &SExpr) -> Option<Footprint> {
        if node.head() != Some("footprint") {
            return None;
        }
        let lib_id = match node.args().first()? {
            SExpr::Str(text) | SExpr::Atom(text) => text.clone(),
            SExpr::List(_) => return None,
        };
        let side = child_text(node, "layer").and_then(Side::from_layer)?;
        let uuid = footprint_uuid(node)?;
        let (position, rotation) = read_at(node)?;
        Some(Footprint {
            uuid,
            lib_id,
            position,
            rotation,
            side,
            reference: field(node, "Reference", "reference").unwrap_or_default(),
            value: field(node, "Value", "value").unwrap_or_default(),
            path: child_text(node, "path").map(str::to_owned),
            pads: node.args().iter().filter_map(Pad::read).collect(),
        })
    }

    /// The UUID of the schematic symbol the footprint belongs to: the last segment of its
    /// `(path ...)`.
    pub fn symbol_uuid(&self) -> Option<Uuid> {
        let path = self.path.as_deref()?;
        let last = path.rsplit('/').next()?;
        Uuid::parse_str(last).ok()
    }

    /// Where `pad` sits on the board: its offset rotated by the footprint's rotation, then
    /// moved to the footprint position. Quarter turns are exact; other angles round to the
    /// nearest nanometer.
    pub fn pad_position(&self, pad: &Pad) -> Point {
        let (x, y) = rotate((pad.position.x.0, pad.position.y.0), self.rotation);
        self.position.offset(Nm(x), Nm(y))
    }

    /// The footprint moved by `(dx, dy)`. Pads and graphics are stored relative to the
    /// origin, so only the origin moves.
    pub fn moved_by(&self, dx: Nm, dy: Nm) -> Footprint {
        let mut next = self.clone();
        next.position = next.position.offset(dx, dy);
        next
    }

    /// Rebuilds the file node from the typed state, reusing `previous` so unknown children
    /// keep their text and order. Pads merge with the previous node's pads in file order. A
    /// `(path ...)` the previous node lacked goes after its last property.
    pub fn to_node(&self, previous: &SExpr) -> SExpr {
        let mut items = vec![
            SExpr::atom("footprint"),
            SExpr::string(self.lib_id.as_str()),
        ];
        let mut pads = self.pads.iter();
        let mut path = self.path.as_deref().map(path_node);
        let mut property_end = None;
        for child in previous.args().iter().skip(1) {
            match child.head() {
                Some("layer") => items.push(SExpr::node(
                    "layer",
                    vec![SExpr::string(self.side.copper_layer())],
                )),
                Some("uuid") => items.push(SExpr::node(
                    "uuid",
                    vec![SExpr::string(self.uuid.to_string())],
                )),
                Some("at") => items.push(board_at_node(self.position, self.rotation)),
                Some("property") => {
                    items.push(match text_arg(child, 0) {
                        Some("Reference") => with_arg(child, 1, &self.reference),
                        Some("Value") => with_arg(child, 1, &self.value),
                        _ => child.clone(),
                    });
                    property_end = Some(items.len());
                }
                Some("fp_text") => items.push(match text_arg(child, 0) {
                    Some("reference") => with_arg(child, 1, &self.reference),
                    Some("value") => with_arg(child, 1, &self.value),
                    _ => child.clone(),
                }),
                Some("path") => items.extend(path.take()),
                Some("pad") => match pads.next() {
                    Some(pad) => items.push(pad.to_node(child)),
                    None => items.push(child.clone()),
                },
                _ => items.push(child.clone()),
            }
        }
        if let Some(path) = path {
            items.insert(property_end.unwrap_or(items.len()), path);
        }
        SExpr::List(items)
    }
}

/// The footprint node rotated about its own origin by `by`. Its `(at ...)` angle and those
/// of its pads and texts turn with it; offsets are stored unrotated, so they stay.
pub(crate) fn rotate_node(node: &SExpr, by: Angle) -> SExpr {
    let mut next = node.clone();
    for item in next.items_mut().into_iter().flatten().skip(1) {
        match item.head() {
            Some("at") => *item = turned_at(item, by),
            Some("pad" | "property" | "fp_text" | "fp_text_box") => {
                for child in item.items_mut().into_iter().flatten().skip(1) {
                    if child.head() == Some("at") {
                        *child = turned_at(child, by);
                    }
                }
            }
            _ => {}
        }
    }
    next
}

/// The footprint node flipped to the other side of the board the way KiCad flips top to
/// bottom about the footprint origin: child offsets mirror vertically, angles negate, `F.`
/// layers swap with `B.` layers, and texts toggle mirroring. 3D models follow the side
/// without edits, and zones are left as they are.
pub(crate) fn flip_node(node: &SExpr) -> SExpr {
    let mut next = node.clone();
    for item in next.items_mut().into_iter().flatten().skip(1) {
        match item.head() {
            Some("at") => *item = flipped_at(item, false),
            Some("model" | "zone") => {}
            _ => flip_child(item),
        }
    }
    next
}

/// Flips one child of a footprint node, and everything below it, in place.
fn flip_child(node: &mut SExpr) {
    match node.head() {
        Some("at") => *node = flipped_at(node, true),
        Some("start" | "end" | "mid" | "center" | "xy" | "offset") => {
            if let Some(y) = node.items_mut().and_then(|items| items.get_mut(2)) {
                *y = negated(y);
            }
        }
        Some("layer" | "layers") => {
            for name in node.items_mut().into_iter().flatten().skip(1) {
                if let SExpr::Str(text) = name {
                    *text = flipped_layer(text);
                }
            }
        }
        Some("effects") => toggle_mirror(node),
        Some("model" | "zone") => {}
        _ => {
            for child in node.items_mut().into_iter().flatten().skip(1) {
                flip_child(child);
            }
        }
    }
}

/// `(at x y [angle])` with its angle turned by `by`. The angle is written when the node had
/// one or the result isn't zero; trailing flags such as `unlocked` are kept.
fn turned_at(at: &SExpr, by: Angle) -> SExpr {
    let args = at.args();
    let current = match args.len() {
        0..=2 => Angle::ZERO,
        _ => match angle(at, 2) {
            Ok(current) => current,
            Err(_) => return at.clone(),
        },
    };
    with_angle(at, current.rotate(by))
}

/// `(at x y [angle])` with its angle negated, and its y offset too when `mirror` is set.
fn flipped_at(at: &SExpr, mirror: bool) -> SExpr {
    let current = match at.args().len() {
        0..=2 => Angle::ZERO,
        _ => match angle(at, 2) {
            Ok(current) => current,
            Err(_) => return at.clone(),
        },
    };
    let mut next = with_angle(at, Angle::from_decidegrees(-current.decidegrees()));
    if mirror && let Some(y) = next.items_mut().and_then(|items| items.get_mut(2)) {
        *y = negated(y);
    }
    next
}

/// `(at x y ...)` with `angle` as its third argument, written only when the node had an
/// angle or `angle` isn't zero.
fn with_angle(at: &SExpr, angle: Angle) -> SExpr {
    let args = at.args();
    let mut items = vec![SExpr::atom("at")];
    items.extend(args.iter().take(2).cloned());
    if args.len() > 2 || angle != Angle::ZERO {
        items.push(SExpr::atom(deg_text(angle)));
    }
    items.extend(args.iter().skip(3).cloned());
    SExpr::List(items)
}

/// A number atom with its sign flipped, keeping KiCad's decimal text. Zero stays `0`.
fn negated(value: &SExpr) -> SExpr {
    let Some(text) = value.text() else {
        return value.clone();
    };
    match text.strip_prefix('-') {
        Some(positive) => SExpr::atom(positive),
        None if text.trim_start_matches(['0', '.']).is_empty() => value.clone(),
        None => SExpr::atom(format!("-{text}")),
    }
}

/// The layer on the other side: `F.SilkS` for `B.SilkS` and back. Layers without a side,
/// such as `*.Cu` or `In1.Cu`, stay.
fn flipped_layer(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("F.") {
        format!("B.{rest}")
    } else if let Some(rest) = name.strip_prefix("B.") {
        format!("F.{rest}")
    } else {
        name.to_owned()
    }
}

/// Toggles `mirror` in a text's `(effects ... (justify ...))`, dropping an emptied
/// `(justify)`.
fn toggle_mirror(effects: &mut SExpr) {
    let Some(items) = effects.items_mut() else {
        return;
    };
    let is_mirror = |item: &SExpr| matches!(item, SExpr::Atom(atom) if atom == "mirror");
    let justify = items.iter().position(|item| item.head() == Some("justify"));
    match justify {
        Some(index) => {
            if let Some(parts) = items[index].items_mut() {
                if parts.iter().any(is_mirror) {
                    parts.retain(|part| !is_mirror(part));
                } else {
                    parts.push(SExpr::atom("mirror"));
                }
            }
        }
        None => items.push(SExpr::node("justify", vec![SExpr::atom("mirror")])),
    }
    items.retain(|item| item.head() != Some("justify") || !item.args().is_empty());
}

/// The net name of a `(net "Name")` or `(net 3 "Name")` node. The unnamed net `""`, KiCad's
/// "no net", reads as `None`.
pub(crate) fn net_name(node: &SExpr) -> Option<&str> {
    for arg in node.args() {
        if let SExpr::Str(name) = arg {
            return (!name.is_empty()).then_some(name.as_str());
        }
    }
    None
}

/// Writes `(net "Name")`, the KiCad 10 form.
pub(crate) fn net_node(name: &str) -> SExpr {
    SExpr::node("net", vec![SExpr::string(name)])
}

/// Writes `(path "/...")`.
fn path_node(path: &str) -> SExpr {
    SExpr::node("path", vec![SExpr::string(path)])
}

/// Writes `(at x y [angle])` the way board files do: the angle only when it is not zero.
pub(crate) fn board_at_node(position: Point, rotation: Angle) -> SExpr {
    let mut args = vec![
        SExpr::atom(mm_text(position.x)),
        SExpr::atom(mm_text(position.y)),
    ];
    if rotation != Angle::ZERO {
        args.push(SExpr::atom(deg_text(rotation)));
    }
    SExpr::node("at", args)
}

/// The `(uuid ...)` of a `(footprint ...)` node.
pub(crate) fn footprint_uuid(node: &SExpr) -> Option<Uuid> {
    if node.head() != Some("footprint") {
        return None;
    }
    child_text(node, "uuid").and_then(|text| Uuid::parse_str(text).ok())
}

/// The value of `(property "<key>" "<value>")`, or of the older `(fp_text <kind> "<value>")`.
fn field(node: &SExpr, key: &str, kind: &str) -> Option<String> {
    if let Some(value) = property(node, key) {
        return Some(value.to_owned());
    }
    children(node, "fp_text")
        .find(|item| text_arg(item, 0) == Some(kind))
        .and_then(|item| text_arg(item, 1))
        .map(str::to_owned)
}

/// A copy of `node` with argument `index` replaced by the string `value`.
fn with_arg(node: &SExpr, index: usize, value: &str) -> SExpr {
    let mut next = node.clone();
    if let Some(items) = next.items_mut()
        && let Some(slot) = items.get_mut(index + 1)
    {
        *slot = SExpr::string(value);
    }
    next
}

/// Rotates a Y-down offset counterclockwise on screen by `angle` (the KiCad convention).
/// Quarter turns are exact; other angles use floating point and round to the nanometer.
pub(crate) fn rotate(offset: (i64, i64), angle: Angle) -> (i64, i64) {
    let (x, y) = offset;
    match angle.decidegrees() {
        0 => (x, y),
        900 => (y, -x),
        1800 => (-x, -y),
        2700 => (-y, x),
        decidegrees => {
            let radians = (f64::from(decidegrees) / 10.0).to_radians();
            let (sin, cos) = radians.sin_cos();
            let (x, y) = (x as f64, y as f64);
            (
                (x * cos + y * sin).round() as i64,
                (y * cos - x * sin).round() as i64,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr;

    const FOOTPRINT_UUID: &str = "0b6f3c1e-4a2d-4f6b-9c8e-1d2a3b4c5d6e";
    const SYMBOL_UUID: &str = "11111111-2222-4333-8444-555555555555";

    /// An `R_0603_1608Metric` placed on a KiCad 10 board, as KiCad writes it.
    fn resistor_source() -> String {
        format!(
            "(footprint \"Resistor_SMD:R_0603_1608Metric\" (layer \"F.Cu\") \
            (uuid \"{FOOTPRINT_UUID}\") (at 100 50 90) \
            (descr \"Resistor SMD 0603 (1608 Metric)\") (tags \"resistor\") \
            (property \"Reference\" \"R1\" (at 0 -1.43 90) (layer \"F.SilkS\") \
            (effects (font (size 1 1) (thickness 0.15)))) \
            (property \"Value\" \"10k\" (at 0 1.43 90) (layer \"F.Fab\")) \
            (path \"/{SYMBOL_UUID}\") (sheetname \"/\") (sheetfile \"amp.kicad_sch\") \
            (attr smd) \
            (fp_line (start -0.8 -0.4125) (end 0.8 -0.4125) (stroke (width 0.1) (type solid)) \
            (layer \"F.Fab\")) \
            (pad \"1\" smd roundrect (at -0.825 0 90) (size 0.8 0.95) \
            (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") (roundrect_rratio 0.25) (net \"GND\") \
            (pintype \"passive\") (uuid \"0b6f3c1e-4a2d-4f6b-9c8e-1d2a3b4c5d01\")) \
            (pad \"2\" smd roundrect (at 0.825 0 90) (size 0.8 0.95) \
            (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") (roundrect_rratio 0.25) \
            (pintype \"passive\") (uuid \"0b6f3c1e-4a2d-4f6b-9c8e-1d2a3b4c5d02\")) \
            (model \"${{KICAD10_3DMODEL_DIR}}/Resistor_SMD.3dshapes/R_0603_1608Metric.step\" \
            (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0))))"
        )
    }

    fn resistor_node() -> SExpr {
        sexpr::parse(&resistor_source()).unwrap()
    }

    #[test]
    fn footprints_read_their_typed_fields() {
        let footprint = Footprint::read(&resistor_node()).unwrap();
        assert_eq!(footprint.uuid, Uuid::parse_str(FOOTPRINT_UUID).unwrap());
        assert_eq!(footprint.lib_id, "Resistor_SMD:R_0603_1608Metric");
        assert_eq!(
            footprint.position,
            Point::new(Nm(100_000_000), Nm(50_000_000))
        );
        assert_eq!(footprint.rotation, Angle::from_decidegrees(900));
        assert_eq!(footprint.side, Side::Front);
        assert_eq!(
            (footprint.reference.as_str(), footprint.value.as_str()),
            ("R1", "10k")
        );
        assert_eq!(
            footprint.symbol_uuid(),
            Some(Uuid::parse_str(SYMBOL_UUID).unwrap())
        );
        assert_eq!(footprint.pads.len(), 2);
        assert_eq!(footprint.pads[0].number, "1");
        assert_eq!(footprint.pads[0].net.as_deref(), Some("GND"));
        assert_eq!(footprint.pads[0].position, Point::new(Nm(-825_000), Nm(0)));
        assert_eq!(footprint.pads[1].net, None);
    }

    #[test]
    fn pad_positions_follow_the_footprint_rotation() {
        let footprint = Footprint::read(&resistor_node()).unwrap();
        assert_eq!(
            footprint.pad_position(&footprint.pads[0]),
            Point::new(Nm(100_000_000), Nm(50_825_000))
        );
        let angled = Footprint {
            rotation: Angle::from_decidegrees(450),
            ..footprint.clone()
        };
        assert_eq!(
            angled.pad_position(&angled.pads[1]),
            Point::new(Nm(100_583_363), Nm(49_416_637))
        );
    }

    #[test]
    fn editing_a_footprint_keeps_unknown_children_in_place() {
        let previous = resistor_node();
        let mut footprint = Footprint::read(&previous)
            .unwrap()
            .moved_by(Nm(1_270_000), Nm(0));
        footprint.reference = "R7".to_owned();
        footprint.pads[0].net = None;
        footprint.pads[1].net = Some("VCC".to_owned());
        let node = footprint.to_node(&previous);
        let text = node.to_pretty_string();
        assert!(text.contains("(at 101.27 50 90)"), "{text}");
        assert!(text.contains("(property \"Reference\" \"R7\""), "{text}");
        assert!(text.contains("(effects"), "{text}");
        assert!(text.contains("(model"), "{text}");
        assert!(text.contains("(sheetfile \"amp.kicad_sch\")"), "{text}");
        assert!(!text.contains("(net \"GND\")"), "{text}");
        assert!(
            text.contains("(net \"VCC\")\n\t\t(pintype \"passive\")"),
            "{text}"
        );
        assert_eq!(Footprint::read(&node), Some(footprint));
    }

    #[test]
    fn unchanged_footprints_write_back_identically() {
        let previous = resistor_node();
        let footprint = Footprint::read(&previous).unwrap();
        assert_eq!(footprint.to_node(&previous), previous);
    }

    #[test]
    fn older_footprints_read_numbered_nets_and_text_fields() {
        let source = format!(
            "(footprint \"Device:R\" (layer \"B.Cu\") (uuid \"{FOOTPRINT_UUID}\") (at 5 6) \
            (fp_text reference \"R2\" (at 0 0)) (fp_text value \"1k\" (at 0 1)) \
            (pad \"1\" thru_hole circle (at 0 0) (size 1.6 1.6) (drill 0.8) \
            (net 3 \"/OUT\")))"
        );
        let node = sexpr::parse(&source).unwrap();
        let footprint = Footprint::read(&node).unwrap();
        assert_eq!(footprint.side, Side::Back);
        assert_eq!(footprint.rotation, Angle::ZERO);
        assert_eq!(footprint.reference, "R2");
        assert_eq!(footprint.value, "1k");
        assert_eq!(footprint.path, None);
        assert_eq!(footprint.pads[0].net.as_deref(), Some("/OUT"));

        let mut linked = footprint.clone();
        linked.path = Some(format!("/{SYMBOL_UUID}"));
        let text = linked.to_node(&node).to_pretty_string();
        assert!(text.contains("(net \"/OUT\")"), "{text}");
        let path = format!("(path \"/{SYMBOL_UUID}\")");
        assert!(text.contains(&path), "{text}");
        assert!(text.contains("(at 5 6)"), "{text}");
    }

    /// Reads a footprint from source text.
    fn typed(source: &str) -> Option<Footprint> {
        Footprint::read(&sexpr::parse(source).unwrap())
    }

    #[test]
    fn malformed_footprints_are_not_typed() {
        let source = resistor_source();
        assert!(typed(&source.replace("(at 100 50 90)", "")).is_none());
        let (front, inner) = ("(layer \"F.Cu\")", "(layer \"In1.Cu\")");
        assert!(typed(&source.replace(front, inner)).is_none());
        assert!(typed(&source.replace(FOOTPRINT_UUID, "not-a-uuid")).is_none());
    }

    #[test]
    fn rotating_turns_the_footprint_pads_and_texts() {
        let node = rotate_node(&resistor_node(), Angle::from_decidegrees(900));
        let footprint = Footprint::read(&node).unwrap();
        assert_eq!(footprint.rotation, Angle::from_decidegrees(1800));
        assert_eq!(footprint.pads[0].rotation, Angle::from_decidegrees(1800));
        assert_eq!(footprint.pads[0].position, Point::new(Nm(-825_000), Nm(0)));
        let text = node.to_pretty_string();
        assert!(text.contains("(at 0 -1.43 180)"), "{text}");
        let back = rotate_node(&node, Angle::from_decidegrees(2700));
        assert_eq!(back, resistor_node());
    }

    #[test]
    fn flipping_moves_the_footprint_to_the_back() {
        let node = flip_node(&resistor_node());
        let footprint = Footprint::read(&node).unwrap();
        assert_eq!(footprint.side, Side::Back);
        assert_eq!(footprint.rotation, Angle::from_decidegrees(2700));
        assert_eq!(
            footprint.pad_position(&footprint.pads[0]),
            Point::new(Nm(100_000_000), Nm(49_175_000))
        );
        let text = node.to_pretty_string();
        assert!(
            text.contains("(layers \"B.Cu\" \"B.Mask\" \"B.Paste\")"),
            "{text}"
        );
        assert!(text.contains("(at -0.825 0 270)"), "{text}");
        assert!(text.contains("(at 0 1.43 270)"), "{text}");
        assert!(text.contains("(layer \"B.SilkS\")"), "{text}");
        assert!(text.contains("(justify mirror)"), "{text}");
        assert!(text.contains("(start -0.8 0.4125)"), "{text}");
        assert_eq!(flip_node(&node), resistor_node());
    }

    #[test]
    fn rotation_is_exact_on_quarter_turns() {
        let quarter = |steps: i32| Angle::from_decidegrees(steps * 900);
        assert_eq!(rotate((3, 4), quarter(0)), (3, 4));
        assert_eq!(rotate((3, 4), quarter(1)), (4, -3));
        assert_eq!(rotate((3, 4), quarter(2)), (-3, -4));
        assert_eq!(rotate((3, 4), quarter(3)), (-4, 3));
        assert_eq!(
            rotate((1_000_000, 0), Angle::from_decidegrees(300)),
            (866_025, -500_000)
        );
    }
}
