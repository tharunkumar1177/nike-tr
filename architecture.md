# Tron System Architecture

## Purpose and authority

This document owns the shared contracts of Tron, a local-first desktop application for schematic capture, circuit simulation, PCB layout, fabrication output, and AI-assisted design. Every rule here binds two or more responsibilities. Responsibility-local rules live in the [blueprint manifest](blueprints/README.md).

Authority order when sources disagree: inspected code and tests for current behavior, then this document for intended shared behavior, then blueprints. Every contract below is **intended** until a blueprint cites code and tests that enforce it.

## System boundaries

Tron is a single-user desktop application. Projects are files on the user's disk.

External actors:

- **User** — edits designs through the desktop UI.
- **Filesystem** — project files, the cached component library, and application settings.
- **GitHub Actions** — the only build authority. It produces installers, lockfiles, and the library index asset.
- **Library asset host** — GitHub Releases of this repository serve the prebuilt component library index.
- **LLM provider** — a user-configured HTTPS endpoint, either a hosted API with the user's own key or a local OpenAI-compatible server. Tron ships no key and has no backend of its own.

Excluded: user accounts, cloud project storage, real-time collaboration, telemetry, and any hosted Tron service.

## Component model

Tron is built as a Tauri 2 application: a Rust workspace plus a React and TypeScript UI.

| Component | Language | Owns |
|---|---|---|
| `crates/core` | Rust | The design document, the command and undo model, S-expression read and write, geometry, ERC, DRC |
| `crates/library` | Rust | The SQLite symbol and footprint index: building it in CI, then querying and caching it at runtime |
| `crates/sim` | Rust | SPICE netlist export and the `libngspice` FFI session |
| `crates/ai` | Rust | The LLM HTTP client, the circuit DSL compiler, and the verification loop |
| `crates/fab` | Rust | Gerber, Excellon, BOM, and pick-and-place writers |
| `src-tauri` | Rust | The application shell: IPC command handlers, windows, settings, and wiring the crates together |
| `ui` | TypeScript, React | Rendering, input handling, panels, and dialogs. It holds no authoritative design state |

Dependency direction: `ui` talks to `src-tauri` over IPC only. `src-tauri` depends on every crate. `sim`, `fab`, and `ai` depend on `core`. `ai` also depends on `sim` and `library`. `core` depends on no other Tron crate.

## Shared contracts

### Design document ownership

- One design document per project holds both the logical view (schematic) and the physical view (board). The two are linked by stable identifiers, not by position.
- Only `crates/core` mutates the document, and only by applying a **command**. A command is a serializable, invertible edit. Undo and redo replay command inverses.
- Every other component reads the document through `core` APIs or receives copies. The UI never holds the source of truth.
- Every schematic symbol instance, board footprint, net, wire, track, via, and zone has a UUID that persists across save and load.

### Coordinate and unit representation

- Every physical length and position is an `i64` count of **nanometers**. One user-facing millimeter equals 1,000,000 units.
- The schematic and the board use separate coordinate spaces with the same unit. X grows to the right and Y grows downward, matching KiCad.
- Angles are stored as `i32` tenths of a degree, normalized to `[0, 3600)`.
- Floating point is allowed only for rendering and for simulation values. It is never written back into document geometry.
- Simulation quantities use SI base units as `f64`: volts, amperes, seconds, and hertz.

### File format

- Projects are saved as KiCad-compatible S-expression files: `.kicad_sch` for the schematic and `.kicad_pcb` for the board. A `tron.json` project manifest records Tron-only settings.
- Every write is atomic: write to a temporary file in the same directory, flush it, then rename it over the target.
- Unknown S-expression nodes are kept and written back unchanged, so a round trip never loses data Tron doesn't understand.

### IPC contract

- The UI calls named Tauri commands that take typed request payloads and return `Result<T, AppError>`.
- Document changes reach the UI as batched **diff events** with a monotonically increasing document revision. The UI drops any diff whose base revision doesn't match its own and asks for a fresh snapshot.
- Large reads are scoped to the viewport. Full-document serialization over IPC is forbidden in edit loops.
- Request and response types are defined in Rust and exported to TypeScript, so the two sides cannot drift.

### Connectivity and netlist

- `core` alone derives nets from schematic connectivity: pins, wires, junctions, labels, and power symbols.
- The netlist is the one interface between the logical and physical views, and also the input to `sim` and `ai` verification.
- Updating the board from the schematic is an explicit user command that applies a reviewable diff. It never runs silently.

### Error and recovery

- All fallible operations return `AppError { kind, message, detail }`. `kind` is a stable machine-readable enum that the UI maps to user-facing copy.
- No silent failure: every error is either shown to the user or handled with a documented fallback.
- A crash or failed write leaves the last saved file intact, as the atomic write guarantees.
- External processes and engines (ngspice, LLM calls, freerouting) run off the UI thread and can be cancelled.

### Secrets and network

- LLM API keys are stored only in the operating system credential store, never in project files or logs.
- Tron makes network calls only to the configured LLM endpoint and the library asset host, and only after a user action or consent.
- Project content is sent to an LLM only when the user invokes an AI feature, and the UI shows which endpoint will receive it.

## Cross-cutting flows

### Simulate a schematic

The user starts a simulation. `core` derives the netlist, `sim` exports a SPICE deck and runs it in `libngspice` on a worker thread, results stream back as typed vectors, and the UI plots them. Simulation never mutates the document.

### Schematic to board

The user runs "Update board". `core` diffs the netlist against the board, the UI shows the proposed additions and removals, and the user accepts. `core` then applies a single undoable command.

### AI generation with verification

The user describes a circuit. `ai` asks the model for circuit DSL source, compiles it into a candidate document fragment, runs ERC through `core`, and runs the simulation assertions through `sim`. Failures go back to the model for a bounded number of retries. Only a candidate that passes is offered to the user, who merges it as a single undoable command.

## Verification boundaries

- Rust unit tests cover `core` invariants: commands and their inverses, coordinate math, S-expression round trips, ERC, and DRC.
- Golden-file tests cover file format and fabrication writers.
- `sim` integration tests run real ngspice in CI against known circuits with analytically known answers.
- UI component tests run in Vitest. IPC types are checked by generating TypeScript bindings in CI and failing on any diff.
- The Windows installer build in CI is the release gate. Nothing is built or verified locally.

## Build and dependency policy

- GitHub Actions is the only build authority. Lockfiles (`Cargo.lock`, `package-lock.json`) are generated in CI and committed from CI artifacts, never written by hand.
- Dependency versions are declared explicitly and checked against crates.io and npm.
- Third-party binaries (`libngspice`, a JRE for freerouting) are downloaded in CI from pinned URLs with pinned SHA-256 checksums.

## Blueprints

See [the blueprint manifest](blueprints/README.md).
