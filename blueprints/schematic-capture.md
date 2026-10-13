# Schematic capture

## Outcome or responsibility

The user draws a circuit by placing library symbols, connecting them with wires, junctions, net labels, and power symbols, and editing component properties. Electrical rule checks flag mistakes. The derived netlist feeds simulation, the board, and AI verification.

## Current verified status

**Status:** Partial

Slice 4a is green in [CI run 38406455082](https://github.com/tharunkumar1177/tron/actions/runs/38406455082), including the Windows installer. Covered by tests:

- Core: placed symbols and `(lib_symbols)` round-trip a hand-written KiCad 10 golden file byte for byte. Place, move, rotate or mirror, and delete each undo and redo exactly. Definitions are added on first use and removed with the last instance.
- IPC: payload serialization, definition lookup from the fixture index, an unknown symbol ID, and a diff walk through every command.
- UI: Vitest covers placing from the library browser with the snapped position, R during placement, Escape, select and Delete, a drag that sends one `move_symbols`, and rendering instances from a diff. Unit tests cover snapping, transforms, bounds, and hit-testing.

Slice 4b (wires, labels, properties, netlist, and ERC) is Planned.

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

## Acceptance or verification criteria

- [x] Symbols can be placed from the library, rotated, mirrored, moved, and deleted.
- [ ] Wires snap to the grid and to pins. Junctions appear automatically at T-intersections.
- [ ] The netlist of a reference KiCad schematic matches KiCad's own netlist export.
- [ ] ERC reports each rule violation with a jump-to-location action.
- [ ] Editing a 300-symbol schematic holds 60 frames per second for pan and drag.

## Remaining gaps and unknowns

- Support for hierarchical sheets is deferred until after slice 4.
- Placement in the installed app has not been checked by hand. CI covers it only in jsdom. Opening a saved 4a schematic in KiCad itself has not been verified either.
- Rotated property text keeps a plain angle. KiCad's normalization of text angle into justification is not applied, so text may sit at 180° or 270°.
- Placed symbols don't copy `in_bom`, `on_board`, `exclude_from_sim`, or `dnp`. Those flags are read from the embedded definition.
