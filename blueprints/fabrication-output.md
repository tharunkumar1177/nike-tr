# Fabrication output

## Outcome or responsibility

The user exports everything a board house needs: Gerber RS-274X layers, Excellon drill files, a bill of materials, and a pick-and-place file, bundled as a zip ready for upload.

## Current verified status

**Status:** Planned

## Architecture dependencies

- [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation)
- [Verification boundaries](../architecture.md#verification-boundaries)

## Local rules and implications

### Coordinate and unit representation

Gerber output uses millimeters with format 4.6, which represents nanometer integers exactly. Nothing is rounded through floating point.

### Verification boundaries

Each writer has golden-file tests on reference boards. The Gerber output is also parsed back by an independent reader in CI.

## Related blueprints

### Required

- [board-layout](board-layout.md) — the source geometry and layers.

### Impact checks

None

## Relevant implementation and tests

- `crates/fab` — Planned

## Acceptance or verification criteria

- [ ] Exports copper, mask, paste, silkscreen, and edge-cut Gerbers with X2 attributes.
- [ ] Exports Excellon drill files for plated and non-plated holes.
- [ ] Exports a BOM as CSV grouped by value and footprint, and a pick-and-place file with rotation per side.
- [ ] Golden tests pass, and a re-parse of the output yields geometry identical to the source.

## Remaining gaps and unknowns

- IPC-2581 and ODB++ are out of scope.
