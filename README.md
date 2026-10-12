# Tron

Tron is a local-first desktop application for electronics design: schematic capture, ngspice circuit simulation, PCB layout, fabrication output, and AI-assisted design. Projects are plain files on your disk, saved in a KiCad-compatible format. There are no accounts, no cloud storage, and no telemetry.

> Tron is in early development. Windows is the first supported platform; macOS and Linux will follow.

## Status

| Area | State |
|---|---|
| Project shell: create, open, save, undo, redo, zoomable canvas | Done |
| Component library: downloadable index and search | In progress |
| Schematic capture, simulation, PCB layout, fabrication output, AI assistant, autorouting | Planned |

The full ordered plan is in [ROADMAP.md](ROADMAP.md).

## Stack

- **Shell:** [Tauri 2](https://tauri.app/)
- **Core:** Rust workspace (`crates/`)
- **UI:** React and TypeScript, built with Vite and tested with Vitest

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | Design document, command and undo model, S-expression and KiCad file handling |
| `src-tauri` | Application shell: IPC commands, native menu, settings |
| `ui` | React user interface |
| `promo` | Remotion promo video |
| `.github/workflows` | CI and promo-video workflows |
| `architecture.md` | Shared technical contracts |
| `blueprints/` | Per-responsibility design records, routed through [blueprints/README.md](blueprints/README.md) |

## Building

GitHub Actions is the only build authority. Every push runs lint, type-check, tests, and the Windows installer build in [`.github/workflows/ci.yml`](.github/workflows/ci.yml). Download the installer from the artifacts of a green run on the [Actions tab](https://github.com/tharunkumar1177/tron/actions).

Lockfiles (`Cargo.lock`, `package-lock.json`) are generated in CI and committed from CI artifacts. Don't write them by hand.

## Contributing

Read [AGENTS.md](AGENTS.md) for working rules, then [architecture.md](architecture.md) and the blueprint for the area you are changing.

## License

Not yet chosen.
