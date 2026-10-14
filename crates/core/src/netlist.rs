//! The netlist derived from schematic connectivity.
//!
//! Items connect where their connection points share an exact coordinate. Labels with the
//! same text join, and global labels and power symbols join by name across the design. Each
//! net is named after its highest-priority driver, in KiCad's order.

use std::collections::{BTreeMap, HashMap};

use crate::kicad::{PinConnection, Schematic};
use crate::symbol::is_power_symbol;
use crate::units::Point;
use crate::wiring::LabelKind;

/// One symbol pin on a net.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetNode {
    pub reference: String,
    pub pin: String,
    /// The pin name, empty when the pin has none.
    pub pin_function: String,
    /// The KiCad electrical type, such as `passive` or `power_in`.
    pub pin_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    pub name: String,
    /// Sorted by reference, then pin number.
    pub nodes: Vec<NetNode>,
}

/// What names a net, lowest priority first, as KiCad ranks drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    LocalLabel,
    Power,
    GlobalLabel,
}

#[derive(Default)]
pub(crate) struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    pub(crate) fn add(&mut self) -> usize {
        self.parent.push(self.parent.len());
        self.parent.len() - 1
    }

    pub(crate) fn find(&mut self, mut element: usize) -> usize {
        while self.parent[element] != element {
            let grandparent = self.parent[self.parent[element]];
            self.parent[element] = grandparent;
            element = grandparent;
        }
        element
    }

    pub(crate) fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[b] = a;
        }
    }
}

struct PinEntry {
    element: usize,
    node: NetNode,
    /// Another pin of the same symbol has this pin's name.
    name_has_duplicates: bool,
}

#[derive(Default)]
struct Builder {
    set: DisjointSet,
    points: HashMap<Point, usize>,
    names: HashMap<String, usize>,
    /// Connection items per element: wires, labels, and pins.
    items: Vec<usize>,
    drivers: Vec<(usize, Priority, String)>,
    pins: Vec<PinEntry>,
}

impl Builder {
    fn element(&mut self) -> usize {
        self.items.push(0);
        self.set.add()
    }

    fn point(&mut self, point: Point) -> usize {
        if let Some(element) = self.points.get(&point) {
            return *element;
        }
        let element = self.element();
        self.points.insert(point, element);
        element
    }

    /// The element shared by every item named `key`.
    fn name(&mut self, key: String) -> usize {
        if let Some(element) = self.names.get(&key) {
            return *element;
        }
        let element = self.element();
        self.names.insert(key, element);
        element
    }

    /// Joins `element` to the named net and records it as a driver.
    fn drive(&mut self, element: usize, key: String, priority: Priority, name: String) {
        let named = self.name(key);
        self.set.union(element, named);
        self.drivers.push((element, priority, name));
    }

    fn add_pins(&mut self, reference: &str, value: &str, power: bool, pins: Vec<PinConnection>) {
        let mut name_counts: HashMap<&str, usize> = HashMap::new();
        for pin in &pins {
            *name_counts.entry(pin.name.as_str()).or_default() += 1;
        }
        let duplicates: Vec<bool> = pins
            .iter()
            .map(|pin| name_counts[pin.name.as_str()] > 1)
            .collect();
        for (pin, name_has_duplicates) in pins.into_iter().zip(duplicates) {
            let element = self.point(pin.position);
            self.items[element] += 1;
            if power && pin.electrical == "power_in" {
                let name = value.to_owned();
                self.drive(element, global_key(&name), Priority::Power, name);
                continue;
            }
            if pin.hidden && pin.electrical == "power_in" && !pin.name.is_empty() {
                let name = pin.name.clone();
                self.drive(element, global_key(&name), Priority::Power, name);
            }
            if reference.starts_with('#') {
                continue;
            }
            let node = NetNode {
                reference: reference.to_owned(),
                pin: pin.number,
                pin_function: pin.name,
                pin_type: pin.electrical,
            };
            self.pins.push(PinEntry {
                element,
                node,
                name_has_duplicates,
            });
        }
    }
}

fn global_key(name: &str) -> String {
    format!("global:{name}")
}

/// KiCad's automatic name for a net driven only by pins, after the pin that names it:
/// `Net-(R1-Pad1)`, or `Net-(U1-VCC)` for a named pin. A lone pin is `unconnected-(...)`,
/// and a named pin that is unconnected or shares its name gets its number too.
fn automatic_name(pin: &PinEntry, unconnected: bool) -> String {
    let prefix = if unconnected { "unconnected" } else { "Net" };
    let node = &pin.node;
    let pin_name = if node.pin_function.is_empty() {
        format!("Pad{}", node.pin)
    } else if unconnected || pin.name_has_duplicates {
        format!("{}-Pad{}", node.pin_function, node.pin)
    } else {
        node.pin_function.clone()
    };
    format!("{prefix}-({}-{pin_name})", node.reference)
}

