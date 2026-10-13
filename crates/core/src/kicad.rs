//! KiCad schematic and board files.
//!
//! Each file is held as its full S-expression tree. Typed accessors read and
//! replace the parts Tron edits, and every other node passes through untouched.

use crate::kicad_value::{angle, child_text, children, flag, point, text_arg};
use crate::sexpr::{self, ParseError, SExpr};
use crate::symbol::{SymbolPreview, Xy};
use crate::units::{Angle, Nm, Point};
use crate::wiring::{Junction, Label, Sheet, Wire, node_uuid};
use uuid::Uuid;

pub const SCHEMATIC_FORMAT_VERSION: &str = "20231120";
pub const BOARD_FORMAT_VERSION: &str = "20240108";
pub const GENERATOR: &str = "tron";
pub const GENERATOR_VERSION: &str = "0.1";
pub const TITLE_BLOCK_COMMENTS: usize = 9;

fn is_title_block_field(node: &SExpr) -> bool {
    matches!(
        node.head(),
        Some("title" | "date" | "rev" | "company" | "comment")
    )
}

fn is_sheet_header(node: &SExpr) -> bool {
    matches!(
        node.head(),
        Some("version" | "generator" | "generator_version" | "uuid" | "paper")
    )
}

/// Why a file could not be read as the expected KiCad document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    Syntax(ParseError),
    WrongKind { expected: &'static str },
}

/// The title block printed on the schematic sheet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TitleBlock {
    pub title: String,
    pub date: String,
    pub revision: String,
    pub company: String,
    /// `comments[0]` is KiCad's `comment 1`.
    pub comments: [String; TITLE_BLOCK_COMMENTS],
}

impl TitleBlock {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
            && self.date.is_empty()
            && self.revision.is_empty()
            && self.company.is_empty()
            && self.comments.iter().all(String::is_empty)
    }

    fn read(node: &SExpr) -> TitleBlock {
        let mut block = TitleBlock::default();
        for child in node.args() {
            let first_text = || {
                child
                    .args()
                    .first()
                    .and_then(SExpr::text)
                    .unwrap_or_default()
                    .to_owned()
            };
            match child.head() {
                Some("title") => block.title = first_text(),
                Some("date") => block.date = first_text(),
                Some("rev") => block.revision = first_text(),
                Some("company") => block.company = first_text(),
                Some("comment") => {
                    let number = child
                        .args()
                        .first()
                        .and_then(SExpr::text)
                        .and_then(|n| n.parse::<usize>().ok());
                    let text = child.args().get(1).and_then(SExpr::text);
                    if let (Some(number), Some(text)) = (number, text)
                        && (1..=TITLE_BLOCK_COMMENTS).contains(&number)
                    {
                        block.comments[number - 1] = text.to_owned();
                    }
                }
                _ => {}
            }
        }
        block
    }

    /// Builds the `title_block` node, keeping any children Tron doesn't edit.
    fn to_node(&self, previous: Option<&SExpr>) -> SExpr {
        let mut args = Vec::new();
        let fields = [
            ("title", &self.title),
            ("date", &self.date),
            ("rev", &self.revision),
            ("company", &self.company),
        ];
        for (head, value) in fields {
            if !value.is_empty() {
                args.push(SExpr::node(head, vec![SExpr::string(value.as_str())]));
            }
        }
        for (index, comment) in self.comments.iter().enumerate() {
            if !comment.is_empty() {
                args.push(SExpr::node(
                    "comment",
                    vec![
                        SExpr::atom((index + 1).to_string()),
                        SExpr::string(comment.as_str()),
                    ],
                ));
            }
        }
        if let Some(previous) = previous {
            args.extend(
                previous
                    .args()
                    .iter()
                    .filter(|child| !is_title_block_field(child))
                    .cloned(),
            );
        }
        SExpr::node("title_block", args)
    }
}

/// The drawing sheet size, in the schematic's coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSize {
    pub width: Nm,
    pub height: Nm,
}

impl PageSize {
    const fn from_micrometers(width: i64, height: i64) -> PageSize {
        PageSize {
            width: Nm(width * 1_000),
            height: Nm(height * 1_000),
        }
    }

    pub const A4: PageSize = PageSize::from_micrometers(297_000, 210_000);

    fn named(name: &str) -> Option<PageSize> {
        let size = match name {
            "A5" => PageSize::from_micrometers(210_000, 148_000),
            "A4" => PageSize::A4,
            "A3" => PageSize::from_micrometers(420_000, 297_000),
            "A2" => PageSize::from_micrometers(594_000, 420_000),
            "A1" => PageSize::from_micrometers(841_000, 594_000),
            "A0" => PageSize::from_micrometers(1_189_000, 841_000),
            "A" | "USLetter" => PageSize::from_micrometers(279_400, 215_900),
            "B" | "USLedger" => PageSize::from_micrometers(431_800, 279_400),
            "C" => PageSize::from_micrometers(558_800, 431_800),
            "D" => PageSize::from_micrometers(863_600, 558_800),
            "E" => PageSize::from_micrometers(1_117_600, 863_600),
            "USLegal" => PageSize::from_micrometers(355_600, 215_900),
            _ => return None,
        };
        Some(size)
    }

    /// Reads `(paper "A4")`, `(paper "A3" portrait)`, or `(paper "User" w h)`.
    /// A missing or unrecognized paper falls back to A4, as KiCad does.
    fn read(node: Option<&SExpr>) -> PageSize {
        let Some(node) = node else {
            return PageSize::A4;
        };
        let args = node.args();
        let name = args.first().and_then(SExpr::text).unwrap_or_default();
        let size = if name == "User" {
            let millimeters = |index: usize| {
                args.get(index)
                    .and_then(SExpr::text)
                    .and_then(|text| text.parse::<f64>().ok())
                    .and_then(|mm| Nm::from_mm(mm).ok())
                    .filter(|nm| nm.0 > 0)
            };
            match (millimeters(1), millimeters(2)) {
                (Some(width), Some(height)) => PageSize { width, height },
                _ => PageSize::A4,
            }
        } else {
            PageSize::named(name).unwrap_or(PageSize::A4)
        };
        let portrait = args.iter().any(|arg| arg.text() == Some("portrait"));
        if portrait && size.width > size.height {
            PageSize {
                width: size.height,
                height: size.width,
            }
        } else {
            size
        }
    }
}

fn parse_kind(source: &str, expected: &'static str) -> Result<SExpr, FileError> {
    let root = sexpr::parse(source).map_err(FileError::Syntax)?;
    if root.head() == Some(expected) {
        Ok(root)
    } else {
        Err(FileError::WrongKind { expected })
    }
}

fn header(kind: &str, version: &str) -> Vec<SExpr> {
    vec![
        SExpr::atom(kind),
        SExpr::node("version", vec![SExpr::atom(version)]),
        SExpr::node("generator", vec![SExpr::string(GENERATOR)]),
        SExpr::node("generator_version", vec![SExpr::string(GENERATOR_VERSION)]),
    ]
}

/// A `.kicad_sch` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schematic {
    root: SExpr,
}

