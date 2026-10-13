# Schematic capture

## Outcome or responsibility

The user draws a circuit by placing library symbols, connecting them with wires, junctions, net labels, and power symbols, and editing component properties. Electrical rule checks flag mistakes. The derived netlist feeds simulation, the board, and AI verification.

## Current verified status

**Status:** Partial

Slice 4a is green in [CI run 38406455082](https://github.com/tharunkumar1177/tron/actions/runs/38406455082), including the Windows installer. Covered by tests:

- Core: placed symbols and `(lib_symbols)` round-trip a hand-written KiCad 10 golden file byte for byte. Place, move, rotate or mirror, and delete each undo and redo exactly. Definitions are added on first use and removed with the last instance.
- IPC: payload serialization, definition lookup from the fixture index, an unknown symbol ID, and a diff walk through every command.
- UI: Vitest covers placing from the library browser with the snapped position, R during placement, Escape, select and Delete, a drag that sends one `move_symbols`, and rendering instances from a diff. Unit tests cover snapping, transforms, bounds, and hit-testing.

Slice 4b (connectivity and netlist) is green in [CI run 38434705527](https://github.com/tharunkumar1177/tron/actions/runs/38434705527), including the Windows installer. Covered by tests:

- Core: the `divider` golden schematic round-trips. Its pin points and derived nets match the expected KiCad names (`+5V`, `/OUT`, `GND`, `VIN`, `unconnected-(R3-Pad2)`). Deleting and redrawing its wiring through commands rebuilds the same wires, junction, and nets, and undo returns the original bytes. Unit tests cover wire splitting, automatic junctions, crossings without junctions, label text rules, and automatic net names.
- IPC: wiring fields in diffs and the label payload.
- UI: Vitest covers drawing a bent wire, toggling the bend and finishing on a pin, Escape cancelling and then exiting, selecting and deleting a wire, repeated local labels with counting names and rotation, a global label with a shape (Alt+L), the power picker from the toolbar, and the picker without the library. `wiring.test.ts` covers routes, snapping, hit-testing, and label names.

Slice 4c (properties and ERC) is Partial. 4c-1 (no-connect flags and annotation) is green in [CI run 38451138951](https://github.com/tharunkumar1177/tron/actions/runs/38451138951), including the Windows installer. Covered by tests:

- Core: placing a no-connect flag on the `divider` golden's free R3 pin round-trips, leaves the nets unchanged, and rejects a second flag at the same point. Moving, deleting, and rotating (rejected) flags undo back to the original bytes. Annotation numbers new resistors top to bottom and reuses a freed number (`R2`, then `R4`, `R5`), updates `(instances)`, leaves `#PWR01` alone, records nothing when nothing needs numbering, and undoes in one step. Unit tests cover reference prefix parsing.
- IPC: `noConnects` in snapshots and diffs, and the "Place No-Connect Flag" undo label.
- UI: Vitest covers Q mode with pin snapping and repeated placement, Escape, the core's duplicate-flag message, selecting and deleting a flag (R does nothing), and Tools > Annotate Schematic with both result messages. `wiring.test.ts` covers flag hit-testing.

4c-2 (property editing) is green in [CI run 38455427365](https://github.com/tharunkumar1177/tron/actions/runs/38455427365). Covered by tests:

- Core: fields list the mandatory four first; one dialog edit adds, hides, and removes fields and undoes in one step; setting Reference updates `(instances)`; empty, duplicate, missing-mandatory, and over-long fields are rejected without recording an undo step.
- IPC: fields round-trip through the UI payload.
- UI: Vitest covers E opening the dialog with focus on Reference, the empty-Reference error, Show toggles, Add Field with focus on the new name, Apply's payload, removing a custom field from a double-click, a core error shown in the dialog, and U editing Reference inline with validation.

4c-3 and 4c-4 are Planned.

## Architecture dependencies

- [Design document ownership](../architecture.md#design-document-ownership)
- [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation)
- [Connectivity and netlist](../architecture.md#connectivity-and-netlist)
- [IPC contract](../architecture.md#ipc-contract)

## Local rules and implications

### Design document ownership

Every placement, wire edit, and property change is one undoable command. A drag counts as a single command, not one per mouse move.

### Coordinate and unit representation

The schematic grid defaults to 1.27 mm (50 mil), which is 1,270,000 nm, matching KiCad symbol pin spacing.

### Connectivity and netlist

Nets are recomputed incrementally when an edit touches connectivity. ERC runs on the derived netlist: unconnected pins, outputs driving each other, power pins with no source, and duplicate reference designators.

### IPC contract

During a drag, the UI renders a local preview and commits a single command on release.

### Slice 4a plan: symbol placement (approved 2026-10-13)

Data model, in `crates/core`:

- Placed symbols embed their full definition in the schematic's `(lib_symbols)`, keyed by library ID `<lib>:<name>`, so a project renders and opens in KiCad without the downloaded library. The definition comes from `tron_library::Index::symbol_definition`. A definition is added the first time its library ID is placed and removed when no instance uses it, both inside the same undoable command.
- A placed `(symbol ...)` keeps its `lib_id`, `(at x y angle)`, optional `(mirror x|y)`, `(unit 1)`, a UUID, the `Reference` (unannotated, such as `R?`), `Value`, `Footprint`, and `Datasheet` properties copied from the definition with their `(at ...)` positions transformed by the placement, its `(pin "n" (uuid ...))` entries, and the `(instances (project "<name>" (path "/<root uuid>" (reference "R?") (unit 1))))` block. Unknown nodes are kept so files round-trip.
- Symbol-geometry extraction (KiCad symbol to `SymbolPreview`) moves from `crates/library` into `crates/core`, so the core can produce geometry from embedded `(lib_symbols)`. `crates/library` uses the core extractor, and the index JSON and generated TypeScript bindings stay byte-identical.
- Commands: place, move, rotate or mirror, and delete, each one undo step. Rotation is in 90° steps.

IPC, in `src-tauri/src/ipc.rs`:

- `ProjectSnapshot` and `DocumentDiff` carry `definitions` (one `SymbolPreview` per library ID, sent once) and `instances` (UUID, library ID, position in nm, rotation, mirror, and the visible Reference and Value text with positions).
- Commands: `place_symbol(libraryId, x, y, rotation, mirror)` (the backend looks up the definition through the library index, so raw KiCad text never crosses IPC), `move_symbols`, `transform_symbols`, and `delete_symbols`.

Interaction design (designer plan):

- Placement starts from the library browser by double-click, Enter, or drag onto the canvas. A ghost of the symbol at `globalAlpha` 0.5 follows the cursor, snapped to the 1.27 mm grid. A click or Enter places it, and placement stays active for repeated placement. Escape or right-click exits. Starting placement focuses the canvas, which has `tabIndex={0}` and no focus outline.
- R rotates 90° clockwise, X mirrors horizontally, and Y mirrors vertically, both during placement and on a selection. Transforms are about the symbol origin.
- In normal mode, a click selects a symbol, a click on empty space clears the selection, dragging a selected symbol moves it with snapping, and Delete or Backspace removes it.
- Colors: the symbol stroke is `--color-primary`, the body fill `--color-surface-elevated`, the pins `--color-text-error`, and the text `--color-text-primary` at `--text-sm` in `--font-family`. When selected, the stroke, pins, and text use `--color-focus`, with a dashed `[4, 4]` 1 px `--color-focus` box padded by `--space-1` around the symbol bounds.
- Cursors: `crosshair` on empty canvas and while placing, `pointer` over a symbol (`.canvas--pointer`), and `grabbing` while dragging or panning.
- Status bar hints: while placing, "Click to place. R: Rotate, X/Y: Mirror. Esc: Cancel". With a selection, "Symbol selected. Drag to move. R/X/Y: Transform. Del: Remove".
- While a definition is loading, show a dashed `--color-text-secondary` box with "Loading..." at the cursor. If loading fails, exit placement and show "Failed to load symbol" in `--color-text-error` in the status bar. Placement can't start while the library isn't downloaded.
- A visually hidden `aria-live="polite"` region announces "Placing <name>", "Symbol placed", "Symbol selected", and "Symbol deleted".
- Reference and Value text is drawn at the positions from the placed symbol's property `(at ...)` fields, hidden when the property is hidden.

### Slice 4b plan: connectivity and netlist (approved 2026-10-13)

Data model and commands, in `crates/core`:

- `(wire (pts (xy) (xy)) (stroke ...) (uuid ...))`, `(junction (at x y) (diameter 0) (color 0 0 0 0) (uuid ...))`, `(label "text" (at x y angle) (effects ...) (uuid ...))`, and `(global_label "text" (shape input|output|bidirectional|tri_state|passive) (at ...) (effects ...) (uuid ...))` are typed in `wiring.rs`. Unknown children pass through edits.
- Every wiring edit is one `EditItems` command that removes and inserts top-level schematic nodes at recorded child indices. Draw, label placement, move, rotate, and delete all use it, so undo restores the exact file.
- `draw_wire` takes the polyline the UI drew. Each segment must be horizontal or vertical. Within the same command, a new wire end, label anchor, or pin end that lands on another wire's interior splits that wire. A new wire is also split where an existing wire end or pin end lies on it. The command then adds a junction where at least two wire ends meet and three or more connection points coincide, and removes auto junctions that are no longer needed at the points it touched. Overlapping collinear wires are not merged.
- Pin connection points come from the embedded definition, hidden pins included, for units 0 and the placed unit. They are transformed like properties: Y flip, then mirror, then rotation, then offset.
- `netlist.rs` derives nets by union-find over exact connection points, as the [Connectivity and netlist](../architecture.md#connectivity-and-netlist) contract says. Local labels join by text. Global labels and power symbols join by name. Symbols whose reference starts with `#` (power symbols) are drivers, not nodes. Local-label nets on the root sheet are named `/<text>`, as KiCad does. Netlists are recomputed in full on demand. A single sheet is linear in its item count, so this is not incremental yet.

IPC: `SymbolInstance` gains `pins` (every connection point in schematic nm). `ProjectSnapshot` and `DocumentDiff` gain `wires`, `junctions`, and `labels`. New commands are `draw_wire`, `place_label`, `move_wiring`, `rotate_labels`, and `delete_wiring`. Power symbols use the existing `place_symbol` with a library search hit from library `power`.

Interaction design (designer plans):

- Wire mode starts with W or a toolbar "Draw wire (W)" button (`aria-pressed`), and it clears any placement or selection. A click starts a wire. The rubber band has one bend, horizontal first, and `/` toggles it. A click anchors a corner. A double-click, Enter, or a click on a pin end or wire finishes the wire and commits it as one command. The first Escape or right-click cancels the wire in progress, and the second exits the mode.
- The cursor snaps to the 1.27 mm grid, or exactly to a pin end or wire end within 10 screen px. The pin and end indicator is a hollow 8 px `--color-focus` circle with a 2 px stroke. The grid indicator is a 4 px `--color-text-secondary` dot.
- Wires are drawn with a 2 px `--color-primary` stroke, and the preview uses the same stroke at `globalAlpha` 0.5. Junctions are filled 6 px `--color-primary` dots. A selected wire is drawn in `--color-focus`. In normal mode, clicking a wire selects it, dragging moves just that segment (KiCad's Move behavior, without stretching neighbors), and Delete removes it.
- L places a local label and Alt+L (Option+L) a global label, because Ctrl+L toggles the Component Library. The toolbar also has "Add net label (L)", "Add global label (Alt+L)", and "Add power symbol (P)". An inline popover at the cursor takes the name: non-empty, no whitespace, up to 256 characters, and a shape select for global labels. Enter attaches a ghost, and clicking places it. Placement repeats, and trailing digits auto-increment (D0, then D1). R rotates the label in 90° steps. A local label is `--color-text-primary` `--text-sm` text with a 3 px `--color-primary` anchor square. A global label has a `--color-text-warning` outline shaped by its type, a `--color-canvas` fill, and `--color-text-warning` text.
- P opens a power picker at the cursor with GND, +5V, +3V3, and VCC, each resolved through the library search and listed only when the index has it, plus "Search library...". Without the library, it shows "Component library required" and a Download button.
- Status hints: "Click to start wire. Esc: Exit", "Click to anchor. Double-click/Enter/Pin to finish. /: Toggle bend. Esc: Cancel", "Wire selected. Drag to move. Del: Remove", "Enter label name. Esc: Cancel", "Click to place. R: Rotate. Esc: Cancel", and "Label selected. Drag to move. R: Rotate. Del: Remove". The `aria-live` region announces mode and placement changes.

### Slice 4c plan: properties and ERC (approved 2026-10-13)

Four PRs, each green in CI on its own, built in this order.

**4c-1. No-connect flags and annotation.**

- A no-connect flag is `(no_connect (at x y) (uuid "…"))`, kept with wires and junctions in `wiring.rs`. Q places one at the snapped point. Flags are moved and deleted through `EditItems`, like wires.
- Annotation is one `EditItems` command over the whole schematic. It skips references that start with `#` and references that don't end in `?`. Candidates are sorted by Y (top to bottom), then X, then UUID. Each gets the lowest unused number at or above 1 for its prefix. For a multi-unit `lib_id`, consecutive unannotated instances share one reference and take units 1 to N. The Reference property and the `(instances (project (path (reference) (unit))))` entry change together.
- As built: annotation is a `Group` ("Annotate Schematic") of one `TransformSymbols`, which replaces whole symbol nodes. Instances keep their own unit instead of being packed into units 1 to N, because the canvas only draws unit 1 and every placed symbol is unit 1. An instance of a multi-unit `lib_id` joins a reference numbered earlier in the same pass when it has the same `Value` and its unit is still free there. Placed power symbols keep `#PWR?`, as planned.
- As built (UI): the toolbar's "Add no-connect flag" button (Q, `aria-pressed`) toggles placing mode. A ghost X follows the cursor, snapping to pins and wire ends like the wire tool, and each click places a flag until Escape, Q, or right-click. Flags draw as a 1.27 mm X in `--color-primary` (`--color-focus` when selected), select and drag like labels, and can't be rotated. Hints: "Click to place flag. Esc: Cancel" and "Flag selected. Drag to move. Del: Remove". The native **Tools > Annotate Schematic** item has no shortcut and no confirmation. It shows "Annotated N symbols" or "Schematic is already fully annotated" in the status bar until the next input, and announces the same text.

**4c-2. Property editing.** `edit_symbol_properties(uuid, properties)` replaces the symbol node in one `EditItems` step. Each property has a key, a value, a position, a rotation, and visibility, and Reference edits also update `(instances)`. E or a double-click opens the dialog; U and V edit Reference and Value inline. References must not be empty. Duplicates are left to ERC.

- As built: the commands are `get_symbol_fields`, `edit_symbol_fields(uuid, fields)` for the dialog, and `set_symbol_field(uuid, name, value)` for inline edits. Each field is a name, a value, and visibility; positions and rotations of existing fields are kept, and new fields are placed hidden at the symbol anchor. Both edits are a `Group` ("Edit Symbol Properties") of one `TransformSymbols`. Reference, Value, Footprint, and Datasheet are always listed first and can't be removed or renamed. Field names must be unique and non-empty, names are capped at 256 characters, and values at 4096. `PlacedSymbol::to_node` now matches property nodes by key, so removed fields drop out and unknown children are kept, and `SymbolProperty::to_node` writes `(hide yes)` and drops `(hide no)`.
- As built (UI): **Symbol Properties** is a modal grid of Name, Value, and Show columns with Add Field (default name `FieldN`, hidden) and a remove button on custom rows. Apply is disabled until something changes and the fields are valid; core errors show inline. U and V open a small popover at the field's text with Cancel and Apply. E, U, and V act on the selected symbol or else the one under the cursor; a double-click opens the dialog outside tool modes. Success announces "Symbol properties updated".

**4c-3. ERC.** `erc.rs` checks the schematic and its derived netlist and returns transient violations: kind, severity, position, the item UUIDs, and KiCad's message. Violations are never saved in `.kicad_sch`.

| Rule | Message | Default | Position |
| --- | --- | --- | --- |
| `unannotated` | Symbol is not annotated | Error | Symbol anchor |
| `duplicate_reference` | Duplicate reference designators | Error | Symbol anchor |
| `pin_not_connected` | Pin not connected | Error | Pin |
| `pin_not_driven` | Input pin not driven by any Output pins | Error | Input pin |
| `power_pin_not_driven` | Input Power pin not driven by any Output Power pins | Error | Power input pin |
| `pin_to_pin` | Conflict problem between pins | Matrix | Pin |
| `label_dangling` | Label not connected | Error | Label anchor |
| `isolated_pin_label` | Label connected to only one pin | Warning | Label anchor |
| `unconnected_wire_endpoint` | Unconnected wire endpoint | Warning | Wire end |
| `no_connect_connected` | (KiCad text, verify in 4c-3) | Warning | Flag |
| `no_connect_dangling` | (KiCad text, verify in 4c-3) | Warning | Flag |

- Inputs are driven by output, bidirectional, tri-state, passive, and power output pins. Open collector and open emitter pins don't drive. Power inputs need a power output, which a `PWR_FLAG` provides.
- A no-connect flag on a pin satisfies `pin_not_connected` and `isolated_pin_label`. Duplicate references exclude `#` references and units of one multi-unit symbol.
- The default pin matrix, from KiCad's `erc_settings.cpp` (O is OK, W is warning, E is error), with rows and columns in this order: input, output, bidirectional, tri-state, passive, not internally connected, unspecified, power input, power output, open collector, open emitter, no connect.

```text
      I  O  Bi 3S Pa NI Un PI PO OC OE NC
I     O  O  O  O  O  O  W  O  O  O  O  E
O     O  E  O  W  O  O  W  O  E  E  E  E
Bi    O  O  O  O  O  O  W  O  W  O  W  E
3S    O  W  O  O  O  O  W  W  E  W  W  E
Pa    O  O  O  O  O  O  W  O  O  O  O  E
NI    O  O  O  O  O  O  O  O  O  O  O  E
Un    W  W  W  W  W  O  W  W  W  W  W  E
PI    O  O  O  W  O  O  W  O  O  O  O  E
PO    O  E  W  E  O  O  W  O  E  E  E  E
OC    O  E  O  W  O  O  W  O  E  O  O  E
OE    O  E  W  W  O  O  W  O  E  O  O  E
NC    E  E  E  E  E  E  E  E  E  E  E  E
```

- The ERC panel runs the check, lists violations by severity, and pans the canvas to a violation when it's clicked.

**4c-4. Exclusions and severities.** `.kicad_pro` is read as JSON with every key kept. Only the ERC section changes: `rule_severities` and the exclusion list. An `EditErcSettings` command makes changes undoable and marks the project unsaved. The exact KiCad 10 exclusion string identity must be verified against a KiCad-written `.kicad_pro` before 4c-4 is built.

## Related blueprints

### Required

- [project-files](project-files.md) — the canvas shell and the command model.
- [component-library](component-library.md) — where symbols come from.

### Impact checks

- [circuit-simulation](circuit-simulation.md) — when netlist output changes.
- [board-layout](board-layout.md) — when net identity changes.

## Relevant implementation and tests

- `crates/core/src/symbol.rs`, `kicad_value.rs` — symbol geometry extraction, shared with `crates/library`
- `crates/core/src/kicad.rs` — placed symbols, `(lib_symbols)`, and placement transforms. `session.rs` holds the symbol commands. Tests: `crates/core/tests/symbol_placement.rs` and `tests/golden/symbols/amp.kicad_sch`
- `src-tauri/src/ipc.rs` — `place_symbol`, `move_symbols`, `transform_symbols`, `delete_symbols`, and the `definitions` and `instances` fields on `ProjectSnapshot` and `DocumentDiff`
- `ui/src/components/SchematicCanvas.tsx`, `symbols.ts`, `schematicDraw.ts`, `drawShapes.ts` — the placement and selection interactions and drawing. Tests: `App.test.tsx` ("Symbol placement"), `symbols.test.ts`
- `crates/core/src/wiring.rs` — wires, junctions, labels, and the split and junction planner. `netlist.rs` derives nets. `session.rs` holds the `EditItems` wiring commands. Tests: `crates/core/tests/netlist.rs` and `tests/golden/wiring/divider.kicad_sch`
- `src-tauri/src/ipc.rs` — `draw_wire`, `place_label`, `move_wiring`, `rotate_labels`, `delete_wiring`, symbol `pins`, and the `wires`, `junctions`, and `labels` fields
- `ui/src/wiring.ts`, `powerSymbols.ts`, `components/InlineLabelEditor.tsx`, `components/PowerPicker.tsx` — wiring geometry, the label popover, and the power picker. Tests: `App.test.tsx` ("Wiring"), `wiring.test.ts`

## Acceptance or verification criteria

- [x] Symbols can be placed from the library, rotated, mirrored, moved, and deleted.
- [x] Wires snap to the grid and to pins. Junctions appear automatically at T-intersections.
- [ ] The netlist of a reference KiCad schematic matches KiCad's own netlist export.
- [ ] ERC reports each rule violation with a jump-to-location action.
- [ ] Editing a 300-symbol schematic holds 60 frames per second for pan and drag.

## Remaining gaps and unknowns

- Support for hierarchical sheets is deferred until after slice 4.
- Placement in the installed app has not been checked by hand. CI covers it only in jsdom. Opening a saved 4a schematic in KiCad itself has not been verified either.
- Rotated property text keeps a plain angle. KiCad's normalization of text angle into justification is not applied, so text may sit at 180° or 270°.
- Placed symbols don't copy `in_bom`, `on_board`, `exclude_from_sim`, or `dnp`. Those flags are read from the embedded definition.
- The KiCad netlist comparison is pending. `the_netlist_matches_the_kicad_export` stays ignored until the user exports `tests/golden/wiring/divider.net` from KiCad: copy `divider.kicad_sch` into a KiCad project named `divider`, export a KiCad netlist, commit it, and remove the `#[ignore]`.
- Collinear wire pieces are not merged after edits. Moving or placing a symbol does not split wires or update junctions at its pins. Connectivity is recomputed in full on each wiring edit, not incrementally.
- Label hit-testing approximates the text extent from its character count.
- The power picker's "Search library..." only opens the library sidebar. Its final behavior is waiting on a designer decision.
- Wiring in the installed app has not been checked by hand.
