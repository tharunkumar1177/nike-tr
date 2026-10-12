//! Turns what the user typed into a safe SQLite FTS5 query.

/// Terms past this many are ignored.
const MAX_TERMS: usize = 16;

/// Builds an FTS5 `MATCH` expression in which every term must match the start of a word.
///
/// Terms are split on whitespace and on `:`, so `Device:R` searches the library and the name.
/// Each term is quoted, so FTS5 operators and syntax characters in the input are searched as
/// plain text. Returns `None` when the input has no letters or digits.
pub fn match_expression(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == ':')
        .filter(|term| term.chars().any(char::is_alphanumeric))
        .take(MAX_TERMS)
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_TERMS, match_expression};

    #[test]
    fn quotes_every_term_as_a_prefix() {
        let expected = Some("\"usb\"* \"C\"*".to_owned());
        assert_eq!(match_expression("  usb   C "), expected);
    }

    #[test]
    fn splits_library_qualified_names() {
        let expected = Some("\"Device\"* \"R\"*".to_owned());
        assert_eq!(match_expression("Device:R"), expected);
    }

    #[test]
    fn escapes_quotes_and_neutralizes_operators() {
        let expected = Some("\"a\"\"b\"* \"OR\"* \"NEAR(x\"*".to_owned());
        assert_eq!(match_expression("a\"b OR NEAR(x"), expected);
    }

    #[test]
    fn ignores_input_without_letters_or_digits() {
        assert_eq!(match_expression(""), None);
        assert_eq!(match_expression("  \" * - ( ) : "), None);
    }

    #[test]
    fn caps_the_number_of_terms() {
        let query = "a ".repeat(MAX_TERMS + 5);
        let expression = match_expression(&query).unwrap();
        assert_eq!(expression.matches('*').count(), MAX_TERMS);
    }
}
