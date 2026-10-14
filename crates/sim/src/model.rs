//! How a schematic symbol simulates: its SPICE element, parameters, and pin order.
//!
//! The `Sim.Device`, `Sim.Type`, `Sim.Params`, and `Sim.Pins` fields follow KiCad. A symbol
//! without them is inferred from its reference prefix and `Value`, as KiCad does for
//! resistors, capacitors, inductors, and DC sources.

use tron_core::PlacedSymbol;

use crate::value::{parse_value, spice_number};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Element {
    Resistor,
    Capacitor,
    Inductor,
    VoltageSource,
    CurrentSource,
}

impl Element {
    fn letter(self) -> char {
        match self {
            Element::Resistor => 'R',
            Element::Capacitor => 'C',
            Element::Inductor => 'L',
            Element::VoltageSource => 'V',
            Element::CurrentSource => 'I',
        }
    }

    fn from_letter(letter: char) -> Option<Element> {
        match letter.to_ascii_uppercase() {
            'R' => Some(Element::Resistor),
            'C' => Some(Element::Capacitor),
            'L' => Some(Element::Inductor),
            'V' => Some(Element::VoltageSource),
            'I' => Some(Element::CurrentSource),
            _ => None,
        }
    }

    /// The SPICE instance name: the reference, prefixed with the element letter when the
    /// reference starts with another letter, so `BAT1` as a voltage source is `VBAT1`.
    pub(crate) fn instance_name(self, reference: &str) -> String {
        let letter = self.letter();
        match reference.chars().next() {
            Some(first) if first.eq_ignore_ascii_case(&letter) => reference.to_owned(),
            _ => format!("{letter}{reference}"),
        }
    }

    fn is_source(self) -> bool {
        matches!(self, Element::VoltageSource | Element::CurrentSource)
    }
}

/// The model pins of every supported element, in SPICE node order.
const MODEL_PINS: [&str; 2] = ["+", "-"];

/// The symbol pin that each model pin uses by default.
const DEFAULT_PINS: [&str; 2] = ["1", "2"];

const NO_MODEL: &str =
    "It has no simulation model. Set its Sim.Device field, or exclude it from simulation.";

/// A symbol resolved to one SPICE element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Device {
    pub(crate) element: Element,
    /// The symbol pin number on each model pin, in SPICE node order.
    pub(crate) pins: [String; 2],
    /// Everything after the nodes on the SPICE line, such as `1e4` or `DC 5e0`.
    pub(crate) value: String,
}