impl Schematic {
    pub fn new_empty(uuid: Uuid) -> Schematic {
        let mut items = header("kicad_sch", SCHEMATIC_FORMAT_VERSION);
        items.extend([
            SExpr::node("uuid", vec![SExpr::string(uuid.to_string())]),
            SExpr::node("paper", vec![SExpr::string("A4")]),
            SExpr::node("lib_symbols", vec![]),
            SExpr::node(
                "sheet_instances",
                vec![SExpr::node(
                    "path",
                    vec![
                        SExpr::string("/"),
                        SExpr::node("page", vec![SExpr::string("1")]),
                    ],
                )],
            ),
        ]);
        Schematic {
            root: SExpr::List(items),
        }
    }

    pub fn parse(source: &str) -> Result<Schematic, FileError> {
        parse_kind(source, "kicad_sch").map(|root| Schematic { root })
    }

    pub fn to_file_string(&self) -> String {
        self.root.to_pretty_string()
    }

    pub fn title_block(&self) -> TitleBlock {
        self.root
            .child("title_block")
            .map(TitleBlock::read)
            .unwrap_or_default()
    }

    pub fn page_size(&self) -> PageSize {
        PageSize::read(self.root.child("paper"))
    }

    /// Replaces the title block in place, or inserts it after the sheet
    /// header when the file has none. An empty block removes the node.
    pub fn set_title_block(&mut self, block: &TitleBlock) {
        let Some(items) = self.root.items_mut() else {
            return;
        };
        let existing = items
            .iter()
            .position(|item| item.head() == Some("title_block"));
        let node = block.to_node(existing.map(|index| &items[index]));
        let keep_node = !node.args().is_empty();
        match (existing, keep_node) {
            (Some(index), true) => items[index] = node,
            (Some(index), false) => {
                items.remove(index);
            }
            (None, true) => {
                let after_header = items
                    .iter()
                    .rposition(is_sheet_header)
                    .map_or(1, |index| index + 1);
                items.insert(after_header, node);
            }
            (None, false) => {}
        }
    }

    /// Every placed symbol with a readable `lib_id`, `(at ...)`, and `(uuid ...)`, in file
    /// order. A symbol that fails to read stays in the file untouched.
    pub fn symbols(&self) -> Vec<PlacedSymbol> {
        self.root
            .args()
            .iter()
            .filter_map(PlacedSymbol::read)
            .collect()
    }

    /// The placed symbol with `uuid`, if present and readable.
    pub fn symbol(&self, uuid: Uuid) -> Option<PlacedSymbol> {
        self.root.args().iter().find_map(|item| {
            let symbol = PlacedSymbol::read(item)?;
            (symbol.uuid == uuid).then_some(symbol)
        })
    }

    /// The `(symbol "<lib_id>" ...)` embedded in `(lib_symbols)`, if present.
    pub fn lib_symbol(&self, lib_id: &str) -> Option<&SExpr> {
        let node = self.root.child("lib_symbols")?;
        children(node, "symbol").find(|item| text_arg(item, 0) == Some(lib_id))
    }

    /// The library IDs of every embedded definition, in file order.
    pub fn lib_symbol_ids(&self) -> Vec<&str> {
        let Some(node) = self.root.child("lib_symbols") else {
            return Vec::new();
        };
        children(node, "symbol")
            .filter_map(|item| text_arg(item, 0))
            .collect()
    }

    /// The preview geometry of an embedded definition, for drawing its placed instances.
    pub fn symbol_preview(&self, lib_id: &str) -> Option<Result<SymbolPreview, String>> {
        self.lib_symbol(lib_id).map(crate::symbol::symbol_preview)
    }

    /// Every wire with two readable points and a UUID, in file order.
    pub fn wires(&self) -> Vec<Wire> {
        self.root.args().iter().filter_map(Wire::read).collect()
    }

    /// Every junction with a readable position and UUID, in file order.
    pub fn junctions(&self) -> Vec<Junction> {
        self.root.args().iter().filter_map(Junction::read).collect()
    }

    /// Every local and global label with readable text, position, and UUID, in file order.
    pub fn labels(&self) -> Vec<Label> {
        self.root.args().iter().filter_map(Label::read).collect()
    }

    /// Every pin connection point of every placed symbol whose definition is embedded and
    /// readable, hidden pins included.
    pub fn pin_points(&self) -> Vec<Point> {
        let mut points = Vec::new();
        for symbol in self.symbols() {
            let Some(definition) = self.lib_symbol(&symbol.lib_id) else {
                continue;
            };
            if let Ok(pins) = symbol.pin_connections(definition) {
                points.extend(pins.into_iter().map(|pin| pin.position));
            }
        }
        points
    }

    /// The connectivity items the wiring planner works on.
    pub(crate) fn wiring_sheet(&self) -> Sheet {
        Sheet {
            wires: self.wires(),
            junctions: self.junctions(),
            labels: self.labels(),
            pins: self.pin_points(),
        }
    }

    /// The top-level items after the `kicad_sch` head. Item `i` sits at child index `i + 1`.
    pub(crate) fn items(&self) -> &[SExpr] {
        self.root.args()
    }

