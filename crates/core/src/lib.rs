//! Design document model and geometry primitives shared by every Tron component.

pub mod error;
pub mod kicad;
pub mod kicad_value;
pub mod manifest;
pub mod netlist;
pub mod project;
pub mod session;
pub mod sexpr;
pub mod symbol;
pub mod units;
pub mod wiring;

pub use error::CoreError;
pub use kicad::{
    MANDATORY_FIELDS, Mirror, PageSize, PinConnection, PlacedPin, PlacedSymbol, Placement,
    SymbolProperty, TITLE_BLOCK_COMMENTS, TitleBlock,
};
pub use netlist::{Net, NetNode, netlist};
pub use project::{Project, validate_name, write_atomic};
pub use session::{Changes, Command, FieldEdit, Session, SymbolTransform};
pub use symbol::{Fill, Pin, Shape, SymbolGraphics, SymbolPreview, Xy, unit_count};
pub use units::{Angle, Nm, Point, UnitError};
pub use wiring::{Junction, Label, LabelKind, LabelShape, NoConnect, Wire};
