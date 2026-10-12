//! Builds the index file from extracted KiCad symbol and footprint library releases.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::LibraryError;
use crate::index::Writer;
use crate::kicad::has_extension;
use crate::kicad_mod::read_footprint_library;
use crate::kicad_sym::read_symbol_library;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildReport {
    pub symbol_libraries: usize,
    pub symbols: usize,
    pub footprint_libraries: usize,
    pub footprints: usize,
}

/// Builds the index at `output` from the libraries directly inside `symbols_dir` and
/// `footprints_dir`.
///
/// The index is written to a `.partial` file beside `output` and renamed over it only when
/// complete, so a failed build never replaces `output`. If any library cannot be read, the build
/// fails and the error lists every such library.
pub fn build_index(
    symbols_dir: &Path,
    footprints_dir: &Path,
    kicad_tag: &str,
    output: &Path,
) -> Result<BuildReport, LibraryError> {
    let symbol_libraries = discover(symbols_dir, is_symbol_library)?;
    let footprint_libraries = discover(footprints_dir, is_footprint_library)?;
    let partial = partial_path(output);
    remove_if_present(&partial)?;
    let report = write_index(&partial, kicad_tag, &symbol_libraries, &footprint_libraries)?;
    fs::rename(&partial, output).map_err(|source| LibraryError::io(output, source))?;
    Ok(report)
}

fn is_symbol_library(path: &Path) -> bool {
    (path.is_dir() && has_extension(path, "kicad_symdir"))
        || (path.is_file() && has_extension(path, "kicad_sym"))
}

fn is_footprint_library(path: &Path) -> bool {
    path.is_dir() && has_extension(path, "pretty")
}

fn discover(dir: &Path, is_library: fn(&Path) -> bool) -> Result<Vec<PathBuf>, LibraryError> {
    let read_error = |source| LibraryError::io(dir, source);
    let mut libraries = Vec::new();
    for entry in fs::read_dir(dir).map_err(read_error)? {
        let path = entry.map_err(read_error)?.path();
        if is_library(&path) {
            libraries.push(path);
        }
    }
    if libraries.is_empty() {
        return Err(LibraryError::invalid(dir, "contains no KiCad libraries"));
    }
    libraries.sort();
    Ok(libraries)
}

fn partial_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    output.with_file_name(name)
}

fn remove_if_present(path: &Path) -> Result<(), LibraryError> {
    match fs::remove_file(path) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(LibraryError::io(path, err)),
        _ => Ok(()),
    }
}

fn write_index(
    path: &Path,
    kicad_tag: &str,
    symbol_libraries: &[PathBuf],
    footprint_libraries: &[PathBuf],
) -> Result<BuildReport, LibraryError> {
    let writer = Writer::create(path)?;
    let mut report = BuildReport {
        symbol_libraries: symbol_libraries.len(),
        symbols: 0,
        footprint_libraries: footprint_libraries.len(),
        footprints: 0,
    };
    let mut failures = Vec::new();
    for library in symbol_libraries {
        match read_symbol_library(library) {
            Ok(records) => {
                for record in &records {
                    writer.add_symbol(record)?;
                }
                report.symbols += records.len();
            }
            Err(err) => failures.push(err),
        }
    }
    for library in footprint_libraries {
        match read_footprint_library(library) {
            Ok(records) => {
                for record in &records {
                    writer.add_footprint(record)?;
                }
                report.footprints += records.len();
            }
            Err(err) => failures.push(err),
        }
    }
    if !failures.is_empty() {
        return Err(LibraryError::Sources(failures));
    }
    writer.finish(kicad_tag, &report)?;
    Ok(report)
}
