//! KiCad-style S-expression reading and writing.
//!
//! The tree keeps every node it reads, including ones Tron doesn't understand,
//! so writing a parsed file back never drops data. Output is canonical: lists
//! that contain other lists put each nested list on its own tab-indented line.

use std::fmt;

const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SExpr {
    /// A bare token such as a keyword, a number, or a layer flag.
    Atom(String),
    /// A double-quoted string.
    Str(String),
    List(Vec<SExpr>),
}

impl SExpr {
    pub fn atom(value: impl Into<String>) -> SExpr {
        SExpr::Atom(value.into())
    }

    pub fn string(value: impl Into<String>) -> SExpr {
        SExpr::Str(value.into())
    }

    /// Builds `(head args...)`.
    pub fn node(head: &str, args: Vec<SExpr>) -> SExpr {
        let mut items = Vec::with_capacity(args.len() + 1);
        items.push(SExpr::atom(head));
        items.extend(args);
        SExpr::List(items)
    }

    /// The leading atom of a list, such as `title` in `(title "Amp")`.
    pub fn head(&self) -> Option<&str> {
        match self {
            SExpr::List(items) => match items.first() {
                Some(SExpr::Atom(head)) => Some(head),
                _ => None,
            },
            _ => None,
        }
    }

    /// The items of a list after its head. Empty for atoms and strings.
    pub fn args(&self) -> &[SExpr] {
        match self {
            SExpr::List(items) if !items.is_empty() => &items[1..],
            _ => &[],
        }
    }

    pub fn items_mut(&mut self) -> Option<&mut Vec<SExpr>> {
        match self {
            SExpr::List(items) => Some(items),
            _ => None,
        }
    }

    /// The text of an atom or a string.
    pub fn text(&self) -> Option<&str> {
        match self {
            SExpr::Atom(text) | SExpr::Str(text) => Some(text),
            SExpr::List(_) => None,
        }
    }

    /// The first nested list whose head is `head`.
    pub fn child(&self, head: &str) -> Option<&SExpr> {
        self.args().iter().find(|item| item.head() == Some(head))
    }

    /// Writes the expression in canonical form, followed by a newline.
    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        write_expr(self, 0, &mut out);
        out.push('\n');
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column, counted in characters.
    pub column: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// Parses a whole file that holds exactly one top-level list.
pub fn parse(source: &str) -> Result<SExpr, ParseError> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut parser = Parser {
        chars: source.chars().peekable(),
        line: 1,
        column: 1,
    };
    parser.skip_whitespace();
    if parser.peek() != Some('(') {
        return Err(parser.error("expected '(' at the start of the file"));
    }
    let expr = parser.parse_expr(0)?;
    parser.skip_whitespace();
    if parser.peek().is_some() {
        return Err(parser.error("unexpected content after the closing ')'"));
    }
    Ok(expr)
}

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
    column: usize,
}

impl Parser<'_> {
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            line: self.line,
            column: self.column,
            message: message.to_owned(),
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn parse_expr(&mut self, depth: usize) -> Result<SExpr, ParseError> {
        match self.peek() {
            None => Err(self.error("unexpected end of file")),
            Some('(') => self.parse_list(depth),
            Some(')') => Err(self.error("unexpected ')'")),
            Some('"') => self.parse_string(),
            Some(_) => Ok(self.parse_atom()),
        }
    }

    fn parse_list(&mut self, depth: usize) -> Result<SExpr, ParseError> {
        if depth >= MAX_DEPTH {
            return Err(self.error("lists are nested too deeply"));
        }
        let (line, column) = (self.line, self.column);
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                None => {
                    return Err(ParseError {
                        line,
                        column,
                        message: "this '(' is never closed".to_owned(),
                    });
                }
                Some(')') => {
                    self.bump();
                    return Ok(SExpr::List(items));
                }
                Some(_) => items.push(self.parse_expr(depth + 1)?),
            }
        }
    }

    fn parse_string(&mut self) -> Result<SExpr, ParseError> {
        let (line, column) = (self.line, self.column);
        self.bump();
        let mut text = String::new();
        loop {
            match self.bump() {
                None => {
                    return Err(ParseError {
                        line,
                        column,
                        message: "this string is never closed".to_owned(),
                    });
                }
                Some('"') => return Ok(SExpr::Str(text)),
                Some('\\') => match self.bump() {
                    None => {
                        return Err(ParseError {
                            line,
                            column,
                            message: "this string is never closed".to_owned(),
                        });
                    }
                    Some('n') => text.push('\n'),
                    Some('r') => text.push('\r'),
                    Some('t') => text.push('\t'),
                    Some(other) => text.push(other),
                },
                Some(c) => text.push(c),
            }
        }
    }

    fn parse_atom(&mut self) -> SExpr {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || matches!(c, '(' | ')' | '"') {
                break;
            }
            text.push(c);
            self.bump();
        }
        SExpr::Atom(text)
    }
}

