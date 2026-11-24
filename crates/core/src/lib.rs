//! Design document model and geometry primitives shared by every Tron component.

pub mod board;
pub mod board_update;
pub mod erc;
pub mod error;
pub mod footprint;
pub mod kicad;
pub mod kicad_value;
pub mod manifest;
pub mod netlist;
pub mod outline;
pub mod project;
pub mod project_file;
pub mod ratsnest;
pub mod session;
pub mod sexpr;
pub mod symbol;
pub mod units;
pub mod wiring;

pub use board::{
    Board, BoardGraphic, Layer, LayerKind, OutlineShape, Stackup, StackupLayer, layer_ordinal,
};
pub use board_update::{
    BoardUpdateIssue, BoardUpdateOptions, BoardUpdatePlan, FieldChange, FootprintField,
    FootprintReplacement, PadNetChange, PlannedFootprint, plan_board_update, required_footprints,
};
pub use erc::{ERC_RULES, ErcRule, RuleSeverities, RuleSeverity, Severity, Violation};
pub use error::CoreError;
pub use footprint::{Footprint, Pad, Side};
pub use kicad::{
    MANDATORY_FIELDS, Mirror, PageSize, PinConnection, PlacedPin, PlacedSymbol, Placement,
    SymbolProperty, TITLE_BLOCK_COMMENTS, TitleBlock,
};
pub use manifest::{AcSweep, AnalysisSettings};
pub use netlist::{Net, NetNode, netlist};
pub use project::{Project, validate_name, write_atomic};
pub use project_file::ProjectFile;
pub use ratsnest::{Ratsnest, RatsnestLine};
pub use session::{Changes, Command, FieldEdit, FootprintTransform, Session, SymbolTransform};
pub use symbol::{Fill, Pin, Shape, SymbolGraphics, SymbolPreview, Xy, unit_count};
pub use units::{Angle, Nm, Point, UnitError};
pub use wiring::{Junction, Label, LabelKind, LabelShape, NoConnect, Wire};
