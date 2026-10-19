# Circuit simulation

## Outcome or responsibility

The user runs DC operating point, transient, and AC analyses on their schematic with real ngspice, and reads the results as waveforms, Bode plots, and annotated node voltages. Convergence problems are explained, not just reported.

## Current verified status

**Status:** Partial

Slice 5a is built and green in [CI 38576288344](https://github.com/tharunkumar1177/tron/actions/runs/38576288344). `crates/sim` builds the SPICE deck from the core netlist and runs the pre-flight checks. Slice 5b is green in [CI 38585943376](https://github.com/tharunkumar1177/tron/actions/runs/38585943376): `crates/sim` loads the pinned ngspice 46 DLL and runs `.op` and `.tran` against it. Slice 5c is green in [CI 38595951934](https://github.com/tharunkumar1177/tron/actions/runs/38595951934), including the Windows installer. It adds cancellable background runs with progress, min-max waveform decimation, the analysis settings in `tron.json`, and the simulation IPC commands. Slice 5d-1a is green in [CI 38636022839](https://github.com/tharunkumar1177/tron/actions/runs/38636022839). The Windows installer now bundles ngspice 46, its OpenMP runtime, the seven XSPICE code models, and both license notices. The engine skips `spinit` and loads the code models from an absolute path at startup. The 5d-1b backend is green in [CI 39227045078](https://github.com/tharunkumar1177/tron/actions/runs/39227045078). It adds `fetch_op_annotations` with anchors from `crates/sim/src/op.rs`, `ProjectSnapshot.simulationSettings`, and `DocumentDiff.simulationSettingsChanged`. The 5d-1b UI is green in [CI 39248031088](https://github.com/tharunkumar1177/tron/actions/runs/39248031088), including the Windows installer. It adds the Simulation Settings dialog, the simulation panel with its toolbar buttons and Tools menu items, and the `.op` annotations on the canvas. 5d-2 (the probe tool and a basic waveform viewer) comes next and still needs its plans.

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
- `crates/sim/src/ngspice.rs` — the `libloading` wrapper around the ngspice shared API. The library is loaded and initialized once per process, and `Engine::lock` serializes runs through a static mutex. Each run resets ngspice (`destroy all`, `remcirc`), loads the deck, and sends `bg_run`. The caller then waits on a condition variable that the background-thread and exit callbacks signal, and collects every real vector of the current plot. The time vector becomes the plot's `scale`. `Engine::halt` sends `bg_halt` without the engine lock, and `run_cancellable` also takes a cancel flag, so a cancel that arrives before the background thread starts is still honored. `SendStat` percentages go to the run's progress callback. Errors come from ngspice's stderr lines and its exit callback. A second library path in the same process is rejected. `skip_spinit` and `Engine::lock(library, code_models)` follow the 5d-1 runtime rules below. Implemented.
- `crates/sim/src/waveform.rs` — `decimate` and `Plot::waveform`: min-max reduction per x span, the nearest neighbor outside each end of the range, NaN samples dropped, and runs of equal sample count when x decreases. Implemented.
- `crates/core/src/manifest.rs` `AnalysisSettings` and `Session::edit_analysis` — the optional `simulation.analysis` key in `tron.json`, kept with unknown sibling keys, edited as the undoable "Simulation Settings" command. Implemented.
- `src-tauri/src/sim.rs` — `start_simulation` (async: checks the schematic, runs on a blocking worker, emits `simulation-progress`, and resolves with `SimulationFinished`), `cancel_simulation`, and `fetch_waveform` (capped at 20,000 samples). Results are dropped on any document change, open, or close, and a run that overlapped a change returns `simCancelled`. ngspice is found through `TRON_NGSPICE`, then the resource `ngspice/ngspice.dll`. `code_model_dir` uses the `codemodels` folder beside the library, or else `lib/ngspice` in the archive layout, and fails with `simEngineLoad` if neither exists. `src-tauri/tauri.bundle.conf.json` adds the `ngspice/` resources for the bundle job only. `src-tauri/src/ipc.rs` adds `get_analysis_settings` and `edit_analysis_settings`. `src-tauri/src/error.rs` maps each pre-flight and engine error to its own `AppErrorKind`. Implemented.
- `ui/src/si.ts` (`parseSi`, `formatSi`, `formatOpValue`, `formatSeconds`), `ui/src/components/SimulationSettingsDialog.tsx`, `ui/src/components/SimPanel.tsx`, the simulation state in `EditorShell.tsx`, the settings modal and menu IDs in `App.tsx`, `drawOpAnnotations` and `opAnnotationOrigins` in `ui/src/schematicDraw.ts`, the hitboxes and screen-reader list in `SchematicCanvas.tsx`, and the Tools menu items in `src-tauri/src/shell.rs`. Implemented.
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
   **5d split (architect, 2026-10-14).**
   - **5d-1:** bundled ngspice, the analysis settings editor, run and cancel controls with progress and elapsed time, and `.op` annotations on the schematic.
   - **5d-2:** the probe tool and the base waveform viewer.
   - **5d-3:** zoom and pan, two cursors with a delta readout, and the measurement table.

   **5d-1 runtime and bundling.**
   - ngspice.dll loaded through `LoadLibrary` finds `spinit` through `SPICE_SCRIPTS` or the process working directory, not its own location, and the `codemodel` lines inside it are relative to the working directory too. So before loading the library, `crates/sim` sets `SPICE_SCRIPTS` to the library's own folder, which has no `spinit`. It uses `_putenv_s`, because ngspice reads the C runtime's copy of the environment and `std::env::set_var` doesn't update that copy. `ngSpice_nospinit()` can't be used: in ngspice 46 it calls `cp_vset`, which reaches `ft_find_analysis` and dereferences `ft_sim` before `ngSpice_Init` creates it. That crashed every engine test with STATUS_ACCESS_VIOLATION ([CI 38604269148](https://github.com/tharunkumar1177/tron/actions/runs/38604269148)).
   - `Engine::lock(library, code_models)` loads the code models once, right after `ngSpice_Init` and before any circuit, the way KiCad does. Loading them after circuits had run crashed the test process with STATUS_ACCESS_VIOLATION ([CI 38632253529](https://github.com/tharunkumar1177/tron/actions/runs/38632253529)). A later lock that asks for a different folder gets an error.
   - Code model loading sends a `codemodel '<absolute path>'` command for each `*.cm` file. It uses single quotes because ngspice 46's lexer strips them and keeps the path as one word, spaces included. It keeps double quotes as part of the file name ([CI 38626825806](https://github.com/tharunkumar1177/tron/actions/runs/38626825806)). A path that contains `'` is rejected with an error.
   - `spinit` is not shipped. Without it, `.op` and `.tran` on R, C, L, V, and I behave the same.
   - Only the bundle job gets the ngspice files. It downloads the pinned archive, ngspice's `COPYING` from the `ngspice-46` tag, and the LLVM OpenMP `LICENSE.TXT` (for `libomp140.x86_64.dll`, Apache-2.0 with the LLVM exception), each checked against a pinned SHA-256, into `src-tauri/resources/ngspice/{ngspice.dll, libomp140.x86_64.dll, codemodels/*.cm, COPYING, LICENSE-libomp.txt}`.
   - It builds with `--config src-tauri/tauri.bundle.conf.json`. `tauri-build` checks `bundle.resources` at compile time, so the resources can't go in `tauri.conf.json` without breaking the rust job.
   - The `.osdi` and `ivlng` files are not shipped.
   - `ProjectSnapshot` gains `simulationSettings` and `DocumentDiff` gains `simulationSettingsChanged`.
   **5d-1 is built in two PRs.**
   - **5d-1a:** the runtime and bundling above.
   - **5d-1b:** the three UI surfaces below, the snapshot and diff fields, and an op-point anchor mapping over IPC. It is built as two PRs (architect, 2026-10-19):
     - **5d-1b backend.**
       - `ProjectSnapshot.simulationSettings: AnalysisSettings | null`. `ProjectSnapshot` drops `Eq`.
       - `DocumentDiff.simulationSettingsChanged: boolean`. The UI's `applyDiff` returns `null` when it is set, so the UI fetches a fresh snapshot.
       - `crates/sim/src/op.rs` `op_anchors(schematic, deck) -> OpAnchors` and `OpAnchors::values(plot) -> Vec<OpValue>`. Lookups are keyed by lower-case SPICE name, and `<device>#branch` vectors map to device currents.
       - `start_simulation` computes the anchors from the same schematic the deck came from and stores them with the plot.
       - `fetch_op_annotations(runId) -> OpAnnotation[]`, where `OpAnnotation` is `{ vector, kind: "voltage" | "current", label, value, position: Xy }`. It returns nothing for a transient. Values are formatted in the UI.
     - **5d-1b UI.**
       - The settings dialog, the run panel, and the annotations, per the design decisions below. `ui/src/si.ts` parses and formats SI values.
       - The menu items "&Run Simulation" (`F5`), "&Cancel Simulation", and "Simulation Se&ttings..." (`CmdOrCtrl+Alt+S`) go under Tools, with IDs `run-simulation`, `cancel-simulation`, and `simulation-settings`. They ship only with the UI that handles them. Esc cancels in the UI while a run is active, but it is not a menu accelerator.
       - `App.tsx` owns the run state: idle, running, finished, cancelled, or failed, plus progress, elapsed time, and staleness. Any `document-changed` event marks the results out of date and clears the annotations.
       - As built, the run state lives in `EditorShell` next to the ERC state. App drives it with a `simulationTicket` from the Run Simulation menu item. Results go out of date when `project.revision` moves past the revision the run started on, and annotations are hidden while they are out of date. The panel styles are in `styles.css` with the other panels. F5 and `Ctrl+Alt+S` are native menu accelerators, the way the other shortcuts are, and the UI handles only Esc.
     - **5d-1b UI design plans (designer, 2026-10-19).** These refine the design decisions below, and win where they differ.
       - **Settings dialog.**
         - `SimulationSettingsDialog.tsx` takes the props `settings: AnalysisSettings | null`, `onApply(settings) => Promise<ErrorReport | null>`, and `onCancel`.
         - It reuses `Modal`, `.form`, `.field`, and `.field--inline`. Each input and its `.form__error` sit in a `.field` wrapper, with no inline styles.
         - Transient fields are grouped in a `fieldset.form__section` with the legend "Transient". Initial focus goes to the "Analysis type" select (`data-autofocus`).
         - The buttons are OK (submit, disabled only while errors show or a submit is in progress) and Cancel. OK with unchanged settings closes without an IPC call.
         - An IPC failure shows as a `.form__error` with `role="alert"`.
         - Validation runs on every keystroke, and the first failing check shows under its field:
           - Time step: invalid, then ≤ 0 ("Must be greater than 0."), then > final − initial ("Must not exceed final time minus initial time.").
           - Final time: invalid, then ≤ 0 ("Must be greater than 0.").
           - Initial time: invalid, then < 0 ("Must be greater than or equal to 0."), then ≥ final ("Must be less than final time.").
           - Maximum time step, only when not empty: invalid, then ≤ 0, then > final − initial.
         - `ui/src/si.ts`:
           - `parseSi` accepts f, p, n, u, µ, m, k, Meg, G, and T (case-insensitive SPICE rules, so `M` means milli), plus the `4k7` form and plain or exponent numbers.
           - `formatSi` uses ASCII prefixes that round-trip: `1e-5` becomes `10u`, `2.5e-3` becomes `2.5m`, `0` stays `0`, `1.5e-7` becomes `150n`, and `10` stays `10`.
       - **Run panel.**
         - `SimPanel.tsx` and `SimPanel.css` mirror `ErcPanel` (classes `sim-panel`, `sim-panel__header`, and `sim-row`). The panel is rendered in `editor__center`, and the ERC and simulation panels are mutually exclusive.
         - The toolbar gets a group with Run (play icon), Cancel (stop icon), and Settings (gear icon).
         - State: `idle`, `running { progress: number | null, elapsedMs }`, `finished { elapsedMs, stale }`, `cancelled { elapsedMs, stale }`, and `failed { error, stale }`. Elapsed time ticks every 100 ms from a start-time ref.
         - Strings: "Elapsed: N.N s" next to a `<progress>` (indeterminate until a percentage arrives), "Simulation complete in N.N s.", "Simulation cancelled.", and the badge "Out of date".
         - The `aria-live` region announces "Simulation started.", "Simulation complete in N.N seconds.", "Simulation cancelled.", and "Simulation failed: {message}".
         - A failed run shows one error row. For `simNoAnalysis` or `simInvalidAnalysis`, clicking it opens the settings dialog. Otherwise, if the error has a detail (a symbol reference), clicking it jumps to that symbol. The dialog never opens on its own.
         - F5 runs, with `preventDefault`. Esc cancels only while a run is active and otherwise keeps its existing behavior.
       - **Op annotations.**
         - `drawOpAnnotations(context, viewport, annotations, colors, font)` in `schematicDraw.ts` is drawn after the ERC markers.
         - The box's top-left corner sits 8 px right of and 8 px above the anchor. Its size and font are fixed in screen pixels (the base 14 px font, with 4 px by 2 px padding). Boxes at the same anchor stack downward with a 2 px gap.
         - Colors use the existing tokens (surface background, primary text, secondary-text border). The designer named `--color-surface-elevated`, `--color-text-primary`, and `--color-text-secondary`; use the matching existing tokens if those names differ.
         - `formatOpValue(value, kind)` gives 3 significant digits with prefixes from f to T, for example `3.30 V`, `-1.25 mA`, and `-4.57 kV`. A magnitude below 1e-15 shows as `0 V` or `0 A`, and 1e15 or more uses exponent form (`1.23e15 V`).
         - `SchematicCanvas` draws hidden `aria-hidden` hitboxes, each with a `title` holding the exact value. A visually hidden list (`aria-label="Operating point results"`) reads like "Voltage at /OUT: 3.30 V".
         - The toolbar eye toggle "Show operating point annotations" (`aria-pressed`, on by default) sits before the zoom label.

   **5d-1 design decisions (designer, 2026-10-14; the full plans are in the session transcript).**
   - **Settings dialog** (`SimulationSettingsDialog.tsx`, `ui/src/si.ts`).
     - It opens from a toolbar gear, a menu item, and `Ctrl+Alt+S`.
     - An "Analysis type" select offers "Operating Point (.op)" and "Transient (.tran)". The transient fields are "Time step", "Final time", "Initial time", "Maximum time step" (placeholder "Optional"), and "Start from initial conditions (UIC)".
     - Fields hold the raw SI text. Each field shows its own error ("Invalid SI number.", "Must be greater than 0.", "Must be greater than or equal to 0.", "Must be less than final time.", "Must not exceed final time minus initial time."), and OK is disabled while any error shows.
     - With no analysis set, it defaults to a transient of 1u / 1m / 0. OK and Cancel only, so one dialog session is one undo step.
   - **Run panel** (`SimPanel.tsx`, modeled on `ErcPanel`, plus toolbar Run, Cancel, and Settings buttons).
     - F5 runs and Esc cancels.
     - The progress bar is indeterminate until a percentage arrives, alongside "Elapsed: N.N s".
     - The panel shows success ("Simulation complete in {time} s."), "Simulation cancelled.", and an "Out of date" badge after an edit.
     - Pre-flight errors appear as rows like the ERC panel's. Errors that name a symbol jump to it, and analysis errors open the settings dialog.
     - An `aria-live` region announces start, finish, and errors.
   - **Op annotations** (drawn in `schematicDraw.ts`, with hidden hitboxes and a screen-reader list in `SchematicCanvas.tsx`, and an eye toggle in the toolbar).
     - Each voltage sits at its net label, or else at the middle of the net's longest wire. Each current sits at its device's first pin. Ground is hidden.
     - Values show 3 significant digits with an SI prefix and unit, such as `3.30 V` or `-1.25 mA`.
     - Annotations clear on any edit. A tooltip shows the exact value.
     - It needs, per vector: `vector`, `kind` (voltage or current), `label`, and an `anchorNm`.
4. **5d. Bundling and UI.** Ship `ngspice.dll`, `spinit`, the code models, and `COPYING` as Tauri resources. Add the probe tool, `.op` annotations, and a waveform viewer with zoom, two cursors with a delta readout, and measurements (min, max, average, RMS, peak-to-peak). This needs a designer plan first.

## Acceptance or verification criteria

- [x] (5a) The SPICE deck for a divider and an RC circuit is exact text, and each pre-flight error is reported with the symbol or setting involved. Verified by `crates/sim/tests/deck.rs`.
- [x] (5b) An RC low-pass transient matches the analytic response within 1% in a CI test using real ngspice. Verified by `crates/sim/tests/ngspice.rs` in [CI 38585943376](https://github.com/tharunkumar1177/tron/actions/runs/38585943376).
- [x] (5b) A voltage divider `.op` matches the analytic value exactly to 6 significant figures. Verified by the same test file and run, which also checks that a rejected deck reports the ngspice error and that the engine recovers afterward.
- [x] (5c) A long transient reports progress midway through the run and can be cancelled while it runs, after which the engine runs the next deck normally. The waveform fetch respects its budget and keeps peaks. The analysis settings are an undoable edit that survives a save and reopen, and each pre-flight error has its own error kind. Verified by `crates/sim/src/ngspice.rs` and `crates/sim/tests/ngspice.rs` (real ngspice), `crates/sim/src/waveform.rs`, `crates/core/src/session.rs`, `crates/core/src/manifest.rs`, and `src-tauri/src/error.rs` in [CI 38595951934](https://github.com/tharunkumar1177/tron/actions/runs/38595951934).
- [x] (5d-1a) Running an XSPICE `gain` model loaded from a folder whose path contains a space gives the expected `.op` value. The installer build places `ngspice.dll`, `libomp140.x86_64.dll`, `COPYING`, `LICENSE-libomp.txt`, and `codemodels/*.cm` under `ngspice/` beside the app. Every download is checked against a pinned SHA-256. Verified by `crates/sim/tests/ngspice.rs`, `src-tauri/src/sim.rs`, and the bundle job's "Check bundled ngspice" step in [CI 38636022839](https://github.com/tharunkumar1177/tron/actions/runs/38636022839).
- [x] (5d-1b backend) For the divider, each `.op` voltage is placed at its net label (`VIN`, `/OUT`) and the source current at the source's first pin. An unlabelled net uses the middle of its longest wire, and ground, unknown vectors, and transients get no annotation. The snapshot carries the analysis settings, and the diff flags their changes. Verified by `crates/sim/tests/op.rs`, `src-tauri/src/sim.rs`, `src-tauri/src/ipc.rs`, and `ui/src/documentDiff.test.ts` in [CI 39227045078](https://github.com/tharunkumar1177/tron/actions/runs/39227045078).
- [x] (5d-1b UI) SI fields parse and round-trip, each field shows its first failing check, and OK applies the parsed settings (or closes when nothing changed). Running from the Tools menu shows progress and elapsed time, and Esc cancels. An `.op` result is annotated on the canvas and in the screen-reader list, the eye button toggles it, and an edit marks it out of date and hides it. An analysis error opens the settings dialog, and an error that names a symbol jumps to it. Verified by `ui/src/si.test.ts`, `SimulationSettingsDialog.test.tsx`, `SimPanel.test.tsx`, and `App.test.tsx` in [CI 39248031088](https://github.com/tharunkumar1177/tron/actions/runs/39248031088).
- [x] (5d-2) Each wire, label, and pin of the divider resolves to its net's vector, the ground pin to ground, and a symbol body to its device. A source gives `<dev>#branch`, and other devices give "No current available". A probe on a stale or unknown run gives `simNotFound`, and `fetch_waveform` without bounds covers the whole run. In the UI, `M` toggles probe mode. Clicking a pin plots its net, and clicking it again removes it. The viewer fetches each trace the run has at the plot width, summarizes it for screen readers, marks missing vectors "No data", doesn't re-fetch stale results, and shows the `.op` fallback. Verified by `crates/sim/tests/op.rs`, `src-tauri/src/sim.rs`, `waveformPlot.test.ts`, `SimPanel.test.tsx`, and `App.test.tsx` in [CI 39268696849](https://github.com/tharunkumar1177/tron/actions/runs/39268696849), including the Windows installer.
- [ ] The `.ac` −3 dB point of an RC filter matches 1/(2πRC) within 1% (5d-4).
- [ ] Plots support zoom, pan, cursors, and measurement readouts (5d-3).

## Remaining gaps and unknowns

- Encrypted vendor models (PSpice) are not supported.
- **Bundled ngspice in an installed app (unverified):** CI checks that the files are bundled but never runs the installed app. Still unverified: an install under `Program Files`, an install path with non-ASCII characters (ngspice receives a narrow-string path), and a path that contains `'` (rejected by design).
- **Progress on very long transients (unexplained):** a 0.1 s transient with a 0.1 µs maximum step reports progress midway. A 2 s transient with the same step reported none in its first 10 s on the CI runner ([CI 38594420004](https://github.com/tharunkumar1177/tron/actions/runs/38594420004)). The cause is unknown, so the 5d UI must not treat a run without progress as stuck, and should show elapsed time as well.
- **Probe hover and plot drawing (unverified):** jsdom has no 2D canvas, so CI never checks the hover outline, the plot axes, or the traces visually.
- **F5 in the installed app (unverified):** F5 and `Ctrl+Alt+S` rely on the native menu accelerators, like the other shortcuts. CI can't confirm that WebView2 passes F5 to the menu instead of handling it itself.
- **5d-2 contract decisions (user, 2026-10-19):**
  - `fetch_waveform`'s `minX` and `maxX` become optional, and a missing bound means the full range.
  - The deck does **not** add `.options savecurrents`. Only voltage sources and inductors (`<dev>#branch`) give currents, and probing any other device shows "No current available".
  - A probe is sent as the clicked item's ID (wire, label, junction, pin, or symbol). The backend resolves it against the netlist saved with the run, never the current schematic.
- **5d-2 plan (architect, 2026-10-19).**
  1. **`crates/sim/src/op.rs` `ProbeMap`.** Built with `OpAnchors` from the same schematic and deck, and stored with the run. It maps net-item UUIDs (wires, labels, junctions) to their SPICE node, `(symbol uuid, pin number)` to its node, and symbol UUIDs to their device name. Test `probe_map_resolves_targets` checks that a wire on the RC's capacitor node gives that node's vector and that a resistor body gives `NoCurrent`.
  2. **IPC.**
     - `fetch_waveform` takes `minX` and `maxX` as optional (`fetch_waveform_handles_none_range`).
     - `resolve_probe(runId, target: ProbeTarget) -> ProbeResult`.
       - `ProbeTarget` is `netItem { uuid }`, `pin { symbolUuid, pin }`, or `symbol { uuid }`.
       - `ProbeResult` is `vector { vector, label, kind }`, `ground`, or `noCurrent { reference }`.
       - A device gives its current only if the plot has `<dev>#branch`.
       - A missing or stale run gives `simNotFound`.
  3. **Probe tool.** `"probe"` becomes a `CanvasTool`. A click checks pins first, then wires, labels, and junctions, then symbol bodies. `ground` and `noCurrent` show as status hints, not errors. A `vector` result is added to `probedVectors` in `EditorShell`, keyed by vector name, and it persists across runs.
  4. **`WaveformViewer.tsx` inside `SimPanel`.** Plain canvas 2D with no new dependency. Voltages use the left axis and currents the right. The fetch budget is the canvas CSS width, capped at `MAX_WAVEFORM_POINTS`, and the viewer re-fetches when resized. Each legend item removes its trace when clicked, and a vector missing from the current run shows "No data". An `.op` run shows a fallback message instead of the viewer.
- **5d-2 as built (backend).**
  - `ProbeTarget` is `item { uuid }` (a wire, label, junction, or symbol body) or `pin { symbol, position: Xy }`. The UI's `SymbolInstance.pins` holds pin positions but not pin numbers, so a pin is matched by its position.
  - `ProbeResult` adds `notSimulated`, for items on nets the deck doesn't simulate. Its vector field is named `quantity`, because `kind` is the serde tag.
  - Any document change discards the stored results (`SimState::invalidate`), so once results are out of date they can't be probed or re-fetched. The viewer keeps the samples it already fetched, dims them, and doesn't re-fetch on resize.
- **5d-2 probe tool design (designer, 2026-10-19).**
  - **Toolbar:** an IconButton titled "Probe signal (M)" (`ProbeIcon`) after the no-connect flag, with `aria-pressed`. The designer first proposed `V`, but `V` opens the Value editor, so the shortcut is `M` (for measure).
  - **Availability:** always enabled. Without a usable run, a click shows the stale-run notice.
  - **Hover:** a crosshair cursor, and the target is outlined with a 2 px `--color-focus` stroke: a circle for a pin, the segment for a wire, the bounding box for a symbol.
  - **Clicks:** clicking a plotted signal removes it. The tool stays active until Esc or another tool.
  - **Status hint:** "Click a wire or pin for voltage, or a symbol for current. Esc: Exit". The designer's text said "a pin for current", which contradicts the plan, so it was corrected.
  - **Notices** (status bar and `aria-live`):
    - "Probe mode started." and "Probe mode ended." (screen reader only)
    - "Probed voltage at {label}." and "Probed current through {reference}."
    - "Removed probe from {label}."
    - "Ground is always 0 V."
    - "No current available for {reference}."
    - "No simulation data available for unsimulated item." (for a `notSimulated` result)
    - "Simulation out of date or not found. Run a simulation to plot."
- **5d-2 waveform viewer design (designer, 2026-10-19).**
  - **Panel layout:**
    - The run status (the completion text or progress bar) moves into `.sim-panel__header` in place of the "Simulation" title, while a run is active or finished.
    - `.sim-panel__content` holds `WaveformViewer`.
    - The panel is 240 px tall, or 320 px once at least one trace exists. There is no resize handle until 5d-3.
    - Running, cancelled, and failed runs show the existing message or error row instead of the canvas.
  - **Plot:**
    - Time runs along the bottom. Voltage is on the left axis, shown only if a voltage trace exists, and current is on the right axis, likewise.
    - Each axis gets 5 ticks, labelled with `formatSi(value)` plus `s`, `V`, or `A`.
    - Gridlines use `--color-grid-line`, and the 0 V and 0 A lines use `--color-grid-line-major`. The background is `--color-canvas`.
    - Traces are 1.5 px wide. Their colors cycle through `--color-focus`, `--color-text-warning`, `--color-text-error`, and `--color-text-primary` (`index % 4`).
    - A `ResizeObserver` re-fetches the data with the canvas width as the budget.
  - **Legend:**
    - It sits absolutely positioned at the top right in a wrapping row, on `--color-surface-elevated` at 80% opacity with a `--color-border` outline and `--radius-md`.
    - Each item has a 12×2 px color swatch, its label, and an `icon-button` with `RemoveIcon` labelled "Remove {label} from plot".
    - A vector missing from the current run gets 0.5 opacity, a dashed swatch, and "{label} (No data)".
  - **States:**
    - Finished with no traces: "Simulation complete. Select the Probe tool and click a wire or pin to plot its signal."
    - `.op` runs: "Waveforms are not available for Operating Point (.op) analysis. Read the values directly from the schematic annotations."
    - Loading: "Fetching data...". Fetch error: "Failed to load waveform data."
    - When the results are out of date, the viewer drops to 0.5 opacity.
  - **Accessibility:** a visually hidden `ul` labelled "Waveform results" reads each trace, for example "Voltage /OUT: minimum -3.30 V, maximum 3.30 V" or "Current I(V1): No data".
- **5d-3 plan (architect, approved by the user 2026-10-19, built as one PR).**
  - **Backend.** `crates/sim/src/waveform.rs` adds `Plot::series`, `measure(x, y, from, to) -> Option<Measurements>`, and `value_at(x, y, at)`. Samples are joined by straight lines, so the average and RMS integrate each segment exactly, whatever the step sizes. The span is clipped to the run. A span that misses the run, is reversed, or has a decreasing x gives `None`. Where x repeats, the later sample wins.
  - **IPC (approved shared-contract change).** `measure_trace(runId, vector, minX?, maxX?, cursors: number[]) -> TraceReadout { span: TraceMeasurements | null, cursors: (number | null)[] }`, where `TraceMeasurements` is `{ from, to, min, max, average, rms, peakToPeak }`. A stale run or an unknown vector gives `simNotFound`.
  - **UI.** Zoom and pan act on the time axis only, and the value axes fit the visible samples. The span measured is between the cursors when both are placed, and otherwise the visible range. The view is re-fetched and re-measured 120 ms after it stops changing. Results that are out of date freeze the view and the cursors.
- **5d-3 design (designer, 2026-10-19).**
  - **Header:** a "Measure" toggle (`aria-pressed`) and a "Zoom to Fit" button sit in `.sim-panel__actions` while traces are plotted. Both are disabled when the results are out of date, and Zoom to Fit is also disabled at the full view.
  - **Plot** (`aria-label="Waveform plot"`, focusable): the wheel zooms about the pointer, a drag pans, and a double-click fits the run. In Measure mode, a click places cursor A, and a drag places A and then B. Dragging a cursor line (`col-resize` within 4 px) moves it, and double-clicking one removes it. Cursor A is drawn in `--color-focus` and B in `--color-text-warning`. The `data-min-x`, `data-max-x`, `data-cursor-a`, and `data-cursor-b` attributes expose the state to tests.
  - **Keys on the plot:** `+` and `-` zoom, the arrows pan, and `Home` fits. `A` and `B` place a cursor at the center of the view, `Shift` or `Alt` plus an arrow moves A or B, and `Esc` clears the cursors (only when one is placed, so Esc otherwise keeps its global meaning). Handled keys stop propagating.
  - **Table:** it replaces the legend and the hidden summary list. Its caption reads "Measurements cover the span between cursors." or "Measurements cover the visible range." The columns are Signal, Cursor A, Cursor B, Δ, Min, Max, Avg, RMS, Pk-Pk, and Remove. The cursor headers add the time, and Δ adds the time difference and its frequency. A missing value shows `—`. "Fetching data..." and "Failed to load waveform data." show in a spanning row. As built, that row sits above the trace rows instead of replacing them, so the remove buttons stay available.
  - **Resize handle:** a `role="separator"` labelled "Resize simulation panel" on the panel's top edge, from 240 px to 80% of the window height. `ArrowUp` and `ArrowDown` resize it by 20 px, and `Home` and `End` jump to the limits.
  - **Announcements:** "Zoomed to {min} to {max}.", "Cursor A placed at {time}.", "Cursor B moved to {time}.", "Cursor A removed.", and "Cursors cleared."
- **5d-2 viewer fix in 5d-3:** the viewer observed its size only on mount, so a plot first shown after the "Select the Probe tool" message never fetched. Its resize and wheel effects now re-run when the first trace arrives.
- **`.ac` analysis:** the user decided on 2026-10-19 that AC gets its own sub-slice, **5d-4**, after 5d-3. It covers the engine, the settings, and a Bode plot, and it owns the "`.ac` −3 dB" criterion.
- The `start_simulation`, `cancel_simulation`, and `fetch_waveform` commands have no test through Tauri itself. The engine, decimation, settings, and error mapping beneath them are tested.

### Slice 5 research (2026-10-14, unverified until CI confirms)

- **License:** the official Windows shared library build is Modified BSD (base, CIDER, and XSPICE) and can be redistributed. The installer must ship the ngspice `COPYING` notice.
- **Distribution:** ngspice 47 is the latest stable release. The archive is `https://sourceforge.net/projects/ngspice/files/ng-spice-rework/47/ngspice-47_dll_64.7z/download`, a 7z file holding `ngspice.dll`, `spinit`, and the `*.cm` code models. No SHA-256 is published, so CI computes it on the first fetch and the value is then pinned. The DLL's path inside the archive must be confirmed in the first CI run.
- **API:** `sharedspice.h` provides `ngSpice_Init` (with the `SendChar`, `SendStat`, `ControlledExit`, `SendData`, `SendInitData`, and `BGThreadRunning` callbacks), `ngSpice_Circ`, `ngSpice_Command` (`bg_run`, `bg_halt`), `ngGet_Vec_Info`, `ngSpice_CurPlot`, and `ngSpice_AllVecs`. ngspice keeps global state, so only one instance can run per process, and runs must be serialized. The callbacks fire on ngspice's background thread. ngspice must be able to find `spinit` and the code models, either through `SPICE_LIB_DIR` or through `ngSpice_nospinit` and `ngCM_Input_Path`. Convergence failures are reported only as text through `SendChar`.
- **KiCad conventions:** the `Sim.Device`, `Sim.Type`, `Sim.Params`, `Sim.Pins`, `Sim.Library`, and `Sim.Name` fields map a symbol to SPICE. Without them, the device is inferred from the reference prefix (R, C, L, V, I) and the `Value`. Values accept SI suffixes, `Meg` for 10^6 (SPICE `m` means milli), and the `4k7` form for 4.7k. Ground is node `0`, and named nets get a `/` prefix (for example `/OUT`). The `.tran` analysis takes step, stop, start, max step, and UIC. The probe tool plots `V(net)` for a wire and `I(device)` for a pin, and probed signals persist across runs. `.op` results are annotated on the schematic.
- **Why ngspice 46, not 47 (decided 2026-10-14):** the ngspice 47 `ngspice.dll` imports `sndfile.dll` (libsndfile, LGPL-2.1) and `samplerate.dll` (libsamplerate, BSD-2) for its new audio-file commands, and its DLL archive ships neither, so loading fails with OS error 126 ([CI 38583326976](https://github.com/tharunkumar1177/tron/actions/runs/38583326976)). The user chose ngspice 46, the newest DLL build without those imports. It loses only the audio commands, which Tron does not use, and keeps LGPL binaries out of the installer. The CI import check fails the build if a future pin needs a DLL the archive lacks.
- **Archive contents:** the pinned archive is `https://downloads.sourceforge.net/project/ngspice/ng-spice-rework/old-releases/46/ngspice-46_dll_64.7z` with SHA-256 `5c03f25709d6ea7d1e58a82ebe7c1e6eb5177f266787466bf0c716791ba26e4c`, pinned in `ci.yml`. The single top-level folder is `Spice64_dll`. It holds `dll-vs/ngspice.dll` beside its OpenMP runtime `dll-vs/libomp140.x86_64.dll`, `share/ngspice/scripts/spinit`, and the code models in `lib/ngspice/*.cm`. The archive has no `COPYING` or other license file, so 5d must ship the notice from a pinned copy of the ngspice source tree.
- **Known weaknesses in KiCad's simulator:** "timestep too small" usually hides a floating node or a missing model, so pre-flight checks matter. Users ask for a dual-cursor delta readout and a measurement table (min, max, average, RMS, peak-to-peak).
