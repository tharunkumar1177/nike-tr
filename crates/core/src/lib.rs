//! Design document model and geometry primitives shared by every Tron component.

pub mod error;
pub mod kicad;
pub mod kicad_value;
pub mod manifest;
pub mod project;
pub mod session;
pub mod sexpr;
pub mod symbol;
pub mod units;

pub use error::CoreError;
pub use kicad::{
    Mirror, PageSize, PlacedPin, PlacedSymbol, Placement, SymbolProperty, TITLE_BLOCK_COMMENTS,
    TitleBlock,
};
pub use project::{Project, validate_name, write_atomic};
pub use session::{Changes, Command, Session, SymbolTransform};
pub use symbol::{Fill, Pin, Shape, SymbolGraphics, SymbolPreview, Xy};
pub use units::{Angle, Nm, Point, UnitError};
