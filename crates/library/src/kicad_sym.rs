//! Reads KiCad symbol libraries. A library is either a `.kicad_symdir` directory holding one
//! `.kicad_sym` file per symbol, or a single `.kicad_sym` file holding every symbol.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tron_core::sexpr::SExpr;
use tron_core::symbol::{symbol_graphics, unit_and_style};

use crate::error::LibraryError;
use crate::kicad::{
    child_text, children, flag, library_files, library_name, parse_file, property, text_arg,
};
use crate::preview::SymbolPreview;

/// One symbol as stored in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    pub lib: String,
    pub name: String,
    pub description: String,
    pub keywords: String,
    /// A URL, or empty.
    pub datasheet: String,
    /// The reference designator prefix, such as `R` or `U`.
    pub reference: String,
    /// The default footprint as `Library:Footprint`, or empty.
    pub footprint: String,
    /// Space-separated footprint name patterns that fit the symbol.
    pub footprint_filters: String,
    /// The symbol this one takes its graphics and pins from.
    pub extends: Option<String>,
    pub unit_count: u32,
    /// The number of distinct pin numbers across every unit.
    pub pin_count: u32,
    pub power: bool,
    pub preview: SymbolPreview,
    /// The symbol's own `(symbol ...)` definition as KiCad text, before merging with any symbol
    /// it extends. [`flatten`] turns it into the form a schematic embeds.
    pub source: String,
}

struct Definition<'a> {
    path: &'a Path,
    node: &'a SExpr,
}

/// Reads every symbol in the library at `path`, sorted by name.
pub fn read_symbol_library(path: &Path) -> Result<Vec<SymbolRecord>, LibraryError> {
    let lib = library_name(path)?;
    let files = if path.is_dir() {
        library_files(path, "kicad_sym")?
    } else {
        vec![path.to_path_buf()]
    };
    let mut documents = Vec::with_capacity(files.len());
    for file in files {
        let root = parse_file(&file)?;
        if root.head() != Some("kicad_symbol_lib") {
            return Err(LibraryError::invalid(
                &file,
                "is not a KiCad symbol library",
            ));
        }
        documents.push((file, root));
    }

    let mut definitions = BTreeMap::new();
    for (file, root) in &documents {
        for node in children(root, "symbol") {
            let Some(name) = text_arg(node, 0) else {
                return Err(LibraryError::invalid(file, "has a symbol without a name"));
            };
            let definition = Definition { path: file, node };
            if definitions.insert(name, definition).is_some() {
                let message = format!("defines symbol {name:?}, which {lib} already has");
                return Err(LibraryError::invalid(file, message));
            }
        }
    }

    let mut records = Vec::with_capacity(definitions.len());
    for (name, definition) in &definitions {
        match symbol(&lib, name, &definitions) {
            Ok(record) => records.push(record),
            Err(message) => {
                let message = format!("symbol {name:?}: {message}");
                return Err(LibraryError::invalid(definition.path, message));
            }
        }
    }
    Ok(records)
}

fn symbol(
    lib: &str,
    name: &str,
    definitions: &BTreeMap<&str, Definition<'_>>,
) -> Result<SymbolRecord, String> {
    let chain = ancestry(name, definitions)?;
    let own = chain[0];
    let base = chain[chain.len() - 1];
    let text = |key: &str| {
        chain
            .iter()
            .find_map(|definition| property(definition.node, key))
            .unwrap_or_default()
            .to_owned()
    };

    let graphics = symbol_graphics(base.node)?;

    let mut datasheet = text("Datasheet");
    if datasheet == "~" {
        datasheet.clear();
    }
    Ok(SymbolRecord {
        lib: lib.to_owned(),
        name: name.to_owned(),
        description: text("Description"),
        keywords: text("ki_keywords"),
        datasheet,
        reference: text("Reference"),
        footprint: text("Footprint"),
        footprint_filters: text("ki_fp_filters"),
        extends: child_text(own.node, "extends").map(str::to_owned),
        unit_count: graphics.unit_count,
        pin_count: graphics.pin_count,
        power: chain
            .iter()
            .any(|definition| flag(definition.node, "power")),
        preview: graphics.preview,
        source: own.node.to_pretty_string().trim_end().to_owned(),
    })
}

