# Build and release

## Outcome or responsibility

Every push is installed, linted, type-checked, tested, and built into a Windows installer by GitHub Actions. CI is the only place Tron is built, so this workflow is the release gate. It also produces the lockfiles that get committed. Later slices extend it to macOS and Linux.

## Current verified status

**Status:** Planned

No workflow exists yet. Slice 1 delivers it.

## Architecture dependencies

- [Build and dependency policy](../architecture.md#build-and-dependency-policy)
- [Verification boundaries](../architecture.md#verification-boundaries)
- [Component model](../architecture.md#component-model)

## Local rules and implications

### Build and dependency policy

When a lockfile is missing, the workflow generates it and uploads it as an artifact named `lockfiles`. Once lockfiles are committed, CI installs with `npm ci` and `cargo --locked`, so dependency drift fails the build. Downloaded binaries must have their checksums verified before use.

### Verification boundaries

The jobs map onto the verification levels: Rust format, clippy with warnings denied, and tests; TypeScript eslint, `tsc --noEmit`, and Vitest; then the Tauri bundle. The bundle job depends on the check jobs passing.

### Component model

The Cargo workspace members match the crate list in the component model. Adding a crate means updating the workspace and the architecture table together.

## Related blueprints

### Required

None

### Impact checks

- [component-library](component-library.md) — when the job that builds the library index asset changes.
- [circuit-simulation](circuit-simulation.md) — when the pinned `libngspice` download changes.

## Relevant implementation and tests

- `.github/workflows/ci.yml` — Planned
- `src-tauri/tauri.conf.json` — Planned

## Acceptance or verification criteria

- [ ] A push to any branch runs the check jobs (Rust and TypeScript) and the Windows bundle job.
- [ ] Clippy warnings, eslint errors, type errors, and failing tests each fail the run.
- [ ] A successful run uploads the Windows installer as an artifact.
- [ ] A run without committed lockfiles uploads generated lockfiles as an artifact.
- [ ] Once lockfiles are committed, installs are locked and fail on drift.

## Remaining gaps and unknowns

- Code signing for Windows and macOS installers is not planned yet.
- The macOS and Linux matrix is slice 13.
