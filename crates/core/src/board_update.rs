//! Update Board from Schematic.
//!
//! Schematic symbols are matched to board footprints by the symbol UUID at the end of each
//! footprint's `(path ...)`. The plan lists every footprint to add, remove, replace, rename,
//! or rewire, and every symbol that can't be placed. Applying the plan is one undoable
//! step. Footprint definitions come from the component library through the caller, keyed
//! by `<library>:<footprint>`.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use uuid::Uuid;

use crate::board::Board;
use crate::footprint::{Footprint, Side, board_at_node, flip_node, footprint_uuid, rotate_node};
use crate::kicad::{PlacedSymbol, Schematic};
use crate::kicad_value::{child_text, children, length, point, points, text_arg};
use crate::netlist::netlist;
use crate::sexpr::SExpr;
use crate::units::{Angle, Nm, Point};

pub const UPDATE_BOARD_LABEL: &str = "Update Board from Schematic";
/// New footprints wrap to a new row past this width.
const ROW_WIDTH: Nm = Nm(150_000_000);
/// The space between new footprints, and between their rows.
const PART_GAP: Nm = Nm(2_540_000);
/// How far below the board outline, or the origin, new footprints start.
const OUTLINE_GAP: Nm = Nm(10_000_000);

/// The choices of KiCad's Update PCB from Schematic dialog Tron supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardUpdateOptions {
    /// Delete footprints with no symbols.
    pub delete_extra: bool,
    /// Replace footprints with those specified by symbols.
    pub replace_footprints: bool,
}

impl Default for BoardUpdateOptions {
    /// Both options on, as KiCad's dialog starts.
    fn default() -> BoardUpdateOptions {
        BoardUpdateOptions {
            delete_extra: true,
            replace_footprints: true,
        }
    }
}

/// A footprint the update adds or removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFootprint {
    /// The board footprint's UUID; `None` for one the update adds.
    pub uuid: Option<Uuid>,
    pub reference: String,
    pub value: String,
    pub lib_id: String,
}

/// A footprint swapped for the one its symbol now names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootprintReplacement {
    pub uuid: Uuid,
    pub reference: String,
    pub from: String,
    pub to: String,
}

/// The footprint text an update can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FootprintField {
    Reference,
    Value,
}

/// A footprint's Reference or Value changing to match its symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub uuid: Uuid,
    /// The reference the footprint has after the update.
    pub reference: String,
    pub field: FootprintField,
    pub from: String,
    pub to: String,
}

/// A pad joining, leaving, or switching nets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadNetChange {
    pub uuid: Uuid,
    pub reference: String,
    pub pad: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// A symbol the update can't carry to the board, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardUpdateIssue {
    pub reference: String,
    pub message: String,
}

/// Everything an update would change, for the user to review before applying it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoardUpdatePlan {
    /// In reference order, which is also the order they are packed in.
    pub additions: Vec<PlannedFootprint>,
    pub removals: Vec<PlannedFootprint>,
    pub replacements: Vec<FootprintReplacement>,
    pub field_changes: Vec<FieldChange>,
    pub net_changes: Vec<PadNetChange>,
    pub issues: Vec<BoardUpdateIssue>,
}

impl BoardUpdatePlan {
    /// Whether applying the plan would change the board.
    pub fn has_changes(&self) -> bool {
        !(self.additions.is_empty()
            && self.removals.is_empty()
            && self.replacements.is_empty()
            && self.field_changes.is_empty()
            && self.net_changes.is_empty())
    }
}

/// One board part: the symbols of every unit sharing a reference.
#[derive(Debug, Clone)]
struct Part {
    /// Symbol UUIDs in unit order; the first is the one a new footprint's path names.
    symbols: Vec<Uuid>,
    reference: String,
    value: String,
    lib_id: String,
}

/// What applying the plan does to one footprint.
#[derive(Debug, Clone)]
enum Action {
    Add(Part),
    Update {
        uuid: Uuid,
        part: Part,
        replace: bool,
    },
    Remove(Uuid),
}

/// A plan with the actions that carry it out.
struct Analysis {
    plan: BoardUpdatePlan,
    actions: Vec<Action>,
    /// `(reference, pin number)` to net name.
    nets: HashMap<(String, String), String>,
}

/// Top-level nodes paired with their child indices.
pub(crate) type NodeEdits = Vec<(usize, SExpr)>;

