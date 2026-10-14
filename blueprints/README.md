# Blueprint Manifest

## Loading protocol

1. Match the task by intent and synonyms in the router below.
2. Load one primary blueprint and only its linked architecture headings.
3. Inspect the listed implementation and tests.
4. Load an impact-check blueprint only when its trigger applies.
5. For a shared-contract change or ambiguous scope, load all of [architecture.md](../architecture.md) and every row linking the affected heading.
6. Do not load unrelated blueprints.

## Router

| Concepts and synonyms | Responsibility type | Primary blueprint | Required architecture contracts | Required blueprints | Impact checks | Principal implementation and tests |
|---|---|---|---|---|---|---|
| CI, workflow, GitHub Actions, lint, build, installer, release, lockfile, artifact, macOS, Linux | Workflow | [build-and-release](build-and-release.md) | [Build and dependency policy](../architecture.md#build-and-dependency-policy), [Verification boundaries](../architecture.md#verification-boundaries), [Component model](../architecture.md#component-model) | None | [component-library](component-library.md) when the library asset job changes; [circuit-simulation](circuit-simulation.md) when the ngspice download changes | `.github/workflows/`; `src-tauri/tauri.conf.json` |
| project, open, save, new, file, canvas shell, zoom, pan, undo, redo, recent files | Capability | [project-files](project-files.md) | [Design document ownership](../architecture.md#design-document-ownership), [File format](../architecture.md#file-format), [IPC contract](../architecture.md#ipc-contract), [Error and recovery](../architecture.md#error-and-recovery) | None | [schematic-capture](schematic-capture.md) when the canvas or command model changes | `crates/core`; `src-tauri`; `ui/src` |
| library, symbol, footprint, part, search, browser, KiCad libs, index, download, FTS, preview geometry, pinned index | Capability | [component-library](component-library.md) | [Secrets and network](../architecture.md#secrets-and-network), [Error and recovery](../architecture.md#error-and-recovery), [Build and dependency policy](../architecture.md#build-and-dependency-policy), [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation) | None | [schematic-capture](schematic-capture.md) when the symbol schema changes; [board-layout](board-layout.md) when the footprint schema changes | `crates/library`; `src-tauri/src/library.rs`; `.github/workflows/library.yml` |
| schematic, symbol placement, wire, junction, net label, power, properties, ERC, netlist | Capability | [schematic-capture](schematic-capture.md) | [Design document ownership](../architecture.md#design-document-ownership), [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation), [Connectivity and netlist](../architecture.md#connectivity-and-netlist), [IPC contract](../architecture.md#ipc-contract) | [project-files](project-files.md), [component-library](component-library.md) | [circuit-simulation](circuit-simulation.md) when netlist output changes; [board-layout](board-layout.md) when net identity changes | `crates/core`; `ui/src` |
| simulation, SPICE, ngspice, transient, DC operating point, AC sweep, Bode, waveform, probe, convergence | Integration | [circuit-simulation](circuit-simulation.md) | [Simulate a schematic](../architecture.md#simulate-a-schematic), [Connectivity and netlist](../architecture.md#connectivity-and-netlist), [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation), [Error and recovery](../architecture.md#error-and-recovery) | [schematic-capture](schematic-capture.md) | [ai-design](ai-design.md) when simulation assertions change | `crates/sim`; `crates/sim/tests` |
| PCB, board, footprint placement, ratsnest, stackup, layer, track, via, zone, copper pour, routing, DRC, clearance | Capability | [board-layout](board-layout.md) | [Schematic to board](../architecture.md#schematic-to-board), [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation), [Design document ownership](../architecture.md#design-document-ownership), [IPC contract](../architecture.md#ipc-contract) | [schematic-capture](schematic-capture.md), [component-library](component-library.md) | [fabrication-output](fabrication-output.md) when board geometry changes; [autorouting](autorouting.md) when the board schema changes | `crates/core`; `ui/src` |
| Gerber, Excellon, drill, BOM, pick and place, CPL, fabrication, manufacturing export | Capability | [fabrication-output](fabrication-output.md) | [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation), [Verification boundaries](../architecture.md#verification-boundaries) | [board-layout](board-layout.md) | None | `crates/fab` |
| AI, LLM, assistant, chat, prompt, generate circuit, DSL, design review, API key, local model | Capability | [ai-design](ai-design.md) | [AI generation with verification](../architecture.md#ai-generation-with-verification), [Secrets and network](../architecture.md#secrets-and-network), [Design document ownership](../architecture.md#design-document-ownership), [Error and recovery](../architecture.md#error-and-recovery) | [schematic-capture](schematic-capture.md), [circuit-simulation](circuit-simulation.md) | [component-library](component-library.md) when part search changes | `crates/ai` |
| promo, promotional video, ad, advertisement, motion graphics, Remotion, marketing video, render MP4 | Workflow | [promo-video](promo-video.md) | [Build and dependency policy](../architecture.md#build-and-dependency-policy), [Verification boundaries](../architecture.md#verification-boundaries) | None | [build-and-release](build-and-release.md) when the Node version or shared TypeScript and ESLint pins change | `promo/`; `.github/workflows/promo.yml` |
| autoroute, autorouter, freerouting, DSN, SES, Specctra | Integration | [autorouting](autorouting.md) | [Error and recovery](../architecture.md#error-and-recovery), [Build and dependency policy](../architecture.md#build-and-dependency-policy), [Design document ownership](../architecture.md#design-document-ownership) | [board-layout](board-layout.md) | [build-and-release](build-and-release.md) when the JRE bundle changes | `crates/core`; `src-tauri` |

## Status vocabulary

- `Implemented`: verified in current code and tests.
- `Partial`: some acceptance criteria are verified and the gaps are named.
- `Planned`: required behavior is not yet implemented.
- `Unknown`: evidence is insufficient. Inspect before changing.
- `Deprecated`: retained only for migration or compatibility.
- `Superseded`: replaced by the linked blueprint.