    /// The node of the top-level item with `uuid`.
    pub(crate) fn item(&self, uuid: Uuid) -> Option<&SExpr> {
        self.items().iter().find(|item| node_uuid(item) == Some(uuid))
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

    /// The root sheet's own `(uuid ...)`, which placed symbols reference in `(instances ...)`.
    pub fn sheet_uuid(&self) -> Option<Uuid> {
        child_text(&self.root, "uuid").and_then(|text| Uuid::parse_str(text).ok())
    }

    /// Builds the placed symbol and its file node for a place command. `definition` is the
    /// flattened `(symbol ...)` the library index returns for `placement.lib_id`: its
    /// `Reference`, `Value`, `Footprint`, and `Datasheet` properties are copied with their
    /// `(at ...)` offsets mirrored, rotated, and moved by the placement, and its pins get
    /// one `(pin ...)` entry each. The reference is left unannotated (`R?`).
    pub(crate) fn build_placement(
        &self,
        definition: &SExpr,
        placement: Placement<'_>,
        project_name: &str,
    ) -> Result<(PlacedSymbol, SExpr), String> {
        if definition.head() != Some("symbol") || text_arg(definition, 0).is_none() {
            return Err("the definition is not a (symbol \"name\") list".to_owned());
        }
        let sheet_uuid = self
            .sheet_uuid()
            .ok_or_else(|| "the schematic has no sheet (uuid) node".to_owned())?;
        let numbers = crate::symbol::definition_pin_numbers(definition);
        if placement.pin_uuids.len() != numbers.len() {
            return Err(format!(
                "the definition has {} pins but {} pin uuids were given",
                numbers.len(),
                placement.pin_uuids.len()
            ));
        }
        let quarter = placement.rotation_steps.rem_euclid(4);
        let rotation = Angle::from_decidegrees(quarter * 900);
        let mut properties = Vec::with_capacity(PLACED_PROPERTY_KEYS.len());
        let mut definition_properties = Vec::with_capacity(PLACED_PROPERTY_KEYS.len());
        for key in PLACED_PROPERTY_KEYS {
            let definition_node =
                children(definition, "property").find(|item| text_arg(item, 0) == Some(key));
            match definition_node {
                Some(node) => {
                    let mut property = SymbolProperty::read(node)
                        .ok_or_else(|| format!("the definition's {key} property is malformed"))?;
                    if key == "Reference" {
                        property.value.push('?');
                    }
                    properties.push(place_property(
                        property,
                        placement.position,
                        placement.mirror,
                        quarter,
                        rotation,
                    ));
                    definition_properties.push(Some(node));
                }
                None => {
                    properties.push(SymbolProperty {
                        key: key.to_owned(),
                        value: if key == "Reference" { "?" } else { "" }.to_owned(),
                        position: placement.position,
                        rotation: Angle::ZERO,
                        hidden: matches!(key, "Footprint" | "Datasheet"),
                    });
                    definition_properties.push(None);
                }
            }
        }
        let pins = numbers
            .into_iter()
            .zip(placement.pin_uuids.iter().copied())
            .map(|(number, uuid)| PlacedPin { number, uuid })
            .collect();
        let placed = PlacedSymbol {
            lib_id: placement.lib_id.to_owned(),
            position: placement.position,
            rotation,
            mirror: placement.mirror,
            unit: 1,
            uuid: placement.uuid,
            properties,
            pins,
        };
        let node = placed.new_node(definition_properties, project_name, sheet_uuid);
        Ok((placed, node))
    }

    /// The child index a new placed symbol is inserted at: before `(sheet_instances)` or
    /// `(embedded_fonts)`, or at the end when the file has neither.
    pub(crate) fn placed_insert_index(&self) -> usize {
        self.root
            .args()
            .iter()
            .position(|item| matches!(item.head(), Some("sheet_instances" | "embedded_fonts")))
            .map(|position| position + 1)
            .unwrap_or_else(|| self.root.args().len() + 1)
    }

    /// The child index and node of the placed symbol with `uuid`.
    pub(crate) fn placed_entry(&self, uuid: Uuid) -> Option<(usize, SExpr)> {
        self.root
            .args()
            .iter()
            .position(|item| placed_uuid(item) == Some(uuid))
            .map(|position| (position + 1, self.root.args()[position].clone()))
    }

    /// Inserts a placed symbol node at child `index`, clamped to the end.
    pub(crate) fn insert_placed_at(&mut self, index: usize, node: SExpr) {
        if let Some(items) = self.root.items_mut() {
            items.insert(index.min(items.len()), node);
        }
    }

    /// Removes the placed symbol with `uuid`, returning its child index and node.
    pub(crate) fn remove_placed(&mut self, uuid: Uuid) -> Option<(usize, SExpr)> {
        let items = self.root.items_mut()?;
        let index = items
            .iter()
            .position(|item| placed_uuid(item) == Some(uuid))?;
        Some((index, items.remove(index)))
    }

    /// Rebuilds the node of the placed symbol with `symbol.uuid` from the typed state,
    /// keeping the previous node's unknown children. No-op when the symbol is absent.
    pub(crate) fn replace_placed(&mut self, symbol: &PlacedSymbol) {
        let Some(items) = self.root.items_mut() else {
            return;
        };
        let Some(index) = items
            .iter()
            .position(|item| placed_uuid(item) == Some(symbol.uuid))
        else {
            return;
        };
        let node = symbol.to_node(&items[index]);
        items[index] = node;
    }

    /// The child index within `(lib_symbols)` where a definition named `lib_id` belongs,
    /// keeping the definitions sorted by name.
    pub(crate) fn definition_insert_index(&self, lib_id: &str) -> usize {
        let Some(node) = self.root.child("lib_symbols") else {
            return 1;
        };
        node.args()
            .iter()
            .position(|item| item.head() == Some("symbol") && text_arg(item, 0) > Some(lib_id))
            .map(|position| position + 1)
            .unwrap_or_else(|| node.args().len() + 1)
    }

    /// The child index within `(lib_symbols)` and the node of the definition named `lib_id`.
    pub(crate) fn definition_entry(&self, lib_id: &str) -> Option<(usize, SExpr)> {
        let node = self.root.child("lib_symbols")?;
        node.args()
            .iter()
            .position(|item| item.head() == Some("symbol") && text_arg(item, 0) == Some(lib_id))
            .map(|position| (position + 1, node.args()[position].clone()))
    }

    /// Inserts a definition node at child `index` within `(lib_symbols)`, creating the node
    /// after the sheet header when the file has none.
    pub(crate) fn insert_definition_at(&mut self, index: usize, node: SExpr) {
        let Some(lib_symbols) = self.ensure_lib_symbols() else {
            return;
        };
        let Some(items) = self.root.items_mut() else {
            return;
        };
        let Some(definitions) = items[lib_symbols].items_mut() else {
            return;
        };
        definitions.insert(index.min(definitions.len()), node);
    }

    /// Removes the embedded definition named `lib_id`, returning its child index within
    /// `(lib_symbols)` and its node.
    pub(crate) fn remove_definition(&mut self, lib_id: &str) -> Option<(usize, SExpr)> {
        let items = self.root.items_mut()?;
        let index = items
            .iter()
            .position(|item| item.head() == Some("lib_symbols"))?;
        let definitions = items[index].items_mut()?;
        let position = definitions
            .iter()
            .position(|item| item.head() == Some("symbol") && text_arg(item, 0) == Some(lib_id))?;
        Some((position, definitions.remove(position)))
    }

    /// Whether the file has a `(lib_symbols)` node.
    pub(crate) fn has_lib_symbols(&self) -> bool {
        self.root.child("lib_symbols").is_some()
    }

    /// Removes the `(lib_symbols)` node when it holds no definitions.
    pub(crate) fn remove_empty_lib_symbols(&mut self) {
        let Some(items) = self.root.items_mut() else {
            return;
        };
        let empty = items
            .iter()
            .any(|item| item.head() == Some("lib_symbols") && item.args().is_empty());
        if empty {
            items.retain(|item| item.head() != Some("lib_symbols"));
        }
    }

    /// The child index of the `(lib_symbols)` node, inserting an empty one after the sheet
    /// header when the file has none.
    fn ensure_lib_symbols(&mut self) -> Option<usize> {
        let items = self.root.items_mut()?;
        if let Some(index) = items
            .iter()
            .position(|item| item.head() == Some("lib_symbols"))
        {
            return Some(index);
        }
        let after_header = items
            .iter()
            .rposition(|item| is_sheet_header(item) || item.head() == Some("title_block"))
            .map_or(1, |index| index + 1);
        items.insert(after_header, SExpr::node("lib_symbols", vec![]));
        Some(after_header)
    }
}

/// The property keys copied from a definition into a placed symbol, in KiCad's order.
const PLACED_PROPERTY_KEYS: [&str; 4] = ["Reference", "Value", "Footprint", "Datasheet"];

/// How a new symbol is placed on the schematic: where it sits, how it is oriented, and the
/// fresh UUIDs it and its pins get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement<'a> {
    /// The `<lib>:<name>` of the symbol's embedded definition.
    pub lib_id: &'a str,
    /// The symbol origin in schematic coordinates.
    pub position: Point,
    /// Rotation in 90° steps counterclockwise on screen (the KiCad angle convention).
    pub rotation_steps: i32,
    /// A horizontal (`Mirror::X`) or vertical (`Mirror::Y`) flip, applied before rotation.
    pub mirror: Option<Mirror>,
    /// The placed symbol's UUID.
    pub uuid: Uuid,
    /// One fresh UUID per definition pin, in the order
    /// [`crate::symbol::definition_pin_numbers`] returns them.
    pub pin_uuids: &'a [Uuid],
}

