# Schematic capture

## Outcome or responsibility

The user draws a circuit by placing library symbols, connecting them with wires, junctions, net labels, and power symbols, and editing component properties. Electrical rule checks flag mistakes. The derived netlist feeds simulation, the board, and AI verification.

## Current verified status

**Status:** Planned

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

## Related blueprints

### Required

- [project-files](project-files.md) — the canvas shell and the command model.
- [component-library](component-library.md) — where symbols come from.

### Impact checks

- [circuit-simulation](circuit-simulation.md) — when netlist output changes.
- [board-layout](board-layout.md) — when net identity changes.

## Relevant implementation and tests

- `crates/core` — Planned
- `ui/src` — Planned

## Acceptance or verification criteria

- [ ] Symbols can be placed from the library, rotated, mirrored, moved, and deleted.
- [ ] Wires snap to the grid and to pins. Junctions appear automatically at T-intersections.
- [ ] The netlist of a reference KiCad schematic matches KiCad's own netlist export.
- [ ] ERC reports each rule violation with a jump-to-location action.
- [ ] Editing a 300-symbol schematic holds 60 frames per second for pan and drag.

## Remaining gaps and unknowns

- Support for hierarchical sheets is deferred until after slice 4.
