//! The SPICE deck for a schematic and an analysis.

use std::collections::{BTreeMap, HashMap, HashSet};

use tron_core::kicad::Schematic;
use tron_core::{Net, PlacedSymbol, netlist};

use crate::analysis::Analysis;
use crate::error::PreflightError;
use crate::model::device;

const TITLE: &str = "* Tron simulation";

/// The SPICE ground node.
pub const GROUND: &str = "0";

/// A SPICE deck ready for ngspice, with the names that map its results back to the
/// schematic.
#[derive(Debug, Clone, PartialEq)]
pub struct Deck {
    /// The title, one line per device, the analysis, and `.end`.
    pub lines: Vec<String>,
    /// Every SPICE node a device uses, other than ground, mapped to its net name.
    pub nodes: BTreeMap<String, String>,
    /// Every SPICE device name mapped to its symbol reference.
    pub devices: BTreeMap<String, String>,
}

impl Deck {
    pub fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }
}

/// Builds the deck that runs `analysis` on `schematic`, after checking that the circuit can
/// be simulated. Symbols excluded from simulation, power symbols, and other `#` symbols are
/// left out.
pub fn deck(schematic: &Schematic, analysis: Option<&Analysis>) -> Result<Deck, PreflightError> {
    let analysis = analysis.ok_or(PreflightError::NoAnalysis)?;
    let directive = analysis
        .directive()
        .map_err(PreflightError::InvalidAnalysis)?;
    let nets = netlist(schematic).map_err(PreflightError::Netlist)?;
    let node_names = node_names(&nets);
    let mut pin_nets: HashMap<&str, HashMap<&str, (&str, &str)>> = HashMap::new();
    for net in &nets {
        let node = node_names[net.name.as_str()].as_str();
        for pin in &net.nodes {
            pin_nets
                .entry(pin.reference.as_str())
                .or_default()
                .insert(pin.pin.as_str(), (node, net.name.as_str()));
        }
    }

    let mut deck = Deck {
        lines: vec![TITLE.to_owned()],
        nodes: BTreeMap::new(),
        devices: BTreeMap::new(),
    };
    let mut instance_names = HashSet::new();
    let mut grounded = false;
    for symbol in schematic.symbols() {
        let reference = symbol.reference();
        if reference.starts_with('#') || excluded(schematic, &symbol) {
            continue;
        }
        if reference.ends_with('?') {
            return Err(PreflightError::Unannotated(reference.to_owned()));
        }
        let pin_numbers = pin_numbers(schematic, &symbol)?;
        let device = device(&symbol, &pin_numbers).map_err(|reason| PreflightError::NoModel {
            reference: reference.to_owned(),
            reason,
        })?;
        let name = device.element.instance_name(reference);
        if !instance_names.insert(name.to_ascii_lowercase()) {
            return Err(PreflightError::DuplicateReference(reference.to_owned()));
        }
        let mut line = name.clone();
        for pin in &device.pins {
            let found = pin_nets
                .get(reference)
                .and_then(|pins| pins.get(pin.as_str()));
            let Some(&(node, net)) = found else {
                return Err(PreflightError::Netlist(format!(
                    "{reference} pin {pin} is not on any net."
                )));
            };
            if node == GROUND {
                grounded = true;
            } else {
                deck.nodes.insert(node.to_owned(), net.to_owned());
            }
            line.push(' ');
            line.push_str(node);
        }
        line.push(' ');
        line.push_str(&device.value);
        deck.lines.push(line);
        deck.devices.insert(name, reference.to_owned());
    }
    if deck.devices.is_empty() {
        return Err(PreflightError::Empty);
    }
    if !grounded {
        return Err(PreflightError::NoGround);
    }
    deck.lines.push(directive);
    deck.lines.push(".end".to_owned());
    Ok(deck)
}

/// Whether the symbol is excluded from simulation, by KiCad's `(exclude_from_sim yes)` or
/// the older `Sim.Enable` field set to `0`.
fn excluded(schematic: &Schematic, symbol: &PlacedSymbol) -> bool {
    schematic.symbol_flag(symbol.uuid, "exclude_from_sim")
        || symbol.property("Sim.Enable").map(str::trim) == Some("0")
}

fn pin_numbers(
    schematic: &Schematic,
    symbol: &PlacedSymbol,
) -> Result<Vec<String>, PreflightError> {
    let reference = symbol.reference();
    let definition = schematic.lib_symbol(&symbol.lib_id).ok_or_else(|| {
        PreflightError::Netlist(format!(
            "{reference} uses {}, which is not embedded in the schematic.",
            symbol.lib_id
        ))
    })?;
    let pins = symbol
        .pin_connections(definition)
        .map_err(|error| PreflightError::Netlist(format!("{reference}: {error}")))?;
    Ok(pins.into_iter().map(|pin| pin.number).collect())
}

/// Whether a net is ground: KiCad simulates nets named `GND` or `0` as node `0`.
fn is_ground(net: &str) -> bool {
    net == "GND" || net == GROUND
}

/// A SPICE node name for every net. Characters ngspice would misread become `_`, and
/// because ngspice ignores case, names that collide get a numeric suffix.
fn node_names(nets: &[Net]) -> HashMap<&str, String> {
    let mut taken: HashSet<String> = HashSet::from([GROUND.to_owned()]);
    let mut names = HashMap::with_capacity(nets.len());
    for net in nets {
        let name = if is_ground(&net.name) {
            GROUND.to_owned()
        } else {
            let base: String = net.name.chars().map(node_char).collect();
            let mut candidate = base.clone();
            let mut suffix = 2;
            while !taken.insert(candidate.to_ascii_lowercase()) {
                candidate = format!("{base}_{suffix}");
                suffix += 1;
            }
            candidate
        };
        names.insert(net.name.as_str(), name);
    }
    names
}

fn node_char(c: char) -> char {
    if c.is_ascii_alphanumeric() || "/_-+.".contains(c) {
        c
    } else {
        '_'
    }
}
