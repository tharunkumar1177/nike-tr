# Build and release

## Outcome or responsibility

Every push is installed, linted, type-checked, tested, and built into a Windows installer by GitHub Actions. CI is the only place Tron is built, so this workflow is the release gate. It also produces the lockfiles that get committed. Later slices extend it to macOS and Linux.

## Current verified status

**Status:** Implemented (Windows only)

Verified on 2026-10-10 by these CI runs:

- [Run 37982619266](https://github.com/tharunkumar1177/tron/actions/runs/37982619266): bootstrap run. It generated the `lockfiles` artifact, and `cargo fmt --check` failed the run as intended.
- [Run 37983461090](https://github.com/tharunkumar1177/tron/actions/runs/37983461090): the first run on committed lockfiles. A failing `cargo test` failed the run.
- [Run 37985560054](https://github.com/tharunkumar1177/tron/actions/runs/37985560054): all four jobs green, including the IPC bindings drift check. It uploaded `windows-installer` (NSIS and MSI).
- [Run 38166582257](https://github.com/tharunkumar1177/tron/actions/runs/38166582257) (pull request #1): green, and `Publish GitHub Release` was skipped.
- [Run 38168340413](https://github.com/tharunkumar1177/tron/actions/runs/38168340413) (merge to `main`): green, and it published [v0.1.0-build.6](https://github.com/tharunkumar1177/tron/releases/tag/v0.1.0-build.6) on commit `54fe35e`. Checked with `gh release download`: the `.exe` has an `MZ` header and the `.msi` has an OLE compound-file header.

## Architecture dependencies

- [Build and dependency policy](../architecture.md#build-and-dependency-policy)
- [Verification boundaries](../architecture.md#verification-boundaries)
- [Component model](../architecture.md#component-model)

## Local rules and implications

### Build and dependency policy

When a lockfile is missing, the workflow generates it and uploads it as an artifact named `lockfiles`. Once lockfiles are committed, CI installs with `npm ci` and `cargo --locked`, so dependency drift fails the build. Downloaded binaries must have their checksums verified before use.

### Verification boundaries

The jobs map onto the verification levels: Rust format, clippy with warnings denied, and tests; TypeScript eslint, `tsc --noEmit`, and Vitest; then the Tauri bundle. The bundle job depends on the check jobs passing.

Once dependencies are installed, every check step in a job runs even if an earlier check failed, so one run reports all failures. When the Prettier check fails, the step runs `prettier --write` and prints the `git diff`, because no formatter runs locally. `cargo fmt --check` prints its own diff.

### Releases

Only pushes to `main`, which include merged PRs, publish a GitHub Release. The `release` job runs after `bundle` and attaches the NSIS and MSI installers to the tag `v<version>-build.<run number>`, where the version comes from `src-tauri/tauri.conf.json`. Re-running a run uploads to the same tag and replaces its assets. Pull-request and branch runs publish only the `windows-installer` artifact, because unmerged code is never released. Runs on `main` are never cancelled by newer runs, so a release can't be cut off partway.

### Component model

The Cargo workspace members match the crate list in the component model. Adding a crate means updating the workspace and the architecture table together.

## Related blueprints

### Required

None

### Impact checks

- [component-library](component-library.md) — when the job that builds the library index asset changes.
- [circuit-simulation](circuit-simulation.md) — when the pinned `libngspice` download changes.

## Relevant implementation and tests

- `.github/workflows/ci.yml` — jobs `lockfiles`, then `web` and `rust`, then `bundle`, then `release` (on `main` only)
- `.cargo/config.toml` — points the ts-rs export directory at `ui/src/bindings`
- `src-tauri/tauri.conf.json` — bundle targets and icon set
- `app-icon.svg` — the icon source. CI runs `tauri icon` on it, so no binary icons are committed.
- `crates/core/src/units.rs` — unit tests for the nanometer and angle contracts
- `src-tauri/src/ipc.rs` — the `AppInfo` IPC type, its serialization test, and the ts-rs export
- `ui/src/App.test.tsx`, `ui/src/ipc.test.ts` — Vitest tests for UI states and IPC error handling

## Acceptance or verification criteria

- [x] A push to any branch runs the check jobs (Rust and TypeScript) and the Windows bundle job.
- [x] Clippy warnings, eslint errors, type errors, and failing tests each fail the run. A failing format check and a failing test were each observed failing a run. Clippy, eslint, and tsc failures fail through the same exit-code path: `-D warnings` and non-zero exit codes.
- [x] A successful run uploads the Windows installer as an artifact.
- [x] A run without committed lockfiles uploads generated lockfiles as an artifact.
- [x] A push to `main` publishes a GitHub Release with the NSIS and MSI installers attached. PR runs don't publish one.
- [x] Once lockfiles are committed, installs are locked (`npm ci`, `cargo --locked`) and fail on drift.

## Local notes

- TypeScript is pinned to 6.0.x because `typescript-eslint` 8.71 supports only `typescript <6.1.0`. Move to 7.x once its peer range allows.
- Node is pinned to 24.21.0 in CI. Vitest 5 and jsdom 30 require Node 22.12 or newer and Node 24.15 or newer.
- The bootstrap `package-lock.json` is generated on Linux with `--package-lock-only` and was confirmed to include the Windows native bindings (`@tauri-apps/cli-win32-x64-msvc`, `@rolldown/binding-win32-x64-msvc`).
- When the drift check fails, CI uploads an `ipc-bindings` artifact with the regenerated TypeScript files to commit.

## Remaining gaps and unknowns

- Code signing for Windows and macOS installers is not planned yet.
- The macOS and Linux matrix is slice 13.
- The `reviewer` subagent's sandbox can't run shell commands, so it can't use `gh`. For slice 1 the main agent read only the failing step logs with `gh` instead.
