//! The electrical rules check: KiCad's ERC rules over the schematic's connectivity.
//!
//! Items connect the same way as in the netlist: where their connection points share an
//! exact coordinate, and by name for labels, global labels, and power symbols. Violations
//! are transient. Each has a stable `id` built from its rule and the items it names, so a
//! later exclusion can refer to it.

use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use crate::kicad::{PlacedSymbol, Schematic};
use crate::netlist::DisjointSet;
use crate::symbol::is_power_symbol;
use crate::units::Point;
use crate::wiring::LabelKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Warning,
    Error,
}

/// How a rule is reported, as `.kicad_pro` stores it in `erc.rule_severities`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleSeverity {
    Error,
    Warning,
    Ignore,
}

impl RuleSeverity {
    /// The `.kicad_pro` spelling.
    pub fn key(self) -> &'static str {
        match self {
            RuleSeverity::Error => "error",
            RuleSeverity::Warning => "warning",
            RuleSeverity::Ignore => "ignore",
        }
    }

    pub fn from_key(key: &str) -> Option<RuleSeverity> {
        match key {
            "error" => Some(RuleSeverity::Error),
            "warning" => Some(RuleSeverity::Warning),
            "ignore" => Some(RuleSeverity::Ignore),
            _ => None,
        }
    }
}

impl From<Severity> for RuleSeverity {
    fn from(severity: Severity) -> RuleSeverity {
        match severity {
            Severity::Error => RuleSeverity::Error,
            Severity::Warning => RuleSeverity::Warning,
        }
    }
}

/// The setting of each rule by key. A rule that isn't listed keeps its default.
pub type RuleSeverities = BTreeMap<String, RuleSeverity>;

/// One ERC rule: KiCad's settings key, message, and default severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErcRule {
    pub key: &'static str,
    pub message: &'static str,
    pub severity: Severity,
}

pub const UNANNOTATED: ErcRule = ErcRule {
    key: "unannotated",
    message: "Symbol is not annotated",
    severity: Severity::Error,
};
pub const DUPLICATE_REFERENCE: ErcRule = ErcRule {
    key: "duplicate_reference",
    message: "Duplicate reference designators",
    severity: Severity::Error,
};
pub const PIN_NOT_CONNECTED: ErcRule = ErcRule {
    key: "pin_not_connected",
    message: "Pin not connected",
    severity: Severity::Error,
};
pub const PIN_NOT_DRIVEN: ErcRule = ErcRule {
    key: "pin_not_driven",
    message: "Input pin not driven by any Output pins",
    severity: Severity::Error,
};
pub const POWER_PIN_NOT_DRIVEN: ErcRule = ErcRule {
    key: "power_pin_not_driven",
    message: "Input Power pin not driven by any Output Power pins",
    severity: Severity::Error,
};
/// Its severity per violation comes from the pin conflict matrix. Its setting only turns
/// it off: any setting other than ignore leaves the matrix in charge.
pub const PIN_TO_PIN: ErcRule = ErcRule {
    key: "pin_to_pin",
    message: "Conflict problem between pins",
    severity: Severity::Warning,
};
pub const LABEL_DANGLING: ErcRule = ErcRule {
    key: "label_dangling",
    message: "Label not connected",
    severity: Severity::Error,
};
pub const ISOLATED_PIN_LABEL: ErcRule = ErcRule {
    key: "isolated_pin_label",
    message: "Label connected to only one pin",
    severity: Severity::Warning,
};
pub const UNCONNECTED_WIRE_ENDPOINT: ErcRule = ErcRule {
    key: "unconnected_wire_endpoint",
    message: "Unconnected wire endpoint",
    severity: Severity::Warning,
};
pub const NO_CONNECT_CONNECTED: ErcRule = ErcRule {
    key: "no_connect_connected",
    message: "A pin with a \"no connection\" flag is connected",
    severity: Severity::Warning,
};
pub const NO_CONNECT_DANGLING: ErcRule = ErcRule {
    key: "no_connect_dangling",
    message: "Unconnected \"no connection\" flag",
    severity: Severity::Warning,
};

