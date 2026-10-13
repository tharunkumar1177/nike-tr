# Tron Roadmap

Tron is a local-first desktop app (Tauri 2, Rust, React) for schematic capture, ngspice simulation, PCB layout, fabrication output, and AI-assisted design. Shared contracts are in [architecture.md](architecture.md). Routing to responsibilities is in [blueprints/README.md](blueprints/README.md).

## How to resume

1. Find the first unchecked slice below.
2. Load its primary blueprint through the manifest.
3. Follow the build loop in `AGENTS.md`: architect plan, then design plan for UI slices, then build, push, CI review, record, and PR.

## Decisions on record

- Stack: Tauri 2, Rust workspace, React and TypeScript. Approved with the Phase 0 plan.
- The component library is downloaded on first launch, not bundled. This was defaulted and can be revisited before slice 3.
- Windows ships first. macOS and Linux come in slice 13.
- Repository: `tharunkumar1177/tron`.
- Promo video side slice (approved 2026-10-12): Remotion, 30 seconds, 1920×1080, graphics only with no mocked app screens, rendered in its own CI workflow, no audio.
- Slice 3 split into 3a, 3b, and 3c (approved 2026-10-13). The index is pinned by a pull request that the library workflow opens after publishing a release. Preview geometry is pre-extracted into the index as JSON in nanometers. The workspace minimum Rust version is 1.88, which rusqlite 0.40 requires.
- Slice order kept as 3c before 4 (decided 2026-10-13): schematic capture places symbols from the library browser. The download consent prompt moved from 3b to 3c, so all library UI is designed together.
- Slice 4 split into 4a and 4b (approved 2026-10-13). Placed symbols embed their full KiCad definition in the schematic's `(lib_symbols)`, so a project renders and opens in KiCad without the downloaded library. The index therefore stores each symbol's definition (schema version 2), which ships first as its own PR with a new pinned release.

## Slices

- [x] **1. Walking skeleton** — repo scaffold, CI runs lint, type-check, test, and the Windows installer build. Lockfiles come from CI. Blueprint: [build-and-release](blueprints/build-and-release.md)
- [x] **Side slice: Promo video** — a 30-second, 1920×1080 motion-graphics ad built with Remotion in `promo/`. A separate workflow renders it to MP4 and uploads it as an artifact. Graphics only, with no app screens. Blueprint: [promo-video](blueprints/promo-video.md)
- [x] **2. Project shell** (UI) — create, open, and save a project. Empty canvas with zoom, pan, undo, and redo. Blueprint: [project-files](blueprints/project-files.md)
- [x] **3a. Library index pipeline** — `crates/library` turns a pinned KiCad library release into a SQLite index with full-text search and preview geometry. A CI workflow builds the full index, benchmarks search, publishes it as a release asset with its SHA-256, and opens a pull request that pins it. Blueprint: [component-library](blueprints/component-library.md)
- [x] **3b. Library download and cache** — downloads the pinned index only after consent, resumes interrupted downloads, verifies the SHA-256, caches the file atomically, and fetches it again when the cache is corrupt. IPC commands for status, download, decline, cancel, and search. Blueprint: [component-library](blueprints/component-library.md)
- [x] **3c. Library browser** (UI) — the first-launch download consent prompt with progress, a search panel with ranked symbol and footprint results, symbol and footprint previews drawn from the stored geometry, an empty state that explains how to download, and the KiCad attribution. Blueprint: [component-library](blueprints/component-library.md)
- [ ] **4a. Symbol placement** (UI) — place library symbols on the schematic, then select, move, rotate, mirror, and delete them, each as one undo step, saved to `.kicad_sch` with embedded `(lib_symbols)`. Prerequisite: the index stores symbol definitions and an r2 release is pinned. Blueprint: [schematic-capture](blueprints/schematic-capture.md)
- [ ] **4b. Connectivity and ERC** (UI) — wires, junctions, net labels, power symbols, property editing, netlist, and ERC. Blueprint: [schematic-capture](blueprints/schematic-capture.md)
- [ ] **5. Simulation** (UI) — `.op` and `.tran` through libngspice, a probe tool, and a waveform viewer. Blueprint: [circuit-simulation](blueprints/circuit-simulation.md)
- [ ] **6. AC sweep and convergence** (UI) — `.ac` with a Bode plot, gmin and source stepping, and readable convergence errors. Blueprint: [circuit-simulation](blueprints/circuit-simulation.md)
- [ ] **7. PCB foundation** (UI) — update the board from the schematic, footprints, ratsnest, outline, and stackup. Blueprint: [board-layout](blueprints/board-layout.md)
- [ ] **8. Manual routing and DRC** (UI) — tracks, vias, zones, R-Tree DRC, and a violations panel. Blueprint: [board-layout](blueprints/board-layout.md)
- [ ] **9. Fabrication output** (UI) — Gerber, Excellon, BOM, and pick-and-place export. Blueprint: [fabrication-output](blueprints/fabrication-output.md)
- [ ] **10. AI assistant** (UI) — endpoint and key settings, plus design review and result explanations. Blueprint: [ai-design](blueprints/ai-design.md)
- [ ] **11. AI circuit generation** (UI) — prompt to DSL, then compile, ERC, and simulation checks, then merge into the schematic. Blueprint: [ai-design](blueprints/ai-design.md)
- [ ] **12. Autorouting** — DSN export, freerouting, SES import. Needs approval for the JRE bundle. Blueprint: [autorouting](blueprints/autorouting.md)
- [ ] **13. macOS and Linux builds** — extend the CI matrix and installers. Blueprint: [build-and-release](blueprints/build-and-release.md)

