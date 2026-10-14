# Circuit simulation

## Outcome or responsibility

The user runs DC operating point, transient, and AC analyses on their schematic with real ngspice, and reads the results as waveforms, Bode plots, and annotated node voltages. Convergence problems are explained, not just reported.

## Current verified status

**Status:** Partial

Slice 5a is built and green in [CI 38576288344](https://github.com/tharunkumar1177/tron/actions/runs/38576288344). `crates/sim` builds the SPICE deck from the core netlist and runs the pre-flight checks. Slice 5b is green in [CI 38585943376](https://github.com/tharunkumar1177/tron/actions/runs/38585943376): `crates/sim` loads the pinned ngspice 46 DLL and runs `.op` and `.tran` against it. There is no session, IPC, or UI yet (5c and 5d).

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

- `crates/sim/src/deck.rs` — the deck builder: ground nets (`GND`, `0`) become node `0`, net names are made safe for ngspice and kept distinct regardless of case, and symbols that are excluded from simulation (`exclude_from_sim` or `Sim.Enable` set to `0`) or are `#` symbols are skipped. Implemented.
- `crates/sim/src/model.rs` — maps a symbol to an R, C, L, V, or I element from `Sim.Device`, `Sim.Type` (DC, SIN, PULSE), `Sim.Params`, and `Sim.Pins`, or infers it from the reference prefix and `Value`. A `Sim.Library` model is a clear error for now. Implemented.
- `crates/sim/src/value.rs` — parses SI values and writes exact round-trip numbers for SPICE. Implemented.
- `crates/sim/src/analysis.rs` — the `.op` and `.tran` directives and their range checks. Implemented.
- `crates/core/src/kicad.rs` `Schematic::symbol_flag` — reads `(exclude_from_sim yes)` on a placed symbol.
- `crates/sim/src/ngspice.rs` — the `libloading` wrapper around the ngspice shared API. The library is loaded and initialized once per process, and `Engine::lock` serializes runs through a static mutex. Each run resets ngspice (`destroy all`, `remcirc`), loads the deck, runs it in the foreground, and collects every real vector of the current plot. Errors come from ngspice's stderr lines and its exit callback. A second library path in the same process is rejected. Implemented.
- Tests: unit tests in each `crates/sim/src` module, and `crates/sim/tests/deck.rs`, which builds KiCad-format divider and RC schematics and checks the exact deck text and every pre-flight error. `crates/sim/tests/ngspice.rs` runs real ngspice from `TRON_NGSPICE`, which CI sets after downloading and checking the pinned archive.

## Slice 5 plan (approved 2026-10-14)

Four sequential PRs on branches named `slice-5a-...` to `slice-5d-...`:

1. **5a. SPICE deck export and pre-flight checks.** Add `crates/sim`, which depends on `tron-core`. Map each symbol to a SPICE device from its `Sim.*` fields, or else from its reference prefix (R, C, L, V, I) and `Value`. Parse values with SI suffixes, `Meg`, and the `4k7` form. Ground nets become node `0`. Pre-flight checks report a missing ground, a symbol with no simulation model, and a missing analysis. Unit tests cover the deck text and every pre-flight error.
2. **5b. ngspice FFI and CI integration.** Write a custom `libloading` wrapper for `ngSpice_Init`, `ngSpice_Circ`, `ngSpice_Command`, `ngSpice_CurPlot`, `ngSpice_AllVecs`, and `ngGet_Vec_Info`. CI downloads the pinned ngspice 46 archive, checks its SHA-256, and extracts it. Integration tests run real ngspice: a divider `.op` matches to 6 significant figures, and an RC low-pass `.tran` matches the analytic response within 1%.
3. **5c. Session and IPC.** One serialized worker runs each simulation and can be cancelled with `bg_halt`. Add the commands `start_simulation`, `cancel_simulation`, and `fetch_waveform` (min-max decimation), a progress event, the pre-flight `AppErrorKind`s, and analysis settings in `tron.json` through the undo stack.
   **5c step plan (architect, 2026-10-14).**
   - **Settings.** `crates/core/src/manifest.rs` owns the serde type `AnalysisSettings` (`OperatingPoint`, or `Transient { step, stop, start, max_step, initial_conditions }`) in an optional `simulation` key. `crates/sim` converts it to `Analysis`. Files without the key still load. The edit is an undoable session command that marks the project dirty. `f64` fields mean `Manifest`, `Changes`, and `Command` drop `Eq`.
   - **Engine.** One run at a time. The calling thread holds the engine guard, resets ngspice, loads the deck, sends `bg_run`, and waits on a condition variable. That variable is signalled by the `BGThreadRunning` and `ControlledExit` callbacks, which never call back into ngspice. The same thread then reads the errors and copies the plot. `halt()` sends `bg_halt` without the guard, which is safe while the runner only waits. A halted run returns `EngineError::Cancelled` and its partial plot is discarded. Progress percentages parsed from `SendStat` go to a process-wide sink that `crates/sim` exposes, so it stays independent of Tauri.
   - **Decimation.** `decimate(x, y, min_x, max_x, budget) -> Vec<Point>`. A budget below 2 is raised to 2. `min_x > max_x` returns nothing. A single point is returned as is. NaN points are dropped. If x is not increasing, the range filter is skipped and the data is chunked by index. Otherwise each of `budget / 2` chunks contributes its minimum and maximum, in x order.
   - **IPC (`src-tauri/src/sim.rs`).**
     - `start_simulation` is async. It resolves with `SimulationFinished { run_id, vectors }` or an `AppError`, and emits `simulation-progress { percentage }` while it runs.
     - `cancel_simulation()` succeeds even when nothing is running.
     - `fetch_waveform(run_id, vector, min_x, max_x, budget)` returns `Vec<Point { x, y }>`.
     - `get_analysis_settings` and `edit_analysis_settings` read and change the settings. The project snapshot and document diff gain a settings field with the 5d UI that uses it.
     - Any published document change drops the stored results.
     - ngspice is found through `TRON_NGSPICE`, then the resource `ngspice/ngspice.dll`.
   - **Errors.** One `AppErrorKind` per pre-flight problem (`SimNoAnalysis`, `SimInvalidAnalysis`, `SimNoGround`, `SimNoModel`, `SimUnannotated`, `SimDuplicateReference`, `SimNetlist`, `SimEmpty`), plus `SimEngineLoad`, `SimCircuitRejected`, `SimAnalysisFailed`, `SimRunning`, `SimCancelled`, and `SimNotFound`.
4. **5d. Bundling and UI.** Ship `ngspice.dll`, `spinit`, the code models, and `COPYING` as Tauri resources. Add the probe tool, `.op` annotations, and a waveform viewer with zoom, two cursors with a delta readout, and measurements (min, max, average, RMS, peak-to-peak). This needs a designer plan first.

## Acceptance or verification criteria

- [x] (5a) The SPICE deck for a divider and an RC circuit is exact text, and each pre-flight error is reported with the symbol or setting involved. Verified by `crates/sim/tests/deck.rs`.
- [x] (5b) An RC low-pass transient matches the analytic response within 1% in a CI test using real ngspice. Verified by `crates/sim/tests/ngspice.rs` in [CI 38585943376](https://github.com/tharunkumar1177/tron/actions/runs/38585943376).
- [x] (5b) A voltage divider `.op` matches the analytic value exactly to 6 significant figures. Verified by the same test file and run, which also checks that a rejected deck reports the ngspice error and that the engine recovers afterward.
- [ ] The `.ac` −3 dB point of an RC filter matches 1/(2πRC) within 1%.
- [ ] Probing a wire or pin plots its signal, and plots support zoom, cursors, and measurement readouts.

## Remaining gaps and unknowns

- Encrypted vendor models (PSpice) are not supported.

### Slice 5 research (2026-10-14, unverified until CI confirms)

- **License:** the official Windows shared library build is Modified BSD (base, CIDER, and XSPICE) and can be redistributed. The installer must ship the ngspice `COPYING` notice.
- **Distribution:** ngspice 47 is the latest stable release. The archive is `https://sourceforge.net/projects/ngspice/files/ng-spice-rework/47/ngspice-47_dll_64.7z/download`, a 7z file holding `ngspice.dll`, `spinit`, and the `*.cm` code models. No SHA-256 is published, so CI computes it on the first fetch and the value is then pinned. The DLL's path inside the archive must be confirmed in the first CI run.
- **API:** `sharedspice.h` provides `ngSpice_Init` (with the `SendChar`, `SendStat`, `ControlledExit`, `SendData`, `SendInitData`, and `BGThreadRunning` callbacks), `ngSpice_Circ`, `ngSpice_Command` (`bg_run`, `bg_halt`), `ngGet_Vec_Info`, `ngSpice_CurPlot`, and `ngSpice_AllVecs`. ngspice keeps global state, so only one instance can run per process, and runs must be serialized. The callbacks fire on ngspice's background thread. ngspice must be able to find `spinit` and the code models, either through `SPICE_LIB_DIR` or through `ngSpice_nospinit` and `ngCM_Input_Path`. Convergence failures are reported only as text through `SendChar`.
- **KiCad conventions:** the `Sim.Device`, `Sim.Type`, `Sim.Params`, `Sim.Pins`, `Sim.Library`, and `Sim.Name` fields map a symbol to SPICE. Without them, the device is inferred from the reference prefix (R, C, L, V, I) and the `Value`. Values accept SI suffixes, `Meg` for 10^6 (SPICE `m` means milli), and the `4k7` form for 4.7k. Ground is node `0`, and named nets get a `/` prefix (for example `/OUT`). The `.tran` analysis takes step, stop, start, max step, and UIC. The probe tool plots `V(net)` for a wire and `I(device)` for a pin, and probed signals persist across runs. `.op` results are annotated on the schematic.
- **Why ngspice 46, not 47 (decided 2026-10-14):** the ngspice 47 `ngspice.dll` imports `sndfile.dll` (libsndfile, LGPL-2.1) and `samplerate.dll` (libsamplerate, BSD-2) for its new audio-file commands, and its DLL archive ships neither, so loading fails with OS error 126 ([CI 38583326976](https://github.com/tharunkumar1177/tron/actions/runs/38583326976)). The user chose ngspice 46, the newest DLL build without those imports. It loses only the audio commands, which Tron does not use, and keeps LGPL binaries out of the installer. The CI import check fails the build if a future pin needs a DLL the archive lacks.
- **Archive contents:** the pinned archive is `https://downloads.sourceforge.net/project/ngspice/ng-spice-rework/old-releases/46/ngspice-46_dll_64.7z` with SHA-256 `5c03f25709d6ea7d1e58a82ebe7c1e6eb5177f266787466bf0c716791ba26e4c`, pinned in `ci.yml`. The single top-level folder is `Spice64_dll`. It holds `dll-vs/ngspice.dll` beside its OpenMP runtime `dll-vs/libomp140.x86_64.dll`, `share/ngspice/scripts/spinit`, and the code models in `lib/ngspice/*.cm`. The archive has no `COPYING` or other license file, so 5d must ship the notice from a pinned copy of the ngspice source tree.
- **Known weaknesses in KiCad's simulator:** "timestep too small" usually hides a floating node or a missing model, so pre-flight checks matter. Users ask for a dual-cursor delta readout and a measurement table (min, max, average, RMS, peak-to-peak).