/// The `<library>:<footprint>` IDs the schematic's symbols name, for the caller to look up
/// in the component library.
pub fn required_footprints(schematic: &Schematic) -> BTreeSet<String> {
    schematic
        .symbols()
        .iter()
        .filter(|symbol| !symbol.reference().starts_with('#'))
        .filter_map(|symbol| symbol.property("Footprint"))
        .map(str::trim)
        .filter(|lib_id| lib_id.contains(':'))
        .map(str::to_owned)
        .collect()
}

/// Plans the update of `board` from `schematic`. `definitions` holds the board-ready
/// `(footprint ...)` definition of each ID [`required_footprints`] names that the library
/// has. Fails only when the schematic's connectivity can't be derived.
pub fn plan_board_update(
    schematic: &Schematic,
    board: &Board,
    definitions: &HashMap<String, SExpr>,
    options: BoardUpdateOptions,
) -> Result<BoardUpdatePlan, String> {
    analyze(schematic, board, definitions, options).map(|analysis| analysis.plan)
}

/// The board node edits that apply the update, recorded like [`plan_board_update`] plans
/// them. New footprints get UUIDs from `fresh`, and so does every item inside them.
pub(crate) fn board_edits(
    schematic: &Schematic,
    board: &Board,
    definitions: &HashMap<String, SExpr>,
    options: BoardUpdateOptions,
    sheet_file: &str,
    fresh: &mut dyn FnMut() -> Uuid,
) -> Result<(NodeEdits, NodeEdits), String> {
    let analysis = analyze(schematic, board, definitions, options)?;
    let builder = Builder {
        definitions,
        nets: &analysis.nets,
        sheet_file,
    };
    let mut packer = Packer::new(board);
    let mut added = Vec::new();
    let mut updated: HashMap<Uuid, SExpr> = HashMap::new();
    let mut deleted: HashSet<Uuid> = HashSet::new();
    for action in &analysis.actions {
        match action {
            Action::Add(part) => {
                let definition = builder.definition(part)?;
                let position = packer.place(extent(definition));
                let uuid = fresh();
                added.push(builder.new_node(part, uuid, position, fresh)?);
            }
            Action::Update {
                uuid,
                part,
                replace,
            } => {
                let Some(node) = board.footprint_node(*uuid) else {
                    continue;
                };
                let footprint = Footprint::read(node)
                    .ok_or_else(|| format!("the footprint {uuid} can't be read"))?;
                let next = if *replace {
                    builder.replacement(&footprint, part, fresh)?
                } else {
                    builder.updated(&footprint, part).to_node(node)
                };
                updated.insert(*uuid, next);
            }
            Action::Remove(uuid) => {
                deleted.insert(*uuid);
            }
        }
    }

    let insert_at = board.footprint_insert_index();
    let items = board.items();
    let mut removed = Vec::new();
    let mut inserted = Vec::new();
    let mut next = 1;
    let mut push_added = |inserted: &mut NodeEdits, next: &mut usize| {
        for node in added.drain(..) {
            inserted.push((*next, node));
            *next += 1;
        }
    };
    for (position, node) in items.iter().enumerate() {
        let index = position + 1;
        if index == insert_at {
            push_added(&mut inserted, &mut next);
        }
        let uuid = footprint_uuid(node);
        if uuid.is_some_and(|uuid| deleted.contains(&uuid)) {
            removed.push((index, node.clone()));
        } else if let Some(after) = uuid.and_then(|uuid| updated.remove(&uuid)) {
            removed.push((index, node.clone()));
            inserted.push((next, after));
            next += 1;
        } else {
            next += 1;
        }
    }
    push_added(&mut inserted, &mut next);
    Ok((removed, inserted))
}