/// The device `symbol` simulates as. `pin_numbers` lists the placed unit's pins. The error
/// is a sentence explaining what to change.
pub(crate) fn device(symbol: &PlacedSymbol, pin_numbers: &[String]) -> Result<Device, String> {
    let field = |key: &str| {
        symbol
            .property(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    if let Some(library) = field("Sim.Library") {
        return Err(format!(
            "It uses the SPICE model library \"{library}\", which Tron cannot simulate yet."
        ));
    }
    let element = match field("Sim.Device") {
        Some(name) => {
            let mut letters = name.chars();
            match (letters.next().and_then(Element::from_letter), letters.next()) {
                (Some(element), None) => element,
                _ => {
                    return Err(format!(
                        "Its Sim.Device \"{name}\" is not supported yet. Tron simulates R, C, \
                         L, V, and I devices."
                    ));
                }
            }
        }
        None => symbol
            .reference()
            .chars()
            .next()
            .and_then(Element::from_letter)
            .ok_or_else(|| NO_MODEL.to_owned())?,
    };
    let params = match field("Sim.Params") {
        Some(text) => parse_params(text)?,
        None => Vec::new(),
    };
    let value = field("Value").unwrap_or_default();
    let line = if element.is_source() {
        source_value(field("Sim.Type"), &params, value)?
    } else {
        if let Some(kind) = field("Sim.Type") {
            return Err(format!("Its Sim.Type \"{kind}\" is not supported yet."));
        }
        let key = element.letter().to_ascii_lowercase().to_string();
        reject_unknown(&params, &[key.as_str()])?;
        let magnitude = match lookup(&params, &key) {
            Some(text) => parse_param(&key, text)?,
            None => parse_value(value).map_err(|error| format!("Its Value: {error}"))?,
        };
        spice_number(magnitude)
    };
    Ok(Device {
        element,
        pins: pin_map(field("Sim.Pins"), pin_numbers)?,
        value: line,
    })
}

/// The SPICE text of an independent source: `DC <v>`, `SIN(...)`, or `PULSE(...)`, with
/// `AC <magnitude>` when `ac` is set.
fn source_value(
    kind: Option<&str>,
    params: &[(String, String)],
    value: &str,
) -> Result<String, String> {
    let kind = kind.unwrap_or("DC").to_ascii_uppercase();
    let (keys, required): (&[&str], &[&str]) = match kind.as_str() {
        "DC" => (&["dc"], &[]),
        "SIN" => (
            &["dc", "ampl", "f", "td", "theta", "phase"],
            &["ampl", "f"],
        ),
        "PULSE" => (
            &["y1", "y2", "td", "tr", "tf", "tw", "per", "np"],
            &["y1", "y2"],
        ),
        _ => {
            return Err(format!(
                "Its Sim.Type \"{kind}\" is not supported yet. Tron simulates DC, SIN, and \
                 PULSE sources."
            ));
        }
    };
    let mut allowed = keys.to_vec();
    allowed.push("ac");
    reject_unknown(params, &allowed)?;
    if let Some(missing) = required.iter().find(|key| lookup(params, key).is_none()) {
        return Err(format!("A {kind} source needs {missing} in its Sim.Params."));
    }
    let mut text = if kind == "DC" {
        let level = match lookup(params, "dc") {
            Some(text) => parse_param("dc", text)?,
            None => {
                let bare = strip_dc(value);
                parse_value(bare).map_err(|error| format!("Its Value: {error}"))?
            }
        };
        format!("DC {}", spice_number(level))
    } else {
        let mut values = Vec::with_capacity(keys.len());
        for key in keys {
            let text = lookup(params, key);
            values.push(text.map(|text| parse_param(key, text)).transpose()?);
        }
        let used = values.iter().rposition(Option::is_some).map_or(0, |last| last + 1);
        let arguments: Vec<String> = values[..used]
            .iter()
            .map(|value| spice_number(value.unwrap_or(0.0)))
            .collect();
        format!("{kind}({})", arguments.join(" "))
    };
    if let Some(ac) = lookup(params, "ac") {
        let magnitude = parse_param("ac", ac)?;
        text = if kind == "DC" {
            format!("{text} AC {}", spice_number(magnitude))
        } else {
            format!("AC {} {text}", spice_number(magnitude))
        };
    }
    Ok(text)
}

/// A `Value` such as `5` or `DC 5`, without the `DC`.
fn strip_dc(value: &str) -> &str {
    match value.get(..2) {
        Some(prefix) if prefix.eq_ignore_ascii_case("dc") => value[2..].trim_start(),
        _ => value,
    }
}

fn lookup<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn parse_param(key: &str, text: &str) -> Result<f64, String> {
    parse_value(text).map_err(|error| format!("Its Sim.Params {key}: {error}"))
}

fn reject_unknown(params: &[(String, String)], allowed: &[&str]) -> Result<(), String> {
    match params.iter().find(|(key, _)| !allowed.contains(&key.as_str())) {
        Some((key, _)) => Err(format!(
            "Its Sim.Params \"{key}\" is not supported for this device."
        )),
        None => Ok(()),
    }
}

/// Parses `Sim.Params` such as `dc=0 ampl=1 f="1k"` into lowercase keys and their values.
fn parse_params(text: &str) -> Result<Vec<(String, String)>, String> {
    let malformed = || format!("Its Sim.Params \"{text}\" is not a list of name=value pairs.");
    let mut params: Vec<(String, String)> = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=').ok_or_else(malformed)?;
        if key.is_empty() || key.contains(char::is_whitespace) {
            return Err(malformed());
        }
        let (value, after) = match after.strip_prefix('"') {
            Some(quoted) => {
                let (value, after) = quoted.split_once('"').ok_or_else(malformed)?;
                if !after.is_empty() && !after.starts_with(char::is_whitespace) {
                    return Err(malformed());
                }
                (value, after)
            }
            None => {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                after.split_at(end)
            }
        };
        if value.is_empty() || value.contains('"') {
            return Err(malformed());
        }
        let key = key.to_ascii_lowercase();
        if params.iter().any(|(existing, _)| *existing == key) {
            return Err(format!("Its Sim.Params sets \"{key}\" more than once."));
        }
        params.push((key, value.to_owned()));
        rest = after.trim_start();
    }
    Ok(params)
}

/// The symbol pin on each model pin, from `Sim.Pins` such as `1=+ 2=-`, or pins `1` and `2`.
fn pin_map(field: Option<&str>, pin_numbers: &[String]) -> Result<[String; 2], String> {
    let has_pin = |number: &str| pin_numbers.iter().any(|pin| pin == number);
    let Some(text) = field else {
        let defaults_present = DEFAULT_PINS.iter().all(|&pin| has_pin(pin));
        if !defaults_present || pin_numbers.len() != DEFAULT_PINS.len() {
            return Err(format!(
                "It has {} pins, so set its Sim.Pins field to map them to + and -.",
                pin_numbers.len()
            ));
        }
        return Ok(DEFAULT_PINS.map(str::to_owned));
    };
    let malformed = || format!("Its Sim.Pins \"{text}\" must map two pins, such as \"1=+ 2=-\".");
    let mut mapped: [Option<String>; 2] = [None, None];
    for pair in text.split_whitespace() {
        let (pin, model_pin) = pair.split_once('=').ok_or_else(malformed)?;
        let slot = MODEL_PINS
            .iter()
            .position(|name| *name == model_pin)
            .ok_or_else(malformed)?;
        if !has_pin(pin) {
            return Err(format!("Its Sim.Pins names pin {pin}, which the symbol does not have."));
        }
        if mapped[slot].replace(pin.to_owned()).is_some() {
            return Err(malformed());
        }
    }
    match mapped {
        [Some(positive), Some(negative)] => Ok([positive, negative]),
        _ => Err(malformed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tron_core::{Angle, Point, SymbolProperty};
    use uuid::Uuid;

    fn property(key: &str, value: &str) -> SymbolProperty {
        SymbolProperty {
            key: key.to_owned(),
            value: value.to_owned(),
            position: Point::ORIGIN,
            rotation: Angle::ZERO,
            hidden: false,
        }
    }

    fn symbol(reference: &str, value: &str, fields: &[(&str, &str)]) -> PlacedSymbol {
        let mut properties = vec![property("Reference", reference), property("Value", value)];
        properties.extend(fields.iter().map(|(key, value)| property(key, value)));
        PlacedSymbol {
            lib_id: "Device:R".to_owned(),
            position: Point::ORIGIN,
            rotation: Angle::ZERO,
            mirror: None,
            unit: 1,
            uuid: Uuid::nil(),
            properties,
            pins: Vec::new(),
        }
    }

    fn two_pins() -> Vec<String> {
        vec!["1".to_owned(), "2".to_owned()]
    }

    fn resolve(reference: &str, value: &str, fields: &[(&str, &str)]) -> Result<Device, String> {
        device(&symbol(reference, value, fields), &two_pins())
    }

    fn line(reference: &str, value: &str, fields: &[(&str, &str)]) -> String {
        resolve(reference, value, fields)
            .unwrap_or_else(|error| panic!("{reference}: {error}"))
            .value
    }

    #[test]
    fn passives_are_inferred_from_the_reference_and_value() {
        assert_eq!(line("R1", "4k7", &[]), "4.7e3");
        assert_eq!(line("C3", "100n", &[]), "1e-7");
        assert_eq!(line("L2", "10uH", &[]), "1e-5");
        let resistor = resolve("R1", "1k", &[]).unwrap();
        assert_eq!(resistor.element, Element::Resistor);
        assert_eq!(resistor.pins, ["1".to_owned(), "2".to_owned()]);
    }

    #[test]
    fn sim_fields_override_the_inference() {
        let fields = [("Sim.Device", "R"), ("Sim.Params", "r=2.2k")];
        assert_eq!(line("RV1", "pot", &fields), "2.2e3");
        let swapped = resolve("R1", "1k", &[("Sim.Pins", "1=- 2=+")]).unwrap();
        assert_eq!(swapped.pins, ["2".to_owned(), "1".to_owned()]);
        let battery = resolve("BAT1", "", &[("Sim.Device", "V"), ("Sim.Params", "dc=9")]);
        assert_eq!(battery.unwrap().element, Element::VoltageSource);
    }

    #[test]
    fn sources_write_dc_sin_and_pulse() {
        assert_eq!(line("V1", "5", &[]), "DC 5e0");
        assert_eq!(line("V1", "dc 3.3", &[]), "DC 3.3e0");
        assert_eq!(line("I1", "1m", &[]), "DC 1e-3");
        let dc = [("Sim.Type", "DC"), ("Sim.Params", "dc=12 ac=1")];
        assert_eq!(line("V1", "", &dc), "DC 1.2e1 AC 1e0");
        let sin = [("Sim.Type", "SIN"), ("Sim.Params", "dc=0 ampl=1 f=1k")];
        assert_eq!(line("V2", "", &sin), "SIN(0e0 1e0 1e3)");
        let pulse = [
            ("Sim.Type", "PULSE"),
            ("Sim.Params", "y1=0 y2=5 tr=\"1u\" per=1m"),
        ];
        assert_eq!(line("V3", "", &pulse), "PULSE(0e0 5e0 0e0 1e-6 0e0 0e0 1e-3)");
    }

    #[test]
    fn instance_names_start_with_the_element_letter() {
        assert_eq!(Element::Resistor.instance_name("R1"), "R1");
        assert_eq!(Element::Resistor.instance_name("r1"), "r1");
        assert_eq!(Element::VoltageSource.instance_name("BAT1"), "VBAT1");
    }

    #[test]
    fn unusable_models_explain_what_to_change() {
        let cases: [(&str, &str, &[(&str, &str)], &str); 12] = [
            ("U1", "LM358", &[], "no simulation model"),
            ("R1", "R", &[], "\"R\" is not a valid value"),
            ("Q1", "2N3904", &[("Sim.Device", "NPN")], "\"NPN\" is not supported"),
            ("D1", "1N4148", &[("Sim.Library", "diodes.lib")], "diodes.lib"),
            ("R1", "1k", &[("Sim.Type", "POT")], "\"POT\" is not supported"),
            ("V1", "", &[("Sim.Type", "EXP")], "\"EXP\" is not supported"),
            ("V1", "", &[("Sim.Type", "SIN"), ("Sim.Params", "ampl=1")], "needs f"),
            ("R1", "1k", &[("Sim.Params", "tc1=0.1")], "\"tc1\" is not supported"),
            ("R1", "1k", &[("Sim.Params", "r=1k r=2k")], "more than once"),
            ("R1", "1k", &[("Sim.Params", "r 1k")], "name=value"),
            ("R1", "1k", &[("Sim.Pins", "1=+ 3=-")], "pin 3"),
            ("R1", "1k", &[("Sim.Pins", "1=+")], "must map two pins"),
        ];
        for (reference, value, fields, expected) in cases {
            let error = resolve(reference, value, fields).unwrap_err();
            assert!(error.contains(expected), "{reference}: {error}");
        }
        let three = vec!["1".to_owned(), "2".to_owned(), "3".to_owned()];
        let error = device(&symbol("R1", "1k", &[]), &three).unwrap_err();
        assert!(error.contains("3 pins"), "{error}");
    }

    #[test]
    fn params_accept_quoted_values() {
        let params = parse_params("dc=0  AMPL=\"1\" f=1k").unwrap();
        let keys: Vec<&str> = params.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["dc", "ampl", "f"]);
        assert_eq!(params[1].1, "1");
        assert!(parse_params("a=\"1").is_err());
        assert!(parse_params("a=\"1\"b=2").is_err());
        assert!(parse_params("=1").is_err());
    }
}
