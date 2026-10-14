//! An open project with its command history.
//!
//! Every edit is an invertible [`Command`]. Undo applies the inverse of the
//! latest command and redo applies it again, so both restore exact states.

use std::collections::HashMap;
use std::path::Path;

use uuid::Uuid;

use crate::erc::{RuleSeverities, Violation};
use crate::error::CoreError;
use crate::kicad::{
    MANDATORY_FIELDS, Mirror, PlacedSymbol, Placement, Schematic, SymbolProperty, TitleBlock,
};
use crate::kicad_value::text_arg;
use crate::manifest::{MAX_GRID, MIN_GRID};
use crate::project::Project;
use crate::project_file::ProjectFile;
use crate::sexpr::SExpr;
use crate::symbol::unit_count;
use crate::units::{Angle, Nm, Point};
use crate::wiring::{
    self, ChangedItem, Label, LabelKind, NoConnect, Wire, WiringPlan, WiringRequest, node_uuid,
};

pub const MAX_FIELD_CHARS: usize = 256;
/// The most points one drawn wire can have.
pub const MAX_WIRE_POINTS: usize = 1024;
pub const PROPERTIES_LABEL: &str = "Project Properties";
pub const ANNOTATE_LABEL: &str = "Annotate Schematic";
pub const EDIT_FIELDS_LABEL: &str = "Edit Symbol Properties";
pub const ERC_SEVERITIES_LABEL: &str = "Edit ERC Severities";
/// The longest field value, such as a datasheet URL.
pub const MAX_FIELD_VALUE_CHARS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetTitleBlock {
        before: Box<TitleBlock>,
        after: Box<TitleBlock>,
    },
    SetGridSize {
        before: Nm,
        after: Nm,
    },
    /// Symbols were placed; each definition was embedded on first use. The indices are the
    /// child positions the nodes are inserted at, in ascending order.
    PlaceSymbols {
        symbols: Vec<(usize, SExpr)>,
        /// `(index, node)` within `(lib_symbols)`.
        definitions: Vec<(usize, SExpr)>,
        /// The placement created the `(lib_symbols)` node itself; undo removes it again.
        created_lib_symbols: bool,
    },
    /// Symbols were removed, with definitions that became unused, all at their original
    /// child positions so undo restores them exactly.
    DeleteSymbols {
        symbols: Vec<(usize, SExpr)>,
        /// `(index, node)` within `(lib_symbols)`.
        definitions: Vec<(usize, SExpr)>,
        /// The removed `(lib_symbols)` node had been created by the placement being undone.
        created_lib_symbols: bool,
    },
    /// A set of symbols moved by the same offset.
    MoveSymbols {
        uuids: Vec<Uuid>,
        dx: Nm,
        dy: Nm,
    },
    /// A set of symbols rotated or mirrored about their own origins.
    TransformSymbols {
        /// `(uuid, before, after)` per transformed symbol.
        symbols: Vec<(Uuid, PlacedSymbol, PlacedSymbol)>,
    },
    /// Top-level wiring nodes (wires, junctions, and labels) were replaced. `removed` holds
    /// the nodes taken out, at their child indices before the edit; `inserted` holds the
    /// nodes put in, at their child indices after it. Both are in ascending index order.
    EditItems {
        label: String,
        removed: Vec<(usize, SExpr)>,
        inserted: Vec<(usize, SExpr)>,
    },
    /// The `.kicad_pro` settings were replaced.
    SetKicadProject {
        before: Box<ProjectFile>,
        after: Box<ProjectFile>,
    },
    /// Commands applied in order as one undo step.
    Group {
        label: String,
        commands: Vec<Command>,
    },
}

/// The transform applied by [`Session::transform_symbols`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolTransform {
    /// Quarter turns counterclockwise on screen (the KiCad angle convention).
    Rotate(i32),
    /// Mirror about the symbol origin: `Mirror::X` flips horizontally, `Mirror::Y`
    /// vertically.
    Mirror(Mirror),
}

impl Command {
    pub fn label(&self) -> &str {
        match self {
            Command::SetTitleBlock { .. } => "Title Block",
            Command::SetGridSize { .. } => "Grid Size",
            Command::PlaceSymbols { .. } => "Place Symbol",
            Command::DeleteSymbols { .. } => "Delete Symbol",
            Command::MoveSymbols { .. } => "Move Symbol",
            Command::TransformSymbols { .. } => "Transform Symbol",
            Command::SetKicadProject { .. } => "Project Settings",
            Command::EditItems { label, .. } | Command::Group { label, .. } => label,
        }
    }

    pub fn inverse(&self) -> Command {
        match self {
            Command::SetTitleBlock { before, after } => Command::SetTitleBlock {
                before: after.clone(),
                after: before.clone(),
            },
            Command::SetGridSize { before, after } => Command::SetGridSize {
                before: *after,
                after: *before,
            },
            Command::PlaceSymbols {
                symbols,
                definitions,
                created_lib_symbols,
            } => Command::DeleteSymbols {
                symbols: symbols.clone(),
                definitions: definitions.clone(),
                created_lib_symbols: *created_lib_symbols,
            },
            Command::DeleteSymbols {
                symbols,
                definitions,
                created_lib_symbols,
            } => Command::PlaceSymbols {
                symbols: symbols.clone(),
                definitions: definitions.clone(),
                created_lib_symbols: *created_lib_symbols,
            },
            Command::MoveSymbols { uuids, dx, dy } => Command::MoveSymbols {
                uuids: uuids.clone(),
                dx: -*dx,
                dy: -*dy,
            },
            Command::TransformSymbols { symbols } => Command::TransformSymbols {
                symbols: symbols
                    .iter()
                    .map(|(uuid, before, after)| (*uuid, after.clone(), before.clone()))
                    .collect(),
            },
            Command::EditItems {
                label,
                removed,
                inserted,
            } => Command::EditItems {
                label: label.clone(),
                removed: inserted.clone(),
                inserted: removed.clone(),
            },
            Command::SetKicadProject { before, after } => Command::SetKicadProject {
                before: after.clone(),
                after: before.clone(),
            },
            Command::Group { label, commands } => Command::Group {
                label: label.clone(),
                commands: commands.iter().rev().map(Command::inverse).collect(),
            },
        }
    }

