//! Circuit simulation: the SPICE deck for a schematic, checked before ngspice runs.

mod analysis;
mod deck;
mod error;
mod model;
mod value;

pub use analysis::{Analysis, Transient};
pub use deck::{Deck, GROUND, deck};
pub use error::PreflightError;
pub use value::{parse_value, spice_number};
