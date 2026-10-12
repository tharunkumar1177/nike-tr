//! Helpers for reading KiCad library files and the values inside their S-expression trees.
//!
//! Value readers return a plain message on failure; callers attach the file path.

use std::fs;
use std::path::{Path, PathBuf};

use tron_core::sexpr::{self, SExpr};
use tron_core::units::{Angle, Nm, Point};

use crate::error::LibraryError;

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

/// The children of `node` whose head is `head`.
pub(crate) fn children<'a>(node: &'a SExpr, head: &str) -> impl Iterator<Item = &'a SExpr> {
    node.args()
        .iter()
        .filter(move |item| item.head() == Some(head))
}

/// The child of `node` whose head is `head`, or a message naming both.
pub(crate) fn required<'a>(node: &'a SExpr, head: &str) -> Result<&'a SExpr, String> {
    let parent = node.head().unwrap_or("?");
    node.child(head)
        .ok_or_else(|| format!("({parent}) has no ({head})"))
}

/// The text of argument `index` when it is an atom or a string.
pub(crate) fn text_arg(node: &SExpr, index: usize) -> Option<&str> {
    node.args().get(index).and_then(SExpr::text)
}

/// The first argument of child `head`, such as `Resistor` in `(descr "Resistor")`.
pub(crate) fn child_text<'a>(node: &'a SExpr, head: &str) -> Option<&'a str> {
    node.child(head).and_then(|child| text_arg(child, 0))
}

/// The value of `(property "key" "value" ...)`.
pub(crate) fn property<'a>(node: &'a SExpr, key: &str) -> Option<&'a str> {
    children(node, "property")
        .find(|item| text_arg(item, 0) == Some(key))
        .and_then(|item| text_arg(item, 1))
}

/// Whether `node` holds `(head)`, `(head yes)`, or the older bare `head` atom.
pub(crate) fn flag(node: &SExpr, head: &str) -> bool {
    node.args().iter().any(|item| match item {
        SExpr::Atom(atom) => atom == head,
        SExpr::List(_) => item.head() == Some(head) && text_arg(item, 0) != Some("no"),
        SExpr::Str(_) => false,
    })
}

pub(crate) fn number(node: &SExpr, index: usize) -> Result<f64, String> {
    let head = node.head().unwrap_or("?");
    let Some(text) = text_arg(node, index) else {
        return Err(format!("({head}) is missing value {}", index + 1));
    };
    match text.parse::<f64>() {
        Ok(value) => Ok(value),
        Err(_) => Err(format!("({head}) has a non-numeric value {text:?}")),
    }
}

/// Argument `index` read as millimeters.
pub(crate) fn length(node: &SExpr, index: usize) -> Result<Nm, String> {
    let value = number(node, index)?;
    let head = node.head().unwrap_or("?");
    Nm::from_mm(value).map_err(|err| format!("({head}) length {value}: {err}"))
}

/// Argument `index` read as degrees.
pub(crate) fn angle(node: &SExpr, index: usize) -> Result<Angle, String> {
    let value = number(node, index)?;
    let head = node.head().unwrap_or("?");
    Angle::from_degrees(value).map_err(|err| format!("({head}) angle {value}: {err}"))
}

/// `(head x y ...)` read as a point in millimeters.
pub(crate) fn point(node: &SExpr) -> Result<Point, String> {
    Ok(Point::new(length(node, 0)?, length(node, 1)?))
}

/// The vertices of `(pts (xy x y) ...)`. An `(arc (start) (mid) (end))` vertex contributes its
/// three points.
pub(crate) fn points(node: &SExpr) -> Result<Vec<Point>, String> {
    let mut points = Vec::new();
    for item in required(node, "pts")?.args() {
        match item.head() {
            Some("xy") => points.push(point(item)?),
            Some("arc") => {
                for head in ["start", "mid", "end"] {
                    points.push(point(required(item, head)?)?);
                }
            }
            _ => {}
        }
    }
    Ok(points)
}

/// The stroke width of a graphic item, from `(stroke (width w))` or the older `(width w)`.
pub(crate) fn stroke_width(node: &SExpr) -> Result<Option<Nm>, String> {
    let width = node
        .child("stroke")
        .and_then(|stroke| stroke.child("width"))
        .or_else(|| node.child("width"));
    match width {
        Some(width) => length(width, 0).map(Some),
        None => Ok(None),
    }
}