    fn apply(&self, project: &mut Project, changes: &mut Changes) {
        match self {
            Command::SetTitleBlock { after, .. } => {
                project.set_title_block(after);
                changes.title_block = true;
            }
            Command::SetGridSize { after, .. } => {
                project.set_grid_size(*after);
                changes.grid_size = true;
            }
            Command::PlaceSymbols {
                symbols,
                definitions,
                ..
            } => {
                // Symbols first: their indices were recorded before a missing
                // `(lib_symbols)` node was created, which shifts the root children.
                for (index, node) in symbols {
                    project
                        .schematic_mut()
                        .insert_placed_at(*index, node.clone());
                }
                for (index, node) in definitions {
                    project
                        .schematic_mut()
                        .insert_definition_at(*index, node.clone());
                }
                changes.symbols = !symbols.is_empty();
                changes.definitions = !definitions.is_empty();
            }
            Command::DeleteSymbols {
                symbols,
                definitions,
                created_lib_symbols,
            } => {
                for (_, node) in symbols {
                    if let Some(symbol) = PlacedSymbol::read(node) {
                        project.schematic_mut().remove_placed(symbol.uuid);
                    }
                }
                for (_, node) in definitions {
                    if let Some(name) = text_arg(node, 0) {
                        project.schematic_mut().remove_definition(name);
                    }
                }
                if *created_lib_symbols {
                    project.schematic_mut().remove_empty_lib_symbols();
                }
                changes.symbols = !symbols.is_empty();
                changes.definitions = !definitions.is_empty();
            }
            Command::MoveSymbols { uuids, dx, dy } => {
                for uuid in uuids {
                    if let Some(symbol) = project.schematic().symbol(*uuid) {
                        let moved = symbol.moved_by(*dx, *dy);
                        project.schematic_mut().replace_placed(&moved);
                    }
                }
                changes.symbols = !uuids.is_empty();
            }
            Command::TransformSymbols { symbols } => {
                for (_, _, after) in symbols {
                    project.schematic_mut().replace_placed(after);
                }
                changes.symbols = !symbols.is_empty();
            }
            Command::EditItems {
                removed, inserted, ..
            } => {
                let schematic = project.schematic_mut();
                for (index, _) in removed.iter().rev() {
                    schematic.remove_item_at(*index);
                }
                for (index, node) in inserted {
                    schematic.insert_placed_at(*index, node.clone());
                }
                changes.wiring = true;
            }
            Command::SetKicadProject { after, .. } => {
                project.set_kicad_project((**after).clone());
                changes.erc_settings = true;
            }
            Command::Group { commands, .. } => {
                for command in commands {
                    command.apply(project, changes);
                }
            }
        }
    }
}

/// Which parts of the document a command touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    pub title_block: bool,
    pub grid_size: bool,
    /// Placed symbols were added, moved, transformed, or removed.
    pub symbols: bool,
    /// The embedded `(lib_symbols)` definitions changed.
    pub definitions: bool,
    /// Wires, junctions, or labels were added, moved, or removed.
    pub wiring: bool,
    /// The ERC rule severities in `.kicad_pro` changed.
    pub erc_settings: bool,
}

impl Changes {
    pub fn any(self) -> bool {
        self.title_block
            || self.grid_size
            || self.symbols
            || self.definitions
            || self.wiring
            || self.erc_settings
    }
}

#[derive(Debug, Clone)]
struct Entry {
    id: u64,
    command: Command,
}

#[derive(Debug, Clone)]
pub struct Session {
    project: Project,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    next_id: u64,
    /// The id on top of the undo stack when the project was last saved.
    saved_top: Option<u64>,
}

impl Session {
    pub fn new(project: Project) -> Session {
        Session {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            next_id: 1,
            saved_top: None,
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn is_dirty(&self) -> bool {
        self.undo.last().map(|entry| entry.id) != self.saved_top
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|entry| entry.command.label())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|entry| entry.command.label())
    }

    /// Sets the title block and grid as one undo step. Unchanged values are
    /// left out, and an edit that changes nothing records nothing.
    pub fn edit_properties(
        &mut self,
        title_block: TitleBlock,
        grid_size: Nm,
    ) -> Result<Changes, CoreError> {
        validate_title_block(&title_block)?;
        if !(MIN_GRID..=MAX_GRID).contains(&grid_size) {
            return Err(CoreError::InvalidEdit(format!(
                "The grid size must be between {} mm and {} mm.",
                MIN_GRID.to_mm(),
                MAX_GRID.to_mm()
            )));
        }
        let mut commands = Vec::new();
        let current_block = self.project.title_block();
        if current_block != title_block {
            commands.push(Command::SetTitleBlock {
                before: Box::new(current_block),
                after: Box::new(title_block),
            });
        }
        let current_grid = self.project.grid_size();
        if current_grid != grid_size {
            commands.push(Command::SetGridSize {
                before: current_grid,
                after: grid_size,
            });
        }
        if commands.is_empty() {
            return Ok(Changes::default());
        }
        Ok(self.execute(Command::Group {
            label: PROPERTIES_LABEL.to_owned(),
            commands,
        }))
    }

    /// Places a library symbol on the schematic as one undo step. `definition` is the
    /// flattened `(symbol ...)` the library index returns for `placement.lib_id`; it is
    /// embedded in `(lib_symbols)` on first use and removed again when the last instance
    /// using it is deleted. The reference is left unannotated (`R?`).
    pub fn place_symbol(
        &mut self,
        definition: &SExpr,
        placement: Placement<'_>,
    ) -> Result<Changes, CoreError> {
        let Placement { lib_id, uuid, .. } = placement;
        if lib_id.is_empty() {
            return Err(CoreError::InvalidEdit(
                "The library ID is empty.".to_owned(),
            ));
        }
        if self.project.schematic().symbol(uuid).is_some() {
            return Err(CoreError::InvalidEdit(
                "A symbol with this UUID is already placed.".to_owned(),
            ));
        }
        let project_name = self.project.name().to_owned();
        let (_, node) = self
            .project
            .schematic()
            .build_placement(definition, placement, &project_name)
            .map_err(CoreError::InvalidEdit)?;
        let index = self.project.schematic().placed_insert_index();
        let created_lib_symbols = !self.project.schematic().has_lib_symbols();
        let definitions = if self.project.schematic().lib_symbol(lib_id).is_some() {
            Vec::new()
        } else {
            let mut renamed = definition.clone();
            if let Some(items) = renamed.items_mut() {
                items[1] = SExpr::string(lib_id);
            }
            let index = self.project.schematic().definition_insert_index(lib_id);
            vec![(index, renamed)]
        };
        let created_lib_symbols = created_lib_symbols && !definitions.is_empty();
        Ok(self.execute(Command::PlaceSymbols {
            symbols: vec![(index, node)],
            definitions,
            created_lib_symbols,
        }))
    }

    /// Moves every symbol in `uuids` by `(dx, dy)` as one undo step.
    pub fn move_symbols(&mut self, uuids: &[Uuid], dx: Nm, dy: Nm) -> Result<Changes, CoreError> {
        let uuids = dedupe(uuids);
        if uuids.is_empty() || (dx == Nm::ZERO && dy == Nm::ZERO) {
            return Ok(Changes::default());
        }
        self.check_symbols(&uuids)?;
        Ok(self.execute(Command::MoveSymbols { uuids, dx, dy }))
    }

    /// Rotates or mirrors every symbol in `uuids` about its own origin as one undo step.
    pub fn transform_symbols(
        &mut self,
        uuids: &[Uuid],
        transform: SymbolTransform,
    ) -> Result<Changes, CoreError> {
        let no_change = matches!(
            transform,
            SymbolTransform::Rotate(steps) if steps.rem_euclid(4) == 0
        );
        let uuids = dedupe(uuids);
        if uuids.is_empty() || no_change {
            return Ok(Changes::default());
        }
        let mut symbols = Vec::with_capacity(uuids.len());
        for uuid in &uuids {
            let before = self
                .project
                .schematic()
                .symbol(*uuid)
                .ok_or_else(missing_symbol)?;
            let after = match transform {
                SymbolTransform::Rotate(steps) => before.rotated(steps),
                SymbolTransform::Mirror(axis) => before.mirrored(axis),
            };
            symbols.push((*uuid, before, after));
        }
        Ok(self.execute(Command::TransformSymbols { symbols }))
    }

