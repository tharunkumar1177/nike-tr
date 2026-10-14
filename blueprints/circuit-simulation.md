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

### File format and IPC contract

The analysis settings live in `tron.json` (see [File format](../architecture.md#file-format)), so a project opened in KiCad has no simulation command. Result vectors stay in the Rust session, and the waveform viewer fetches decimated slices for its visible range (see [IPC contract](../architecture.md#ipc-contract)).

### Error and recovery

When a run fails to converge, `sim` retries with gmin stepping and then source stepping before failing. The error names the node or device involved when ngspice reports one.

## Related blueprints

### Required

- [schematic-capture](schematic-capture.md) — the netlist source and symbol simulation properties.

### Impact checks

- [ai-design](ai-design.md) — when the simulation assertion API changes.

## Relevant implementation and tests

- `crates/sim` — Planned

## Slice 5 plan (approved 2026-10-14)

Four sequential PRs on branches named `slice-5a-...` to `slice-5d-...`:

1. **5a. SPICE deck export and pre-flight checks.** Add `crates/sim`, which depends on `tron-core`. Map each symbol to a SPICE device from its `Sim.*` fields, or else from its reference prefix (R, C, L, V, I) and `Value`. Parse values with SI suffixes, `Meg`, and the `4k7` form. Ground nets become node `0`. Pre-flight checks report a missing ground, a symbol with no simulation model, and a missing analysis. Unit tests cover the deck text and every pre-flight error.
2. **5b. ngspice FFI and CI integration.** Write a custom `libloading` wrapper for `ngSpice_Init`, `ngSpice_Circ`, `ngSpice_Command`, `ngSpice_CurPlot`, `ngSpice_AllVecs`, and `ngGet_Vec_Info`. CI downloads the pinned ngspice 47 archive, checks its SHA-256, and extracts it. Integration tests run real ngspice: a divider `.op` matches to 6 significant figures, and an RC low-pass `.tran` matches the analytic response within 1%.
3. **5c. Session and IPC.** One serialized worker runs each simulation and can be cancelled with `bg_halt`. Add the commands `start_simulation`, `cancel_simulation`, and `fetch_waveform` (min-max decimation), a progress event, the pre-flight `AppErrorKind`s, and analysis settings in `tron.json` through the undo stack.
4. **5d. Bundling and UI.** Ship `ngspice.dll`, `spinit`, the code models, and `COPYING` as Tauri resources. Add the probe tool, `.op` annotations, and a waveform viewer with zoom, two cursors with a delta readout, and measurements (min, max, average, RMS, peak-to-peak). This needs a designer plan first.

## Acceptance or verification criteria

- [ ] (5a) The SPICE deck for a divider and an RC circuit is exact text, and each pre-flight error is reported with the symbol or setting involved.
- [ ] An RC low-pass transient matches the analytic response within 1% in a CI test using real ngspice.
- [ ] A voltage divider `.op` matches the analytic value exactly to 6 significant figures.
- [ ] The `.ac` −3 dB point of an RC filter matches 1/(2πRC) within 1%.
- [ ] Probing a wire or pin plots its signal, and plots support zoom, cursors, and measurement readouts.

## Remaining gaps and unknowns

- Encrypted vendor models (PSpice) are not supported.

### Slice 5 research (2026-10-14, unverified until CI confirms)

- **License:** the official Windows shared library build is Modified BSD (base, CIDER, and XSPICE) and can be redistributed. The installer must ship the ngspice `COPYING` notice.
- **Distribution:** ngspice 47 is the latest stable release. The archive is `https://sourceforge.net/projects/ngspice/files/ng-spice-rework/47/ngspice-47_dll_64.7z/download`, a 7z file holding `ngspice.dll`, `spinit`, and the `*.cm` code models. No SHA-256 is published, so CI computes it on the first fetch and the value is then pinned. The DLL's path inside the archive must be confirmed in the first CI run.
- **API:** `sharedspice.h` provides `ngSpice_Init` (with the `SendChar`, `SendStat`, `ControlledExit`, `SendData`, `SendInitData`, and `BGThreadRunning` callbacks), `ngSpice_Circ`, `ngSpice_Command` (`bg_run`, `bg_halt`), `ngGet_Vec_Info`, `ngSpice_CurPlot`, and `ngSpice_AllVecs`. ngspice keeps global state, so only one instance can run per process, and runs must be serialized. The callbacks fire on ngspice's background thread. ngspice must be able to find `spinit` and the code models, either through `SPICE_LIB_DIR` or through `ngSpice_nospinit` and `ngCM_Input_Path`. Convergence failures are reported only as text through `SendChar`.
- **KiCad conventions:** the `Sim.Device`, `Sim.Type`, `Sim.Params`, `Sim.Pins`, `Sim.Library`, and `Sim.Name` fields map a symbol to SPICE. Without them, the device is inferred from the reference prefix (R, C, L, V, I) and the `Value`. Values accept SI suffixes, `Meg` for 10^6 (SPICE `m` means milli), and the `4k7` form for 4.7k. Ground is node `0`, and named nets get a `/` prefix (for example `/OUT`). The `.tran` analysis takes step, stop, start, max step, and UIC. The probe tool plots `V(net)` for a wire and `I(device)` for a pin, and probed signals persist across runs. `.op` results are annotated on the schematic.
- **Known weaknesses in KiCad's simulator:** "timestep too small" usually hides a floating node or a missing model, so pre-flight checks matter. Users ask for a dual-cursor delta readout and a measurement table (min, max, average, RMS, peak-to-peak).
