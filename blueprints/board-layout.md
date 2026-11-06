# Board layout

## Outcome or responsibility

The user turns a schematic into a physical board. They pull footprints and nets in from the schematic, define the outline and stackup, place parts guided by the ratsnest, route tracks and vias, pour copper zones, and fix design rule violations.

## Current verified status

**Status:** Planned

Slice 7 is in progress, starting with 7a (footprint definitions in the library index).

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

- `crates/core` — Planned
- `ui/src` — Planned

## Acceptance or verification criteria

- [ ] Updating the board from the schematic adds, removes, and renames footprints and nets through a reviewable diff.
- [ ] The ratsnest shows the minimum spanning tree for each net and updates live while parts move.
- [ ] The user can route tracks at 45 and 90 degrees with layer switching through vias.
- [ ] Zones fill with clearance and thermal reliefs, and isolated islands are removed.
- [ ] DRC detects clearance, width, annular ring, unrouted net, and board edge violations, verified by golden test boards.

## Remaining gaps and unknowns

- A golden `.kicad_pcb` written by KiCad 10 after Update PCB from Schematic will be supplied by the user later. Until then its comparison test ships ignored, like the `divider.net` netlist test.
- Push-and-shove interactive routing is out of scope for slice 8. Walkaround routing comes first.