/// Every rule, in the order violations are listed within a severity.
pub const ERC_RULES: [ErcRule; 11] = [
    UNANNOTATED,
    DUPLICATE_REFERENCE,
    PIN_NOT_CONNECTED,
    PIN_NOT_DRIVEN,
    POWER_PIN_NOT_DRIVEN,
    PIN_TO_PIN,
    LABEL_DANGLING,
    ISOLATED_PIN_LABEL,
    UNCONNECTED_WIRE_ENDPOINT,
    NO_CONNECT_CONNECTED,
    NO_CONNECT_DANGLING,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The rule key and the items it names, such as `pin_not_connected:<uuid>:2`.
    pub id: String,
    pub rule: &'static str,
    pub severity: Severity,
    pub message: &'static str,
    pub position: Point,
    /// The symbols, wires, labels, and flags involved.
    pub uuids: Vec<Uuid>,
    /// One description per item, such as `R3 pin 2 [Passive]`.
    pub items: Vec<String>,
}

/// KiCad's pin electrical types, in the order of the conflict matrix.
const PIN_TYPES: [&str; 12] = [
    "input",
    "output",
    "bidirectional",
    "tri_state",
    "passive",
    "free",
    "unspecified",
    "power_in",
    "power_out",
    "open_collector",
    "open_emitter",
    "no_connect",
];
const PIN_TYPE_NAMES: [&str; 12] = [
    "Input",
    "Output",
    "Bidirectional",
    "Tri-state",
    "Passive",
    "Free",
    "Unspecified",
    "Power input",
    "Power output",
    "Open collector",
    "Open emitter",
    "Unconnected",
];
const INPUT: usize = 0;
const UNSPECIFIED: usize = 6;
const POWER_IN: usize = 7;
const POWER_OUT: usize = 8;
const NO_CONNECT: usize = 11;
/// Types that drive an input pin: output, bidirectional, tri-state, passive, power output.
const DRIVERS: [usize; 5] = [1, 2, 3, 4, POWER_OUT];

/// KiCad's default pin conflict matrix: O is fine, W a warning, and E an error.
const PIN_MATRIX: [&str; 12] = [
    "OOOOOOWOOOOE",
    "OEOWOOWOEEEE",
    "OOOOOOWOWOWE",
    "OWOOOOWWEWWE",
    "OOOOOOWOOOOE",
    "OOOOOOOOOOOE",
    "WWWWWOWWWWWE",
    "OOOWOOWOOOOE",
    "OEWEOOWOEEEE",
    "OEOWOOWOEOOE",
    "OEWWOOWOEOOE",
    "EEEEEEEEEEEE",
];

fn pin_type(electrical: &str) -> usize {
    PIN_TYPES
        .iter()
        .position(|name| *name == electrical)
        .unwrap_or(UNSPECIFIED)
}

fn conflict(a: usize, b: usize) -> Option<Severity> {
    match PIN_MATRIX[a].as_bytes()[b] {
        b'W' => Some(Severity::Warning),
        b'E' => Some(Severity::Error),
        _ => None,
    }
}

struct ErcPin {
    symbol: Uuid,
    reference: String,
    number: String,
    kind: usize,
    hidden: bool,
    position: Point,
    element: usize,
}

impl ErcPin {
    fn describe(&self) -> String {
        let kind = PIN_TYPE_NAMES[self.kind];
        format!("{} pin {} [{kind}]", self.reference, self.number)
    }

    fn key(&self) -> String {
        format!("{}:{}", self.symbol, self.number)
    }
}

/// What sits at one connection point.
#[derive(Default, Clone, Copy)]
struct Occupancy {
    pins: usize,
    wire_ends: usize,
    labels: usize,
    flags: usize,
}

