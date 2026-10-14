//! Component values in KiCad's SI notation, and numbers written for ngspice.

/// SI prefixes and their powers of ten, longest first so `Meg` wins over `M`. `M` is mega
/// and `m` is milli, as in KiCad's fields; SPICE's own notation would read `M` as milli.
const PREFIXES: [(&str, i32); 15] = [
    ("Meg", 6),
    ("meg", 6),
    ("MEG", 6),
    ("a", -18),
    ("f", -15),
    ("p", -12),
    ("n", -9),
    ("u", -6),
    ("µ", -6),
    ("μ", -6),
    ("m", -3),
    ("k", 3),
    ("K", 3),
    ("M", 6),
    ("G", 9),
];

/// Units allowed after the number and prefix. They carry no meaning for the value.
const UNITS: [&str; 9] = ["Ω", "Ohm", "ohm", "F", "H", "V", "A", "Hz", "s"];

/// Parses a value such as `10k`, `4k7`, `4R7`, `1.5u`, `2.2µF`, `1M`, `1Meg`, `100m`, or
/// `1e-6`. The prefix may stand in for the decimal point, and a unit may follow. The
/// result is the double nearest the decimal value, so `100n` is exactly `1e-7`.
pub fn parse_value(text: &str) -> Result<f64, String> {
    let invalid = || format!("\"{text}\" is not a valid value.");
    let trimmed = text.trim();
    let (mantissa, rest) = split_number(trimmed).ok_or_else(invalid)?;
    let has_exponent = mantissa.contains(['e', 'E']);
    let plain_integer = mantissa
        .chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == '+');
    let (exponent, rest) = match PREFIXES.iter().find(|(prefix, _)| rest.starts_with(prefix)) {
        Some(_) if has_exponent => return Err(invalid()),
        Some((prefix, exponent)) => (*exponent, &rest[prefix.len()..]),
        None if plain_integer && rest.starts_with('R') => (0, &rest[1..]),
        None => (0, rest),
    };
    let fraction_len = rest.chars().take_while(char::is_ascii_digit).count();
    let (number, unit) = if fraction_len > 0 {
        if !plain_integer {
            return Err(invalid());
        }
        let (fraction, unit) = rest.split_at(fraction_len);
        (format!("{mantissa}.{fraction}"), unit)
    } else {
        (mantissa.to_owned(), rest)
    };
    if !unit.is_empty() && !UNITS.contains(&unit) {
        return Err(invalid());
    }
    let decimal = if exponent == 0 {
        number
    } else {
        format!("{number}e{exponent}")
    };
    match decimal.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(invalid()),
    }
}

/// Splits `text` into a leading decimal number, with an optional exponent, and the rest.
fn split_number(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end += 1;
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let mut digit_count = end - digits_start;
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        let fraction_start = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        digit_count += end - fraction_start;
    }
    if digit_count == 0 {
        return None;
    }
    if end < bytes.len() && matches!(bytes[end], b'e' | b'E') {
        let mut exponent = end + 1;
        if exponent < bytes.len() && matches!(bytes[exponent], b'+' | b'-') {
            exponent += 1;
        }
        let exponent_digits = exponent;
        while exponent < bytes.len() && bytes[exponent].is_ascii_digit() {
            exponent += 1;
        }
        if exponent > exponent_digits {
            end = exponent;
        }
    }
    Some(text.split_at(end))
}

/// A number as ngspice reads it back exactly: the shortest round-trip form, in exponent
/// notation, so SPICE never applies a scale suffix.
pub fn spice_number(value: f64) -> String {
    format!("{value:e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_prefixed_values_parse() {
        let cases = [
            ("10k", 10e3),
            ("10K", 10e3),
            ("4k7", 4.7e3),
            ("4R7", 4.7),
            ("2M2", 2.2e6),
            ("1.5u", 1.5e-6),
            ("2.2µF", 2.2e-6),
            ("100n", 1e-7),
            ("1M", 1e6),
            ("1Meg", 1e6),
            ("1meg", 1e6),
            ("100m", 0.1),
            ("1f", 1e-15),
            ("1F", 1.0),
            ("1mH", 1e-3),
            ("10kΩ", 10e3),
            ("5V", 5.0),
            ("-3.3", -3.3),
            ("1e-6", 1e-6),
            ("2.5E3", 2.5e3),
            (" 47 ", 47.0),
            (".5", 0.5),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_value(text), Ok(expected), "{text}");
        }
    }

    #[test]
    fn malformed_values_are_rejected() {
        let malformed = [
            "", "R", "k", "10x", "1.5k7", "1e3k", "10 k", "--1", "1e400", "4R7k",
        ];
        for text in malformed {
            assert!(parse_value(text).is_err(), "{text} should be rejected");
        }
    }

    #[test]
    fn spice_numbers_round_trip() {
        assert_eq!(spice_number(10e3), "1e4");
        assert_eq!(spice_number(4.7e3), "4.7e3");
        assert_eq!(spice_number(1e-6), "1e-6");
        assert_eq!(spice_number(0.0), "0e0");
        assert_eq!(spice_number(-5.0), "-5e0");
        for value in [1.0 / 3.0, 4.7e-9, 12345.678] {
            assert_eq!(spice_number(value).parse::<f64>().unwrap(), value);
        }
    }
}