## Current checkpoint

Slice 2 is done: a green CI run ([38296991964](https://github.com/tharunkumar1177/tron/actions/runs/38296991964)) passed every check and built the Windows installer. The menu and canvas interactions still need a manual check in the installer (see the project-files blueprint gaps). The promo video side slice is also done: a green run ([38293438957](https://github.com/tharunkumar1177/tron/actions/runs/38293438957)) rendered it and uploaded it as the `promo-video` artifact. Slice 3a is done. Release `library-kicad-10.0.7-r1` was published ([38313118321](https://github.com/tharunkumar1177/tron/actions/runs/38313118321)), and its pin was merged in [PR #5](https://github.com/tharunkumar1177/tron/pull/5). Slice 3b is green in CI ([38315533279](https://github.com/tharunkumar1177/tron/actions/runs/38315533279)). Slice 3c merged in [PR #7](https://github.com/tharunkumar1177/tron/pull/7) with red CI. [PR #9](https://github.com/tharunkumar1177/tron/pull/9) restored a green `main` ([38373811172](https://github.com/tharunkumar1177/tron/actions/runs/38373811172)).

Slice 4a is in progress. Its first PR, branch `slice-4a-symbol-source`, adds the stored symbol definitions and `Index::symbol_definition`, with `SCHEMA_VERSION` 2. After it merges, run the library workflow on `main` with `publish` enabled and revision 2, then merge the pin PR. Until that pin merges, the app on `main` can't open the r1 index. Next comes the 4a document model. It follows the architect's plan: move symbol-geometry extraction from `crates/library` into `crates/core`, parse and write `(lib_symbols)` and placed `(symbol)` items in `crates/core/src/kicad.rs`, add place, move, rotate or mirror, and delete commands in `session.rs`, carry `definitions` (one per library ID) and `instances` in `ProjectSnapshot` and `DocumentDiff`, then build the canvas UI from the designer's 4a plan. That plan covers the placement ghost at 50% alpha, the dashed selection box in `--color-focus`, R, X, and Y for rotate and mirror, Escape to cancel, status-bar hints, and an aria-live region. The designer still needs an answer on Reference and Value text: their positions come from the symbol's property `(at ...)` fields.

Codebase entry points: `crates/core` (S-expressions, KiCad files, project lifecycle, undo engine), `src-tauri/src/ipc.rs` (IPC commands and ts-rs types), `src-tauri/src/library.rs` (library download, cache, and search commands), `src-tauri/src/shell.rs` (native menu), `ui/src` (React), `.github/workflows/ci.yml`.
