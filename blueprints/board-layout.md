# Board layout

## Outcome or responsibility

The user turns a schematic into a physical board. They pull footprints and nets in from the schematic, define the outline and stackup, place parts guided by the ratsnest, route tracks and vias, pour copper zones, and fix design rule violations.

## Current verified status

**Status:** In progress

7a is done: the index stores footprint sources ([PR #29](https://github.com/tharunkumar1177/tron/pull/29)), and release `library-kicad-10.0.7-r3` is pinned ([PR #30](https://github.com/tharunkumar1177/tron/pull/30)). 7b merged in [PR #31](https://github.com/tharunkumar1177/tron/pull/31). `crates/core/src/board.rs` types the `(layers)` table, `setup.stackup`, and the `Edge.Cuts` outline, and `crates/core/src/footprint.rs` types footprints and pads. Opening an older board upgrades it in memory: it gets the KiCad 10 header and layer ordinals, and numbered nets become named nets. New projects get KiCad's 2-layer table and stackup, and saving refuses coordinates outside KiCad's 32-bit nanometer range.

7c merged in [PR #32](https://github.com/tharunkumar1177/tron/pull/32). `crates/core/src/board_update.rs` plans and applies Update Board from Schematic. It matches by `path` UUID, groups multi-unit symbols by reference, and skips power symbols, `(on_board no)` symbols, and symbols with an empty `Footprint`. It reports unannotated symbols and footprints missing from the library as problems, keeps `board_only` footprints, and packs new footprints in 150 mm rows with 2.54 mm gaps, 10 mm below the outline. Replacing a footprint keeps its position, rotation, and side. `crates/core/src/ratsnest.rs` builds each net's minimum spanning tree over a `spade` Delaunay triangulation and recomputes only nets whose pads changed. Core also has `move_footprints` and `transform_footprints` (rotate and flip) as single undo steps, ready for 7d. The IPC has `plan_board_update` and `apply_board_update(base_revision)` with `boardUpdateConflict`, the board in the snapshot, and the board fields and changed-net ratsnest in the diff. `Index::footprint_definition_named` looks footprints up by library and name.

7d merged in [PR #33](https://github.com/tharunkumar1177/tron/pull/33), following the designer plan. A Schematic and Board switcher sits at the left of the toolbar, and View > Switch Schematic/Board (Ctrl+Tab) toggles it. Each view keeps its own viewport, and the board fits its outline and footprints on first show. `ui/src/components/BoardCanvas.tsx` draws the `Edge.Cuts` outline, back then front pads by shape with drill holes, Reference above and Value below each footprint, and the ratsnest. Clicking selects a footprint; dragging moves it on a 0.635 mm grid with one `move_footprints` on release, R rotates it clockwise, F flips it, and Escape clears the selection. Ratsnest ends on the dragged pads follow the drag locally. Tools > Update Board from Schematic (F8) opens a dialog that plans, lists problems, additions, removals, replacements, text changes, and pad net changes, re-plans when an option changes, and applies the reviewed revision. A conflict re-plans with a notice, a missing library offers the download, and a successful update shows the board. The snapshot's pads carry their shape, size, angle, corner radius, and drill, and each footprint carries board bounds from its courtyard.

7e is green in CI, including the Windows installer, on branch `slice-7e-outline-stackup`, and closes slice 7. Tools > Board Outline... edits the `Edge.Cuts` outline as a rectangle (width and height from its corner) or as a typed polygon of 3 to 1024 corners; a new corner goes halfway along the closing edge. The dialog opens an existing rectangle, polygon, or closed loop of lines as it is, and warns that arcs, circles, or separate parts are replaced by their bounding rectangle. An empty board starts from its footprints' box plus 10 mm, or a 100 mm square. `crates/core/src/outline.rs` checks the polygon exactly in integers (at least three distinct corners, KiCad's coordinate range, a nonzero area, no crossing or touching edges) and writes one `gr_poly` with a 0.05 mm stroke. `Session::edit_board_outline` replaces every `Edge.Cuts` graphic in one "Edit Board Outline" undo step, and applying the same single polygon is a no-op. Tools > Layer Stackup... picks 2, 4, 6, or 8 copper layers and previews the resulting stackup. `Session::edit_stackup` rewrites `(layers)`, `setup.stackup`, and `general.thickness` in place as one "Edit Stackup" step. It keeps the user names and types of copper layers that remain, every non-copper layer, the copper finish, and the other `setup` keys. It refuses to drop an inner layer that a footprint, track, via, zone, or graphic still uses, naming that layer. A board without those header nodes gets them in KiCad's header order.

## Architecture dependencies

- [Schematic to board](../architecture.md#schematic-to-board)
- [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation)
- [Design document ownership](../architecture.md#design-document-ownership)
- [IPC contract](../architecture.md#ipc-contract)

## Local rules and implications

### Schematic to board

New footprints from an update are placed in a cluster outside the board outline. Removed parts are listed in the diff dialog before deletion.

### Coordinate and unit representation

All DRC math uses integer nanometers. Clearance checks use an R-Tree broad phase followed by exact polygon distance.

### Design document ownership

An interactive route is previewed in the UI and committed as one command when the segment is finished.

### IPC contract

DRC runs incrementally on changed items during editing, and fully on demand. Violations are sent to the UI as a list of items with their locations.

### Slice 7 plan

Approved by the user on 2026-11-06. Slice 7 ships as five sub-slices, each its own PR:

1. **7a. Footprint definitions in the index.** Schema version 3 stores each footprint's source, and `Index::footprint_definition` returns it as a board embeds it. A library run on `main` publishes `library-kicad-10.0.7-r3`, and its workflow pull request pins it. See [component-library](component-library.md).
2. **7b. Typed board model.** New `crates/core` board and footprint modules type the footprints (UUID, `lib:name`, position, rotation, side, Reference and Value, `(path "/<symbol-uuid>")`, and pads with net names), the `Edge.Cuts` graphics, the `(layers)` table, and `setup.stackup`. Everything else (zones, tracks, vias, 3D models, other `setup` keys) stays as raw S-expressions. The writer emits KiCad 10 (`version 20260206`, `generator_version "10.0"`, the KiCad 10 layer ordinals: `F.Cu` 0, `B.Cu` 2, `In1.Cu` 4, `Edge.Cuts` 25). The reader also accepts the older numbered net table and `(net 1 "X")` pad nets. Coordinates are checked against KiCad's 32-bit nanometer range on export.
3. **7c. Update engine and ratsnest.** `board_update.rs` matches symbols to footprints by the footprint's `path` UUID and produces a `BoardUpdatePlan`: additions, removals, replacements, reference and value changes, pad net changes, and per-symbol errors. A symbol with an empty `Footprint` property is skipped. A footprint missing from the index is listed as an error, not a failure. The options "Delete footprints with no symbols" and "Replace footprints with those specified by symbols" are both on by default. Re-linking by reference is left out. Applying the plan is one `Command::Group`, "Update Board from Schematic". New footprints are packed in rows below the outline's bounding box, or below the origin when there is no outline, ordered by reference. `ratsnest.rs` builds, per net, union-find clusters of connected copper (each pad is its own cluster until slice 8 adds tracks), a Delaunay triangulation of the anchors with `spade`, and a Kruskal minimum spanning tree with a deterministic tie-break on length and then endpoint ids. Each net gets clusters − 1 edges, and only changed nets are recomputed.
4. **7d. Board UI** (designer plan first). A view switcher in `EditorShell`, a `BoardCanvas`, the update dialog listing every change, moving, rotating, and flipping footprints, and the ratsnest overlay. While a footprint is dragged, the UI shifts its ratsnest lines locally, and Rust recomputes them on drop.
5. **7e. Outline and stackup.** `edit_board_outline` rejects an outline that is not one closed polygon. The stackup dialog supports any even copper count from 2 to 8 and rewrites `(layers)` and `setup.stackup` with KiCad's defaults: 1.6 mm total, 0.035 mm copper, 0.01 mm masks, and FR4 dielectrics (εr 4.5, loss tangent 0.02) of (1.6 − 0.035·copper layers − 0.02) / dielectric layers each.

Approved contract changes, written into [architecture.md](../architecture.md) by the sub-slice that implements them:

- IPC: `ProjectSnapshot` gains the board, and `DocumentDiff` gains `footprints`, `boardOutline`, `stackup`, and `ratsnest` (changed nets only). New commands are `plan_board_update`, `apply_board_update(base_revision)`, `move_footprints`, `transform_footprints`, `edit_board_outline`, and `edit_stackup`. A stale `apply_board_update` fails with the new error kind `boardUpdateConflict`.
- File format: Tron edits `.kicad_pcb` and manages its `(layers)` table and `setup.stackup`.
- Coordinates: pad positions at arbitrary angles are computed with floating-point trigonometry and rounded to the nearest nanometer when the command runs. The document stays integer.
- Dependencies: `spade` for the Delaunay triangulation, and later its R-Tree for DRC.

The whole board is sent in the snapshot when a project opens. Viewport-scoped board reads wait for slice 8.

## Related blueprints

### Required

- [schematic-capture](schematic-capture.md) — the source of nets and parts.
- [component-library](component-library.md) — where footprints come from.

### Impact checks

- [fabrication-output](fabrication-output.md) — when board geometry or layers change.
- [autorouting](autorouting.md) — when the board schema changes.

## Relevant implementation and tests

- `crates/core/src/board.rs`, `crates/core/src/footprint.rs` — Implemented (7b), with unit tests for the upgrade, round trip, layers, stackups, outline shapes, and footprint edits
- `crates/core/tests/project_files.rs` — Implemented: an upgraded board reopens unchanged, and an out-of-range board is not saved
- `crates/core/src/board_update.rs`, `crates/core/src/ratsnest.rs` — Implemented (7c), with unit tests for reference order, row packing, courtyard extents, spanning trees (square, collinear, stacked, 40 points), and incremental ratsnest updates
- `crates/core/tests/board_update.rs` — Implemented: the golden divider with the real KiCad 10 `R_0603_1608Metric` footprint gets three footprints with their nets, the `/OUT` ratsnest line, undo and redo, a no-op re-plan, save and reopen, value changes, removal, missing footprints, and footprint move, rotate, and flip. The KiCad 10 comparison test is ignored until the golden board exists.
- `src-tauri/src/board.rs`, `src-tauri/src/ipc.rs` — Implemented (7c): board IPC types and the plan and apply commands, with serialization tests
- `ui/src/documentDiff.ts` — Implemented: applies the board fields and replaces the changed nets' ratsnest lines
- `crates/library/src/kicad_mod.rs` — Implemented (7d): `placed_geometry` reads a placed footprint's pads and courtyard extent, with unit tests for courtyard extents and footprints without a courtyard
- `src-tauri/src/board.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/shell.rs` — Implemented (7d): pad geometry and footprint bounds in the snapshot, `move_footprints` and `transform_footprints`, and the Switch Schematic/Board and Update Board from Schematic menu items, with serialization tests
- `ui/src/board.ts`, `ui/src/boardDraw.ts`, `ui/src/components/BoardCanvas.tsx`, `ui/src/components/BoardUpdateDialog.tsx` — Implemented (7d): board fitting, picking, pad rounding, and the drag-shifted ratsnest, unit tested in `ui/src/board.test.ts`; the view switch, footprint select, drag, rotate, flip, empty-board hint, and every dialog state are tested in `ui/src/App.test.tsx`
- `crates/core/src/outline.rs`, `crates/core/src/board.rs`, `crates/core/src/session.rs` — Implemented (7e): outline validation and the `gr_poly` writer, the stackup rewrite, and the two edits, with unit tests for duplicate corners, range, zero area, crossing, folding, and touching edges
- `crates/core/tests/board_setup.rs` — Implemented (7e): drawing, replacing, and re-applying an outline with undo, redo, and save; an in-place replacement that undoes exactly; growing 2 to 4 to 6 layers while keeping user names, non-copper layers, the finish, and `setup` keys; refusing invalid counts and a used inner layer; and a bare board getting its header nodes in order
- `src-tauri/src/ipc.rs`, `src-tauri/src/shell.rs` — Implemented (7e): `edit_board_outline` and `edit_stackup`, and the Board Outline and Layer Stackup menu items, with a diff serialization test
- `ui/src/boardSetup.ts`, `ui/src/components/BoardOutlineDialog.tsx`, `ui/src/components/LayerStackupDialog.tsx` — Implemented (7e): reading the current outline, the polygon checks, and the stackup preview, unit tested in `ui/src/boardSetup.test.ts`; both dialogs' validation, apply, and refusal paths are tested in `ui/src/App.test.tsx`
- Board canvas drawing in the installed app — Not verified: jsdom has no canvas, so the drawing needs a manual check in the installer

## Acceptance or verification criteria

- [x] Updating the board from the schematic adds, removes, and renames footprints and nets through a reviewable diff.
- [x] The ratsnest shows the minimum spanning tree for each net and updates live while parts move.
- [ ] The user can route tracks at 45 and 90 degrees with layer switching through vias.
- [ ] Zones fill with clearance and thermal reliefs, and isolated islands are removed.
- [ ] DRC detects clearance, width, annular ring, unrouted net, and board edge violations, verified by golden test boards.

## Remaining gaps and unknowns

- A golden `.kicad_pcb` written by KiCad 10 after Update PCB from Schematic will be supplied by the user later. Until then its comparison test ships ignored, like the `divider.net` netlist test.
- Push-and-shove interactive routing is out of scope for slice 8. Walkaround routing comes first.
