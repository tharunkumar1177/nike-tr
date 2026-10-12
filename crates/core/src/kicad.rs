//! KiCad schematic and board files.
//!
//! Each file is held as its full S-expression tree. Typed accessors read and
//! replace the parts Tron edits, and every other node passes through untouched.

use crate::sexpr::{self, ParseError, SExpr};
use crate::units::Nm;
use uuid::Uuid;

pub const SCHEMATIC_FORMAT_VERSION: &str = "20231120";
pub const BOARD_FORMAT_VERSION: &str = "20240108";
pub const GENERATOR: &str = "tron";
pub const GENERATOR_VERSION: &str = "0.1";
pub const TITLE_BLOCK_COMMENTS: usize = 9;

fn is_title_block_field(node: &SExpr) -> bool {
    matches!(node.head(), Some("title" | "date" | "rev" | "company" | "comment"))
}

fn is_sheet_header(node: &SExpr) -> bool {
    matches!(
        node.head(),
        Some("version" | "generator" | "generator_version" | "uuid" | "paper")
    )
}

/// Why a file could not be read as the expected KiCad document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    Syntax(ParseError),
    WrongKind { expected: &'static str },
}

/// The title block printed on the schematic sheet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TitleBlock {
    pub title: String,
    pub date: String,
    pub revision: String,
    pub company: String,
    /// `comments[0]` is KiCad's `comment 1`.
    pub comments: [String; TITLE_BLOCK_COMMENTS],
}

impl TitleBlock {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
            && self.date.is_empty()
            && self.revision.is_empty()
            && self.company.is_empty()
            && self.comments.iter().all(String::is_empty)
    }

    fn read(node: &SExpr) -> TitleBlock {
        let mut block = TitleBlock::default();
        for child in node.args() {
            let first_text = || {
                child
                    .args()
                    .first()
                    .and_then(SExpr::text)
                    .unwrap_or_default()
                    .to_owned()
            };
            match child.head() {
                Some("title") => block.title = first_text(),
                Some("date") => block.date = first_text(),
                Some("rev") => block.revision = first_text(),
                Some("company") => block.company = first_text(),
                Some("comment") => {
                    let number = child
                        .args()
                        .first()
                        .and_then(SExpr::text)
                        .and_then(|n| n.parse::<usize>().ok());
                    let text = child.args().get(1).and_then(SExpr::text);
                    if let (Some(number), Some(text)) = (number, text) {
                        if (1..=TITLE_BLOCK_COMMENTS).contains(&number) {
                            block.comments[number - 1] = text.to_owned();
                        }
                    }
                }
                _ => {}
            }
        }
        block
    }

    /// Builds the `title_block` node, keeping any children Tron doesn't edit.
    fn to_node(&self, previous: Option<&SExpr>) -> SExpr {
        let mut args = Vec::new();
        let fields = [
            ("title", &self.title),
            ("date", &self.date),
            ("rev", &self.revision),
            ("company", &self.company),
        ];
        for (head, value) in fields {
            if !value.is_empty() {
                args.push(SExpr::node(head, vec![SExpr::string(value.as_str())]));
            }
        }
        for (index, comment) in self.comments.iter().enumerate() {
            if !comment.is_empty() {
                args.push(SExpr::node(
                    "comment",
                    vec![
                        SExpr::atom((index + 1).to_string()),
                        SExpr::string(comment.as_str()),
                    ],
                ));
            }
        }
        if let Some(previous) = previous {
            args.extend(
                previous
                    .args()
                    .iter()
                    .filter(|child| !is_title_block_field(child))
                    .cloned(),
            );
        }
        SExpr::node("title_block", args)
    }
}

/// The drawing sheet size, in the schematic's coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSize {
    pub width: Nm,
    pub height: Nm,
}

impl PageSize {
    const fn from_micrometers(width: i64, height: i64) -> PageSize {
        PageSize {
            width: Nm(width * 1_000),
            height: Nm(height * 1_000),
        }
    }

    pub const A4: PageSize = PageSize::from_micrometers(297_000, 210_000);

    fn named(name: &str) -> Option<PageSize> {
        let size = match name {
            "A5" => PageSize::from_micrometers(210_000, 148_000),
            "A4" => PageSize::A4,
            "A3" => PageSize::from_micrometers(420_000, 297_000),
            "A2" => PageSize::from_micrometers(594_000, 420_000),
            "A1" => PageSize::from_micrometers(841_000, 594_000),
            "A0" => PageSize::from_micrometers(1_189_000, 841_000),
            "A" | "USLetter" => PageSize::from_micrometers(279_400, 215_900),
            "B" | "USLedger" => PageSize::from_micrometers(431_800, 279_400),
            "C" => PageSize::from_micrometers(558_800, 431_800),
            "D" => PageSize::from_micrometers(863_600, 558_800),
            "E" => PageSize::from_micrometers(1_117_600, 863_600),
            "USLegal" => PageSize::from_micrometers(355_600, 215_900),
            _ => return None,
        };
        Some(size)
    }

