# Board layout

## Outcome or responsibility

The user turns a schematic into a physical board. They pull footprints and nets in from the schematic, define the outline and stackup, place parts guided by the ratsnest, route tracks and vias, pour copper zones, and fix design rule violations.

## Current verified status

**Status:** Planned

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

- Push-and-shove interactive routing is out of scope for slice 8. Walkaround routing comes first.