    /// Deletes every symbol in `uuids` as one undo step. Definitions no longer used by any
    /// remaining symbol leave `(lib_symbols)` within the same step.
    pub fn delete_symbols(&mut self, uuids: &[Uuid]) -> Result<Changes, CoreError> {
        let uuids = dedupe(uuids);
        if uuids.is_empty() {
            return Ok(Changes::default());
        }
        let schematic = self.project.schematic();
        let mut symbols = Vec::with_capacity(uuids.len());
        let mut deleted_lib_ids: Vec<String> = Vec::new();
        for uuid in &uuids {
            let Some((index, node)) = schematic.placed_entry(*uuid) else {
                return Err(missing_symbol());
            };
            if let Some(symbol) = PlacedSymbol::read(&node)
                && !deleted_lib_ids.contains(&symbol.lib_id)
            {
                deleted_lib_ids.push(symbol.lib_id);
            }
            symbols.push((index, node));
        }
        symbols.sort_by_key(|(index, _)| *index);

        let remaining: Vec<String> = schematic
            .symbols()
            .into_iter()
            .filter(|symbol| !uuids.contains(&symbol.uuid))
            .map(|symbol| symbol.lib_id)
            .collect();
        let mut definitions = Vec::new();
        for lib_id in &deleted_lib_ids {
            if !remaining.contains(lib_id)
                && let Some(entry) = schematic.definition_entry(lib_id)
            {
                definitions.push(entry);
            }
        }
        definitions.sort_by_key(|(index, _)| *index);
        Ok(self.execute(Command::DeleteSymbols {
            symbols,
            definitions,
            created_lib_symbols: false,
        }))
    }

    /// Draws a wire along `points` as one undo step, one segment per pair of points, each
    /// horizontal or vertical. A wire a new segment ends inside is split there and gets a
    /// junction; a new segment is split wherever a pin end or wire end lies inside it.
    pub fn draw_wire(
        &mut self,
        points: &[Point],
        fresh: &mut impl FnMut() -> Uuid,
    ) -> Result<Changes, CoreError> {
        if points.len() > MAX_WIRE_POINTS {
            return Err(CoreError::InvalidEdit(format!(
                "A wire can have at most {MAX_WIRE_POINTS} points."
            )));
        }
        let segments = wiring::polyline_segments(points);
        let segments = segments.map_err(CoreError::InvalidEdit)?;
        let mut new_wires = Vec::with_capacity(segments.len());
        for (start, end) in segments {
            let uuid = fresh();
            new_wires.push(Wire { uuid, start, end });
        }
        let request = WiringRequest {
            new_wires,
            ..WiringRequest::default()
        };
        Ok(self.edit_wiring("Draw Wire", request, fresh))
    }

    /// Places a local or global label anchored at `position` as one undo step. A wire the
    /// anchor lands inside is split there so the label connects.
    pub fn place_label(
        &mut self,
        kind: LabelKind,
        text: &str,
        position: Point,
        rotation_steps: i32,
        fresh: &mut impl FnMut() -> Uuid,
    ) -> Result<Changes, CoreError> {
        if let Err(message) = wiring::validate_label_text(text) {
            return Err(CoreError::InvalidEdit(message));
        }
        let label = Label {
            uuid: fresh(),
            kind,
            text: text.to_owned(),
            position,
            rotation: quarter_turns(rotation_steps),
        };
        let request = WiringRequest {
            new_labels: vec![label],
            ..WiringRequest::default()
        };
        Ok(self.edit_wiring("Place Label", request, fresh))
    }

    /// Places a no-connect flag at `position` as one undo step. A point holds at most one
    /// flag.
    pub fn place_no_connect(
        &mut self,
        position: Point,
        fresh: &mut impl FnMut() -> Uuid,
    ) -> Result<Changes, CoreError> {
        let schematic = self.project.schematic();
        if schematic
            .no_connects()
            .iter()
            .any(|flag| flag.position == position)
        {
            return Err(CoreError::InvalidEdit(
                "A no-connect flag is already there.".to_owned(),
            ));
        }
        let flag = NoConnect {
            uuid: fresh(),
            position,
        };
        let request = WiringRequest {
            new_no_connects: vec![flag],
            ..WiringRequest::default()
        };
        Ok(self.edit_wiring("Place No-Connect Flag", request, fresh))
    }

    /// Moves the wires, labels, and no-connect flags in `uuids` by `(dx, dy)` as one undo
    /// step. Wires keep their own ends; connected neighbors don't stretch.
    pub fn move_wiring(
        &mut self,
        uuids: &[Uuid],
        dx: Nm,
        dy: Nm,
        fresh: &mut impl FnMut() -> Uuid,
    ) -> Result<Changes, CoreError> {
        let uuids = dedupe(uuids);
        if uuids.is_empty() || (dx == Nm::ZERO && dy == Nm::ZERO) {
            return Ok(Changes::default());
        }
        let selection = self.wiring_selection(&uuids)?;
        let noun = selection.noun();
        let moved_wires = selection
            .wires
            .iter()
            .map(|wire| wire.moved_by(dx, dy))
            .collect();
        let mut changed_labels = selection.labels;
        for label in &mut changed_labels {
            label.position = label.position.offset(dx, dy);
        }
        let moved_no_connects = selection
            .no_connects
            .iter()
            .map(|flag| flag.moved_by(dx, dy))
            .collect();
        let request = WiringRequest {
            moved_wires,
            changed_labels,
            moved_no_connects,
            ..WiringRequest::default()
        };
        Ok(self.edit_wiring(&format!("Move {noun}"), request, fresh))
    }

    /// Rotates the labels in `uuids` about their anchors by `steps` quarter turns as one
    /// undo step.
    pub fn rotate_labels(
        &mut self,
        uuids: &[Uuid],
        steps: i32,
        fresh: &mut impl FnMut() -> Uuid,
    ) -> Result<Changes, CoreError> {
        let uuids = dedupe(uuids);
        if uuids.is_empty() || steps.rem_euclid(4) == 0 {
            return Ok(Changes::default());
        }
        let selection = self.wiring_selection(&uuids)?;
        if !selection.wires.is_empty() || !selection.no_connects.is_empty() {
            return Err(CoreError::InvalidEdit(
                "Only labels can be rotated.".to_owned(),
            ));
        }
        let mut changed_labels = selection.labels;
        for label in &mut changed_labels {
            label.rotation = label.rotation.rotate(quarter_turns(steps));
        }
        let request = WiringRequest {
            changed_labels,
            ..WiringRequest::default()
        };
        Ok(self.edit_wiring("Rotate Label", request, fresh))
    }

    /// Deletes the wires, labels, and no-connect flags in `uuids` as one undo step, with
    /// the junctions that are no longer needed.
    pub fn delete_wiring(
        &mut self,
        uuids: &[Uuid],
        fresh: &mut impl FnMut() -> Uuid,
    ) -> Result<Changes, CoreError> {
        let uuids = dedupe(uuids);
        if uuids.is_empty() {
            return Ok(Changes::default());
        }
        let noun = self.wiring_selection(&uuids)?.noun();
        let request = WiringRequest {
            deleted: uuids.into_iter().collect(),
            ..WiringRequest::default()
        };
        Ok(self.edit_wiring(&format!("Delete {noun}"), request, fresh))
    }