/// The axis a placed symbol is mirrored across, from its `(mirror x|y)` node: `X` flips it
/// horizontally on screen, `Y` vertically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirror {
    X,
    Y,
}

impl Mirror {
    /// The token in the `(mirror ...)` node.
    fn as_str(self) -> &'static str {
        match self {
            Mirror::X => "x",
            Mirror::Y => "y",
        }
    }
}

/// One `(property ...)` of a placed symbol. Children Tron doesn't edit, such as
/// `(effects ...)`, live in the file node and pass through edits untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolProperty {
    pub key: String,
    pub value: String,
    pub position: Point,
    pub rotation: Angle,
    pub hidden: bool,
}

impl SymbolProperty {
    fn read(node: &SExpr) -> Option<SymbolProperty> {
        let key = node.args().first().and_then(SExpr::text)?;
        let value = node.args().get(1).and_then(SExpr::text).unwrap_or_default();
        let (position, rotation) = read_at(node).unwrap_or((Point::ORIGIN, Angle::ZERO));
        Some(SymbolProperty {
            key: key.to_owned(),
            value: value.to_owned(),
            position,
            rotation,
            hidden: flag(node, "hide"),
        })
    }

    /// Builds the property node with the typed key, value, and `(at ...)`, keeping every
    /// other child of `previous` (the definition's or the current file's node) as it is.
    fn to_node(&self, previous: Option<&SExpr>) -> SExpr {
        let mut items = vec![
            SExpr::atom("property"),
            SExpr::string(self.key.as_str()),
            SExpr::string(self.value.as_str()),
            at_node(self.position, self.rotation),
        ];
        match previous {
            Some(previous) => items.extend(
                previous
                    .args()
                    .iter()
                    .skip(2)
                    .filter(|child| child.head() != Some("at"))
                    .cloned(),
            ),
            None => {
                if self.hidden {
                    items.push(SExpr::node("hide", vec![SExpr::atom("yes")]));
                }
            }
        }
        SExpr::List(items)
    }
}

/// A pin of a placed symbol and the schematic point it connects at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinConnection {
    pub number: String,
    pub name: String,
    /// The KiCad electrical type, such as `passive` or `power_in`.
    pub electrical: String,
    pub hidden: bool,
    pub position: Point,
}

/// A placed symbol's pin UUID entry: `(pin "<number>" (uuid ...))`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedPin {
    pub number: String,
    pub uuid: Uuid,
}

/// A symbol placed on the schematic: one `(symbol ...)` item. The fields Tron edits are
/// typed; everything else in the node, such as `(in_bom ...)` or the `(instances ...)`
/// block, is preserved through edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedSymbol {
    pub lib_id: String,
    pub position: Point,
    pub rotation: Angle,
    pub mirror: Option<Mirror>,
    pub unit: u32,
    pub uuid: Uuid,
    pub properties: Vec<SymbolProperty>,
    pub pins: Vec<PlacedPin>,
}

impl PlacedSymbol {
    /// Reads a `(symbol ...)` node. Returns `None` when the node is not a placed symbol or
    /// its `lib_id`, `(at ...)`, or `(uuid ...)` is missing or malformed; such nodes are
    /// left untouched by every edit.
    pub(crate) fn read(node: &SExpr) -> Option<PlacedSymbol> {
        if node.head() != Some("symbol") {
            return None;
        }
        let lib_id = child_text(node, "lib_id")?.to_owned();
        let (position, rotation) = read_at(node)?;
        let uuid = placed_uuid(node)?;
        let mirror = match child_text(node, "mirror") {
            Some("x") => Some(Mirror::X),
            Some("y") => Some(Mirror::Y),
            _ => None,
        };
        let unit = child_text(node, "unit")
            .and_then(|text| text.parse().ok())
            .unwrap_or(1);
        let properties = children(node, "property")
            .filter_map(SymbolProperty::read)
            .collect();
        let pins = children(node, "pin")
            .filter_map(|pin| {
                let number = pin.args().first().and_then(SExpr::text)?.to_owned();
                let uuid = child_text(pin, "uuid").and_then(|text| Uuid::parse_str(text).ok())?;
                Some(PlacedPin { number, uuid })
            })
            .collect();
        Some(PlacedSymbol {
            lib_id,
            position,
            rotation,
            mirror,
            unit,
            uuid,
            properties,
            pins,
        })
    }

