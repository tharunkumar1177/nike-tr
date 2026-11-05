//! Reads ngspice's output after a run that failed to converge, and names the nets and
//! symbols it blamed.
//!
//! ngspice 46 retries every DC operating point with gmin and then source stepping itself,
//! and reports what went wrong only as text. The patterns below are its own messages, from
//! `cktop.c`, `niiter.c`, `dctran.c`, and `ckttroub.c`. ngspice lower-cases every deck line,
//! so the names in them are lower case.

use std::collections::BTreeMap;

use crate::deck::{Deck, GROUND};

/// How a run reached its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convergence {
    /// The first Newton iteration converged.
    Direct,
    /// ngspice needed gmin or source stepping. The results are still exact.
    Stepped,
    /// The run failed, and converged only once a 1 GΩ shunt joined every node to ground,
    /// so the results are approximate.
    RshuntRescue,
}

/// What kind of convergence failure ngspice reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosisKind {
    /// The circuit matrix is singular, as with a floating node or a loop of voltage sources.
    SingularMatrix,
    /// Newton iteration hit its limit.
    Iterations,
    /// The transient's time step fell below its minimum.
    TimestepTooSmall,
    /// The operating point failed after every stepping method.
    OperatingPoint,
}

/// A convergence failure, with the schematic names ngspice blamed.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnosis {
    pub kind: DiagnosisKind,
    /// The simulation time of a transient failure, in seconds.
    pub time: Option<f64>,
    /// The schematic nets ngspice named, in its order and without repeats.
    pub nets: Vec<String>,
    /// The symbols ngspice named, or the first simulated symbol on each named net, in its
    /// order and without repeats.
    pub references: Vec<String>,
    /// The SPICE device a transient failure blamed, in lower case, such as `c1`.
    pub device: Option<String>,
}

const NOT_CONVERGED: &str = "The operating point did not converge";
const UNSOLVED: &str =
    "The operating point could not be computed, even with gmin and source stepping.";
const STEP_TOO_SMALL: &str = "Transient step too small at";

impl Diagnosis {
    /// The symbol to show the user first, if any was blamed.
    pub fn reference(&self) -> Option<&str> {
        self.references.first().map(String::as_str)
    }

    /// One sentence for the user, naming the nets or the device involved.
    pub fn message(&self) -> String {
        match self.kind {
            DiagnosisKind::SingularMatrix => match self.nets.as_slice() {
                [] => format!("{NOT_CONVERGED}: singular matrix."),
                [net] => format!("{NOT_CONVERGED}: singular matrix at net {net}."),
                nets => format!("{NOT_CONVERGED}: singular matrix at nets {}.", list(nets)),
            },
            DiagnosisKind::Iterations => format!("{NOT_CONVERGED}: too many iterations."),
            DiagnosisKind::OperatingPoint => UNSOLVED.to_owned(),
            DiagnosisKind::TimestepTooSmall => {
                let at = match self.time {
                    Some(time) => format_seconds(time),
                    None => "the initial timepoint".to_owned(),
                };
                if let (Some(device), Some(reference)) = (&self.device, self.reference()) {
                    format!("{STEP_TOO_SMALL} {at} in {device} ({reference}).")
                } else if let Some(net) = self.nets.first() {
                    format!("{STEP_TOO_SMALL} {at} near net {net}.")
                } else {
                    format!("{STEP_TOO_SMALL} {at}.")
                }
            }
        }
    }
}

/// `A and B`, or `A, B, and C`.
fn list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Seconds to 3 significant digits with an SI prefix, such as `1.20 ms`, as the UI shows
/// times.
fn format_seconds(seconds: f64) -> String {
    const PREFIXES: [&str; 10] = ["f", "p", "n", "µ", "m", "", "k", "M", "G", "T"];
    let magnitude = seconds.abs();
    if magnitude < 1e-15 {
        return "0 s".to_owned();
    }
    if magnitude >= 1e15 || magnitude.is_nan() {
        return format!("{seconds:.2e} s");
    }
    let sign = if seconds < 0.0 { "-" } else { "" };
    let mut exponent = (magnitude.log10() / 3.0).floor() as i32 * 3;
    if magnitude / 10f64.powi(exponent) < 1.0 {
        exponent -= 3;
    }
    let mut text = three_digits(magnitude / 10f64.powi(exponent));
    if text.parse::<f64>().is_ok_and(|value| value >= 1000.0) && exponent < 12 {
        exponent += 3;
        text = three_digits(magnitude / 10f64.powi(exponent));
    }
    let prefix = PREFIXES[((exponent + 15) / 3).clamp(0, 9) as usize];
    format!("{sign}{text} {prefix}s")
}

