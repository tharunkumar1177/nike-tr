//! KiCad board files.
//!
//! A `.kicad_pcb` is held as its full S-expression tree. Footprints, the `Edge.Cuts`
//! outline, the `(layers)` table, and `setup.stackup` are read as typed values; zones,
//! tracks, vias, and every other node pass through untouched. Files are upgraded to the
//! KiCad 10 form when they are read: the header names format `20260206`, layer ordinals use
//! KiCad 10's numbering, and nets are referenced by name instead of through a numbered table.

use std::collections::HashMap;

use crate::footprint::{Footprint, footprint_uuid, net_name, net_node};
use crate::kicad::{FileError, GENERATOR, mm_text, parse_kind};
use crate::kicad_value::{child_text, children, length, number, point, points, stroke_width};
use crate::sexpr::SExpr;
use crate::units::{Nm, Point};
use uuid::Uuid;

pub const BOARD_FORMAT_VERSION: &str = "20260206";
pub const BOARD_GENERATOR_VERSION: &str = "10.0";

/// The first board format with KiCad 10's layer numbering. Older files carry layer bit
/// masks in the old numbering, which can't be read under the new version.
const RENUMBERED_LAYERS_VERSION: u32 = 20241229;

/// The largest coordinate KiCad stores: board positions are 32-bit nanometers.
pub const MAX_COORDINATE: Nm = Nm(i32::MAX as i64);

/// Total board thickness of a new stackup, in nanometers.
const BOARD_THICKNESS: i64 = 1_600_000;
const COPPER_THICKNESS: i64 = 35_000;
const MASK_THICKNESS: i64 = 10_000;

/// What a `(layers)` entry is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Signal,
    Power,
    Mixed,
    Jumper,
    User,
}

impl LayerKind {
    /// The kind named by the token in a `(layers)` entry, such as `signal`.
    fn parse(token: &str) -> Option<LayerKind> {
        match token {
            "signal" => Some(LayerKind::Signal),
            "power" => Some(LayerKind::Power),
            "mixed" => Some(LayerKind::Mixed),
            "jumper" => Some(LayerKind::Jumper),
            "user" => Some(LayerKind::User),
            _ => None,
        }
    }
}

/// One entry of the board's `(layers)` table: `(25 "Edge.Cuts" user)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// The KiCad layer ID.
    pub ordinal: u32,
    /// The canonical name other nodes refer to, such as `F.SilkS`.
    pub name: String,
    pub kind: LayerKind,
    /// The display name, such as `F.Silkscreen`, when it differs from the canonical name.
    pub user_name: Option<String>,
}

impl Layer {
    /// Whether the layer carries copper.
    pub fn is_copper(&self) -> bool {
        self.name.ends_with(".Cu")
    }

    /// Reads `(<ordinal> "<name>" <kind> ["<user name>"])`.
    fn read(node: &SExpr) -> Option<Layer> {
        let SExpr::List(items) = node else {
            return None;
        };
        let ordinal = items.first()?.text()?.parse().ok()?;
        let name = items.get(1)?.text()?.to_owned();
        let kind = LayerKind::parse(items.get(2)?.text()?)?;
        let user_name = items.get(3).and_then(SExpr::text).map(str::to_owned);
        Some(Layer {
            ordinal,
            name,
            kind,
            user_name,
        })
    }
}

/// The KiCad 10 ID of the layer named `name`, or `None` for a name KiCad doesn't define.
pub fn layer_ordinal(name: &str) -> Option<u32> {
    let fixed = match name {
        "F.Cu" => Some(0),
        "F.Mask" => Some(1),
        "B.Cu" => Some(2),
        "B.Mask" => Some(3),
        "F.SilkS" => Some(5),
        "B.SilkS" => Some(7),
        "F.Adhes" => Some(9),
        "B.Adhes" => Some(11),
        "F.Paste" => Some(13),
        "B.Paste" => Some(15),
        "Dwgs.User" => Some(17),
        "Cmts.User" => Some(19),
        "Eco1.User" => Some(21),
        "Eco2.User" => Some(23),
        "Edge.Cuts" => Some(25),
        "Margin" => Some(27),
        "B.CrtYd" => Some(29),
        "F.CrtYd" => Some(31),
        "B.Fab" => Some(33),
        "F.Fab" => Some(35),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }
    if let Some(n) = numbered_layer(name, "In", ".Cu", 30) {
        return Some(2 + 2 * n);
    }
    numbered_layer(name, "User.", "", 45).map(|n| 37 + 2 * n)
}

