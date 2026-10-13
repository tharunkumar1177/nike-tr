# Component library

## Outcome or responsibility

The user can search and browse schematic symbols and PCB footprints from the open KiCad libraries without bundling 250MB into the installer. CI compiles a pinned library release into a SQLite index and publishes it as a GitHub Release asset. The app downloads it once with the user's consent, caches it, and works offline after that.

The work ships in three slices: 3a builds and publishes the index, 3b downloads and caches it in the app, and 3c adds the library browser UI.

## Current verified status

**Status:** Partial

Evidence for slice 3a:

- [CI run 38308133670](https://github.com/tharunkumar1177/tron/actions/runs/38308133670) is green: lint, rustfmt, clippy, the `crates/library` unit and integration tests, and the Windows installer.
- [Library run 38308133672](https://github.com/tharunkumar1177/tron/actions/runs/38308133672) built the full index from KiCad 10.0.7: 22,905 symbols from 223 libraries and 15,464 footprints from 155 libraries. `library.db.gz` is 16.5 MB. The slowest benchmark query, the single letter `r`, had a median of 30.0 ms. Every other query took under 3 ms. `R`, `LM358`, and `R_0603_1608Metric` each rank first for their own names.

- [Library run 38313118321](https://github.com/tharunkumar1177/tron/actions/runs/38313118321) on `main` published release `library-kicad-10.0.7-r1` and opened [PR #5](https://github.com/tharunkumar1177/tron/pull/5), which pins it. CI passed on that branch ([38313211491](https://github.com/tharunkumar1177/tron/actions/runs/38313211491)), and the pin is merged.

Evidence for slice 3b:

- [CI run 38315533279](https://github.com/tharunkumar1177/tron/actions/runs/38315533279) is green, including the `src-tauri/src/library.rs` tests and the Windows installer. The tests serve a real index, built from the KiCad fixtures, over a local HTTP server. They cover a whole download, resuming after a cut connection, a server that ignores or refuses the range, cancelling, rejecting a checksum mismatch, and discarding stale or damaged caches.

Slice 3c is Planned. The first-launch consent prompt moved to 3c along with the rest of the library UI. Slice 3b ships its state and commands.

## Architecture dependencies

- [Secrets and network](../architecture.md#secrets-and-network)
- [Error and recovery](../architecture.md#error-and-recovery)
- [Build and dependency policy](../architecture.md#build-and-dependency-policy)
- [Coordinate and unit representation](../architecture.md#coordinate-and-unit-representation)

## Local rules and implications

### Secrets and network

The first launch asks before downloading the library. Declining leaves the app usable with an empty library, and the browser explains how to download it later. The only download URL is the one in `crates/library/pinned.json`, and the client allows HTTPS only.

The answer is stored as `libraryDownload` in the app settings file. The app makes no request until the user agrees. After that, each launch resumes the download if no valid index is cached.

### Download and cache

- The cache lives in `<app local data>/library`. The download goes to `<sha256>.gz.partial`, named by the pinned checksum, so bytes from a different pin are never mixed in. Partial files from other pins are deleted.
- Resume sends `Range: bytes=<n>-`. A 206 with a matching `Content-Range` is appended. A 200 restarts the file. A 416 or a mismatched range retries once without a range. A file larger than the pinned size is rejected.
- Cancelling uses a `CancellationToken` that is checked between chunks. The partial file is flushed and synced, so the next attempt resumes.
- Install steps, in order: verify the SHA-256 of the compressed file, unpack it to `library.db.unpacked`, check that it opens through `Index::open`, rename it over `library.db`, then write `library.db.sha256` atomically. A crash before that last write leaves the cache marked stale, so it is fetched again.
- At launch, a cache whose `library.db.sha256` doesn't match the pin, or that fails to open, is deleted. A search that fails on an open index also discards the cache and downloads it again if the user agreed.
- IPC: `get_library_status`, `download_library` (records consent, starts in the background), `decline_library_download`, `cancel_library_download`, and `search_library(query, limit ≤ 200)`. Every status change, including progress at most every 100 ms, is sent as the `library-status` event with a `LibraryStatus` payload: `missing`, `downloading`, `installing`, or `ready`. A failed download shows up as `missing` with its `error`.

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
- `crates/library/src/pinned.rs` — the typed, validated `pinned.json`
- `src-tauri/src/library.rs` — consent, download, verify, install, cache checks, IPC commands, and their tests

## Acceptance or verification criteria

Slice 3a:

- [x] The publish run attaches the index to a release along with its checksum.
- [x] A full-text search returns ranked symbols and footprints in under 50 ms on the full index.
- [x] A CI job builds the full index from the pinned KiCad tag, and its integration tests run on real KiCad 10.0.7 files.
- [x] The publish run opens a pull request that pins the asset URL and SHA-256, and CI runs on it.

Slice 3b:

- [x] Declining the download is recorded, makes no network request, and leaves search answering `libraryNotReady` rather than failing the app.
- [x] An interrupted or corrupted download is recovered.

Slice 3c:

- [ ] The first launch asks before downloading. Declining leaves the app usable with an empty library.
- [ ] The library browser previews symbol and footprint graphics.
- [ ] The browser shows the KiCad attribution stored in the index.

## Remaining gaps and unknowns

- KiCad libraries are CC-BY-SA 4.0 with a design-use exception. Redistributing them as a derived index requires attribution. The index stores the attribution text, and release notes include it. The exact wording still needs confirmation.
- The 3D model library (about 5GB) is out of scope.
- Custom pad outlines are previewed as their anchor shape, and symbol text is not stored.
- Publishing needs the repository setting "Allow GitHub Actions to create and approve pull requests". It was enabled on 2026-10-13.
- The real download from GitHub Releases, which redirects to its asset host, has not been checked in an installed build. CI tests use a local server.