/// A value from 1 to 1000 to 3 significant digits, such as `1.20`, `12.0`, or `120`.
fn three_digits(value: f64) -> String {
    let decimals = (2 - value.log10().floor() as i32).clamp(0, 2) as usize;
    format!("{value:.decimals$}")
}

/// Whether the output of a run that succeeded shows that ngspice needed gmin or source
/// stepping to reach its operating point.
pub fn stepped(output: &[String]) -> bool {
    output.iter().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("note: starting")
            && (lower.contains("gmin stepping") || lower.contains("source stepping"))
    })
}

/// The convergence failure in `output`, the lines ngspice printed during a failed run of
/// `deck`, or `None` when the failure was something else.
pub fn diagnose(output: &[String], deck: &Deck) -> Option<Diagnosis> {
    let lines: Vec<String> = output
        .iter()
        .map(|line| line.trim().to_ascii_lowercase())
        .collect();
    let timestep = lines.iter().find_map(|line| timestep_too_small(line));
    let mut singular = Vec::new();
    for line in &lines {
        singular.extend(singular_nodes(line));
    }
    let mut blame = Blame::new(deck);
    let mut device = None;
    let kind = if let Some(failure) = &timestep {
        match failure.culprit {
            Some(Culprit::Node(node)) => {
                blame.node(node);
                device = node.strip_suffix("#branch").map(str::to_owned);
            }
            Some(Culprit::Device(name)) => {
                blame.device(name);
                device = Some(name.to_owned());
            }
            None => singular.iter().for_each(|node| blame.node(node)),
        }
        DiagnosisKind::TimestepTooSmall
    } else if !singular.is_empty() {
        singular.iter().for_each(|node| blame.node(node));
        DiagnosisKind::SingularMatrix
    } else if lines.iter().any(|line| iteration_limit(line)) {
        DiagnosisKind::Iterations
    } else if lines.iter().any(|line| operating_point_failed(line)) {
        DiagnosisKind::OperatingPoint
    } else {
        return None;
    };
    Some(Diagnosis {
        kind,
        time: timestep.and_then(|failure| failure.time),
        nets: blame.nets,
        references: blame.references,
        device,
    })
}

/// What a `Timestep too small` line blamed.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Culprit<'a> {
    Node(&'a str),
    Device(&'a str),
}

/// A `Timestep too small` line: when the step failed, and what ngspice blamed.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TimestepFailure<'a> {
    time: Option<f64>,
    culprit: Option<Culprit<'a>>,
}

/// Reads `TRAN: Timestep too small; time = %g, timestep = %g: trouble with node "%s"`, its
/// `trouble with %s-instance %s` and `cause unrecorded.` variants, and the `initial
/// timepoint: ` form, which has no time.
fn timestep_too_small(line: &str) -> Option<TimestepFailure<'_>> {
    let rest = &line[line.find("timestep too small")?..];
    let time = rest
        .find("time = ")
        .map(|start| &rest[start + "time = ".len()..])
        .and_then(|text| text.split([',', ' ']).next())
        .and_then(|number| number.parse().ok());
    let blamed = rest
        .find("trouble with ")
        .and_then(|start| culprit(&rest[start + "trouble with ".len()..]));
    Some(TimestepFailure {
        time,
        culprit: blamed,
    })
}

/// The culprit after `trouble with `: `node "%s"`, or `%s-instance %s`.
fn culprit(trouble: &str) -> Option<Culprit<'_>> {
    if let Some(node) = trouble.strip_prefix("node ") {
        let name = node.trim().trim_matches('"').trim();
        return (!name.is_empty()).then_some(Culprit::Node(name));
    }
    let (_, instance) = trouble.split_once("-instance ")?;
    let name = instance.split_whitespace().next()?;
    Some(Culprit::Device(name))
}