/// The number `n` in a layer name `<prefix><n><suffix>`, when it is between 1 and `max`.
fn numbered_layer(name: &str, prefix: &str, suffix: &str, max: u32) -> Option<u32> {
    let digits = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    let n = digits.parse().ok()?;
    (1..=max).contains(&n).then_some(n)
}

/// Child `head`'s first argument read as millimeters.
fn child_length(node: &SExpr, head: &str) -> Option<Nm> {
    length(node.child(head)?, 0).ok()
}

/// Child `head`'s first argument read as a number.
fn child_value(node: &SExpr, head: &str) -> Option<f64> {
    number(node.child(head)?, 0).ok()
}

/// One `(layer ...)` of `setup.stackup`: a copper, dielectric, mask, paste, or silkscreen
/// layer of the physical board.
#[derive(Debug, Clone, PartialEq)]
pub struct StackupLayer {
    /// The board layer name, such as `F.Cu`, or `dielectric 1`.
    pub name: String,
    /// KiCad's type name, such as `copper`, `core`, `prepreg`, or `Top Solder Mask`.
    pub kind: String,
    pub thickness: Option<Nm>,
    pub material: Option<String>,
    /// The relative permittivity of a dielectric or mask.
    pub epsilon_r: Option<f64>,
    pub loss_tangent: Option<f64>,
}

impl StackupLayer {
    /// Reads `(layer "<name>" (type "<kind>") (thickness mm) ...)`.
    fn read(node: &SExpr) -> Option<StackupLayer> {
        let name = node.args().first().and_then(SExpr::text)?.to_owned();
        let kind = child_text(node, "type")?.to_owned();
        Some(StackupLayer {
            name,
            kind,
            thickness: child_length(node, "thickness"),
            material: child_text(node, "material").map(str::to_owned),
            epsilon_r: child_value(node, "epsilon_r"),
            loss_tangent: child_value(node, "loss_tangent"),
        })
    }

    /// Whether the layer is a dielectric between two copper layers.
    pub fn is_dielectric(&self) -> bool {
        matches!(self.kind.as_str(), "core" | "prepreg")
    }
}

/// The board's physical layer stack, from `setup.stackup`, top to bottom.
#[derive(Debug, Clone, PartialEq)]
pub struct Stackup {
    pub layers: Vec<StackupLayer>,
    /// The copper surface finish, such as `None` or `ENIG`.
    pub copper_finish: Option<String>,
}

impl Stackup {
    /// The sum of every layer's thickness.
    pub fn thickness(&self) -> Nm {
        let total: i64 = self
            .layers
            .iter()
            .filter_map(|layer| layer.thickness)
            .map(|thickness| thickness.0)
            .sum();
        Nm(total)
    }
}

/// The geometry of a board graphic on `Edge.Cuts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlineShape {
    /// A straight segment.
    Line { start: Point, end: Point },
    /// A circular arc through three points.
    Arc {
        start: Point,
        mid: Point,
        end: Point,
    },
    /// An axis-aligned rectangle between opposite corners.
    Rect { start: Point, end: Point },
    /// A circle through `end` around `center`.
    Circle { center: Point, end: Point },
    /// A closed polygon through its corners.
    Polygon { points: Vec<Point> },
}

impl OutlineShape {
    /// Every point that defines the shape.
    pub fn points(&self) -> Vec<Point> {
        match self {
            OutlineShape::Line { start, end } | OutlineShape::Rect { start, end } => {
                vec![*start, *end]
            }
            OutlineShape::Arc { start, mid, end } => vec![*start, *mid, *end],
            OutlineShape::Circle { center, end } => vec![*center, *end],
            OutlineShape::Polygon { points } => points.clone(),
        }
    }
}

/// A top-level `gr_line`, `gr_arc`, `gr_rect`, `gr_circle`, or `gr_poly` on `Edge.Cuts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardGraphic {
    pub uuid: Option<Uuid>,
    pub shape: OutlineShape,
    /// The stroke width; zero when the file gives none.
    pub width: Nm,
}

