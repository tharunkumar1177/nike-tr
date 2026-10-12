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

## Slices

- [x] **1. Walking skeleton** — repo scaffold, CI runs lint, type-check, test, and the Windows installer build. Lockfiles come from CI. Blueprint: [build-and-release](blueprints/build-and-release.md)
- [ ] **Side slice: Promo video** — a 30-second, 1920×1080 motion-graphics ad built with Remotion in `promo/`. A separate workflow renders it to MP4 and uploads it as an artifact. Graphics only, with no app screens. Blueprint: [promo-video](blueprints/promo-video.md)
- [ ] **2. Project shell** (UI) — create, open, and save a project. Empty canvas with zoom, pan, undo, and redo. Blueprint: [project-files](blueprints/project-files.md)
- [ ] **3. Library pipeline** (UI) — CI builds the SQLite index, the app downloads it on first launch, and the library browser offers search. Blueprint: [component-library](blueprints/component-library.md)
- [ ] **4. Schematic capture** (UI) — place symbols, wires, junctions, labels, and properties. Netlist and ERC. Blueprint: [schematic-capture](blueprints/schematic-capture.md)
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

Slice 1 is done: a green CI run ([37985560054](https://github.com/tharunkumar1177/tron/actions/runs/37985560054)) built the Windows installer. Next step: slice 2, the project shell. It needs an architect plan and a design plan before any code.

Codebase entry points: `crates/core` (units), `src-tauri/src/ipc.rs` (IPC commands and ts-rs types), `ui/src` (React), `.github/workflows/ci.yml`.