    /// The value of the property named `key`, if the symbol has one.
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|property| property.key == key)
            .map(|property| property.value.as_str())
    }

    /// Where each pin of the placed unit connects on the schematic, hidden pins included.
    /// `definition` is the symbol's embedded `(lib_symbols)` entry. Pin positions are
    /// mirrored, rotated, and moved the same way as the symbol's properties.
    pub fn pin_connections(&self, definition: &SExpr) -> Result<Vec<PinConnection>, String> {
        let quarter = self.rotation.decidegrees() / 900;
        let pins = crate::symbol::unit_pins(definition, self.unit)?;
        let mut connections = Vec::with_capacity(pins.len());
        for (pin, hidden) in pins {
            let Xy(x, y) = pin.connection;
            let (x, y) = match self.mirror {
                Some(Mirror::X) => (-x, y),
                Some(Mirror::Y) => (x, -y),
                None => (x, y),
            };
            let (x, y) = rotate_offset((x, y), quarter);
            connections.push(PinConnection {
                number: pin.number,
                name: pin.name,
                electrical: pin.electrical,
                hidden,
                position: self.position.offset(Nm(x), Nm(y)),
            });
        }
        Ok(connections)
    }

    /// The value of the `Reference` property, or the unannotated marker when there is none.
    pub fn reference(&self) -> &str {
        self.properties
            .iter()
            .find(|property| property.key == "Reference")
            .map(|property| property.value.as_str())
            .unwrap_or("?")
    }

    /// The symbol moved by `(dx, dy)`, properties included.
    pub fn moved_by(&self, dx: Nm, dy: Nm) -> PlacedSymbol {
        let mut next = self.clone();
        next.position = next.position.offset(dx, dy);
        for property in &mut next.properties {
            property.position = property.position.offset(dx, dy);
        }
        next
    }

    /// The symbol rotated about its own origin by `steps` quarter turns, counterclockwise
    /// on screen for positive steps (the KiCad angle convention). Properties rotate with it.
    pub fn rotated(&self, steps: i32) -> PlacedSymbol {
        let quarter = steps.rem_euclid(4);
        let rotation = Angle::from_decidegrees(quarter * 900);
        let mut next = self.clone();
        next.rotation = next.rotation.rotate(rotation);
        for property in &mut next.properties {
            let relative = (
                property.position.x.0 - self.position.x.0,
                property.position.y.0 - self.position.y.0,
            );
            let (x, y) = rotate_offset(relative, quarter);
            property.position = self.position.offset(Nm(x), Nm(y));
            property.rotation = property.rotation.rotate(rotation);
        }
        next
    }

    /// The symbol mirrored about its own origin. Properties mirror with it. Mirroring a
    /// symbol that is already mirrored across the other axis composes into a rotation, so
    /// the result always has at most one `(mirror ...)` node.
    pub fn mirrored(&self, axis: Mirror) -> PlacedSymbol {
        let negate = |angle: Angle| Angle::from_decidegrees(-angle.decidegrees());
        let mut next = self.clone();
        next.rotation = match (axis, self.mirror) {
            (Mirror::X, Some(Mirror::Y)) | (Mirror::Y, Some(Mirror::X)) => {
                Angle::from_decidegrees(1800 - self.rotation.decidegrees())
            }
            _ => negate(self.rotation),
        };
        next.mirror = match (axis, self.mirror) {
            (Mirror::X, None) => Some(Mirror::X),
            (Mirror::Y, None) => Some(Mirror::Y),
            (_, Some(_)) => None,
        };
        for property in &mut next.properties {
            let relative = (
                property.position.x.0 - self.position.x.0,
                property.position.y.0 - self.position.y.0,
            );
            let (x, y) = match axis {
                Mirror::X => (-relative.0, relative.1),
                Mirror::Y => (relative.0, -relative.1),
            };
            property.position = self.position.offset(Nm(x), Nm(y));
            property.rotation = match axis {
                Mirror::X => Angle::from_decidegrees(1800 - property.rotation.decidegrees()),
                Mirror::Y => negate(property.rotation),
            };
        }
        next
    }

    /// Builds the file node for a freshly placed symbol, reusing each definition property's
    /// remaining children. The `(instances ...)` block names the project and the root sheet.
    fn new_node(
        &self,
        definition_properties: Vec<Option<&SExpr>>,
        project_name: &str,
        sheet_uuid: Uuid,
    ) -> SExpr {
        let mut items = vec![
            SExpr::atom("symbol"),
            SExpr::node("lib_id", vec![SExpr::string(self.lib_id.as_str())]),
            at_node(self.position, self.rotation),
        ];
        if let Some(mirror) = self.mirror {
            items.push(SExpr::node("mirror", vec![SExpr::atom(mirror.as_str())]));
        }
        items.push(SExpr::node(
            "unit",
            vec![SExpr::atom(self.unit.to_string())],
        ));
        items.push(SExpr::node(
            "uuid",
            vec![SExpr::string(self.uuid.to_string())],
        ));
        for (property, previous) in self.properties.iter().zip(definition_properties) {
            items.push(property.to_node(previous));
        }
        for pin in &self.pins {
            items.push(pin_node(pin, None));
        }
        items.push(SExpr::node(
            "instances",
            vec![SExpr::node(
                "project",
                vec![
                    SExpr::string(project_name),
                    SExpr::node(
                        "path",
                        vec![
                            SExpr::string(format!("/{sheet_uuid}")),
                            SExpr::node("reference", vec![SExpr::string(self.reference())]),
                            SExpr::node("unit", vec![SExpr::atom(self.unit.to_string())]),
                        ],
                    ),
                ],
            )],
        ));
        SExpr::List(items)
    }

    /// Rebuilds the file node from the typed state, reusing `previous` so unknown children
    /// keep their text and order. Properties and pins merge with the previous node's
    /// children one by one, in file order.
    fn to_node(&self, previous: &SExpr) -> SExpr {
        let mut properties = self.properties.iter();
        let mut pins = self.pins.iter();
        let mut items = vec![SExpr::atom("symbol")];
        let mut at_index = None;
        let mut mirror_written = false;
        for child in previous.args() {
            match child.head() {
                Some("lib_id") => items.push(SExpr::node(
                    "lib_id",
                    vec![SExpr::string(self.lib_id.as_str())],
                )),
                Some("at") => {
                    at_index = Some(items.len());
                    items.push(at_node(self.position, self.rotation));
                }
                Some("mirror") => {
                    if let Some(mirror) = self.mirror {
                        items.push(SExpr::node("mirror", vec![SExpr::atom(mirror.as_str())]));
                        mirror_written = true;
                    }
                }
                Some("unit") => items.push(SExpr::node(
                    "unit",
                    vec![SExpr::atom(self.unit.to_string())],
                )),
                Some("uuid") => items.push(SExpr::node(
                    "uuid",
                    vec![SExpr::string(self.uuid.to_string())],
                )),
                Some("property") => match properties.next() {
                    Some(property) => items.push(property.to_node(Some(child))),
                    None => items.push(child.clone()),
                },
                Some("pin") => match pins.next() {
                    Some(pin) => items.push(pin_node(pin, Some(child))),
                    None => items.push(child.clone()),
                },
                _ => items.push(child.clone()),
            }
        }
        if let (Some(mirror), false) = (self.mirror, mirror_written) {
            let node = SExpr::node("mirror", vec![SExpr::atom(mirror.as_str())]);
            match at_index {
                Some(index) => items.insert(index + 1, node),
                None => items.push(node),
            }
        }
        for property in properties {
            items.push(property.to_node(None));
        }
        for pin in pins {
            items.push(pin_node(pin, None));
        }
        SExpr::List(items)
    }
}

/// Reads a node's child `(at x y [angle])`: the point in millimeters and the angle, which
/// defaults to no rotation when missing.
pub(crate) fn read_at(node: &SExpr) -> Option<(Point, Angle)> {
    let at = node.child("at")?;
    let position = point(at).ok()?;
    let rotation = if at.args().len() > 2 {
        angle(at, 2).ok()?
    } else {
        Angle::ZERO
    };
    Some((position, rotation))
}

/// Writes `(at x y angle)` with KiCad's millimeter and degree formatting.
pub(crate) fn at_node(position: Point, rotation: Angle) -> SExpr {
    SExpr::node(
        "at",
        vec![
            SExpr::atom(mm_text(position.x)),
            SExpr::atom(mm_text(position.y)),
            SExpr::atom(deg_text(rotation)),
        ],
    )
}

/// Writes `(pin "<number>" (uuid ...))`, keeping any other children of the previous node.
fn pin_node(pin: &PlacedPin, previous: Option<&SExpr>) -> SExpr {
    let mut items = vec![
        SExpr::atom("pin"),
        SExpr::string(pin.number.as_str()),
        SExpr::node("uuid", vec![SExpr::string(pin.uuid.to_string())]),
    ];
    if let Some(previous) = previous {
        items.extend(
            previous
                .args()
                .iter()
                .skip(1)
                .filter(|child| child.head() != Some("uuid"))
                .cloned(),
        );
    }
    SExpr::List(items)
}

/// The parsed `(uuid ...)` of a placed `(symbol ...)` node.
fn placed_uuid(node: &SExpr) -> Option<Uuid> {
    if node.head() != Some("symbol") {
        return None;
    }
    child_text(node, "uuid").and_then(|text| Uuid::parse_str(text).ok())
}