impl BoardGraphic {
    /// Reads an `Edge.Cuts` graphic. Graphics on other layers, and the pre-KiCad 6 arc form
    /// without a `(mid ...)` point, are not typed.
    fn read(node: &SExpr) -> Option<BoardGraphic> {
        if child_text(node, "layer") != Some("Edge.Cuts") {
            return None;
        }
        let at = |head: &str| point(node.child(head)?).ok();
        let shape = match node.head()? {
            "gr_line" => OutlineShape::Line {
                start: at("start")?,
                end: at("end")?,
            },
            "gr_arc" => OutlineShape::Arc {
                start: at("start")?,
                mid: at("mid")?,
                end: at("end")?,
            },
            "gr_rect" => OutlineShape::Rect {
                start: at("start")?,
                end: at("end")?,
            },
            "gr_circle" => OutlineShape::Circle {
                center: at("center")?,
                end: at("end")?,
            },
            "gr_poly" => OutlineShape::Polygon {
                points: points(node).ok()?,
            },
            _ => return None,
        };
        Some(BoardGraphic {
            uuid: child_text(node, "uuid").and_then(|text| Uuid::parse_str(text).ok()),
            shape,
            width: stroke_width(node).ok()?.unwrap_or(Nm::ZERO),
        })
    }
}

/// A `.kicad_pcb` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    root: SExpr,
}

impl Board {
    /// A blank two-layer board with KiCad's default layers and a 1.6 mm FR4 stackup.
    pub fn new_empty() -> Board {
        Board {
            root: SExpr::List(vec![
                SExpr::atom("kicad_pcb"),
                SExpr::node("version", vec![SExpr::atom(BOARD_FORMAT_VERSION)]),
                SExpr::node("generator", vec![SExpr::string(GENERATOR)]),
                SExpr::node(
                    "generator_version",
                    vec![SExpr::string(BOARD_GENERATOR_VERSION)],
                ),
                SExpr::node(
                    "general",
                    vec![SExpr::node(
                        "thickness",
                        vec![SExpr::atom(mm_text(Nm(BOARD_THICKNESS)))],
                    )],
                ),
                SExpr::node("paper", vec![SExpr::string("A4")]),
                default_layers(2),
                SExpr::node(
                    "setup",
                    vec![
                        default_stackup(2),
                        SExpr::node("pad_to_mask_clearance", vec![SExpr::atom("0")]),
                    ],
                ),
            ]),
        }
    }

    /// Parses a board file and upgrades it to the KiCad 10 form.
    pub fn parse(source: &str) -> Result<Board, FileError> {
        let mut root = parse_kind(source, "kicad_pcb")?;
        upgrade(&mut root);
        Ok(Board { root })
    }

    pub fn to_file_string(&self) -> String {
        self.root.to_pretty_string()
    }

    /// Every footprint with a readable name, side, UUID, and position, in file order. A
    /// footprint that fails to read stays in the file untouched.
    pub fn footprints(&self) -> Vec<Footprint> {
        self.root
            .args()
            .iter()
            .filter_map(Footprint::read)
            .collect()
    }

    /// The footprint with `uuid`, if present and readable.
    pub fn footprint(&self, uuid: Uuid) -> Option<Footprint> {
        self.root.args().iter().find_map(|item| {
            let footprint = Footprint::read(item)?;
            (footprint.uuid == uuid).then_some(footprint)
        })
    }

    /// Every readable top-level graphic on `Edge.Cuts`, in file order.
    pub fn edge_cuts(&self) -> Vec<BoardGraphic> {
        self.root
            .args()
            .iter()
            .filter_map(BoardGraphic::read)
            .collect()
    }

    /// Every readable entry of the `(layers)` table, in file order.
    pub fn layers(&self) -> Vec<Layer> {
        self.root
            .child("layers")
            .map(|node| node.args().iter().filter_map(Layer::read).collect())
            .unwrap_or_default()
    }

    /// The number of copper layers in the `(layers)` table.
    pub fn copper_layer_count(&self) -> usize {
        self.layers()
            .iter()
            .filter(|layer| layer.is_copper())
            .count()
    }

    /// The physical stackup, or `None` when the file's setup has none.
    pub fn stackup(&self) -> Option<Stackup> {
        let node = self.root.child("setup")?.child("stackup")?;
        Some(Stackup {
            layers: children(node, "layer")
                .filter_map(StackupLayer::read)
                .collect(),
            copper_finish: child_text(node, "copper_finish").map(str::to_owned),
        })
    }

    /// The node of the footprint with `uuid`.
    pub(crate) fn footprint_node(&self, uuid: Uuid) -> Option<&SExpr> {
        self.items()
            .iter()
            .find(|item| footprint_uuid(item) == Some(uuid))
    }