#[derive(Default)]
struct Graph {
    set: DisjointSet,
    points: HashMap<Point, usize>,
    names: HashMap<String, usize>,
    occupancy: HashMap<Point, Occupancy>,
}

impl Graph {
    fn point(&mut self, point: Point) -> usize {
        if let Some(element) = self.points.get(&point) {
            return *element;
        }
        let element = self.set.add();
        self.points.insert(point, element);
        element
    }

    fn join_name(&mut self, element: usize, key: String) {
        let named = match self.names.get(&key) {
            Some(named) => *named,
            None => {
                let named = self.set.add();
                self.names.insert(key, named);
                named
            }
        };
        self.set.union(element, named);
    }

    fn at(&mut self, point: Point) -> &mut Occupancy {
        self.occupancy.entry(point).or_default()
    }

    fn occupancy(&self, point: Point) -> Occupancy {
        self.occupancy.get(&point).copied().unwrap_or_default()
    }
}

/// The pins and labels of one net, as indices.
#[derive(Default)]
struct NetItems {
    pins: Vec<usize>,
    labels: Vec<usize>,
}

impl NetItems {
    /// Whether the net reaches anything beyond a single pin.
    fn connected(&self) -> bool {
        self.pins.len() > 1 || !self.labels.is_empty()
    }
}

fn violation(rule: ErcRule, position: Point, parts: &[String]) -> Violation {
    Violation {
        id: format!("{}:{}", rule.key, parts.join(":")),
        rule: rule.key,
        severity: rule.severity,
        message: rule.message,
        position,
        uuids: Vec::new(),
        items: Vec::new(),
    }
}

fn describe_symbol(symbol: &PlacedSymbol) -> String {
    let value = symbol.property("Value").unwrap_or_default();
    format!("Symbol {} [{value}]", symbol.reference())
}

impl ErcRule {
    /// This rule's setting in `settings`, or its default.
    pub fn setting(&self, settings: &RuleSeverities) -> RuleSeverity {
        let default = RuleSeverity::from(self.severity);
        settings.get(self.key).copied().unwrap_or(default)
    }
}