/// A definition property copied into a placement: its `(at ...)` flipped to the schematic's
/// Y-down space, mirrored, rotated by the placement, and moved to the symbol position.
fn place_property(
    definition: SymbolProperty,
    position: Point,
    mirror: Option<Mirror>,
    quarter: i32,
    rotation: Angle,
) -> SymbolProperty {
    let (x, y) = (definition.position.x.0, -definition.position.y.0);
    let (x, y) = match mirror {
        Some(Mirror::X) => (-x, y),
        Some(Mirror::Y) => (x, -y),
        None => (x, y),
    };
    let (x, y) = rotate_offset((x, y), quarter);
    let base = match mirror {
        Some(Mirror::X) => Angle::from_decidegrees(1800 - definition.rotation.decidegrees()),
        Some(Mirror::Y) => Angle::from_decidegrees(-definition.rotation.decidegrees()),
        None => definition.rotation,
    };
    SymbolProperty {
        position: position.offset(Nm(x), Nm(y)),
        rotation: base.rotate(rotation),
        ..definition
    }
}

/// Rotates a Y-down offset by `steps` quarter turns counterclockwise on screen, exactly.
fn rotate_offset(offset: (i64, i64), steps: i32) -> (i64, i64) {
    match steps.rem_euclid(4) {
        0 => offset,
        1 => (offset.1, -offset.0),
        2 => (-offset.0, -offset.1),
        _ => (-offset.1, offset.0),
    }
}

/// Formats a length in millimeters the way KiCad writes it: exact decimals, no trailing
/// zeros.
pub(crate) fn mm_text(value: Nm) -> String {
    if value.0 == 0 {
        return "0".to_owned();
    }
    let sign = if value.0 < 0 { "-" } else { "" };
    let absolute = value.0.unsigned_abs();
    let whole = absolute / 1_000_000;
    let fraction = absolute % 1_000_000;
    if fraction == 0 {
        return format!("{sign}{whole}");
    }
    let fraction = format!("{fraction:06}");
    format!("{sign}{whole}.{}", fraction.trim_end_matches('0'))
}

/// Formats an angle in degrees the way KiCad writes it.
fn deg_text(angle: Angle) -> String {
    let decidegrees = angle.decidegrees();
    let fraction = decidegrees % 10;
    if fraction == 0 {
        format!("{}", decidegrees / 10)
    } else {
        format!("{}.{}", decidegrees / 10, fraction)
    }
}

/// A `.kicad_pcb` file. Tron doesn't edit boards yet, so it is preserved as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    root: SExpr,
}

impl Board {
    pub fn new_empty() -> Board {
        let layer = |id: &str, name: &str, kind: &str, user_name: Option<&str>| {
            let mut items = vec![SExpr::atom(id), SExpr::string(name), SExpr::atom(kind)];
            if let Some(user_name) = user_name {
                items.push(SExpr::string(user_name));
            }
            SExpr::List(items)
        };
        let mut items = header("kicad_pcb", BOARD_FORMAT_VERSION);
        items.extend([
            SExpr::node(
                "general",
                vec![SExpr::node("thickness", vec![SExpr::atom("1.6")])],
            ),
            SExpr::node("paper", vec![SExpr::string("A4")]),
            SExpr::node(
                "layers",
                vec![
                    layer("0", "F.Cu", "signal", None),
                    layer("31", "B.Cu", "signal", None),
                    layer("36", "B.SilkS", "user", Some("B.Silkscreen")),
                    layer("37", "F.SilkS", "user", Some("F.Silkscreen")),
                    layer("38", "B.Mask", "user", None),
                    layer("39", "F.Mask", "user", None),
                    layer("44", "Edge.Cuts", "user", None),
                ],
            ),
            SExpr::node(
                "setup",
                vec![SExpr::node("pad_to_mask_clearance", vec![SExpr::atom("0")])],
            ),
            SExpr::node("net", vec![SExpr::atom("0"), SExpr::string("")]),
        ]);
        Board {
            root: SExpr::List(items),
        }
    }

    pub fn parse(source: &str) -> Result<Board, FileError> {
        parse_kind(source, "kicad_pcb").map(|root| Board { root })
    }