    /// The top-level items after the `kicad_pcb` head. Item `i` sits at child index `i + 1`.
    pub(crate) fn items(&self) -> &[SExpr] {
        self.root.args()
    }

    /// Removes the top-level item at child `index`.
    pub(crate) fn remove_item_at(&mut self, index: usize) {
        if let Some(items) = self.root.items_mut()
            && index > 0
            && index < items.len()
        {
            items.remove(index);
        }
    }

    /// Inserts a top-level node at child `index`, clamped to the end.
    pub(crate) fn insert_item_at(&mut self, index: usize, node: SExpr) {
        if let Some(items) = self.root.items_mut() {
            items.insert(index.clamp(1, items.len()), node);
        }
    }

    /// The child index a new footprint is inserted at: after the last footprint, else before
    /// the first board graphic, track, zone, or group, else at the end. KiCad writes
    /// footprints in that place.
    pub(crate) fn footprint_insert_index(&self) -> usize {
        let items = self.items();
        if let Some(last) = items
            .iter()
            .rposition(|item| item.head() == Some("footprint"))
        {
            return last + 2;
        }
        items
            .iter()
            .position(follows_footprints)
            .map_or(items.len() + 1, |position| position + 1)
    }

    /// Checks that every typed coordinate fits KiCad's 32-bit nanometer range, naming the
    /// first item that doesn't.
    pub fn check_export_bounds(&self) -> Result<(), String> {
        let check = |what: &str, point: Point| {
            let fits = |value: Nm| value.abs() <= MAX_COORDINATE;
            if fits(point.x) && fits(point.y) {
                return Ok(());
            }
            Err(format!(
                "{what} at ({}, {}) mm is outside the ±{} mm area KiCad boards can hold",
                mm_text(point.x),
                mm_text(point.y),
                mm_text(MAX_COORDINATE)
            ))
        };
        for footprint in self.footprints() {
            let name = format!("Footprint {}", footprint.reference);
            check(&name, footprint.position)?;
            for pad in &footprint.pads {
                let name = format!("Pad {} of {}", pad.number, footprint.reference);
                check(&name, footprint.pad_position(pad))?;
            }
        }
        for graphic in self.edge_cuts() {
            for point in graphic.shape.points() {
                check("The board outline", point)?;
            }
        }
        Ok(())
    }
}

/// Top-level items KiCad writes after the footprints.
const AFTER_FOOTPRINTS: [&str; 11] = [
    "dimension",
    "image",
    "target",
    "segment",
    "arc",
    "via",
    "zone",
    "group",
    "generated",
    "embedded_fonts",
    "embedded_files",
];

/// Whether KiCad writes the top-level `node` after the footprints.
fn follows_footprints(node: &SExpr) -> bool {
    let head = node.head().unwrap_or_default();
    head.starts_with("gr_") || AFTER_FOOTPRINTS.contains(&head)
}

/// The `(layers)` table of a new board with `copper` copper layers: the copper layers in
/// stack order, then KiCad's technical and user layers.
pub(crate) fn default_layers(copper: usize) -> SExpr {
    let entry = |name: &str, kind: &str, user_name: Option<&str>| {
        let ordinal = layer_ordinal(name).unwrap_or_default();
        let mut items = vec![
            SExpr::atom(ordinal.to_string()),
            SExpr::string(name),
            SExpr::atom(kind),
        ];
        items.extend(user_name.map(SExpr::string));
        SExpr::List(items)
    };
    let mut layers = Vec::new();
    for name in copper_names(copper) {
        layers.push(entry(&name, "signal", None));
    }
    let technical = [
        ("F.Adhes", Some("F.Adhesive")),
        ("B.Adhes", Some("B.Adhesive")),
        ("F.Paste", None),
        ("B.Paste", None),
        ("F.SilkS", Some("F.Silkscreen")),
        ("B.SilkS", Some("B.Silkscreen")),
        ("F.Mask", None),
        ("B.Mask", None),
        ("Dwgs.User", Some("User.Drawings")),
        ("Cmts.User", Some("User.Comments")),
        ("Eco1.User", Some("User.Eco1")),
        ("Eco2.User", Some("User.Eco2")),
        ("Edge.Cuts", None),
        ("Margin", None),
        ("F.CrtYd", Some("F.Courtyard")),
        ("B.CrtYd", Some("B.Courtyard")),
        ("F.Fab", None),
        ("B.Fab", None),
    ];
    for (name, user_name) in technical {
        layers.push(entry(name, "user", user_name));
    }
    SExpr::node("layers", layers)
}