    /// The wires, labels, and no-connect flags named by `uuids`, or an error when any is
    /// none of those.
    fn wiring_selection(&self, uuids: &[Uuid]) -> Result<WiringSelection, CoreError> {
        let schematic = self.project.schematic();
        let all_wires = schematic.wires();
        let all_labels = schematic.labels();
        let all_flags = schematic.no_connects();
        let mut selection = WiringSelection::default();
        for uuid in uuids {
            if let Some(wire) = all_wires.iter().find(|wire| wire.uuid == *uuid) {
                selection.wires.push(*wire);
            } else if let Some(label) = all_labels.iter().find(|label| label.uuid == *uuid) {
                selection.labels.push(label.clone());
            } else if let Some(flag) = all_flags.iter().find(|flag| flag.uuid == *uuid) {
                selection.no_connects.push(*flag);
            } else {
                return Err(CoreError::InvalidEdit(
                    "A selected item is not a wire, label, or no-connect flag on the schematic."
                        .to_owned(),
                ));
            }
        }
        Ok(selection)
    }

    /// Numbers every unannotated symbol as one undo step and returns how many changed.
    /// A reference ending in `?` gets the lowest unused number for its prefix, in order
    /// top to bottom, then left to right. References starting with `#` are left alone.
    /// Units keep their numbers: an instance of a multi-unit symbol joins an instance
    /// numbered earlier in the same pass with the same `lib_id` and `Value` whose
    /// reference doesn't have its unit yet.
    pub fn annotate(&mut self) -> (Changes, usize) {
        let schematic = self.project.schematic();
        let symbols = schematic.symbols();
        let mut used: HashMap<String, Vec<u32>> = HashMap::new();
        for symbol in &symbols {
            if let Some((prefix, number)) = annotated_reference(symbol.reference()) {
                used.entry(prefix.to_owned()).or_default().push(number);
            }
        }
        let mut pending: Vec<&PlacedSymbol> = symbols
            .iter()
            .filter(|symbol| unannotated_prefix(symbol.reference()).is_some())
            .collect();
        pending.sort_by_key(|symbol| {
            let at = symbol.position;
            (at.y, at.x, symbol.uuid)
        });

        let mut shared: Vec<SharedReference> = Vec::new();
        let mut edits = Vec::with_capacity(pending.len());
        for symbol in pending {
            let Some(prefix) = unannotated_prefix(symbol.reference()) else {
                continue;
            };
            let value = symbol.property("Value").unwrap_or_default();
            let multi_unit = schematic
                .lib_symbol(&symbol.lib_id)
                .is_some_and(|definition| unit_count(definition) > 1);
            let joined = shared.iter_mut().find(|entry| {
                multi_unit
                    && entry.lib_id == symbol.lib_id
                    && entry.value == value
                    && !entry.units.contains(&symbol.unit)
            });
            let reference = match joined {
                Some(entry) => {
                    entry.units.push(symbol.unit);
                    entry.reference.clone()
                }
                None => {
                    let numbers = used.entry(prefix.to_owned()).or_default();
                    let number = (1..).find(|n| !numbers.contains(n)).unwrap_or(1);
                    numbers.push(number);
                    let reference = format!("{prefix}{number}");
                    shared.push(SharedReference {
                        lib_id: symbol.lib_id.clone(),
                        value: value.to_owned(),
                        reference: reference.clone(),
                        units: vec![symbol.unit],
                    });
                    reference
                }
            };
            let after = symbol.with_reference(&reference);
            edits.push((symbol.uuid, symbol.clone(), after));
        }
        if edits.is_empty() {
            return (Changes::default(), 0);
        }
        let count = edits.len();
        let command = Command::Group {
            label: ANNOTATE_LABEL.to_owned(),
            commands: vec![Command::TransformSymbols { symbols: edits }],
        };
        (self.execute(command), count)
    }

    /// Runs the electrical rules check on the current schematic with the project's rule
    /// severities.
    pub fn erc(&self) -> Result<Vec<Violation>, CoreError> {
        let severities = self.project.rule_severities();
        let schematic = self.project.schematic();
        crate::erc::check(schematic, &severities).map_err(CoreError::InvalidEdit)
    }

    /// The setting of every ERC rule, or an error when `.kicad_pro` can't be changed.
    pub fn erc_severities(&self) -> Result<RuleSeverities, CoreError> {
        self.project
            .kicad_project()
            .check_writable()
            .map_err(|reason| self.unwritable_settings(&reason))?;
        Ok(self.project.rule_severities())
    }

    /// Changes the ERC rule severities in `.kicad_pro` as one undo step, creating the file
    /// on the next save if there is none. Rules left out keep their setting, and an edit
    /// that changes nothing records nothing.
    pub fn edit_erc_severities(
        &mut self,
        severities: &RuleSeverities,
    ) -> Result<Changes, CoreError> {
        let current = self.project.rule_severities();
        let mut changed = RuleSeverities::new();
        for (key, setting) in severities {
            let Some(previous) = current.get(key) else {
                return Err(CoreError::InvalidEdit(format!(
                    "There is no ERC rule named {key}."
                )));
            };
            if previous != setting {
                changed.insert(key.clone(), *setting);
            }
        }
        if changed.is_empty() {
            return Ok(Changes::default());
        }
        let before = self.project.kicad_project().clone();
        let file_name = format!("{}.kicad_pro", self.project.name());
        let after = before
            .with_rule_severities(&file_name, &changed)
            .map_err(|reason| self.unwritable_settings(&reason))?;
        Ok(self.execute(Command::Group {
            label: ERC_SEVERITIES_LABEL.to_owned(),
            commands: vec![Command::SetKicadProject {
                before: Box::new(before),
                after: Box::new(after),
            }],
        }))
    }

    fn unwritable_settings(&self, reason: &str) -> CoreError {
        CoreError::InvalidEdit(format!(
            "{}.kicad_pro can't be read, so its ERC settings can't be changed: {reason}",
            self.project.name()
        ))
    }

    /// The fields of the symbol `uuid` as the properties dialog shows them: the mandatory
    /// fields first, in KiCad's order (an empty one stands in for any the file lacks),
    /// then the others in file order.
    pub fn symbol_fields(&self, uuid: Uuid) -> Result<Vec<FieldEdit>, CoreError> {
        let symbol = self
            .project
            .schematic()
            .symbol(uuid)
            .ok_or_else(missing_symbol)?;
        let mut fields = Vec::with_capacity(symbol.properties.len());
        for key in MANDATORY_FIELDS {
            let field = match symbol.properties.iter().find(|p| p.key == key) {
                Some(property) => FieldEdit::from(property),
                None => FieldEdit {
                    name: key.to_owned(),
                    value: String::new(),
                    visible: matches!(key, "Reference" | "Value"),
                },
            };
            fields.push(field);
        }
        let others = symbol
            .properties
            .iter()
            .filter(|property| !MANDATORY_FIELDS.contains(&property.key.as_str()));
        fields.extend(others.map(FieldEdit::from));
        Ok(fields)
    }