/// Matches the schematic's parts to the board's footprints and works out every change.
fn analyze(
    schematic: &Schematic,
    board: &Board,
    definitions: &HashMap<String, SExpr>,
    options: BoardUpdateOptions,
) -> Result<Analysis, String> {
    let mut nets = HashMap::new();
    for net in netlist(schematic)? {
        for node in net.nodes {
            nets.insert((node.reference, node.pin), net.name.clone());
        }
    }
    let (parts, protected, issues) = parts(schematic);
    let mut plan = BoardUpdatePlan {
        issues,
        ..BoardUpdatePlan::default()
    };
    let mut actions = Vec::new();
    let footprints = board.footprints();
    let mut claimed: HashSet<Uuid> = HashSet::new();
    for part in parts {
        let matched = footprints.iter().find(|footprint| {
            !claimed.contains(&footprint.uuid)
                && footprint
                    .symbol_uuid()
                    .is_some_and(|symbol| part.symbols.contains(&symbol))
        });
        let Some(footprint) = matched else {
            if definitions.contains_key(&part.lib_id) {
                plan.additions.push(PlannedFootprint {
                    uuid: None,
                    reference: part.reference.clone(),
                    value: part.value.clone(),
                    lib_id: part.lib_id.clone(),
                });
                actions.push(Action::Add(part));
            } else {
                plan.issues.push(missing_definition(&part));
            }
            continue;
        };
        claimed.insert(footprint.uuid);
        let mut replace = options.replace_footprints && footprint.lib_id != part.lib_id;
        if replace && !definitions.contains_key(&part.lib_id) {
            plan.issues.push(missing_definition(&part));
            replace = false;
        }
        let before = plan.clone();
        if replace {
            plan.replacements.push(FootprintReplacement {
                uuid: footprint.uuid,
                reference: part.reference.clone(),
                from: footprint.lib_id.clone(),
                to: part.lib_id.clone(),
            });
        }
        let fields = [
            (
                FootprintField::Reference,
                &footprint.reference,
                &part.reference,
            ),
            (FootprintField::Value, &footprint.value, &part.value),
        ];
        for (field, from, to) in fields {
            if from != to {
                plan.field_changes.push(FieldChange {
                    uuid: footprint.uuid,
                    reference: part.reference.clone(),
                    field,
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }
        let pad_numbers = match definitions.get(&part.lib_id) {
            Some(definition) if replace => definition_pad_numbers(definition),
            _ => footprint
                .pads
                .iter()
                .map(|pad| pad.number.clone())
                .collect(),
        };
        let mut seen = HashSet::new();
        for number in pad_numbers {
            if !seen.insert(number.clone()) {
                continue;
            }
            let from = footprint
                .pads
                .iter()
                .find(|pad| pad.number == number)
                .and_then(|pad| pad.net.clone());
            let to = pad_net(&nets, &part.reference, &number);
            if from != to {
                plan.net_changes.push(PadNetChange {
                    uuid: footprint.uuid,
                    reference: part.reference.clone(),
                    pad: number,
                    from,
                    to,
                });
            }
        }
        if plan != before {
            actions.push(Action::Update {
                uuid: footprint.uuid,
                part,
                replace,
            });
        }
    }
    for footprint in &footprints {
        let kept = claimed.contains(&footprint.uuid)
            || footprint
                .symbol_uuid()
                .is_some_and(|symbol| protected.contains(&symbol));
        if kept || !options.delete_extra || is_board_only(board, footprint.uuid) {
            continue;
        }
        plan.removals.push(PlannedFootprint {
            uuid: Some(footprint.uuid),
            reference: footprint.reference.clone(),
            value: footprint.value.clone(),
            lib_id: footprint.lib_id.clone(),
        });
        actions.push(Action::Remove(footprint.uuid));
    }
    Ok(Analysis {
        plan,
        actions,
        nets,
    })
}

/// The schematic's board parts in reference order, the symbol UUIDs whose footprints must
/// stay because their symbol has a problem, and the problems.
fn parts(schematic: &Schematic) -> (Vec<Part>, HashSet<Uuid>, Vec<BoardUpdateIssue>) {
    let mut groups: Vec<(String, Vec<PlacedSymbol>)> = Vec::new();
    let mut protected = HashSet::new();
    let mut issues = Vec::new();
    for symbol in schematic.symbols() {
        let reference = symbol.reference().to_owned();
        if reference.starts_with('#') || not_on_board(schematic, symbol.uuid) {
            continue;
        }
        let lib_id = symbol.property("Footprint").unwrap_or_default().trim();
        if lib_id.is_empty() {
            protected.insert(symbol.uuid);
            continue;
        }
        if reference.ends_with('?') {
            protected.insert(symbol.uuid);
            issues.push(BoardUpdateIssue {
                message: format!("{reference} is not annotated. Annotate the schematic first."),
                reference,
            });
            continue;
        }
        match groups.iter_mut().find(|(name, _)| *name == reference) {
            Some((_, symbols)) => symbols.push(symbol),
            None => groups.push((reference, vec![symbol])),
        }
    }
    let mut parts = Vec::new();
    for (reference, mut symbols) in groups {
        symbols.sort_by_key(|symbol| symbol.unit);
        let lib_id = symbols[0].property("Footprint").unwrap_or_default().trim();
        let consistent = symbols.windows(2).all(|pair| {
            pair[0].unit != pair[1].unit
                && pair[0].lib_id == pair[1].lib_id
                && pair[1].property("Footprint").map(str::trim) == Some(lib_id)
        });
        let problem = if !consistent {
            Some(format!(
                "Several symbols share the reference {reference}. Annotate the schematic again."
            ))
        } else if !lib_id.contains(':') {
            Some(format!(
                "The footprint \"{lib_id}\" of {reference} doesn't name its library, as in \
                Resistor_SMD:R_0603_1608Metric."
            ))
        } else {
            None
        };
        if let Some(message) = problem {
            protected.extend(symbols.iter().map(|symbol| symbol.uuid));
            issues.push(BoardUpdateIssue { reference, message });
            continue;
        }
        parts.push(Part {
            symbols: symbols.iter().map(|symbol| symbol.uuid).collect(),
            value: symbols[0].property("Value").unwrap_or_default().to_owned(),
            lib_id: lib_id.to_owned(),
            reference,
        });
    }
    parts.sort_by(|a, b| reference_order(&a.reference, &b.reference));
    (parts, protected, issues)
}

/// The issue for a part whose footprint the library doesn't have.
fn missing_definition(part: &Part) -> BoardUpdateIssue {
    BoardUpdateIssue {
        reference: part.reference.clone(),
        message: format!(
            "The footprint {} of {} isn't in the component library.",
            part.lib_id, part.reference
        ),
    }
}

/// Whether the symbol with `uuid` is excluded from the board with `(on_board no)`.
fn not_on_board(schematic: &Schematic, uuid: Uuid) -> bool {
    schematic
        .item(uuid)
        .is_some_and(|node| child_text(node, "on_board") == Some("no"))
}

/// Whether the board footprint with `uuid` is marked `(attr ... board_only)`, so it never
/// needs a symbol.
fn is_board_only(board: &Board, uuid: Uuid) -> bool {
    let Some(attr) = board
        .footprint_node(uuid)
        .and_then(|node| node.child("attr"))
    else {
        return false;
    };
    attr.args()
        .iter()
        .any(|arg| arg.text() == Some("board_only"))
}

/// The net a pad joins: the net of the symbol pin with the same number.
fn pad_net(nets: &HashMap<(String, String), String>, reference: &str, pad: &str) -> Option<String> {
    if pad.is_empty() {
        return None;
    }
    nets.get(&(reference.to_owned(), pad.to_owned())).cloned()
}

/// The pad numbers of a footprint definition, in file order.
fn definition_pad_numbers(definition: &SExpr) -> Vec<String> {
    children(definition, "pad")
        .filter_map(|pad| text_arg(pad, 0))
        .map(str::to_owned)
        .collect()
}

/// Orders references the way people count them: `R2` before `R10`.
fn reference_order(a: &str, b: &str) -> Ordering {
    reference_key(a)
        .cmp(&reference_key(b))
        .then_with(|| a.cmp(b))
}

/// Splits a reference into its letters, its number, and anything after the number.
fn reference_key(reference: &str) -> (&str, u64, &str) {
    let letters = reference
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(reference.len());
    let (prefix, rest) = reference.split_at(letters);
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (number, suffix) = rest.split_at(digits);
    (prefix, number.parse().unwrap_or(0), suffix)
}

/// Builds footprint nodes from library definitions.
struct Builder<'a> {
    definitions: &'a HashMap<String, SExpr>,
    nets: &'a HashMap<(String, String), String>,
    sheet_file: &'a str,
}

impl Builder<'_> {
    /// The library definition of the part's footprint.
    fn definition(&self, part: &Part) -> Result<&SExpr, String> {
        self.definitions
            .get(&part.lib_id)
            .ok_or_else(|| format!("the footprint {} has no definition", part.lib_id))
    }

    /// A new front-side footprint at `position`, unrotated, linked to the part's first
    /// symbol, with its pads on the part's nets.
    fn new_node(
        &self,
        part: &Part,
        uuid: Uuid,
        position: Point,
        fresh: &mut dyn FnMut() -> Uuid,
    ) -> Result<SExpr, String> {
        let definition = self.definition(part)?;
        if definition.head() != Some("footprint") {
            return Err(format!(
                "the definition of {} is not a footprint",
                part.lib_id
            ));
        }
        let mut items = vec![
            SExpr::atom("footprint"),
            SExpr::string(part.lib_id.as_str()),
            SExpr::node("layer", vec![SExpr::string(Side::Front.copper_layer())]),
            SExpr::node("uuid", vec![SExpr::string(uuid.to_string())]),
            board_at_node(position, Angle::ZERO),
        ];
        for child in definition.args().iter().skip(1) {
            if matches!(child.head(), Some("layer" | "uuid" | "at")) {
                continue;
            }
            let mut child = child.clone();
            refresh_uuids(&mut child, fresh);
            items.push(child);
        }
        let property_end = items
            .iter()
            .rposition(|item| item.head() == Some("property"))
            .map_or(items.len(), |index| index + 1);
        let link = [
            SExpr::node("path", vec![SExpr::string(format!("/{}", part.symbols[0]))]),
            SExpr::node("sheetname", vec![SExpr::string("/")]),
            SExpr::node("sheetfile", vec![SExpr::string(self.sheet_file)]),
        ];
        items.splice(property_end..property_end, link);
        let node = SExpr::List(items);
        let footprint = Footprint::read(&node)
            .ok_or_else(|| format!("the definition of {} can't be placed", part.lib_id))?;
        Ok(self.updated(&footprint, part).to_node(&node))
    }

    /// The part's new footprint in place of `old`: same UUID, position, rotation, and side.
    fn replacement(
        &self,
        old: &Footprint,
        part: &Part,
        fresh: &mut dyn FnMut() -> Uuid,
    ) -> Result<SExpr, String> {
        let node = self.new_node(part, old.uuid, old.position, fresh)?;
        Ok(match old.side {
            Side::Front => rotate_node(&node, old.rotation),
            Side::Back => {
                let unturned = Angle::from_decidegrees(-old.rotation.decidegrees());
                flip_node(&rotate_node(&node, unturned))
            }
        })
    }

    /// `footprint` with the part's reference, value, and pad nets.
    fn updated(&self, footprint: &Footprint, part: &Part) -> Footprint {
        let mut next = footprint.clone();
        next.reference = part.reference.clone();
        next.value = part.value.clone();
        for pad in &mut next.pads {
            pad.net = pad_net(self.nets, &part.reference, &pad.number);
        }
        next
    }
}

/// Gives every `(uuid ...)` inside `node` a fresh UUID, so copies of one definition never
/// share item UUIDs.
fn refresh_uuids(node: &mut SExpr, fresh: &mut dyn FnMut() -> Uuid) {
    for child in node.items_mut().into_iter().flatten().skip(1) {
        if child.head() == Some("uuid") {
            *child = SExpr::node("uuid", vec![SExpr::string(fresh().to_string())]);
        } else {
            refresh_uuids(child, fresh);
        }
    }
}

/// The bounding box of a footprint definition around its origin: its courtyard, else its
/// pads, else a 1 mm square.
fn extent(definition: &SExpr) -> (Point, Point) {
    let mut courtyard = Bounds::default();
    let mut pads = Bounds::default();
    for child in definition.args() {
        let layer = child_text(child, "layer").unwrap_or_default();
        if child.head().is_some_and(|head| head.starts_with("fp_")) && layer.ends_with("CrtYd") {
            for head in ["start", "end", "mid"] {
                if let Some(at) = child.child(head).and_then(|node| point(node).ok()) {
                    courtyard.add(at);
                }
            }
            if let Ok(vertices) = points(child) {
                vertices.into_iter().for_each(|at| courtyard.add(at));
            }
            if let (Some(center), Some(end)) = (child.child("center"), child.child("end"))
                && let (Ok(center), Ok(end)) = (point(center), point(end))
            {
                let dx = (end.x.0 - center.x.0) as f64;
                let dy = (end.y.0 - center.y.0) as f64;
                let radius = Nm(dx.hypot(dy).round() as i64);
                courtyard.add(center.offset(-radius, -radius));
                courtyard.add(center.offset(radius, radius));
            }
        }
        if child.head() == Some("pad")
            && let Some(at) = child.child("at").and_then(|node| point(node).ok())
        {
            let size = child.child("size");
            let half = |index: usize| {
                size.and_then(|size| length(size, index).ok())
                    .map_or(Nm::ZERO, |side| Nm(side.0 / 2))
            };
            let reach = half(0).max(half(1));
            pads.add(at.offset(-reach, -reach));
            pads.add(at.offset(reach, reach));
        }
    }
    let fallback = Nm(500_000);
    courtyard.or(pads).unwrap_or((
        Point::new(-fallback, -fallback),
        Point::new(fallback, fallback),
    ))
}

/// A growing axis-aligned bounding box.
#[derive(Debug, Clone, Copy, Default)]
struct Bounds(Option<(Point, Point)>);

impl Bounds {
    /// Grows the box to hold `at`.
    fn add(&mut self, at: Point) {
        self.0 = Some(match self.0 {
            None => (at, at),
            Some((min, max)) => (
                Point::new(min.x.min(at.x), min.y.min(at.y)),
                Point::new(max.x.max(at.x), max.y.max(at.y)),
            ),
        });
    }

    /// This box, or `other` when this one is empty.
    fn or(self, other: Bounds) -> Option<(Point, Point)> {
        self.0.or(other.0)
    }
}

/// Packs new footprints in rows below the board outline.
struct Packer {
    left: Nm,
    x: Nm,
    top: Nm,
    row_height: Nm,
}

impl Packer {
    /// Starts at the lower left of the outline's bounding box, or below the origin when the
    /// board has no outline.
    fn new(board: &Board) -> Packer {
        let mut outline = Bounds::default();
        for graphic in board.edge_cuts() {
            graphic
                .shape
                .points()
                .into_iter()
                .for_each(|at| outline.add(at));
        }
        let (left, bottom) = match outline.0 {
            Some((min, max)) => (min.x, max.y),
            None => (Nm::ZERO, Nm::ZERO),
        };
        Packer {
            left,
            x: left,
            top: bottom + OUTLINE_GAP,
            row_height: Nm::ZERO,
        }
    }

    /// The origin for a footprint with bounding box `extent`, placed at the end of the
    /// current row, or at the start of a new one once the row is full.
    fn place(&mut self, extent: (Point, Point)) -> Point {
        let (min, max) = extent;
        let width = max.x - min.x;
        let height = max.y - min.y;
        if self.x > self.left && self.x - self.left + width > ROW_WIDTH {
            self.top = self.top + self.row_height + PART_GAP;
            self.x = self.left;
            self.row_height = Nm::ZERO;
        }
        let origin = Point::new(self.x - min.x, self.top - min.y);
        self.x = self.x + width + PART_GAP;
        self.row_height = self.row_height.max(height);
        origin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_sort_by_number() {
        let mut references = vec!["R10", "C1", "R2", "R1", "U1B", "U1A"];
        references.sort_by(|a, b| reference_order(a, b));
        assert_eq!(references, ["C1", "R1", "R2", "R10", "U1A", "U1B"]);
    }

    #[test]
    fn new_footprints_pack_in_rows_below_the_outline() {
        let board = Board::parse(
            "(kicad_pcb (version 20260206) \
            (gr_rect (start 100 60) (end 150 100) (layer \"Edge.Cuts\")))",
        )
        .unwrap();
        let mut packer = Packer::new(&board);
        let half = Nm(20_000_000);
        let square = (Point::new(-half, -half), Point::new(half, half));
        let first = packer.place(square);
        assert_eq!(first, Point::new(Nm(120_000_000), Nm(130_000_000)));
        assert_eq!(packer.place(square).y, Nm(130_000_000));
        assert_eq!(packer.place(square).y, Nm(130_000_000));
        let wrapped = packer.place(square);
        assert_eq!(wrapped, Point::new(Nm(120_000_000), Nm(172_540_000)));
    }

    #[test]
    fn extents_prefer_the_courtyard() {
        let definition = crate::sexpr::parse(
            "(footprint \"R\" (fp_rect (start -1.48 -0.73) (end 1.48 0.73) (layer \"F.CrtYd\")) \
            (pad \"1\" smd roundrect (at -0.825 0) (size 0.8 0.95) (layers \"F.Cu\")))",
        )
        .unwrap();
        assert_eq!(
            extent(&definition),
            (
                Point::new(Nm(-1_480_000), Nm(-730_000)),
                Point::new(Nm(1_480_000), Nm(730_000))
            )
        );
        let pads_only =
            crate::sexpr::parse("(footprint \"R\" (pad \"1\" smd rect (at 1 0) (size 0.8 0.4)))")
                .unwrap();
        assert_eq!(
            extent(&pads_only),
            (
                Point::new(Nm(600_000), Nm(-400_000)),
                Point::new(Nm(1_400_000), Nm(400_000))
            )
        );
    }
}