/// The `(stackup)` of a new 1.6 mm board with `copper` copper layers: 0.035 mm copper,
/// 0.01 mm masks, and FR4 dielectrics (εr 4.5, loss tangent 0.02) sharing the remaining
/// thickness equally. A two-layer board has one core; thicker boards alternate prepreg and
/// core, starting and ending with prepreg.
pub(crate) fn default_stackup(copper: usize) -> SExpr {
    let layer = |name: &str, kind: &str, thickness: Option<i64>| {
        let mut items = vec![
            SExpr::string(name),
            SExpr::node("type", vec![SExpr::string(kind)]),
        ];
        if let Some(thickness) = thickness {
            items.push(SExpr::node(
                "thickness",
                vec![SExpr::atom(mm_text(Nm(thickness)))],
            ));
        }
        SExpr::node("layer", items)
    };
    let dielectrics = copper.saturating_sub(1).max(1) as i64;
    let remaining = BOARD_THICKNESS - COPPER_THICKNESS * copper as i64 - 2 * MASK_THICKNESS;
    let dielectric_thickness = (remaining + dielectrics / 2) / dielectrics;
    let mut layers = vec![
        layer("F.SilkS", "Top Silk Screen", None),
        layer("F.Paste", "Top Solder Paste", None),
        layer("F.Mask", "Top Solder Mask", Some(MASK_THICKNESS)),
    ];
    let names = copper_names(copper);
    for (index, name) in names.iter().enumerate() {
        layers.push(layer(name, "copper", Some(COPPER_THICKNESS)));
        if index + 1 == names.len() {
            break;
        }
        let number = index + 1;
        let kind = if names.len() == 2 || number % 2 == 0 {
            "core"
        } else {
            "prepreg"
        };
        let mut dielectric = layer(
            &format!("dielectric {number}"),
            kind,
            Some(dielectric_thickness),
        );
        if let Some(items) = dielectric.items_mut() {
            items.extend([
                SExpr::node("material", vec![SExpr::string("FR4")]),
                SExpr::node("epsilon_r", vec![SExpr::atom("4.5")]),
                SExpr::node("loss_tangent", vec![SExpr::atom("0.02")]),
            ]);
        }
        layers.push(dielectric);
    }
    layers.extend([
        layer("B.Mask", "Bottom Solder Mask", Some(MASK_THICKNESS)),
        layer("B.Paste", "Bottom Solder Paste", None),
        layer("B.SilkS", "Bottom Silk Screen", None),
        SExpr::node("copper_finish", vec![SExpr::string("None")]),
        SExpr::node("dielectric_constraints", vec![SExpr::atom("no")]),
    ]);
    SExpr::node("stackup", layers)
}

/// The copper layer names of a `copper`-layer board, top to bottom.
fn copper_names(copper: usize) -> Vec<String> {
    let inner = copper.saturating_sub(2);
    let mut names = vec!["F.Cu".to_owned()];
    names.extend((1..=inner).map(|n| format!("In{n}.Cu")));
    names.push("B.Cu".to_owned());
    names
}

/// Rewrites a parsed board into the KiCad 10 form: the header, layer ordinals, and nets by
/// name. The numbered top-level net table is dropped once every reference is renamed, and
/// layer bit masks written in the old numbering are dropped so KiCad falls back to its
/// defaults.
fn upgrade(root: &mut SExpr) {
    let Some(items) = root.items_mut() else {
        return;
    };
    let version = items
        .iter()
        .find(|item| item.head() == Some("version"))
        .and_then(child_number)
        .unwrap_or(0);
    let mut nets = HashMap::new();
    items.retain(|item| {
        let Some(id) = numbered_net(item) else {
            return true;
        };
        if let Some(name) = net_name(item) {
            nets.insert(id.to_owned(), name.to_owned());
        }
        false
    });
    upgrade_header(items, version);
    for item in items.iter_mut().skip(1) {
        match item.head() {
            Some("layers") => renumber_layers(item),
            Some("setup") if version < RENUMBERED_LAYERS_VERSION => drop_layer_masks(item),
            _ => {}
        }
        rename_nets(item, &nets);
    }
}

/// The first argument of `node` read as an unsigned integer.
fn child_number(node: &SExpr) -> Option<u32> {
    node.args().first()?.text()?.parse().ok()
}

