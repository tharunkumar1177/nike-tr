# Component library

## Outcome or responsibility

The user can search and browse schematic symbols and PCB footprints from the open KiCad libraries without bundling 250MB into the installer. CI compiles a pinned library release into a SQLite index and publishes it as a GitHub Release asset. The app downloads it once with the user's consent, caches it, and works offline after that.

The work ships in three slices: 3a builds and publishes the index, 3b downloads and caches it in the app, and 3c adds the library browser UI.

## Current verified status

**Status:** Planned

Slice 3a code is written on branch `slice-3-library-pipeline`. None of it has been verified in CI yet.

## Architecture dependencies

- [Secrets and network](../architecture.md#secrets-and-network)
- [Error and recovery](../architecture.md#error-and-recovery)
- [Build and dependency policy](../architecture.md#build-and-dependency-policy)
- [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation)

## Local rules and implications

### Secrets and network

The first launch asks before downloading the library. Declining leaves the app usable with an empty library, and the browser explains how to download it later. The only download URL is the one in `crates/library/pinned.json`.

### Error and recovery

The download resumes after interruption and is verified against the pinned SHA-256 checksum before it replaces the cache. A corrupt cache is detected and fetched again. `Index::open` rejects any file whose SQLite `user_version` is not `SCHEMA_VERSION`, and a non-database file fails with `LibraryError::Database`.

An index build fails if any library cannot be read, including a derived symbol whose `extends` parent is missing or forms a loop. The error lists every failing library. The build writes `<output>.partial` and renames it over the output only on success.

### Build and dependency policy

The KiCad library tag is pinned as `KICAD_TAG_DEFAULT` in `.github/workflows/library.yml`. The published asset's URL and SHA-256 are pinned in `crates/library/pinned.json`. That file is written only by the workflow's pull request, never by hand. Published release assets never change: rebuilding the same tag needs a higher revision, which gives the release tag `library-kicad-<tag>-r<revision>`.

GitLab source archives aren't byte-stable, so their SHA-256 values are recorded in each release's notes for provenance rather than pinned in advance.

### Coordinate and unit representation

Preview geometry is stored as `i64` nanometers in a Y-down space. KiCad symbol files put Y up, so the indexer negates symbol Y coordinates. Footprint files are already Y-down. Pad rotation is stored in tenths of a degree.

### Index format

- One SQLite file with tables `meta`, `symbols`, and `footprints`, plus an FTS5 table `search` over name, library, keywords (footprint tags), and description. Prefix indexes cover 1 to 3 characters.
- Each row stores preview geometry as JSON (`preview` module types), so the UI never parses KiCad files.
- `meta` records `kicad_tag`, `symbol_count`, `footprint_count`, and `attribution`.
- Symbol previews draw unit 1 (or the lowest unit present) in body style 1, together with the unit 0 graphics shared by all units. Hidden pins are counted but not drawn.
- Derived symbols take graphics and pins from the root of their `extends` chain. Each property comes from the nearest symbol in the chain that defines it.
- Footprint previews keep pads plus graphics on `F.SilkS`, `F.Fab`, and `F.CrtYd`.

### Search

User input is split on whitespace and `:`, so `Device:R` searches the library and the name together. Each term is quoted and matched as a word prefix, and every term must match. FTS5 syntax in the input is therefore searched as plain text. Results rank exact name matches first, then by BM25 with weights of 10 for the name, 5 for keywords, 2 for the library, and 1 for the description.

## Related blueprints

### Required

None

### Impact checks

- [schematic-capture](schematic-capture.md) — when the symbol schema in the index changes.
- [board-layout](board-layout.md) — when the footprint schema in the index changes.

## Relevant implementation and tests

- `crates/library/src/kicad_sym.rs`, `kicad_mod.rs` — reading KiCad 10 symbol (`.kicad_symdir` and `.kicad_sym`) and footprint (`.pretty`) libraries
- `crates/library/src/index.rs` — schema, writer, and the read-only `Index` with `search`, `symbol`, `footprint`, and `meta`
- `crates/library/src/search.rs` — the FTS5 query sanitizer
- `crates/library/src/builder.rs` and `src/bin/build_index.rs` — the index build
- `crates/library/src/bin/search_benchmark.rs` — the release gate for size, latency, and ranking
- `crates/library/tests/index.rs` — integration tests on real KiCad 10.0.7 fixtures in `crates/library/tests/fixtures/kicad`
- `.github/workflows/library.yml` — builds and benchmarks the full index on every push that touches the library code; a manual run on `main` publishes and pins it

## Acceptance or verification criteria

Slice 3a:

- [ ] A CI job builds the index from a pinned KiCad library tag and attaches it to a release along with its checksum.
- [ ] A full-text search returns ranked symbols and footprints in under 50 ms on the full index.
- [ ] The publish run opens a pull request that pins the asset URL and SHA-256, and CI runs on it.

Slice 3b:

- [ ] Declining the download leaves the app usable with an empty library.
- [ ] An interrupted or corrupted download is recovered.

Slice 3c:

- [ ] The library browser previews symbol and footprint graphics.
- [ ] The browser shows the KiCad attribution stored in the index.

## Remaining gaps and unknowns

- KiCad libraries are CC-BY-SA 4.0 with a design-use exception. Redistributing them as a derived index requires attribution. The index stores the attribution text, and release notes include it. The exact wording still needs confirmation.
- The 3D model library (about 5GB) is out of scope.
- Custom pad outlines are previewed as their anchor shape, and symbol text is not stored.
- Publishing needs the repository setting "Allow GitHub Actions to create and approve pull requests" to be enabled.