/// The nodes in `Warning: singular matrix:  check node %s` or `check nodes %s and %s`.
fn singular_nodes(line: &str) -> Vec<&str> {
    let Some(start) = line.find("singular matrix") else {
        return Vec::new();
    };
    let rest = &line[start..];
    let Some(check) = rest.find("check node") else {
        return Vec::new();
    };
    let names = &rest[check + "check node".len()..];
    let names = names.strip_prefix('s').unwrap_or(names);
    names
        .split(" and ")
        .filter_map(|name| name.split_whitespace().next())
        .collect()
}

/// `Too many iterations without convergence`, or ngspice's generic iteration-limit error.
fn iteration_limit(line: &str) -> bool {
    line.contains("too many iterations without convergence")
        || line.contains("iteration limit reached")
}

/// The final operating-point error, or a stepping method that gave up.
fn operating_point_failed(line: &str) -> bool {
    line.contains("operating point could not be simulated")
        || line.contains("gmin stepping failed")
        || line.contains("source stepping failed")
}

/// SPICE names mapped to schematic names, as in [`Deck::nodes`] and [`Deck::devices`].
type Names = BTreeMap<String, String>;

/// The entry for `spice_name`, ignoring case as ngspice does.
fn lookup<'m>(names: &'m Names, spice_name: &str) -> Option<(&'m String, &'m String)> {
    names
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(spice_name))
}

/// Collects the schematic names behind the SPICE names ngspice blamed.
struct Blame<'a> {
    deck: &'a Deck,
    nets: Vec<String>,
    references: Vec<String>,
}

impl<'a> Blame<'a> {
    /// Starts with nothing blamed.
    fn new(deck: &'a Deck) -> Blame<'a> {
        Blame {
            deck,
            nets: Vec::new(),
            references: Vec::new(),
        }
    }

    /// Blames a node, or the device of a `<device>#branch` row.
    fn node(&mut self, name: &str) {
        if let Some(device) = name.strip_suffix("#branch") {
            self.device(device);
            return;
        }
        let deck = self.deck;
        let Some((node, net)) = lookup(&deck.nodes, name) else {
            return;
        };
        push_new(&mut self.nets, net);
        if let Some(reference) = self.first_device_on(node) {
            push_new(&mut self.references, reference);
        }
    }

    /// Blames a device by its SPICE instance name.
    fn device(&mut self, name: &str) {
        let deck = self.deck;
        if let Some((_, reference)) = lookup(&deck.devices, name) {
            push_new(&mut self.references, reference);
        }
    }

    /// The reference of the first device line in the deck that connects to `node`. A
    /// device's nodes are the words after its name, up to the first word that is not a
    /// node.
    fn first_device_on(&self, node: &str) -> Option<&'a str> {
        let deck = self.deck;
        for line in &deck.lines {
            let mut words = line.split_whitespace();
            let device = words.next().unwrap_or_default();
            let Some(reference) = deck.devices.get(device) else {
                continue;
            };
            let mut nodes = words.take_while(|word| self.is_node(word));
            if nodes.any(|word| word == node) {
                return Some(reference);
            }
        }
        None
    }

    /// Whether `word` is a node of the deck, ground included.
    fn is_node(&self, word: &str) -> bool {
        word == GROUND || self.deck.nodes.contains_key(word)
    }
}

