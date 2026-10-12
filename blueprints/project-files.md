# Project files and editor shell

## Outcome or responsibility

The user can create, open, and save a Tron project, see it in the application window, and undo or redo any edit. This responsibility covers the project lifecycle, the command and undo engine in `core`, and the canvas shell with zoom and pan that the schematic and board editors build on.

## Current verified status

**Status:** Partial. CI run [38296991964](https://github.com/tharunkumar1177/tron/actions/runs/38296991964) passed lint, type-check, unit and golden tests, and the Windows installer build. Menu shortcuts and canvas interaction have no automated end-to-end test yet, so they still need a manual check in the installer.

- `core` reads and writes KiCad S-expressions with a hand-written parser instead of `winnow`. This keeps exact line and column reporting with no parser-combinator API risk. Unknown nodes round-trip unchanged.
- Every edit is an invertible `Command`. Project Properties applies one grouped command that covers the title block (stored in the schematic) and the grid size (stored in `tron.json`).
- The project is dirty when the top of the undo stack differs from the entry that was on top at the last save.
- The recent-projects list lives in `settings.json` in the app config directory. Failing to update it never fails an open or a save.

## Architecture dependencies

- [Design document ownership](../architecture.md#design-document-ownership)
- [File format](../architecture.md#file-format)
- [IPC contract](../architecture.md#ipc-contract)
- [Error and recovery](../architecture.md#error-and-recovery)

## Local rules and implications

### Design document ownership

The undo stack lives in `core`, next to the document. The UI's undo and redo buttons send commands. They never rebuild state on their own.

### File format

A project is a directory containing `tron.json`, `<name>.kicad_sch`, and `<name>.kicad_pcb`. Opening a plain KiCad project with no `tron.json` creates one on the first save.

### IPC contract

Opening a project returns a snapshot and its revision. After that, the UI reacts only to diff events.

### Error and recovery

If the user closes a project with unsaved changes, they are prompted to save. Parse errors name the file, line, and column.

## Related blueprints

### Required

None

### Impact checks

- [schematic-capture](schematic-capture.md) — when the canvas viewport or the command model changes.

## Relevant implementation and tests

- `crates/core/src/sexpr.rs`, `kicad.rs`, `manifest.rs`, `project.rs`, `session.rs` — S-expressions, KiCad files, `tron.json`, the project lifecycle with atomic writes, and the undo engine. Unit tests are in each module.
- `crates/core/tests/project_files.rs` with `tests/golden/` — golden files, reopen round trip, KiCad-only projects, parse-error positions, refusal cases.
- `src-tauri/src/ipc.rs`, `shell.rs`, `settings.rs`, `error.rs` — IPC commands and diff events, native menu and window title, recent projects, `AppError` mapping.
- `ui/src/App.tsx`, `useProjectDocument.ts`, `documentDiff.ts`, `viewport.ts`, `components/` — welcome screen, editor shell, dialogs, and the canvas. Tests: `App.test.tsx`, `viewport.test.ts`, `documentDiff.test.ts`, `ipc.test.ts`.

## Acceptance or verification criteria

- [ ] New, open, save, and save-as work from the menu and their keyboard shortcuts. Implemented; needs a manual check in the installer.
- [x] A saved project reopens with identical content, verified by a golden round-trip test.
- [x] Undo and redo restore the exact previous document state, verified by unit tests.
- [ ] The canvas zooms around the cursor and pans with the middle mouse button or space-drag. The zoom math is unit tested; the interaction needs a manual check.

## Remaining gaps and unknowns

- Autosave and crash-recovery snapshots are undecided.
- The `CmdOrCtrl+=` and `CmdOrCtrl+-` menu accelerators have not been tried on a real keyboard layout.
- No end-to-end UI test drives the packaged app yet.