/// Derives the nets of a schematic. Every net with at least one symbol pin is listed,
/// sorted by name. Fails when a placed symbol's definition is missing or unreadable.
pub fn netlist(schematic: &Schematic) -> Result<Vec<Net>, String> {
    let mut builder = Builder::default();
    for wire in schematic.wires() {
        let start = builder.point(wire.start);
        let end = builder.point(wire.end);
        builder.set.union(start, end);
        builder.items[start] += 1;
    }
    for label in schematic.labels() {
        let element = builder.point(label.position);
        builder.items[element] += 1;
        let text = label.text;
        match label.kind {
            LabelKind::Local => {
                let key = format!("local:{text}");
                builder.drive(element, key, Priority::LocalLabel, format!("/{text}"));
            }
            LabelKind::Global(_) => {
                builder.drive(element, global_key(&text), Priority::GlobalLabel, text);
            }
        }
    }
    for symbol in schematic.symbols() {
        let reference = symbol.reference().to_owned();
        let Some(definition) = schematic.lib_symbol(&symbol.lib_id) else {
            return Err(format!(
                "{reference} uses {}, which is not embedded in the schematic.",
                symbol.lib_id
            ));
        };
        let pins = symbol
            .pin_connections(definition)
            .map_err(|error| format!("{reference} ({}): {error}", symbol.lib_id))?;
        let value = symbol.property("Value").unwrap_or_default().to_owned();
        let power = is_power_symbol(definition);
        builder.add_pins(&reference, &value, power, pins);
    }
    Ok(builder.finish())
}

impl Builder {
    fn finish(mut self) -> Vec<Net> {
        let mut item_counts: HashMap<usize, usize> = HashMap::new();
        for (element, count) in self.items.iter().enumerate() {
            let root = self.set.find(element);
            *item_counts.entry(root).or_default() += count;
        }
        let mut best_drivers: HashMap<usize, (Priority, String)> = HashMap::new();
        for (element, priority, name) in std::mem::take(&mut self.drivers) {
            let root = self.set.find(element);
            let candidate = (priority, name);
            let better = best_drivers
                .get(&root)
                .is_none_or(|best| outranks(&candidate, best));
            if better {
                best_drivers.insert(root, candidate);
            }
        }
        let mut groups: BTreeMap<usize, Vec<PinEntry>> = BTreeMap::new();
        for pin in std::mem::take(&mut self.pins) {
            let root = self.set.find(pin.element);
            groups.entry(root).or_default().push(pin);
        }
        let mut nets: Vec<Net> = groups
            .into_iter()
            .map(|(root, mut pins)| {
                pins.sort_by(|a, b| a.node.cmp(&b.node));
                let name = match best_drivers.remove(&root) {
                    Some((_, name)) => name,
                    None => {
                        let unconnected = item_counts.get(&root) == Some(&1);
                        automatic_name(&pins[0], unconnected)
                    }
                };
                let nodes = pins.into_iter().map(|pin| pin.node).collect();
                Net { name, nodes }
            })
            .collect();
        nets.sort_by(|a, b| (&a.name, &a.nodes).cmp(&(&b.name, &b.nodes)));
        nets
    }
}

/// Whether driver `a` names a net over `b`: a higher priority wins, and within a priority
/// the alphabetically first name does.
fn outranks(a: &(Priority, String), b: &(Priority, String)) -> bool {
    a.0 > b.0 || (a.0 == b.0 && a.1 < b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(reference: &str, pin: &str, name: &str, duplicates: bool) -> PinEntry {
        PinEntry {
            element: 0,
            node: NetNode {
                reference: reference.to_owned(),
                pin: pin.to_owned(),
                pin_function: name.to_owned(),
                pin_type: "passive".to_owned(),
            },
            name_has_duplicates: duplicates,
        }
    }

    #[test]
    fn automatic_names_follow_kicad() {
        assert_eq!(
            automatic_name(&entry("R1", "1", "", false), false),
            "Net-(R1-Pad1)"
        );
        assert_eq!(
            automatic_name(&entry("R1", "2", "", false), true),
            "unconnected-(R1-Pad2)"
        );
        assert_eq!(
            automatic_name(&entry("U1", "8", "VCC", false), false),
            "Net-(U1-VCC)"
        );
        assert_eq!(
            automatic_name(&entry("U1", "4", "GND", true), false),
            "Net-(U1-GND-Pad4)"
        );
        assert_eq!(
            automatic_name(&entry("U1", "3", "IN", false), true),
            "unconnected-(U1-IN-Pad3)"
        );
    }

    #[test]
    fn the_disjoint_set_joins_transitively() {
        let mut set = DisjointSet::default();
        let elements: Vec<usize> = (0..4).map(|_| set.add()).collect();
        set.union(elements[0], elements[1]);
        set.union(elements[2], elements[1]);
        assert_eq!(set.find(elements[0]), set.find(elements[2]));
        assert_ne!(set.find(elements[0]), set.find(elements[3]));
    }
}