    /// Replaces the fields of the symbol `uuid` with `fields`, in that order, as one undo
    /// step. Fields keep their position and formatting by name; new fields start at the
    /// symbol's anchor. Every mandatory field must be present, names must be unique and
    /// non-empty, and the Reference must not be empty.
    pub fn edit_symbol_fields(
        &mut self,
        uuid: Uuid,
        fields: &[FieldEdit],
    ) -> Result<Changes, CoreError> {
        let before = self
            .project
            .schematic()
            .symbol(uuid)
            .ok_or_else(missing_symbol)?;
        check_fields(fields)?;
        let mut after = before.clone();
        after.properties = fields
            .iter()
            .map(|field| {
                let existing = before.properties.iter().find(|p| p.key == field.name);
                let mut property = match existing {
                    Some(property) => property.clone(),
                    None => SymbolProperty {
                        key: field.name.clone(),
                        value: String::new(),
                        position: before.position,
                        rotation: Angle::ZERO,
                        hidden: true,
                    },
                };
                property.value = field.value.clone();
                property.hidden = !field.visible;
                property
            })
            .collect();
        if after == before {
            return Ok(Changes::default());
        }
        Ok(self.execute(Command::Group {
            label: EDIT_FIELDS_LABEL.to_owned(),
            commands: vec![Command::TransformSymbols {
                symbols: vec![(uuid, before, after)],
            }],
        }))
    }

    /// Sets the value of one field of the symbol `uuid` as one undo step, with the same
    /// rules as [`Session::edit_symbol_fields`].
    pub fn set_symbol_field(
        &mut self,
        uuid: Uuid,
        name: &str,
        value: &str,
    ) -> Result<Changes, CoreError> {
        let mut fields = self.symbol_fields(uuid)?;
        let Some(field) = fields.iter_mut().find(|field| field.name == name) else {
            return Err(CoreError::InvalidEdit(format!(
                "The symbol has no {name} field."
            )));
        };
        field.value = value.to_owned();
        self.edit_symbol_fields(uuid, &fields)
    }

    /// Plans a wiring edit and records it as one undo step. An edit that changes no node
    /// records nothing.
    fn edit_wiring(
        &mut self,
        label: &str,
        request: WiringRequest,
        fresh: &mut impl FnMut() -> Uuid,
    ) -> Changes {
        let schematic = self.project.schematic();
        let plan = wiring::plan(&schematic.wiring_sheet(), request, fresh);
        let (removed, inserted) = item_edits(schematic, &plan);
        if removed.is_empty() && inserted.is_empty() {
            return Changes::default();
        }
        self.execute(Command::EditItems {
            label: label.to_owned(),
            removed,
            inserted,
        })
    }

    /// Errors when any of `uuids` is not a symbol placed on the schematic.
    fn check_symbols(&self, uuids: &[Uuid]) -> Result<(), CoreError> {
        for uuid in uuids {
            if self.project.schematic().symbol(*uuid).is_none() {
                return Err(missing_symbol());
            }
        }
        Ok(())
    }

    pub fn undo(&mut self) -> Result<Changes, CoreError> {
        let entry = self.undo.pop().ok_or(CoreError::NothingToUndo)?;
        let mut changes = Changes::default();
        entry
            .command
            .inverse()
            .apply(&mut self.project, &mut changes);
        self.redo.push(entry);
        Ok(changes)
    }

    pub fn redo(&mut self) -> Result<Changes, CoreError> {
        let entry = self.redo.pop().ok_or(CoreError::NothingToRedo)?;
        let mut changes = Changes::default();
        entry.command.apply(&mut self.project, &mut changes);
        self.undo.push(entry);
        Ok(changes)
    }

    pub fn save(&mut self) -> Result<(), CoreError> {
        self.project.save()?;
        self.mark_saved();
        Ok(())
    }

    /// Saves into the new folder `parent/name`, which becomes the project's
    /// location. On failure the session still points at the old location.
    pub fn save_as(&mut self, parent: &Path, name: &str) -> Result<(), CoreError> {
        self.project = self.project.save_as(parent, name)?;
        self.mark_saved();
        Ok(())
    }

    fn execute(&mut self, command: Command) -> Changes {
        let mut changes = Changes::default();
        command.apply(&mut self.project, &mut changes);
        self.undo.push(Entry {
            id: self.next_id,
            command,
        });
        self.next_id += 1;
        self.redo.clear();
        changes
    }

    fn mark_saved(&mut self) {
        self.saved_top = self.undo.last().map(|entry| entry.id);
    }
}

/// The selection with duplicates removed, order kept.
fn dedupe(uuids: &[Uuid]) -> Vec<Uuid> {
    let mut unique = Vec::with_capacity(uuids.len());
    for uuid in uuids {
        if !unique.contains(uuid) {
            unique.push(*uuid);
        }
    }
    unique
}

/// Top-level nodes paired with their child indices.
type NodeEdits = Vec<(usize, SExpr)>;

/// The node edits that carry out `plan`. Deleted and changed items are removed at their
/// current child indices; changed and new items are inserted at their indices after the
/// edit. Changed items keep their place, and new ones go before `(sheet_instances)`.
fn item_edits(schematic: &Schematic, plan: &WiringPlan) -> (NodeEdits, NodeEdits) {
    let mut changed: HashMap<Uuid, SExpr> = HashMap::new();
    for (uuid, item) in &plan.changed {
        let previous = schematic.item(*uuid);
        let node = match item {
            ChangedItem::Wire(wire) => wire.node(previous),
            ChangedItem::Label(label) => label.to_node(previous),
            ChangedItem::NoConnect(flag) => flag.node(previous),
        };
        changed.insert(*uuid, node);
    }
    let new_index = schematic.placed_insert_index();
    let items = schematic.items();
    let mut removed = Vec::new();
    let mut inserted = Vec::new();
    let mut next = 1;
    for (position, node) in items.iter().enumerate() {
        let index = position + 1;
        if index == new_index {
            insert_new(plan, &mut inserted, &mut next);
        }
        let uuid = node_uuid(node);
        if uuid.is_some_and(|uuid| plan.deleted.contains(&uuid)) {
            removed.push((index, node.clone()));
        } else if let Some(after) = uuid.and_then(|uuid| changed.get(&uuid)) {
            removed.push((index, node.clone()));
            inserted.push((next, after.clone()));
            next += 1;
        } else {
            next += 1;
        }
    }
    if new_index > items.len() {
        insert_new(plan, &mut inserted, &mut next);
    }
    (removed, inserted)
}

fn insert_new(plan: &WiringPlan, inserted: &mut NodeEdits, next: &mut usize) {
    for item in &plan.added {
        inserted.push((*next, item.to_node()));
        *next += 1;
    }
}

/// A symbol field as the properties dialog edits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEdit {
    pub name: String,
    pub value: String,
    pub visible: bool,
}

impl From<&SymbolProperty> for FieldEdit {
    fn from(property: &SymbolProperty) -> FieldEdit {
        FieldEdit {
            name: property.key.clone(),
            value: property.value.clone(),
            visible: !property.hidden,
        }
    }
}

