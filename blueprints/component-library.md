# Component library

## Outcome or responsibility

The user can search and browse schematic symbols and PCB footprints from the open KiCad libraries without bundling 250MB into the installer. CI compiles a pinned library release into a SQLite index and publishes it as a GitHub Release asset. The app downloads it once with the user's consent, caches it, and works offline after that.

## Current verified status

**Status:** Planned

## Architecture dependencies

- [Secrets and network](../architecture.md#secrets-and-network)
- [Error and recovery](../architecture.md#error-and-recovery)
- [Build and dependency policy](../architecture.md#build-and-dependency-policy)

## Local rules and implications

### Secrets and network

The first launch asks before downloading the library. Declining leaves the app usable with an empty library, and the browser explains how to download it later.

### Error and recovery

The download resumes after interruption and is verified against a published SHA-256 checksum before it replaces the cache. A corrupt cache is detected and fetched again.

### Build and dependency policy

The KiCad library release tag and the asset checksum are pinned in the repository. The index is rebuilt in CI only.

## Related blueprints

### Required

None

### Impact checks

- [schematic-capture](schematic-capture.md) — when the symbol schema in the index changes.
- [board-layout](board-layout.md) — when the footprint schema in the index changes.

## Relevant implementation and tests

- `crates/library` — Planned

## Acceptance or verification criteria

- [ ] A CI job builds the index from a pinned KiCad library tag and attaches it to a release along with its checksum.
- [ ] A full-text search returns ranked symbols and footprints in under 50 ms on the full index.
- [ ] The library browser previews symbol and footprint graphics.
- [ ] An interrupted or corrupted download is recovered.

## Remaining gaps and unknowns

- KiCad libraries are CC-BY-SA 4.0 with a design-use exception. Redistributing them as a derived index requires attribution in the About screen. This needs confirmation.
- The 3D model library (about 5GB) is out of scope.