/// The ID of an old-style `(net 3 ...)` node.
fn numbered_net(node: &SExpr) -> Option<&str> {
    if node.head() != Some("net") {
        return None;
    }
    match node.args().first() {
        Some(SExpr::Atom(id)) if id.parse::<u32>().is_ok() => Some(id),
        _ => None,
    }
}

/// Sets the format version and generator, inserting `(generator_version ...)` after
/// `(generator ...)` when the file has none. A newer format version is kept.
fn upgrade_header(items: &mut Vec<SExpr>, version: u32) {
    let ours: u32 = BOARD_FORMAT_VERSION.parse().unwrap_or(u32::MAX);
    let mut generator_index = None;
    let mut has_generator_version = false;
    for (index, item) in items.iter_mut().enumerate() {
        match item.head() {
            Some("version") if version < ours => {
                *item = SExpr::node("version", vec![SExpr::atom(BOARD_FORMAT_VERSION)]);
            }
            Some("generator") => {
                *item = SExpr::node("generator", vec![SExpr::string(GENERATOR)]);
                generator_index = Some(index);
            }
            Some("generator_version") => {
                *item = SExpr::node(
                    "generator_version",
                    vec![SExpr::string(BOARD_GENERATOR_VERSION)],
                );
                has_generator_version = true;
            }
            _ => {}
        }
    }
    if let (Some(index), false) = (generator_index, has_generator_version) {
        items.insert(
            index + 1,
            SExpr::node(
                "generator_version",
                vec![SExpr::string(BOARD_GENERATOR_VERSION)],
            ),
        );
    }
}

/// Rewrites each `(layers)` entry's ordinal to KiCad 10's ID for its name. Entries with a
/// name KiCad doesn't define keep their ordinal.
fn renumber_layers(layers: &mut SExpr) {
    for entry in layers.items_mut().into_iter().flatten().skip(1) {
        let Some(items) = entry.items_mut() else {
            continue;
        };
        let ordinal = items.get(1).and_then(SExpr::text).and_then(layer_ordinal);
        if let (Some(ordinal), Some(first)) = (ordinal, items.first_mut()) {
            *first = SExpr::atom(ordinal.to_string());
        }
    }
}

/// Removes the plot settings' layer bit masks, which are indexed by layer ordinal.
fn drop_layer_masks(setup: &mut SExpr) {
    for child in setup.items_mut().into_iter().flatten().skip(1) {
        if child.head() != Some("pcbplotparams") {
            continue;
        }
        if let Some(items) = child.items_mut() {
            items.retain(|item| {
                !matches!(
                    item.head(),
                    Some("layerselection" | "plot_on_all_layers_selection")
                )
            });
        }
    }
}

