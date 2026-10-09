# Circuit simulation

## Outcome or responsibility

The user runs DC operating point, transient, and AC analyses on their schematic with real ngspice, and reads the results as waveforms, Bode plots, and annotated node voltages. Convergence problems are explained, not just reported.

## Current verified status

**Status:** Planned

## Architecture dependencies

- [Simulate a schematic](../architecture.md#simulate-a-schematic)
- [Connectivity and netlist](../architecture.md#connectivity-and-netlist)
- [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation)
- [Error and recovery](../architecture.md#error-and-recovery)

## Local rules and implications

### Simulate a schematic

`sim` loads `libngspice` dynamically and runs one session per simulation on a worker thread. Cancelling halts the background thread through the ngspice API.

### Connectivity and netlist

Each symbol's `Sim.*` properties (KiCad convention) map it to a SPICE device or model. A symbol with no simulation mapping is a clear pre-flight error, never silently skipped.

### Coordinate and unit representation

Results are `f64` vectors in SI units. SI prefixes appear only in the UI.

### Error and recovery

When a run fails to converge, `sim` retries with gmin stepping and then source stepping before failing. The error names the node or device involved when ngspice reports one.

## Related blueprints

### Required

- [schematic-capture](schematic-capture.md) — the netlist source and symbol simulation properties.

### Impact checks

- [ai-design](ai-design.md) — when the simulation assertion API changes.

## Relevant implementation and tests

- `crates/sim` — Planned

## Acceptance or verification criteria

- [ ] An RC low-pass transient matches the analytic response within 1% in a CI test using real ngspice.
- [ ] A voltage divider `.op` matches the analytic value exactly to 6 significant figures.
- [ ] The `.ac` −3 dB point of an RC filter matches 1/(2πRC) within 1%.
- [ ] Probing a wire or pin plots its signal, and plots support zoom, cursors, and measurement readouts.

## Remaining gaps and unknowns

- `libngspice` is BSD-licensed with some GPL components. Confirm which build is redistributable before slice 5.
- Encrypted vendor models (PSpice) are not supported.