    /// Reads `(paper "A4")`, `(paper "A3" portrait)`, or `(paper "User" w h)`.
    /// A missing or unrecognized paper falls back to A4, as KiCad does.
    fn read(node: Option<&SExpr>) -> PageSize {
        let Some(node) = node else {
            return PageSize::A4;
        };
        let args = node.args();
        let name = args.first().and_then(SExpr::text).unwrap_or_default();
        let size = if name == "User" {
            let millimeters = |index: usize| {
                args.get(index)
                    .and_then(SExpr::text)
                    .and_then(|text| text.parse::<f64>().ok())
                    .and_then(|mm| Nm::from_mm(mm).ok())
                    .filter(|nm| nm.0 > 0)
            };
            match (millimeters(1), millimeters(2)) {
                (Some(width), Some(height)) => PageSize { width, height },
                _ => PageSize::A4,
            }
        } else {
            PageSize::named(name).unwrap_or(PageSize::A4)
        };
        let portrait = args.iter().any(|arg| arg.text() == Some("portrait"));
        if portrait && size.width > size.height {
            PageSize {
                width: size.height,
                height: size.width,
            }
        } else {
            size
        }
    }
}

fn parse_kind(source: &str, expected: &'static str) -> Result<SExpr, FileError> {
    let root = sexpr::parse(source).map_err(FileError::Syntax)?;
    if root.head() == Some(expected) {
        Ok(root)
    } else {
        Err(FileError::WrongKind { expected })
    }
}

fn header(kind: &str, version: &str) -> Vec<SExpr> {
    vec![
        SExpr::atom(kind),
        SExpr::node("version", vec![SExpr::atom(version)]),
        SExpr::node("generator", vec![SExpr::string(GENERATOR)]),
        SExpr::node("generator_version", vec![SExpr::string(GENERATOR_VERSION)]),
    ]
}

/// A `.kicad_sch` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schematic {
    root: SExpr,
}

impl Schematic {
    pub fn new_empty(uuid: Uuid) -> Schematic {
        let mut items = header("kicad_sch", SCHEMATIC_FORMAT_VERSION);
        items.extend([
            SExpr::node("uuid", vec![SExpr::string(uuid.to_string())]),
            SExpr::node("paper", vec![SExpr::string("A4")]),
            SExpr::node("lib_symbols", vec![]),
            SExpr::node(
                "sheet_instances",
                vec![SExpr::node(
                    "path",
                    vec![
                        SExpr::string("/"),
                        SExpr::node("page", vec![SExpr::string("1")]),
                    ],
                )],
            ),
        ]);
        Schematic {
            root: SExpr::List(items),
        }
    }

    pub fn parse(source: &str) -> Result<Schematic, FileError> {
        parse_kind(source, "kicad_sch").map(|root| Schematic { root })
    }

    pub fn to_file_string(&self) -> String {
        self.root.to_pretty_string()
    }

    pub fn title_block(&self) -> TitleBlock {
        self.root
            .child("title_block")
            .map(TitleBlock::read)
            .unwrap_or_default()
    }

    pub fn page_size(&self) -> PageSize {
        PageSize::read(self.root.child("paper"))
    }

    /// Replaces the title block in place, or inserts it after the sheet
    /// header when the file has none. An empty block removes the node.
    pub fn set_title_block(&mut self, block: &TitleBlock) {
        let Some(items) = self.root.items_mut() else {
            return;
        };
        let existing = items
            .iter()
            .position(|item| item.head() == Some("title_block"));
        let node = block.to_node(existing.map(|index| &items[index]));
        let keep_node = !node.args().is_empty();
        match (existing, keep_node) {
            (Some(index), true) => items[index] = node,
            (Some(index), false) => {
                items.remove(index);
            }
            (None, true) => {
                let after_header = items
                    .iter()
                    .rposition(is_sheet_header)
                    .map_or(1, |index| index + 1);
                items.insert(after_header, node);
            }
            (None, false) => {}
        }
    }
}

/// A `.kicad_pcb` file. Tron doesn't edit boards yet, so it is preserved as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    root: SExpr,
}