/// Errors unless `fields` holds every mandatory field once, unique non-empty names, values
/// within the length limit, and a non-empty Reference.
fn check_fields(fields: &[FieldEdit]) -> Result<(), CoreError> {
    let invalid = |message: String| Err(CoreError::InvalidEdit(message));
    let mut names: Vec<&str> = Vec::with_capacity(fields.len());
    for field in fields {
        let name = field.name.as_str();
        if name.trim().is_empty() {
            return invalid("Field names cannot be empty.".to_owned());
        }
        if name.chars().count() > MAX_FIELD_CHARS {
            return invalid(format!(
                "Field names can be at most {MAX_FIELD_CHARS} characters."
            ));
        }
        if field.value.chars().count() > MAX_FIELD_VALUE_CHARS {
            return invalid(format!(
                "{name} can be at most {MAX_FIELD_VALUE_CHARS} characters."
            ));
        }
        if names.contains(&name) {
            return invalid(format!("There is already a field named {name}."));
        }
        names.push(name);
    }
    for key in MANDATORY_FIELDS {
        if !names.contains(&key) {
            return invalid(format!("The {key} field is required."));
        }
    }
    let reference = fields.iter().find(|field| field.name == "Reference");
    if reference.is_some_and(|field| field.value.trim().is_empty()) {
        return invalid("Reference cannot be empty.".to_owned());
    }
    Ok(())
}

/// The wiring items a selection names, by kind.
#[derive(Debug, Default)]
struct WiringSelection {
    wires: Vec<Wire>,
    labels: Vec<Label>,
    no_connects: Vec<NoConnect>,
}

impl WiringSelection {
    /// The word undo labels use for the selection.
    fn noun(&self) -> &'static str {
        let kinds = (
            !self.wires.is_empty(),
            !self.labels.is_empty(),
            !self.no_connects.is_empty(),
        );
        match kinds {
            (true, false, false) => "Wire",
            (false, true, false) => "Label",
            (false, false, true) => "No-Connect Flag",
            _ => "Wiring",
        }
    }
}

/// A reference handed out during annotation, with the units already using it.
struct SharedReference {
    lib_id: String,
    value: String,
    reference: String,
    units: Vec<u32>,
}

/// The prefix of a reference annotation fills in: `R` for `R?`. `None` for annotated
/// references, references starting with `#`, and a bare `?`.
fn unannotated_prefix(reference: &str) -> Option<&str> {
    let prefix = reference.strip_suffix('?')?;
    if prefix.is_empty() || prefix.starts_with('#') {
        return None;
    }
    Some(prefix)
}

/// The prefix and number of an annotated reference: `("R", 12)` for `R12`.
fn annotated_reference(reference: &str) -> Option<(&str, u32)> {
    let prefix = reference.trim_end_matches(|c: char| c.is_ascii_digit());
    if prefix.is_empty() || prefix.len() == reference.len() {
        return None;
    }
    let number = reference[prefix.len()..].parse().ok()?;
    Some((prefix, number))
}

fn quarter_turns(steps: i32) -> Angle {
    Angle::from_decidegrees(steps.rem_euclid(4) * 900)
}

fn missing_symbol() -> CoreError {
    CoreError::InvalidEdit("A symbol in the selection is not placed on the schematic.".to_owned())
}