/// Rewrites every `(net 3 "Name")` or `(net 3)` below `node` as `(net "Name")`, looking the
/// name up in `nets` when the node has none. A reference to the unnamed net, or to an ID the
/// table doesn't hold, is removed: KiCad writes no net for unconnected items.
fn rename_nets(node: &mut SExpr, nets: &HashMap<String, String>) {
    let Some(items) = node.items_mut() else {
        return;
    };
    let mut index = 1;
    while index < items.len() {
        if let Some(id) = numbered_net(&items[index]) {
            let name = net_name(&items[index])
                .map(str::to_owned)
                .or_else(|| nets.get(id).cloned());
            match name {
                Some(name) => {
                    items[index] = net_node(&name);
                    index += 1;
                }
                None => {
                    items.remove(index);
                }
            }
            continue;
        }
        rename_nets(&mut items[index], nets);
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint::Side;

    /// A KiCad 8 board: the old layer numbering, a numbered net table, and nets by number on
    /// pads, tracks, and zones.
    const LEGACY: &str = r#"(kicad_pcb (version 20240108) (generator "pcbnew") (generator_version "8.0")
        (general (thickness 1.6) (legacy_teardrops no))
        (paper "A4")
        (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (37 "F.SilkS" user "F.Silkscreen")
            (44 "Edge.Cuts" user) (50 "User.1" user))
        (setup (pad_to_mask_clearance 0)
            (pcbplotparams (layerselection 0x00010fc_ffffffff) (plot_on_all_layers_selection 0x0000000_00000000) (outputdirectory "")))
        (net 0 "")
        (net 1 "GND")
        (net 2 "/OUT")
        (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu")
            (uuid "0b6f3c1e-4a2d-4f6b-9c8e-1d2a3b4c5d6e") (at 120 80)
            (property "Reference" "R1" (at 0 -1.43 0) (layer "F.SilkS"))
            (property "Value" "10k" (at 0 1.43 0) (layer "F.Fab"))
            (path "/11111111-2222-4333-8444-555555555555")
            (pad "1" smd roundrect (at -0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste") (net 1 "GND"))
            (pad "2" smd roundrect (at 0.825 0) (size 0.8 0.95) (layers "F.Cu" "F.Mask" "F.Paste") (net 0 "")))
        (gr_rect (start 100 60) (end 150 100) (stroke (width 0.05) (type default)) (fill none) (layer "Edge.Cuts")
            (uuid "7f0d2c4a-1b3e-4c5d-8e9f-0a1b2c3d4e5f"))
        (gr_line (start 100 50) (end 150 50) (stroke (width 0.1) (type default)) (layer "F.SilkS"))
        (segment (start 119.175 80) (end 110 80) (width 0.25) (layer "F.Cu") (net 1) (uuid "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d"))
        (zone (net 2) (net_name "/OUT") (layer "B.Cu") (uuid "3c4d5e6f-7a8b-4c9d-8e0f-1a2b3c4d5e6f") (hatch edge 0.5))
    )"#;

    fn legacy() -> Board {
        Board::parse(LEGACY).unwrap()
    }

    #[test]
    fn legacy_boards_upgrade_to_the_kicad_10_form() {
        let text = legacy().to_file_string();
        assert!(text.contains("(version 20260206)"), "{text}");
        assert!(text.contains("(generator \"tron\")"), "{text}");
        assert!(text.contains("(generator_version \"10.0\")"), "{text}");
        assert!(text.contains("(2 \"B.Cu\" signal)"), "{text}");
        assert!(
            text.contains("(5 \"F.SilkS\" user \"F.Silkscreen\")"),
            "{text}"
        );
        assert!(text.contains("(25 \"Edge.Cuts\" user)"), "{text}");
        assert!(text.contains("(39 \"User.1\" user)"), "{text}");
        assert!(!text.contains("layerselection"), "{text}");
        assert!(text.contains("(outputdirectory \"\")"), "{text}");
        assert!(!text.contains("(net 0"), "{text}");
        assert!(!text.contains("(net 1"), "{text}");
        assert_eq!(text.matches("(net \"GND\")").count(), 2, "{text}");
        assert!(text.contains("(net \"/OUT\")"), "{text}");
        assert!(text.contains("(net_name \"/OUT\")"), "{text}");
        assert!(text.contains("(legacy_teardrops no)"), "{text}");
    }

    #[test]
    fn upgraded_boards_read_back_unchanged() {
        let board = legacy();
        assert_eq!(Board::parse(&board.to_file_string()).unwrap(), board);
    }

    #[test]
    fn footprints_and_pad_nets_are_typed() {
        let board = legacy();
        let footprints = board.footprints();
        assert_eq!(footprints.len(), 1);
        let footprint = &footprints[0];
        assert_eq!(footprint.reference, "R1");
        assert_eq!(footprint.side, Side::Front);
        assert_eq!(footprint.pads[0].net.as_deref(), Some("GND"));
        assert_eq!(footprint.pads[1].net, None);
        assert_eq!(board.footprint(footprint.uuid).as_ref(), Some(footprint));
        assert_eq!(board.footprint(Uuid::nil()), None);
    }

    #[test]
    fn only_edge_cuts_graphics_form_the_outline() {
        let outline = legacy().edge_cuts();
        assert_eq!(outline.len(), 1);
        assert_eq!(
            outline[0].shape,
            OutlineShape::Rect {
                start: Point::new(Nm(100_000_000), Nm(60_000_000)),
                end: Point::new(Nm(150_000_000), Nm(100_000_000)),
            }
        );
        assert_eq!(outline[0].width, Nm(50_000));
        assert!(outline[0].uuid.is_some());
    }

    #[test]
    fn every_outline_shape_is_read() {
        let source = r#"(kicad_pcb (version 20260206)
            (gr_line (start 0 0) (end 10 0) (layer "Edge.Cuts"))
            (gr_arc (start 10 0) (mid 12 2) (end 10 4) (stroke (width 0.1)) (layer "Edge.Cuts"))
            (gr_circle (center 5 5) (end 6 5) (layer "Edge.Cuts"))
            (gr_poly (pts (xy 0 0) (xy 1 0) (xy 1 1)) (layer "Edge.Cuts"))
            (gr_arc (start 0 0) (end 1 1) (angle 90) (layer "Edge.Cuts")))"#;
        let outline = Board::parse(source).unwrap().edge_cuts();
        assert_eq!(outline.len(), 4);
        assert!(matches!(outline[0].shape, OutlineShape::Line { .. }));
        assert!(matches!(outline[1].shape, OutlineShape::Arc { .. }));
        assert_eq!(outline[1].width, Nm(100_000));
        assert!(matches!(outline[2].shape, OutlineShape::Circle { .. }));
        assert_eq!(outline[3].shape.points().len(), 3);
    }

    #[test]
    fn a_new_board_has_kicad_layers_and_a_default_stackup() {
        let board = Board::new_empty();
        let layers = board.layers();
        assert_eq!(board.copper_layer_count(), 2);
        assert_eq!(layers[0].name, "F.Cu");
        assert_eq!((layers[1].ordinal, layers[1].name.as_str()), (2, "B.Cu"));
        let edge = layers
            .iter()
            .find(|layer| layer.name == "Edge.Cuts")
            .unwrap();
        assert_eq!((edge.ordinal, edge.kind), (25, LayerKind::User));

        let stackup = board.stackup().unwrap();
        assert_eq!(stackup.thickness(), Nm(BOARD_THICKNESS));
        let dielectric = stackup.layers.iter().find(|l| l.is_dielectric()).unwrap();
        assert_eq!(dielectric.kind, "core");
        assert_eq!(dielectric.thickness, Some(Nm(1_510_000)));
        assert_eq!(dielectric.material.as_deref(), Some("FR4"));
        assert_eq!(dielectric.epsilon_r, Some(4.5));
        assert_eq!(dielectric.loss_tangent, Some(0.02));
        assert_eq!(stackup.copper_finish.as_deref(), Some("None"));
    }

    #[test]
    fn default_stackups_alternate_prepreg_and_core() {
        let board = |copper: usize| Board {
            root: SExpr::node(
                "kicad_pcb",
                vec![SExpr::node("setup", vec![default_stackup(copper)])],
            ),
        };
        let four = board(4).stackup().unwrap();
        let kinds: Vec<_> = four
            .layers
            .iter()
            .filter(|layer| layer.is_dielectric())
            .map(|layer| (layer.kind.as_str(), layer.thickness))
            .collect();
        let thickness = Some(Nm(480_000));
        assert_eq!(
            kinds,
            [
                ("prepreg", thickness),
                ("core", thickness),
                ("prepreg", thickness)
            ]
        );
        assert_eq!(four.thickness(), Nm(BOARD_THICKNESS));
        let eight = board(8).stackup().unwrap();
        let copper = eight.layers.iter().filter(|l| l.kind == "copper");
        assert_eq!(copper.count(), 8);
        assert_eq!(eight.layers[4].thickness, Some(Nm(185_714)));
        assert_eq!(eight.layers[4].name, "dielectric 1");
    }

    #[test]
    fn kicad_10_layer_ids_follow_the_new_numbering() {
        assert_eq!(layer_ordinal("F.Cu"), Some(0));
        assert_eq!(layer_ordinal("B.Cu"), Some(2));
        assert_eq!(layer_ordinal("In1.Cu"), Some(4));
        assert_eq!(layer_ordinal("In30.Cu"), Some(62));
        assert_eq!(layer_ordinal("Edge.Cuts"), Some(25));
        assert_eq!(layer_ordinal("User.1"), Some(39));
        assert_eq!(layer_ordinal("User.45"), Some(127));
        assert_eq!(layer_ordinal("In31.Cu"), None);
        assert_eq!(layer_ordinal("Custom"), None);
    }

    #[test]
    fn coordinates_outside_kicads_range_are_refused() {
        assert_eq!(legacy().check_export_bounds(), Ok(()));
        let far = LEGACY.replace("(at 120 80)", "(at 2200 80)");
        let error = Board::parse(&far)
            .unwrap()
            .check_export_bounds()
            .unwrap_err();
        assert!(
            error.starts_with("Footprint R1 at (2200, 80) mm"),
            "{error}"
        );
        let outline = LEGACY.replace("(end 150 100)", "(end 150 -2147.5)");
        let error = Board::parse(&outline)
            .unwrap()
            .check_export_bounds()
            .unwrap_err();
        assert!(error.starts_with("The board outline"), "{error}");
    }
}