/// Runs every ERC rule, reporting each with its setting in `settings`; ignored rules
/// report nothing. Violations are sorted errors first, then by rule, then top to bottom
/// and left to right. Fails when a placed symbol's definition is missing or unreadable.
pub fn check(schematic: &Schematic, settings: &RuleSeverities) -> Result<Vec<Violation>, String> {
    let symbols = schematic.symbols();
    let wires = schematic.wires();
    let labels = schematic.labels();
    let flags = schematic.no_connects();
    let mut graph = Graph::default();
    let mut pins = Vec::new();

    for symbol in &symbols {
        let reference = symbol.reference().to_owned();
        let Some(definition) = schematic.lib_symbol(&symbol.lib_id) else {
            return Err(format!(
                "{reference} uses {}, which is not embedded in the schematic.",
                symbol.lib_id
            ));
        };
        let connections = symbol
            .pin_connections(definition)
            .map_err(|error| format!("{reference} ({}): {error}", symbol.lib_id))?;
        let power = is_power_symbol(definition);
        let value = symbol.property("Value").unwrap_or_default();
        for pin in connections {
            let element = graph.point(pin.position);
            graph.at(pin.position).pins += 1;
            if pin.electrical == "power_in" {
                if power {
                    graph.join_name(element, format!("global:{value}"));
                } else if pin.hidden && !pin.name.is_empty() {
                    graph.join_name(element, format!("global:{}", pin.name));
                }
            }
            pins.push(ErcPin {
                symbol: symbol.uuid,
                reference: reference.clone(),
                number: pin.number,
                kind: pin_type(&pin.electrical),
                hidden: pin.hidden,
                position: pin.position,
                element,
            });
        }
    }
    for wire in &wires {
        let start = graph.point(wire.start);
        let end = graph.point(wire.end);
        graph.set.union(start, end);
        graph.at(wire.start).wire_ends += 1;
        graph.at(wire.end).wire_ends += 1;
    }
    let mut label_elements = Vec::with_capacity(labels.len());
    for label in &labels {
        let element = graph.point(label.position);
        graph.at(label.position).labels += 1;
        let key = match label.kind {
            LabelKind::Local => format!("local:{}", label.text),
            LabelKind::Global(_) => format!("global:{}", label.text),
        };
        graph.join_name(element, key);
        label_elements.push(element);
    }
    for flag in &flags {
        graph.point(flag.position);
        graph.at(flag.position).flags += 1;
    }

    let mut nets: BTreeMap<usize, NetItems> = BTreeMap::new();
    for (index, pin) in pins.iter().enumerate() {
        let root = graph.set.find(pin.element);
        nets.entry(root).or_default().pins.push(index);
    }
    for (index, element) in label_elements.iter().enumerate() {
        let root = graph.set.find(*element);
        nets.entry(root).or_default().labels.push(index);
    }

    let mut violations = Vec::new();
    check_symbols(&symbols, &mut violations);
    for pin in &pins {
        let root = graph.set.find(pin.element);
        let alone = !nets[&root].connected();
        let flagged = graph.occupancy(pin.position).flags > 0;
        if alone && !pin.hidden && !flagged && pin.kind != NO_CONNECT {
            let mut found = violation(PIN_NOT_CONNECTED, pin.position, &[pin.key()]);
            found.uuids.push(pin.symbol);
            found.items.push(pin.describe());
            violations.push(found);
        }
    }
    for net in nets.values() {
        check_net(net, &pins, &mut violations);
    }
    for (index, label) in labels.iter().enumerate() {
        let here = graph.occupancy(label.position);
        let kind = match label.kind {
            LabelKind::Local => "Label",
            LabelKind::Global(_) => "Global label",
        };
        let rule = if here.pins == 0 && here.wire_ends == 0 {
            LABEL_DANGLING
        } else {
            let root = graph.set.find(label_elements[index]);
            let net_pins = &nets[&root].pins;
            let single = match net_pins.as_slice() {
                [only] => Some(&pins[*only]),
                _ => None,
            };
            match single {
                Some(pin) if graph.occupancy(pin.position).flags == 0 => ISOLATED_PIN_LABEL,
                _ => continue,
            }
        };
        let mut found = violation(rule, label.position, &[label.uuid.to_string()]);
        found.uuids.push(label.uuid);
        found.items.push(format!("{kind} {}", label.text));
        violations.push(found);
    }
    for wire in &wires {
        for (end, point) in [("start", wire.start), ("end", wire.end)] {
            let here = graph.occupancy(point);
            if here.pins + here.wire_ends + here.labels + here.flags > 1 {
                continue;
            }
            let parts = [wire.uuid.to_string(), end.to_owned()];
            let mut found = violation(UNCONNECTED_WIRE_ENDPOINT, point, &parts);
            found.uuids.push(wire.uuid);
            found.items.push("Wire".to_owned());
            violations.push(found);
        }
    }
    for flag in &flags {
        let rule = if graph.occupancy(flag.position).pins == 0 {
            NO_CONNECT_DANGLING
        } else {
            let element = graph.point(flag.position);
            let root = graph.set.find(element);
            match nets.get(&root) {
                Some(net) if net.connected() => NO_CONNECT_CONNECTED,
                _ => continue,
            }
        };
        let mut found = violation(rule, flag.position, &[flag.uuid.to_string()]);
        found.uuids.push(flag.uuid);
        found.items.push("No-connect flag".to_owned());
        violations.push(found);
    }

    apply_settings(&mut violations, settings);
    let rank = |key: &str| ERC_RULES.iter().position(|rule| rule.key == key);
    violations.sort_by(|a, b| {
        let order = |v: &Violation| {
            let position = (v.position.y, v.position.x);
            (std::cmp::Reverse(v.severity), rank(v.rule), position)
        };
        order(a).cmp(&order(b)).then_with(|| a.id.cmp(&b.id))
    });
    Ok(violations)
}