fn write_expr(expr: &SExpr, depth: usize, out: &mut String) {
    match expr {
        SExpr::Atom(text) => out.push_str(text),
        SExpr::Str(text) => write_quoted(text, out),
        SExpr::List(items) => {
            out.push('(');
            let mut broken = false;
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    if matches!(item, SExpr::List(_)) {
                        out.push('\n');
                        push_tabs(depth + 1, out);
                        broken = true;
                    } else {
                        out.push(' ');
                    }
                }
                write_expr(item, depth + 1, out);
            }
            if broken {
                out.push('\n');
                push_tabs(depth, out);
            }
            out.push(')');
        }
    }
}

fn write_quoted(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
}

fn push_tabs(count: usize, out: &mut String) {
    for _ in 0..count {
        out.push('\t');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_atoms_strings_and_nested_lists() {
        let expr = parse("(kicad_sch (version 20231120) (title \"Amp\"))").unwrap();
        assert_eq!(expr.head(), Some("kicad_sch"));
        assert_eq!(
            expr.child("version").and_then(|v| v.args()[0].text()),
            Some("20231120")
        );
        assert_eq!(
            expr.child("title").map(|t| t.args()[0].clone()),
            Some(SExpr::string("Amp"))
        );
    }

    #[test]
    fn unescapes_and_reescapes_strings() {
        let expr = parse(r#"(t "a \"quoted\" back\\slash\nline")"#).unwrap();
        assert_eq!(
            expr.args()[0],
            SExpr::string("a \"quoted\" back\\slash\nline")
        );
        assert_eq!(
            expr.to_pretty_string(),
            "(t \"a \\\"quoted\\\" back\\\\slash\\nline\")\n"
        );
    }

    #[test]
    fn writes_nested_lists_on_indented_lines() {
        let expr = parse("(a (b 1 2) (c (d \"x\")) e)").unwrap();
        assert_eq!(
            expr.to_pretty_string(),
            "(a\n\t(b 1 2)\n\t(c\n\t\t(d \"x\")\n\t) e\n)\n"
        );
    }

    #[test]
    fn writing_then_parsing_returns_the_same_tree() {
        let source = "(root (x 1.5 -2) \"s p\" (empty) (deep (deeper (deepest yes))))";
        let expr = parse(source).unwrap();
        assert_eq!(parse(&expr.to_pretty_string()).unwrap(), expr);
    }

    #[test]
    fn ignores_a_byte_order_mark() {
        assert_eq!(parse("\u{feff}(a)").unwrap(), SExpr::node("a", vec![]));
    }

    #[test]
    fn reports_the_position_of_an_unclosed_list() {
        let err = parse("(a\n  (b\n").unwrap_err();
        assert_eq!((err.line, err.column), (2, 3));
        assert_eq!(err.message, "this '(' is never closed");
    }

    #[test]
    fn reports_the_position_of_an_unclosed_string() {
        let err = parse("(a \"open").unwrap_err();
        assert_eq!((err.line, err.column), (1, 4));
    }

    #[test]
    fn rejects_trailing_content_and_missing_open_paren() {
        let err = parse("(a))").unwrap_err();
        assert_eq!((err.line, err.column), (1, 4));
        let err = parse("  atom").unwrap_err();
        assert_eq!((err.line, err.column), (1, 3));
        let err = parse("").unwrap_err();
        assert_eq!((err.line, err.column), (1, 1));
    }

    #[test]
    fn rejects_excessive_nesting() {
        let source = "(".repeat(MAX_DEPTH + 1) + &")".repeat(MAX_DEPTH + 1);
        assert_eq!(
            parse(&source).unwrap_err().message,
            "lists are nested too deeply"
        );
    }
}