impl Board {
    pub fn new_empty() -> Board {
        let layer = |id: &str, name: &str, kind: &str, user_name: Option<&str>| {
            let mut items = vec![SExpr::atom(id), SExpr::string(name), SExpr::atom(kind)];
            if let Some(user_name) = user_name {
                items.push(SExpr::string(user_name));
            }
            SExpr::List(items)
        };
        let mut items = header("kicad_pcb", BOARD_FORMAT_VERSION);
        items.extend([
            SExpr::node(
                "general",
                vec![SExpr::node("thickness", vec![SExpr::atom("1.6")])],
            ),
            SExpr::node("paper", vec![SExpr::string("A4")]),
            SExpr::node(
                "layers",
                vec![
                    layer("0", "F.Cu", "signal", None),
                    layer("31", "B.Cu", "signal", None),
                    layer("36", "B.SilkS", "user", Some("B.Silkscreen")),
                    layer("37", "F.SilkS", "user", Some("F.Silkscreen")),
                    layer("38", "B.Mask", "user", None),
                    layer("39", "F.Mask", "user", None),
                    layer("44", "Edge.Cuts", "user", None),
                ],
            ),
            SExpr::node(
                "setup",
                vec![SExpr::node("pad_to_mask_clearance", vec![SExpr::atom("0")])],
            ),
            SExpr::node("net", vec![SExpr::atom("0"), SExpr::string("")]),
        ]);
        Board {
            root: SExpr::List(items),
        }
    }

    pub fn parse(source: &str) -> Result<Board, FileError> {
        parse_kind(source, "kicad_pcb").map(|root| Board { root })
    }

    pub fn to_file_string(&self) -> String {
        self.root.to_pretty_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_block() -> TitleBlock {
        let mut block = TitleBlock {
            title: "Audio \"Amp\"".to_owned(),
            date: "2026-10-12".to_owned(),
            revision: "B".to_owned(),
            company: "Bench Labs".to_owned(),
            ..TitleBlock::default()
        };
        block.comments[0] = "First".to_owned();
        block.comments[8] = "Ninth".to_owned();
        block
    }

    #[test]
    fn title_block_round_trips_through_the_schematic() {
        let mut schematic = Schematic::new_empty(Uuid::nil());
        schematic.set_title_block(&sample_block());
        let reparsed = Schematic::parse(&schematic.to_file_string()).unwrap();
        assert_eq!(reparsed.title_block(), sample_block());
        assert_eq!(reparsed, schematic);
    }

    #[test]
    fn title_block_is_inserted_after_the_paper_node() {
        let mut schematic = Schematic::new_empty(Uuid::nil());
        schematic.set_title_block(&sample_block());
        let heads: Vec<_> = schematic.root.args().iter().map(SExpr::head).collect();
        assert_eq!(
            heads,
            [
                Some("version"),
                Some("generator"),
                Some("generator_version"),
                Some("uuid"),
                Some("paper"),
                Some("title_block"),
                Some("lib_symbols"),
                Some("sheet_instances"),
            ]
        );
    }

    #[test]
    fn clearing_the_title_block_removes_the_node() {
        let mut schematic = Schematic::new_empty(Uuid::nil());
        schematic.set_title_block(&sample_block());
        schematic.set_title_block(&TitleBlock::default());
        assert_eq!(schematic, Schematic::new_empty(Uuid::nil()));
    }

    #[test]
    fn editing_the_title_block_keeps_unknown_children() {
        let source = "(kicad_sch (version 20231120) (title_block (title \"Old\") (vendor_note \"keep\")))";
        let mut schematic = Schematic::parse(source).unwrap();
        let block = TitleBlock {
            title: "New".to_owned(),
            ..TitleBlock::default()
        };
        schematic.set_title_block(&block);
        assert_eq!(
            schematic.to_file_string(),
            "(kicad_sch\n\t(version 20231120)\n\t(title_block\n\t\t(title \"New\")\n\t\t(vendor_note \"keep\")\n\t)\n)\n"
        );
    }

    #[test]
    fn paper_sizes_follow_kicad_names_and_orientation() {
        let size = |source: &str| Schematic::parse(source).unwrap().page_size();
        assert_eq!(size("(kicad_sch)"), PageSize::A4);
        assert_eq!(
            size("(kicad_sch (paper \"A3\"))"),
            PageSize::from_micrometers(420_000, 297_000)
        );
        assert_eq!(
            size("(kicad_sch (paper \"A4\" portrait))"),
            PageSize::from_micrometers(210_000, 297_000)
        );
        assert_eq!(
            size("(kicad_sch (paper \"User\" 100.5 80))"),
            PageSize::from_micrometers(100_500, 80_000)
        );
        assert_eq!(size("(kicad_sch (paper \"Napkin\"))"), PageSize::A4);
    }

    #[test]
    fn rejects_a_file_of_the_wrong_kind() {
        assert_eq!(
            Schematic::parse("(kicad_pcb)"),
            Err(FileError::WrongKind {
                expected: "kicad_sch",
            })
        );
        assert!(matches!(Board::parse("(kicad_pcb"), Err(FileError::Syntax(_))));
    }
}