/// Drops the violations of ignored rules and gives the rest their rule's severity, except
/// `pin_to_pin`, whose severity stays the pin conflict matrix's.
fn apply_settings(violations: &mut Vec<Violation>, settings: &RuleSeverities) {
    violations.retain_mut(|found| {
        let Some(rule) = ERC_RULES.iter().find(|rule| rule.key == found.rule) else {
            return true;
        };
        match rule.setting(settings) {
            RuleSeverity::Ignore => false,
            _ if rule.key == PIN_TO_PIN.key => true,
            RuleSeverity::Error => {
                found.severity = Severity::Error;
                true
            }
            RuleSeverity::Warning => {
                found.severity = Severity::Warning;
                true
            }
        }
    });
}

fn check_symbols(symbols: &[PlacedSymbol], violations: &mut Vec<Violation>) {
    let mut by_reference: BTreeMap<&str, Vec<&PlacedSymbol>> = BTreeMap::new();
    for symbol in symbols {
        let reference = symbol.reference();
        if reference.ends_with('?') {
            let parts = [symbol.uuid.to_string()];
            let mut found = violation(UNANNOTATED, symbol.position, &parts);
            found.uuids.push(symbol.uuid);
            found.items.push(describe_symbol(symbol));
            violations.push(found);
        } else if !reference.starts_with('#') {
            by_reference.entry(reference).or_default().push(symbol);
        }
    }
    for group in by_reference.values() {
        for (index, symbol) in group.iter().enumerate() {
            let Some(earlier) = group[..index].iter().find(|e| clashes(e, symbol)) else {
                continue;
            };
            let parts = [symbol.uuid.to_string(), earlier.uuid.to_string()];
            let mut found = violation(DUPLICATE_REFERENCE, symbol.position, &parts);
            found.uuids.extend([symbol.uuid, earlier.uuid]);
            let items = [describe_symbol(symbol), describe_symbol(earlier)];
            found.items.extend(items);
            violations.push(found);
        }
    }
}

/// Two symbols with one reference clash unless they are different units of one part.
fn clashes(a: &PlacedSymbol, b: &PlacedSymbol) -> bool {
    a.lib_id != b.lib_id || a.unit == b.unit
}