fn validate_title_block(block: &TitleBlock) -> Result<(), CoreError> {
    let fields = [&block.title, &block.date, &block.revision, &block.company]
        .into_iter()
        .chain(block.comments.iter());
    for field in fields {
        if field.chars().count() > MAX_FIELD_CHARS || field.contains(['\n', '\r']) {
            return Err(CoreError::InvalidEdit(format!(
                "Title block fields must be a single line of at most {MAX_FIELD_CHARS} characters."
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn session() -> Session {
        Session::new(Project::new(PathBuf::from("unsaved"), "demo", Uuid::nil()).unwrap())
    }

    fn block(title: &str) -> TitleBlock {
        TitleBlock {
            title: title.to_owned(),
            ..TitleBlock::default()
        }
    }

    #[test]
    fn undo_restores_the_exact_previous_project() {
        let mut session = session();
        let original = session.project().clone();
        let changes = session
            .edit_properties(block("Amp"), Nm(2_540_000))
            .unwrap();
        assert_eq!(
            changes,
            Changes {
                title_block: true,
                grid_size: true,
                symbols: false,
                definitions: false,
                wiring: false,
                erc_settings: false,
            }
        );
        let edited = session.project().clone();
        assert_ne!(edited, original);

        session.undo().unwrap();
        assert_eq!(session.project(), &original);
        session.redo().unwrap();
        assert_eq!(session.project(), &edited);
    }

    #[test]
    fn undo_and_redo_walk_through_several_edits() {
        let mut session = session();
        let grid = session.project().grid_size();
        let states: Vec<Project> = ["One", "Two", "Three"]
            .iter()
            .map(|title| {
                session.edit_properties(block(title), grid).unwrap();
                session.project().clone()
            })
            .collect();

        session.undo().unwrap();
        assert_eq!(session.project(), &states[1]);
        session.undo().unwrap();
        assert_eq!(session.project(), &states[0]);
        session.redo().unwrap();
        session.redo().unwrap();
        assert_eq!(session.project(), &states[2]);
        assert!(matches!(session.redo(), Err(CoreError::NothingToRedo)));
    }

    #[test]
    fn an_edit_that_changes_nothing_records_nothing() {
        let mut session = session();
        let grid = session.project().grid_size();
        let changes = session
            .edit_properties(TitleBlock::default(), grid)
            .unwrap();
        assert!(!changes.any());
        assert!(!session.can_undo());
        assert!(!session.is_dirty());
    }

    #[test]
    fn a_new_edit_discards_the_redo_branch() {
        let mut session = session();
        let grid = session.project().grid_size();
        session.edit_properties(block("One"), grid).unwrap();
        session.undo().unwrap();
        assert!(session.can_redo());
        session.edit_properties(block("Two"), grid).unwrap();
        assert!(!session.can_redo());
        assert_eq!(session.undo_label(), Some(PROPERTIES_LABEL));
    }

    #[test]
    fn dirty_state_follows_the_saved_point_through_undo_and_redo() {
        let mut session = session();
        let grid = session.project().grid_size();
        assert!(!session.is_dirty());
        session.edit_properties(block("One"), grid).unwrap();
        assert!(session.is_dirty());
        session.undo().unwrap();
        assert!(!session.is_dirty());
        session.redo().unwrap();
        assert!(session.is_dirty());

        session.mark_saved();
        assert!(!session.is_dirty());
        session.undo().unwrap();
        assert!(session.is_dirty());
        session.redo().unwrap();
        assert!(!session.is_dirty());

        session.undo().unwrap();
        session.edit_properties(block("Two"), grid).unwrap();
        session.undo().unwrap();
        assert!(session.is_dirty(), "the saved state is no longer reachable");
    }

    #[test]
    fn invalid_edits_are_rejected_and_not_recorded() {
        let mut session = session();
        let grid = session.project().grid_size();
        assert!(matches!(
            session.edit_properties(block("two\nlines"), grid),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.edit_properties(block(&"x".repeat(MAX_FIELD_CHARS + 1)), grid),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.edit_properties(TitleBlock::default(), Nm(0)),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(!session.can_undo());
        assert!(matches!(session.undo(), Err(CoreError::NothingToUndo)));
    }

    const SYMBOL_1: Uuid = Uuid::from_u128(1);
    const SYMBOL_2: Uuid = Uuid::from_u128(2);
    const PIN_1: Uuid = Uuid::from_u128(3);
    const PIN_2: Uuid = Uuid::from_u128(4);
    const PIN_3: Uuid = Uuid::from_u128(5);
    const PINS: [Uuid; 2] = [PIN_1, PIN_2];

    fn r_definition() -> SExpr {
        crate::sexpr::parse(
            "(symbol \"Device:R\" \
            (property \"Reference\" \"R\" (at 2.032 0 90)) \
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

    fn led_definition() -> SExpr {
        crate::sexpr::parse(
            "(symbol \"Device:LED\" \
            (property \"Reference\" \"D\" (at 0 2.54 0)) \
            (property \"Value\" \"LED\" (at 0 -2.54 0)) \
            (property \"Footprint\" \"\" (at 0 0 0) (hide yes)) \
            (property \"Datasheet\" \"\" (at 0 0 0) (hide yes)) \
            (symbol \"LED_1_1\" \
            (pin passive line (at -3.81 0 0) (length 2.54) (name \"K\") (number \"1\"))))",
        )
        .unwrap()
    }

    fn place(session: &mut Session, uuid: Uuid) -> Changes {
        let placement = Placement {
            lib_id: "Device:R",
            position: Point::new(Nm(100_000_000), Nm(50_000_000)),
            rotation_steps: 0,
            mirror: None,
            uuid,
            pin_uuids: &PINS,
        };
        session.place_symbol(&r_definition(), placement).unwrap()
    }

    #[test]
    fn placing_adds_the_symbol_and_embeds_the_definition() {
        let mut session = session();
        let changes = place(&mut session, SYMBOL_1);
        assert_eq!(
            changes,
            Changes {
                title_block: false,
                grid_size: false,
                symbols: true,
                definitions: true,
                wiring: false,
                erc_settings: false,
            }
        );
        let schematic = session.project().schematic();
        assert_eq!(schematic.lib_symbol_ids(), ["Device:R"]);
        let symbols = schematic.symbols();
        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.reference(), "R?");
        assert_eq!((symbol.unit, symbol.pins.len()), (1, 2));
        assert_eq!(symbol.uuid, SYMBOL_1);
        assert_eq!(symbol.pins[0].uuid, PIN_1);

        let text = schematic.to_file_string();
        assert!(text.contains("(lib_id \"Device:R\")"), "{text}");
        assert!(text.contains("(symbol \"Device:R\""), "{text}");
        assert!(text.contains("(reference \"R?\")"), "{text}");
        assert!(text.contains("(project \"demo\""), "{text}");
        assert!(text.find("(lib_id").unwrap() < text.find("(sheet_instances").unwrap());
    }

    #[test]
    fn place_undoes_and_redoes_exactly() {
        let mut session = session();
        let original = session.project().clone();
        place(&mut session, SYMBOL_1);
        let placed = session.project().clone();
        session.undo().unwrap();
        assert_eq!(session.project(), &original);
        session.redo().unwrap();
        assert_eq!(session.project(), &placed);
    }

    #[test]
    fn definitions_are_shared_and_removed_with_the_last_instance() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let changes = place(&mut session, SYMBOL_2);
        assert!(!changes.definitions, "the second placement reuses it");
        assert_eq!(session.project().schematic().lib_symbol_ids().len(), 1);
        assert_eq!(session.project().schematic().symbols().len(), 2);

        let changes = session.delete_symbols(&[SYMBOL_2]).unwrap();
        assert!(changes.symbols && !changes.definitions);
        assert_eq!(session.project().schematic().lib_symbol_ids(), ["Device:R"]);

        let changes = session.delete_symbols(&[SYMBOL_1]).unwrap();
        assert!(changes.symbols && changes.definitions);
        assert!(session.project().schematic().lib_symbol_ids().is_empty());
        assert!(session.project().schematic().symbols().is_empty());
    }

    #[test]
    fn delete_undoes_and_redoes_exactly() {
        let mut session = session();
        let original = session.project().clone();
        place(&mut session, SYMBOL_1);
        place(&mut session, SYMBOL_2);
        let placed = session.project().clone();

        session.delete_symbols(&[SYMBOL_1, SYMBOL_2]).unwrap();
        let deleted = session.project().clone();
        assert_eq!(&deleted, &original);

        session.undo().unwrap();
        assert_eq!(session.project(), &placed);
        session.redo().unwrap();
        assert_eq!(session.project(), &deleted);
    }

    #[test]
    fn moving_shifts_symbols_and_their_properties() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let placed = session.project().clone();
        let changes = session
            .move_symbols(&[SYMBOL_1], Nm(10_000_000), Nm(-5_000_000))
            .unwrap();
        assert!(changes.symbols && !changes.definitions);
        let symbol = session.project().schematic().symbols()[0].clone();
        assert_eq!(symbol.position, Point::new(Nm(110_000_000), Nm(45_000_000)));
        assert_eq!(
            symbol.properties[0].position,
            Point::new(Nm(112_032_000), Nm(45_000_000))
        );
        session.undo().unwrap();
        assert_eq!(session.project(), &placed);
        session.redo().unwrap();
        assert_eq!(session.project().schematic().symbols()[0], symbol);
    }

    #[test]
    fn rotating_and_mirroring_transform_about_the_symbol_origin() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let placed = session.project().clone();

        session
            .transform_symbols(&[SYMBOL_1], SymbolTransform::Rotate(1))
            .unwrap();
        let symbol = session.project().schematic().symbols()[0].clone();
        assert_eq!(symbol.rotation, Angle::from_decidegrees(900));
        assert_eq!(symbol.position, Point::new(Nm(100_000_000), Nm(50_000_000)));
        assert_eq!(
            symbol.properties[0].position,
            Point::new(Nm(100_000_000), Nm(47_968_000))
        );
        assert_eq!(symbol.properties[0].rotation, Angle::from_decidegrees(1800));
        session.undo().unwrap();
        assert_eq!(session.project(), &placed);

        session
            .transform_symbols(&[SYMBOL_1], SymbolTransform::Mirror(Mirror::X))
            .unwrap();
        let symbol = session.project().schematic().symbols()[0].clone();
        assert_eq!(symbol.mirror, Some(Mirror::X));
        assert_eq!(symbol.rotation, Angle::ZERO);
        assert_eq!(
            symbol.properties[0].position,
            Point::new(Nm(97_968_000), Nm(50_000_000))
        );
        assert_eq!(symbol.properties[0].rotation, Angle::from_decidegrees(900));
        session.undo().unwrap();
        assert_eq!(session.project(), &placed);
    }

    #[test]
    fn definitions_stay_sorted_by_library_id() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let placement = Placement {
            lib_id: "Device:LED",
            position: Point::ORIGIN,
            rotation_steps: 0,
            mirror: None,
            uuid: SYMBOL_2,
            pin_uuids: &[PIN_3],
        };
        session.place_symbol(&led_definition(), placement).unwrap();
        assert_eq!(
            session.project().schematic().lib_symbol_ids(),
            ["Device:LED", "Device:R"]
        );
    }

    #[test]
    fn no_op_symbol_commands_record_nothing() {
        let mut session = session();
        assert!(
            !session
                .move_symbols(&[SYMBOL_1], Nm::ZERO, Nm::ZERO)
                .unwrap()
                .any()
        );
        assert!(
            !session
                .transform_symbols(&[SYMBOL_1], SymbolTransform::Rotate(4))
                .unwrap()
                .any()
        );
        assert!(!session.delete_symbols(&[]).unwrap().any());
        assert!(!session.can_undo());
    }

    #[test]
    fn annotation_reads_reference_prefixes_and_numbers() {
        assert_eq!(unannotated_prefix("R?"), Some("R"));
        assert_eq!(unannotated_prefix("SW?"), Some("SW"));
        for skipped in ["R1", "#PWR?", "?", ""] {
            assert_eq!(unannotated_prefix(skipped), None, "{skipped}");
        }
        assert_eq!(annotated_reference("R12"), Some(("R", 12)));
        assert_eq!(annotated_reference("#PWR01"), Some(("#PWR", 1)));
        for skipped in ["R?", "R", "12", "U99999999999"] {
            assert_eq!(annotated_reference(skipped), None, "{skipped}");
        }
    }

    #[test]
    fn annotating_an_empty_schematic_records_nothing() {
        let mut session = session();
        let (changes, count) = session.annotate();
        assert!(!changes.any() && count == 0);
        assert!(!session.can_undo());
    }

    #[test]
    fn annotation_numbers_a_placed_symbol_and_undoes() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let (changes, count) = session.annotate();
        assert!(changes.symbols && count == 1);
        let symbols = session.project().schematic().symbols();
        assert_eq!(symbols[0].reference(), "R1");
        let text = session.project().schematic().to_file_string();
        assert!(text.contains("(reference \"R1\")"), "{text}");
        session.undo().unwrap();
        let symbols = session.project().schematic().symbols();
        assert_eq!(symbols[0].reference(), "R?");
    }

    fn field(name: &str, value: &str, visible: bool) -> FieldEdit {
        FieldEdit {
            name: name.to_owned(),
            value: value.to_owned(),
            visible,
        }
    }

    #[test]
    fn symbol_fields_list_the_mandatory_fields_first() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let fields = session.symbol_fields(SYMBOL_1).unwrap();
        assert_eq!(
            fields,
            [
                field("Reference", "R?", true),
                field("Value", "R", true),
                field("Footprint", "", false),
                field("Datasheet", "", false),
            ]
        );
        assert!(session.symbol_fields(SYMBOL_2).is_err());
    }

    #[test]
    fn editing_fields_adds_hides_and_removes_them_in_one_step() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let placed = session.project().clone();
        let mut fields = vec![
            field("Reference", "R7", true),
            field("Value", "10k", false),
            field("Footprint", "Resistor_SMD:R_0603", true),
            field("Datasheet", "", false),
            field("MPN", "RC0603", true),
        ];
        let changes = session.edit_symbol_fields(SYMBOL_1, &fields).unwrap();
        assert!(changes.symbols);
        assert_eq!(session.undo_label(), Some(EDIT_FIELDS_LABEL));
        assert_eq!(session.symbol_fields(SYMBOL_1).unwrap(), fields);
        let text = session.project().schematic().to_file_string();
        assert!(text.contains("(reference \"R7\")"), "{text}");
        let reparsed = Schematic::parse(&text).unwrap();
        let symbol = reparsed.symbol(SYMBOL_1).unwrap();
        assert_eq!(symbol.property("MPN"), Some("RC0603"));
        assert_eq!(symbol.properties[4].position, symbol.position);
        let hidden = |key: &str| {
            let property = symbol.properties.iter().find(|p| p.key == key);
            property.unwrap().hidden
        };
        assert!(hidden("Value") && !hidden("Footprint") && !hidden("MPN"));

        fields.pop();
        session.edit_symbol_fields(SYMBOL_1, &fields).unwrap();
        let text = session.project().schematic().to_file_string();
        assert!(!text.contains("MPN"), "{text}");
        let again = session.edit_symbol_fields(SYMBOL_1, &fields).unwrap();
        assert!(!again.any());

        session.undo().unwrap();
        session.undo().unwrap();
        assert_eq!(session.project(), &placed);
    }

    #[test]
    fn setting_one_field_updates_the_instances_reference() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        session
            .set_symbol_field(SYMBOL_1, "Reference", "R12")
            .unwrap();
        let text = session.project().schematic().to_file_string();
        assert!(text.contains("(reference \"R12\")"), "{text}");
        let unknown = session.set_symbol_field(SYMBOL_1, "MPN", "x");
        assert!(matches!(unknown, Err(CoreError::InvalidEdit(_))));
    }

    #[test]
    fn invalid_field_edits_are_rejected() {
        let mut session = session();
        place(&mut session, SYMBOL_1);
        let base = session.symbol_fields(SYMBOL_1).unwrap();
        let mut expect_error = |fields: &[FieldEdit], message: &str| {
            let result = session.edit_symbol_fields(SYMBOL_1, fields);
            match result {
                Err(CoreError::InvalidEdit(text)) => assert_eq!(text, message),
                other => panic!("expected {message:?}, got {other:?}"),
            }
        };
        let mut fields = base.clone();
        fields[0].value = " ".to_owned();
        expect_error(&fields, "Reference cannot be empty.");
        let mut fields = base.clone();
        fields.push(field("", "x", true));
        expect_error(&fields, "Field names cannot be empty.");
        let mut fields = base.clone();
        fields.push(field("Value", "x", true));
        expect_error(&fields, "There is already a field named Value.");
        let mut fields = base.clone();
        fields.truncate(3);
        expect_error(&fields, "The Datasheet field is required.");
        assert_eq!(session.undo_label(), Some("Place Symbol"));
    }

    #[test]
    fn invalid_symbol_commands_are_rejected() {
        let mut session = session();
        let unknown = Uuid::from_u128(99);
        assert!(matches!(
            session.move_symbols(&[unknown], Nm(1_000_000), Nm::ZERO),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.transform_symbols(&[unknown], SymbolTransform::Rotate(1)),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.delete_symbols(&[unknown]),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(!session.can_undo());

        place(&mut session, SYMBOL_1);
        fn placement(uuid: Uuid, pin_uuids: &[Uuid]) -> Placement<'_> {
            Placement {
                lib_id: "Device:R",
                position: Point::ORIGIN,
                rotation_steps: 0,
                mirror: None,
                uuid,
                pin_uuids,
            }
        }
        assert!(matches!(
            session.place_symbol(&r_definition(), placement(SYMBOL_1, &PINS)),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.place_symbol(
                &crate::sexpr::parse("(wire (pts))").unwrap(),
                placement(SYMBOL_2, &[]),
            ),
            Err(CoreError::InvalidEdit(_))
        ));
        assert!(matches!(
            session.place_symbol(&r_definition(), placement(SYMBOL_2, &PINS[..1])),
            Err(CoreError::InvalidEdit(_))
        ));
        assert_eq!(session.project().schematic().symbols().len(), 1);
    }
}
