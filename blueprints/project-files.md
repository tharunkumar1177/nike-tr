# Project files and editor shell

## Outcome or responsibility

The user can create, open, and save a Tron project, see it in the application window, and undo or redo any edit. This responsibility covers the project lifecycle, the command and undo engine in `core`, and the canvas shell with zoom and pan that the schematic and board editors build on.

## Current verified status

**Status:** Planned

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

- `crates/core` — Planned
- `src-tauri` — Planned
- `ui/src` — Planned

## Acceptance or verification criteria

- [ ] New, open, save, and save-as work from the menu and their keyboard shortcuts.
- [ ] A saved project reopens with identical content, verified by a golden round-trip test.
- [ ] Undo and redo restore the exact previous document state, verified by unit tests.
- [ ] The canvas zooms around the cursor and pans with the middle mouse button or space-drag.

## Remaining gaps and unknowns

- Autosave and crash-recovery snapshots are undecided.