    pub fn to_file_string(&self) -> String {
        self.root.to_pretty_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_block() -> TitleBlock {
        let mut block = TitleBlock {
            title: "Audio \"Amp\"".to_owned(),
            date: "2026-10-12".to_owned(),
            revision: "B".to_owned(),
            company: "Bench Labs".to_owned(),
            ..TitleBlock::default()
        };
        block.comments[0] = "First".to_owned();
        block.comments[8] = "Ninth".to_owned();
        block
    }

    #[test]
    fn title_block_round_trips_through_the_schematic() {
        let mut schematic = Schematic::new_empty(Uuid::nil());
        schematic.set_title_block(&sample_block());
        let reparsed = Schematic::parse(&schematic.to_file_string()).unwrap();
        assert_eq!(reparsed.title_block(), sample_block());
        assert_eq!(reparsed, schematic);
    }

    #[test]
    fn title_block_is_inserted_after_the_paper_node() {
        let mut schematic = Schematic::new_empty(Uuid::nil());
        schematic.set_title_block(&sample_block());
        let heads: Vec<_> = schematic.root.args().iter().map(SExpr::head).collect();
        assert_eq!(
            heads,
            [
                Some("version"),
                Some("generator"),
                Some("generator_version"),
                Some("uuid"),
                Some("paper"),
                Some("title_block"),
                Some("lib_symbols"),
                Some("sheet_instances"),
            ]
        );
    }

    #[test]
    fn clearing_the_title_block_removes_the_node() {
        let mut schematic = Schematic::new_empty(Uuid::nil());
        schematic.set_title_block(&sample_block());
        schematic.set_title_block(&TitleBlock::default());
        assert_eq!(schematic, Schematic::new_empty(Uuid::nil()));
    }

    #[test]
    fn editing_the_title_block_keeps_unknown_children() {
        let source =
            "(kicad_sch (version 20231120) (title_block (title \"Old\") (vendor_note \"keep\")))";
        let mut schematic = Schematic::parse(source).unwrap();
        let block = TitleBlock {
            title: "New".to_owned(),
            ..TitleBlock::default()
        };
        schematic.set_title_block(&block);
        assert_eq!(
            schematic.to_file_string(),
            "(kicad_sch\n\t(version 20231120)\n\t(title_block\n\t\t(title \"New\")\n\t\t(vendor_note \"keep\")\n\t)\n)\n"
        );
    }

    #[test]
    fn paper_sizes_follow_kicad_names_and_orientation() {
        let size = |source: &str| Schematic::parse(source).unwrap().page_size();
        assert_eq!(size("(kicad_sch)"), PageSize::A4);
        assert_eq!(
            size("(kicad_sch (paper \"A3\"))"),
            PageSize::from_micrometers(420_000, 297_000)
        );
        assert_eq!(
            size("(kicad_sch (paper \"A4\" portrait))"),
            PageSize::from_micrometers(210_000, 297_000)
        );
        assert_eq!(
            size("(kicad_sch (paper \"User\" 100.5 80))"),
            PageSize::from_micrometers(100_500, 80_000)
        );
        assert_eq!(size("(kicad_sch (paper \"Napkin\"))"), PageSize::A4);
    }

    #[test]
    fn rejects_a_file_of_the_wrong_kind() {
        assert_eq!(
            Schematic::parse("(kicad_pcb)"),
            Err(FileError::WrongKind {
                expected: "kicad_sch",
            })
        );
        assert!(matches!(
            Board::parse("(kicad_pcb"),
            Err(FileError::Syntax(_))
        ));
    }

    const SHEET_UUID: &str = "5b1c2f0e-8a4d-4c47-9b1e-2f8a6d3c7e10";
    const SYMBOL_UUID: &str = "11111111-2222-4333-8444-555555555555";

    fn placed_source() -> String {
        format!(
            "(kicad_sch (version 20231120) (uuid \"{SHEET_UUID}\") (paper \"A4\") \
            (lib_symbols (symbol \"Device:R\" (property \"Reference\" \"R\" (at 2.032 0 90)))) \
            (symbol (lib_id \"Device:R\") (at 114.3 85.09 90) (mirror y) (unit 1) \
            (dnp no) (uuid \"{SYMBOL_UUID}\") \
            (property \"Reference\" \"R1\" (at 60.96 83.058 90) \
            (effects (font (size 1.27 1.27)))) \
            (pin \"1\" (uuid \"11111111-2222-4333-8444-555555555501\") (custom keep)) \
            (instances (project \"amp\" (path \"/{SHEET_UUID}\" (reference \"R1\") (unit 1)))) \
            (sheet_instances (path \"/\" (page \"1\")))))"
        )
    }

    fn placed_schematic() -> Schematic {
        Schematic::parse(&placed_source()).unwrap()
    }

    #[test]
    fn placed_symbols_read_their_typed_fields() {
        let schematic = placed_schematic();
        let symbols = schematic.symbols();
        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.lib_id, "Device:R");
        assert_eq!(symbol.position, Point::new(Nm(114_300_000), Nm(85_090_000)));
        assert_eq!(symbol.rotation, Angle::from_decidegrees(900));
        assert_eq!(symbol.mirror, Some(Mirror::Y));
        assert_eq!((symbol.unit, symbol.reference()), (1, "R1"));
        assert_eq!(symbol.pins.len(), 1);
        assert_eq!(symbol.pins[0].number, "1");

        let reference = &symbol.properties[0];
        assert_eq!(
            (reference.key.as_str(), reference.value.as_str()),
            ("Reference", "R1")
        );
        assert_eq!(
            reference.position,
            Point::new(Nm(60_960_000), Nm(83_058_000))
        );
        assert_eq!(reference.rotation, Angle::from_decidegrees(900));
        assert!(!reference.hidden);

        assert_eq!(
            schematic.symbol(Uuid::parse_str(SYMBOL_UUID).unwrap()),
            Some(symbol.clone())
        );
        assert_eq!(
            schematic.sheet_uuid(),
            Some(Uuid::parse_str(SHEET_UUID).unwrap())
        );
        assert_eq!(schematic.lib_symbol_ids(), ["Device:R"]);
    }

    #[test]
    fn symbols_without_a_uuid_are_skipped_but_preserved() {
        let source = placed_source().replace(
            "(uuid \"11111111-2222-4333-8444-555555555555\")",
            "(uuid \"not-a-uuid\")",
        );
        let schematic = Schematic::parse(&source).unwrap();
        assert!(schematic.symbols().is_empty());
        assert!(schematic.to_file_string().contains("not-a-uuid"));
    }

    #[test]
    fn editing_a_symbol_keeps_unknown_children_in_place() {
        let mut schematic = placed_schematic();
        let symbol = schematic.symbols()[0].clone();
        let moved = symbol.moved_by(Nm(1_270_000), Nm(-2_540_000));
        schematic.replace_placed(&moved);
        let written = schematic.to_file_string();
        assert!(written.contains("(at 115.57 82.55 90)"), "{written}");
        assert!(written.contains("(at 62.23 80.518 90)"), "{written}");
        assert!(written.contains("(mirror y)"), "{written}");
        assert!(written.contains("(dnp no)"), "{written}");
        assert!(written.contains("(custom keep)"), "{written}");
        assert!(written.contains("(instances"), "{written}");
        assert_eq!(schematic.symbols()[0].position, moved.position);
    }

    #[test]
    fn mirroring_back_drops_the_mirror_node() {
        let mut schematic = placed_schematic();
        let symbol = schematic.symbols()[0].clone();
        let unmirrored = symbol.mirrored(Mirror::Y);
        assert_eq!(unmirrored.mirror, None);
        assert_eq!(unmirrored.rotation, Angle::from_decidegrees(2700));
        schematic.replace_placed(&unmirrored);
        let written = schematic.to_file_string();
        assert!(!written.contains("(mirror"), "{written}");
        assert!(written.contains("(at 114.3 85.09 270)"), "{written}");
    }

    #[test]
    fn rotated_and_mirrored_symbols_transform_property_positions() {
        let origin = Point::new(Nm(100_000_000), Nm(50_000_000));
        let property = SymbolProperty {
            key: "Reference".to_owned(),
            value: "R1".to_owned(),
            position: origin.offset(Nm(2_540_000), Nm(-1_270_000)),
            rotation: Angle::from_decidegrees(900),
            hidden: false,
        };
        let symbol = PlacedSymbol {
            lib_id: "Device:R".to_owned(),
            position: origin,
            rotation: Angle::ZERO,
            mirror: None,
            unit: 1,
            uuid: Uuid::parse_str(SYMBOL_UUID).unwrap(),
            properties: vec![property],
            pins: vec![],
        };

        let rotated = symbol.rotated(1);
        assert_eq!(rotated.rotation, Angle::from_decidegrees(900));
        assert_eq!(
            rotated.properties[0].position,
            Point::new(Nm(98_730_000), Nm(47_460_000))
        );
        assert_eq!(
            rotated.properties[0].rotation,
            Angle::from_decidegrees(1800)
        );
        assert_eq!(symbol.rotated(4), symbol);
        assert_eq!(symbol.rotated(-3), symbol.rotated(1));

        let mirrored = symbol.mirrored(Mirror::X);
        assert_eq!(mirrored.mirror, Some(Mirror::X));
        assert_eq!(
            mirrored.properties[0].position,
            Point::new(Nm(97_460_000), Nm(48_730_000))
        );
        assert_eq!(
            mirrored.properties[0].rotation,
            Angle::from_decidegrees(900)
        );
        assert_eq!(mirrored.mirrored(Mirror::X), symbol);

        let cross = symbol.mirrored(Mirror::Y).mirrored(Mirror::X);
        assert_eq!(cross.mirror, None);
        assert_eq!(cross.rotation, Angle::from_decidegrees(1800));
        assert_eq!(
            cross.properties[0].position,
            Point::new(Nm(97_460_000), Nm(51_270_000))
        );
    }

