# Autorouting

## Outcome or responsibility

The user can autoroute a placed board. Tron exports a Specctra DSN file, runs the bundled freerouting engine, and imports the resulting SES session as tracks and vias that the user reviews and keeps or discards.

## Current verified status

**Status:** Planned

## Architecture dependencies

- [Error and recovery](../architecture.md#error-and-recovery)
- [Build and dependency policy](../architecture.md#build-and-dependency-policy)
- [Design document ownership](../architecture.md#design-document-ownership)

## Local rules and implications

### Error and recovery

Freerouting runs as a cancellable child process with progress reporting. If it fails or is cancelled, the board is untouched.

### Build and dependency policy

The freerouting JAR and a trimmed JRE are downloaded in CI from pinned URLs with pinned checksums. Bundling them needs user approval because they add about 40MB.

### Design document ownership

The imported routing is applied as a single undoable command.

## Related blueprints

### Required

- [board-layout](board-layout.md) — the board schema, design rules, and DRC.

### Impact checks

- [build-and-release](build-and-release.md) — when the JRE or freerouting bundle changes.

## Relevant implementation and tests

- `crates/core` — Planned
- `src-tauri` — Planned

## Acceptance or verification criteria

- [ ] A DSN export of a reference board loads into freerouting, verified in CI.
- [ ] Importing the SES result produces a board that passes DRC.
- [ ] Unrouted nets are reported after import.

## Remaining gaps and unknowns

- The bundle size approval is pending.