/// Appends `value` unless `values` already holds it.
fn push_new(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deck where `V1` drives `IN`, `R1` joins `IN` to `/OUT`, and `C1` joins `/OUT` to a
    /// floating node `N3`.
    fn deck() -> Deck {
        let lines = [
            "* Tron simulation",
            "V1 IN 0 DC 5",
            "R1 IN /OUT 1e3",
            "C1 /OUT N3 1e-6",
            ".op",
            ".end",
        ];
        let nodes = [("IN", "IN"), ("/OUT", "/OUT"), ("N3", "Net-(C1-Pad2)")];
        let devices = [("V1", "V1"), ("R1", "R1"), ("C1", "C1")];
        let owned = |(a, b): (&str, &str)| (a.to_owned(), b.to_owned());
        Deck {
            lines: Vec::from(lines.map(str::to_owned)),
            nodes: nodes.into_iter().map(owned).collect(),
            devices: devices.into_iter().map(owned).collect(),
        }
    }

    /// ngspice output lines, without their stream prefix.
    fn output(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn a_singular_matrix_names_the_net_and_its_first_device() {
        let lines = output(&[
            "Warning: singular matrix:  check node n3",
            "Note: Starting dynamic gmin stepping",
            "Warning: Dynamic gmin stepping failed",
            "Error: The operating point could not be simulated successfully.",
            "doAnalyses: Too many iterations without convergence",
            "run simulation(s) aborted",
        ]);
        let diagnosis = diagnose(&lines, &deck()).unwrap();
        assert_eq!(diagnosis.kind, DiagnosisKind::SingularMatrix);
        assert_eq!(diagnosis.nets, ["Net-(C1-Pad2)"]);
        assert_eq!(diagnosis.references, ["C1"]);
        assert_eq!(diagnosis.reference(), Some("C1"));
        assert_eq!(diagnosis.time, None);
    }

    #[test]
    fn two_singular_nodes_and_a_branch_row_are_all_blamed() {
        let lines = output(&[
            "Warning: singular matrix:  check nodes /out and n3",
            "Warning: singular matrix:  check node v1#branch",
            "Warning: singular matrix:  check node n3",
        ]);
        let diagnosis = diagnose(&lines, &deck()).unwrap();
        assert_eq!(diagnosis.kind, DiagnosisKind::SingularMatrix);
        assert_eq!(diagnosis.nets, ["/OUT", "Net-(C1-Pad2)"]);
        assert_eq!(diagnosis.references, ["R1", "C1", "V1"]);
    }

    #[test]
    fn a_small_timestep_names_its_time_and_node() {
        let line = "TRAN: Timestep too small; time = 0.0012, timestep = 1.25e-20: \
                    trouble with node \"/out\"";
        let diagnosis = diagnose(&output(&[line]), &deck()).unwrap();
        assert_eq!(diagnosis.kind, DiagnosisKind::TimestepTooSmall);
        assert_eq!(diagnosis.time, Some(0.0012));
        assert_eq!(diagnosis.nets, ["/OUT"]);
        assert_eq!(diagnosis.references, ["R1"]);
    }

    #[test]
    fn a_small_timestep_can_blame_a_device_or_nothing() {
        let line = "TRAN: Timestep too small; time = 1e-05, timestep = 1e-21: \
                    trouble with capacitor-instance c1";
        let diagnosis = diagnose(&output(&[line]), &deck()).unwrap();
        assert_eq!(diagnosis.time, Some(1e-5));
        assert!(diagnosis.nets.is_empty());
        assert_eq!(diagnosis.references, ["C1"]);

        let initial = "TRAN: Timestep too small; initial timepoint: cause unrecorded.";
        let diagnosis = diagnose(&output(&[initial]), &deck()).unwrap();
        assert_eq!(diagnosis.kind, DiagnosisKind::TimestepTooSmall);
        assert_eq!(diagnosis.time, None);
        assert!(diagnosis.references.is_empty());
    }

    #[test]
    fn iteration_and_operating_point_failures_blame_no_one() {
        let lines = output(&["doAnalyses: Too many iterations without convergence"]);
        let diagnosis = diagnose(&lines, &deck()).unwrap();
        assert_eq!(diagnosis.kind, DiagnosisKind::Iterations);
        assert!(diagnosis.nets.is_empty());
        assert!(diagnosis.references.is_empty());

        let lines = output(&[
            "Warning: source stepping failed",
            "Error: The operating point could not be simulated successfully.",
        ]);
        let diagnosis = diagnose(&lines, &deck()).unwrap();
        assert_eq!(diagnosis.kind, DiagnosisKind::OperatingPoint);
    }

    #[test]
    fn unknown_names_ground_and_other_failures_are_ignored() {
        let lines = output(&["Warning: singular matrix:  check nodes 0 and x9"]);
        let diagnosis = diagnose(&lines, &deck()).unwrap();
        assert!(diagnosis.nets.is_empty());
        assert!(diagnosis.references.is_empty());

        let other = output(&[
            "Error: unknown subckt: x1 a 0 nosuch",
            "run simulation(s) aborted",
        ]);
        assert_eq!(diagnose(&other, &deck()), None);
    }

    /// A diagnosis of `kind` that blames `nets`, and `references` for their symbols.
    fn blaming(kind: DiagnosisKind, nets: &[&str], references: &[&str]) -> Diagnosis {
        Diagnosis {
            kind,
            time: None,
            nets: output(nets),
            references: output(references),
            device: None,
        }
    }

    #[test]
    fn singular_matrix_messages_name_each_net() {
        let kind = DiagnosisKind::SingularMatrix;
        let none = blaming(kind, &[], &["V2"]);
        let expected = "The operating point did not converge: singular matrix.";
        assert_eq!(none.message(), expected);
        let one = blaming(kind, &["/OUT"], &["R1"]);
        let expected = "The operating point did not converge: singular matrix at net /OUT.";
        assert_eq!(one.message(), expected);
        let two = blaming(kind, &["/OUT", "N3"], &["R1"]);
        let expected = format!("{NOT_CONVERGED}: singular matrix at nets /OUT and N3.");
        assert_eq!(two.message(), expected);
        let three = blaming(kind, &["A", "B", "C"], &[]);
        assert!(three.message().ends_with("at nets A, B, and C."));
    }

    #[test]
    fn operating_point_messages_have_no_names() {
        let iterations = blaming(DiagnosisKind::Iterations, &[], &[]);
        let expected = "The operating point did not converge: too many iterations.";
        assert_eq!(iterations.message(), expected);
        let unsolved = blaming(DiagnosisKind::OperatingPoint, &[], &[]);
        assert_eq!(unsolved.message(), UNSOLVED);
    }

    #[test]
    fn timestep_messages_give_the_time_and_the_culprit() {
        let line = "TRAN: Timestep too small; time = 0.0012, timestep = 1.25e-20: \
                    trouble with node \"/out\"";
        let node = diagnose(&output(&[line]), &deck()).unwrap();
        let expected = "Transient step too small at 1.20 ms near net /OUT.";
        assert_eq!(node.message(), expected);

        let line = "TRAN: Timestep too small; time = 1e-05, timestep = 1e-21: \
                    trouble with capacitor-instance c1";
        let device = diagnose(&output(&[line]), &deck()).unwrap();
        assert_eq!(device.device.as_deref(), Some("c1"));
        let expected = "Transient step too small at 10.0 µs in c1 (C1).";
        assert_eq!(device.message(), expected);

        let initial = "TRAN: Timestep too small; initial timepoint: cause unrecorded.";
        let unknown = diagnose(&output(&[initial]), &deck()).unwrap();
        let expected = "Transient step too small at the initial timepoint.";
        assert_eq!(unknown.message(), expected);

        let line = "TRAN: Timestep too small; initial timepoint: \
                    trouble with node \"v1#branch\"";
        let branch = diagnose(&output(&[line]), &deck()).unwrap();
        let expected = "Transient step too small at the initial timepoint in v1 (V1).";
        assert_eq!(branch.message(), expected);
    }

    #[test]
    fn times_use_three_significant_digits_and_a_prefix() {
        assert_eq!(format_seconds(0.0), "0 s");
        assert_eq!(format_seconds(2.5e-9), "2.50 ns");
        assert_eq!(format_seconds(0.12), "120 ms");
        assert_eq!(format_seconds(0.9996), "1.00 s");
        assert_eq!(format_seconds(42.0), "42.0 s");
        assert_eq!(format_seconds(1e-6), "1.00 µs");
    }

    #[test]
    fn stepping_notes_mark_a_run_as_stepped() {
        let gmin = output(&["Note: Starting dynamic gmin stepping"]);
        assert!(stepped(&gmin));
        let source = output(&["Note: Starting source stepping"]);
        assert!(stepped(&source));
        let warning = output(&["Warning: singular matrix:  check node n3"]);
        assert!(!stepped(&warning));
        assert!(!stepped(&[]));
    }
}