/// Merges a symbol with the symbols it extends into the standalone form a schematic embeds in
/// `(lib_symbols)`, named `<lib>:<name>`. `chain` holds the symbol's own definition followed by
/// each symbol it extends, ending with the one that has the unit drawings.
///
/// Properties and settings such as `(in_bom yes)` come from the nearest definition that has
/// them. Unit drawings come from the last definition, renamed after the symbol.
pub fn flatten(lib: &str, chain: &[SExpr]) -> Result<SExpr, String> {
    let (Some(own), Some(base)) = (chain.first(), chain.last()) else {
        return Err("there is no definition to flatten".to_owned());
    };
    let name = definition_name(own)?;
    let base_name = definition_name(base)?;
    if let Some(parent) = child_text(base, "extends") {
        return Err(format!(
            "{base_name:?} extends {parent:?}, which is missing"
        ));
    }

    let mut settings = Vec::new();
    let mut properties = Vec::new();
    for definition in chain.iter().rev() {
        for item in definition.args().iter().skip(1) {
            match item.head() {
                Some("extends" | "symbol") => {}
                Some("property") => merge(&mut properties, item, same_property),
                _ => merge(&mut settings, item, same_setting),
            }
        }
    }

    let mut units = Vec::new();
    for unit in children(base, "symbol") {
        let unit_name = text_arg(unit, 0).unwrap_or_default();
        let Some((number, style)) = unit_and_style(unit_name) else {
            return Err(format!(
                "unit {unit_name:?} is not named <symbol>_<unit>_<style>"
            ));
        };
        let mut renamed = unit.clone();
        if let Some(items) = renamed.items_mut() {
            items[1] = SExpr::string(format!("{name}_{number}_{style}"));
        }
        units.push(renamed);
    }

    let (trailing, leading): (Vec<_>, Vec<_>) = settings
        .into_iter()
        .partition(|item| item.head() == Some("embedded_fonts"));
    let full_name = format!("{lib}:{name}");
    let mut items = vec![SExpr::atom("symbol"), SExpr::string(full_name)];
    items.extend(leading);
    items.extend(properties);
    items.extend(units);
    items.extend(trailing);
    Ok(SExpr::List(items))
}

fn definition_name(node: &SExpr) -> Result<&str, String> {
    if node.head() != Some("symbol") {
        return Err("a definition is not a (symbol) list".to_owned());
    }
    match text_arg(node, 0) {
        Some(name) => Ok(name),
        None => Err("a definition has no name".to_owned()),
    }
}

/// Replaces the item in `items` that `same` matches with `item`, or appends `item`.
fn merge(items: &mut Vec<SExpr>, item: &SExpr, same: impl Fn(&SExpr, &SExpr) -> bool) {
    match items.iter_mut().find(|existing| same(existing, item)) {
        Some(existing) => *existing = item.clone(),
        None => items.push(item.clone()),
    }
}

fn same_property(a: &SExpr, b: &SExpr) -> bool {
    text_arg(a, 0) == text_arg(b, 0)
}

/// Settings are lists such as `(in_bom yes)`, matched by head, or older bare flags such as
/// `power`, matched by text.
fn same_setting(a: &SExpr, b: &SExpr) -> bool {
    match (a, b) {
        (SExpr::List(_), SExpr::List(_)) => a.head().is_some() && a.head() == b.head(),
        (SExpr::Atom(a), SExpr::Atom(b)) => a == b,
        _ => false,
    }
}

/// The symbol followed by each symbol it extends, ending with the one that has the graphics.
fn ancestry<'m, 'a>(
    name: &str,
    definitions: &'m BTreeMap<&'a str, Definition<'a>>,
) -> Result<Vec<&'m Definition<'a>>, String> {
    let mut chain = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current = name;
    loop {
        if !visited.insert(current) {
            return Err(format!("its extends chain loops back to {current:?}"));
        }
        let Some(definition) = definitions.get(current) else {
            return Err(format!(
                "it extends {current:?}, which is not in the library"
            ));
        };
        chain.push(definition);
        match child_text(definition.node, "extends") {
            Some(parent) => current = parent,
            None => return Ok(chain),
        }
    }
}

#[cfg(test)]
mod tests {
    use tron_core::sexpr;

    use super::*;

    #[test]
    fn flattening_needs_the_whole_extends_chain() {
        let derived = sexpr::parse(r#"(symbol "B" (extends "A"))"#).unwrap();
        let error = flatten("Lib", &[derived]).unwrap_err();
        assert_eq!(error, r#""B" extends "A", which is missing"#);
    }
}
