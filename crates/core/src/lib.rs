//! Design document model and geometry primitives shared by every Tron component.

pub mod error;
pub mod kicad;
pub mod manifest;
pub mod project;
pub mod session;
pub mod sexpr;
pub mod units;

pub use error::CoreError;
pub use kicad::{PageSize, TITLE_BLOCK_COMMENTS, TitleBlock};
pub use project::{Project, validate_name, write_atomic};
pub use session::{Changes, Command, Session};
pub use units::{Angle, Nm, Point, UnitError};