    fn r_definition() -> SExpr {
        sexpr::parse(
            "(symbol \"Device:R\" \
            (property \"Reference\" \"R\" (at 2.032 0 90) (effects (font (size 1.27 1.27)))) \
            (property \"Value\" \"R\" (at 0 0 90)) \
            (property \"Footprint\" \"\" (at -1.778 0 90) (hide yes)) \
            (property \"Datasheet\" \"\" (at 0 0 0) (hide yes)) \
            (symbol \"R_0_1\" (rectangle (start -1.016 -2.54) (end 1.016 2.54))) \
            (symbol \"R_1_1\" \
            (pin passive line (at 0 3.81 270) (length 1.27) (name \"\") (number \"1\")) \
            (pin passive line (at 0 -3.81 90) (length 1.27) (name \"\") (number \"2\"))))",
        )
        .unwrap()
    }

    const PINS: [Uuid; 2] = [Uuid::from_u128(3), Uuid::from_u128(4)];

    fn placement(position: Point, steps: i32, mirror: Option<Mirror>) -> Placement<'static> {
        Placement {
            lib_id: "Device:R",
            position,
            rotation_steps: steps,
            mirror,
            uuid: Uuid::parse_str(SYMBOL_UUID).unwrap(),
            pin_uuids: &PINS,
        }
    }

    #[test]
    fn build_placement_copies_properties_with_transformed_positions() {
        let schematic = Schematic::new_empty(Uuid::parse_str(SHEET_UUID).unwrap());
        let position = Point::new(Nm(114_300_000), Nm(85_090_000));
        let (placed, node) = schematic
            .build_placement(&r_definition(), placement(position, 0, None), "amp")
            .unwrap();

        assert_eq!(placed.reference(), "R?");
        assert_eq!(placed.properties.len(), 4);
        let reference = &placed.properties[0];
        assert_eq!(
            reference.position,
            Point::new(Nm(116_332_000), Nm(85_090_000))
        );
        assert_eq!(reference.rotation, Angle::from_decidegrees(900));
        let footprint = &placed.properties[2];
        assert!(footprint.hidden);
        assert_eq!(
            footprint.position,
            Point::new(Nm(112_522_000), Nm(85_090_000))
        );
        assert_eq!(placed.pins.len(), 2);

        let text = node.to_pretty_string();
        assert!(text.contains("(lib_id \"Device:R\")"), "{text}");
        assert!(text.contains("(at 114.3 85.09 0)"), "{text}");
        assert!(text.contains("(property \"Reference\" \"R?\""), "{text}");
        assert!(text.contains("(effects"), "{text}");
        assert!(text.contains("(hide yes)"), "{text}");
        assert!(text.contains("(pin \"1\""), "{text}");
        assert!(text.contains("(instances"), "{text}");
        assert!(text.contains("(project \"amp\""), "{text}");
        assert!(text.contains(&format!("\"/{SHEET_UUID}\"")), "{text}");
        assert!(text.contains("(reference \"R?\")"), "{text}");
        assert!(!text.contains("(mirror"), "{text}");
    }

    #[test]
    fn build_placement_rotates_and_mirrors_the_definition_offsets() {
        let schematic = Schematic::new_empty(Uuid::parse_str(SHEET_UUID).unwrap());
        let position = Point::new(Nm(100_000_000), Nm(50_000_000));
        let (rotated, _) = schematic
            .build_placement(&r_definition(), placement(position, 1, None), "amp")
            .unwrap();
        assert_eq!(rotated.rotation, Angle::from_decidegrees(900));
        assert_eq!(
            rotated.properties[0].position,
            Point::new(Nm(100_000_000), Nm(47_968_000))
        );
        assert_eq!(
            rotated.properties[0].rotation,
            Angle::from_decidegrees(1800)
        );

        let (mirrored, node) = schematic
            .build_placement(
                &r_definition(),
                placement(position, 0, Some(Mirror::X)),
                "amp",
            )
            .unwrap();
        assert_eq!(mirrored.mirror, Some(Mirror::X));
        assert_eq!(
            mirrored.properties[0].position,
            Point::new(Nm(97_968_000), Nm(50_000_000))
        );
        assert_eq!(
            mirrored.properties[0].rotation,
            Angle::from_decidegrees(900)
        );
        assert!(node.to_pretty_string().contains("(mirror x)"));
    }

    #[test]
    fn build_placement_rejects_bad_input() {
        let schematic = Schematic::new_empty(Uuid::parse_str(SHEET_UUID).unwrap());
        let position = Point::ORIGIN;
        assert!(
            schematic
                .build_placement(
                    &sexpr::parse("(wire (pts (xy 0 0) (xy 1 1)))").unwrap(),
                    placement(position, 0, None),
                    "amp",
                )
                .is_err()
        );
        let short_pins = Placement {
            pin_uuids: &PINS[..1],
            ..placement(position, 0, None)
        };
        assert!(
            schematic
                .build_placement(&r_definition(), short_pins, "amp")
                .is_err()
        );
        let no_sheet_uuid = Schematic::parse("(kicad_sch (version 20231120))").unwrap();
        assert!(
            no_sheet_uuid
                .build_placement(&r_definition(), placement(position, 0, None), "amp")
                .is_err()
        );
    }

    #[test]
    fn definitions_insert_sorted_and_remove_by_name() {
        let mut schematic = Schematic::new_empty(Uuid::parse_str(SHEET_UUID).unwrap());
        let index = schematic.definition_insert_index("Device:R");
        schematic.insert_definition_at(index, r_definition());
        let led = sexpr::parse("(symbol \"Device:LED\" (property \"Reference\" \"D\" (at 0 0 0)))")
            .unwrap();
        let index = schematic.definition_insert_index("Device:LED");
        schematic.insert_definition_at(index, led);
        assert_eq!(schematic.lib_symbol_ids(), ["Device:LED", "Device:R"]);

        let (index, _) = schematic.definition_entry("Device:R").unwrap();
        assert_eq!(index, 2);
        let (_, node) = schematic.remove_definition("Device:R").unwrap();
        assert_eq!(text_arg(&node, 0), Some("Device:R"));
        assert_eq!(schematic.lib_symbol_ids(), ["Device:LED"]);
        assert!(schematic.remove_definition("Device:R").is_none());
    }

    #[test]
    fn millimeters_and_degrees_format_the_kicad_way() {
        assert_eq!(mm_text(Nm(0)), "0");
        assert_eq!(mm_text(Nm(1_000_000)), "1");
        assert_eq!(mm_text(Nm(1_270_000)), "1.27");
        assert_eq!(mm_text(Nm(118_110_000)), "118.11");
        assert_eq!(mm_text(Nm(-100_000)), "-0.1");
        assert_eq!(mm_text(Nm(152_400)), "0.1524");
        assert_eq!(deg_text(Angle::ZERO), "0");
        assert_eq!(deg_text(Angle::from_decidegrees(900)), "90");
        assert_eq!(deg_text(Angle::from_decidegrees(455)), "45.5");
        assert_eq!(deg_text(Angle::from_decidegrees(3599)), "359.9");
    }
}