fn check_net(net: &NetItems, pins: &[ErcPin], violations: &mut Vec<Violation>) {
    let members: Vec<&ErcPin> = net.pins.iter().map(|index| &pins[*index]).collect();
    let has = |kinds: &[usize]| members.iter().any(|pin| kinds.contains(&pin.kind));
    let undriven = [
        (INPUT, PIN_NOT_DRIVEN, has(&DRIVERS)),
        (POWER_IN, POWER_PIN_NOT_DRIVEN, has(&[POWER_OUT])),
    ];
    for (kind, rule, driven) in undriven {
        if driven {
            continue;
        }
        if let Some(pin) = members.iter().find(|pin| pin.kind == kind) {
            let mut found = violation(rule, pin.position, &[pin.key()]);
            found.uuids.push(pin.symbol);
            found.items.push(pin.describe());
            violations.push(found);
        }
    }

    // Each pin reports its worst conflict, the nearest one on a tie, once per pair.
    let mut pairs: Vec<(usize, usize, Severity)> = Vec::new();
    for (index, pin) in members.iter().enumerate() {
        let mut best: Option<(Severity, i128, usize)> = None;
        for (other_index, other) in members.iter().enumerate() {
            if other_index == index {
                continue;
            }
            let Some(severity) = conflict(pin.kind, other.kind) else {
                continue;
            };
            let dx = i128::from(pin.position.x.0 - other.position.x.0);
            let dy = i128::from(pin.position.y.0 - other.position.y.0);
            let distance = dx.abs() + dy.abs();
            let better = best.is_none_or(|(worst, nearest, _)| {
                severity > worst || (severity == worst && distance < nearest)
            });
            if better {
                best = Some((severity, distance, other_index));
            }
        }
        if let Some((severity, _, other_index)) = best {
            let pair = (index.min(other_index), index.max(other_index), severity);
            if !pairs.contains(&pair) {
                pairs.push(pair);
            }
        }
    }
    for (first, second, severity) in pairs {
        let (a, b) = (members[first], members[second]);
        let mut found = violation(PIN_TO_PIN, a.position, &[a.key(), b.key()]);
        found.severity = severity;
        found.uuids.extend([a.symbol, b.symbol]);
        found.items.extend([a.describe(), b.describe()]);
        violations.push(found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pin_matrix_is_symmetric() {
        for a in 0..PIN_TYPES.len() {
            for b in 0..PIN_TYPES.len() {
                assert_eq!(conflict(a, b), conflict(b, a), "{a} and {b}");
            }
        }
    }

    #[test]
    fn pin_conflicts_follow_kicad_defaults() {
        let output = pin_type("output");
        assert_eq!(conflict(output, output), Some(Severity::Error));
        let tri_state = pin_type("tri_state");
        assert_eq!(conflict(output, tri_state), Some(Severity::Warning));
        let passive = pin_type("passive");
        assert_eq!(conflict(passive, pin_type("power_in")), None);
        assert_eq!(pin_type("mystery"), UNSPECIFIED);
        let unspecified = conflict(UNSPECIFIED, INPUT);
        assert_eq!(unspecified, Some(Severity::Warning));
        assert_eq!(conflict(NO_CONNECT, passive), Some(Severity::Error));
    }

    fn found(rule: ErcRule, severity: Severity) -> Violation {
        let mut found = violation(rule, Point::ORIGIN, &["x".to_owned()]);
        found.severity = severity;
        found
    }

    #[test]
    fn settings_ignore_rules_and_override_severities_but_not_the_matrix() {
        let mut violations = vec![
            found(PIN_NOT_CONNECTED, Severity::Error),
            found(ISOLATED_PIN_LABEL, Severity::Warning),
            found(UNANNOTATED, Severity::Error),
            found(PIN_TO_PIN, Severity::Error),
            found(PIN_TO_PIN, Severity::Warning),
        ];
        let settings = RuleSeverities::from([
            ("pin_not_connected".to_owned(), RuleSeverity::Warning),
            ("isolated_pin_label".to_owned(), RuleSeverity::Error),
            ("unannotated".to_owned(), RuleSeverity::Ignore),
            ("pin_to_pin".to_owned(), RuleSeverity::Warning),
        ]);
        apply_settings(&mut violations, &settings);
        let kept: Vec<(&str, Severity)> = violations
            .iter()
            .map(|found| (found.rule, found.severity))
            .collect();
        assert_eq!(
            kept,
            [
                ("pin_not_connected", Severity::Warning),
                ("isolated_pin_label", Severity::Error),
                ("pin_to_pin", Severity::Error),
                ("pin_to_pin", Severity::Warning),
            ]
        );

        let ignore = RuleSeverities::from([("pin_to_pin".to_owned(), RuleSeverity::Ignore)]);
        apply_settings(&mut violations, &ignore);
        assert!(violations.iter().all(|found| found.rule != "pin_to_pin"));
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn rule_severity_keys_round_trip() {
        let settings = [
            RuleSeverity::Error,
            RuleSeverity::Warning,
            RuleSeverity::Ignore,
        ];
        for setting in settings {
            assert_eq!(RuleSeverity::from_key(setting.key()), Some(setting));
        }
        assert_eq!(RuleSeverity::from_key("Error"), None);
    }

    #[test]
    fn every_rule_key_is_unique() {
        for (index, rule) in ERC_RULES.iter().enumerate() {
            let later = &ERC_RULES[index + 1..];
            assert!(later.iter().all(|other| other.key != rule.key), "{rule:?}");
        }
    }
}
