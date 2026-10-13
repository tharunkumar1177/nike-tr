//! Helpers for reading KiCad library files.
//!
//! The S-expression value readers live in `tron_core::kicad_value`; they return a plain
//! message on failure and callers attach the file path.

use std::fs;
use std::path::{Path, PathBuf};

use tron_core::sexpr::{self, SExpr};

use crate::error::LibraryError;

pub(crate) use tron_core::kicad_value::{
    angle, child_text, children, flag, length, number, point, points, property, required,
    stroke_width, text_arg,
};

pub(crate) fn parse_file(path: &Path) -> Result<SExpr, LibraryError> {
    let source = fs::read_to_string(path).map_err(|source| LibraryError::io(path, source))?;
    sexpr::parse(&source).map_err(|err| LibraryError::Parse {
        path: path.to_path_buf(),
        line: err.line,
        column: err.column,
        message: err.message,
    })
}

pub(crate) fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension().is_some_and(|ext| ext == extension)
}

/// The library name KiCad shows for a library file or directory: its name without extension.
pub(crate) fn library_name(path: &Path) -> Result<String, LibraryError> {
    match path.file_stem().and_then(|stem| stem.to_str()) {
        Some(stem) if !stem.is_empty() => Ok(stem.to_owned()),
        _ => Err(LibraryError::invalid(path, "has no usable library name")),
    }
}

/// The files in `dir` with the given extension, sorted by path.
pub(crate) fn library_files(dir: &Path, extension: &str) -> Result<Vec<PathBuf>, LibraryError> {
    let read_error = |source| LibraryError::io(dir, source);
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(read_error)? {
        let path = entry.map_err(read_error)?.path();
        if path.is_file() && has_extension(&path, extension) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
