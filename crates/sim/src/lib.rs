//! Circuit simulation: the SPICE deck for a schematic, checked before ngspice runs, and the
//! ngspice engine that runs it. Only `ngspice` uses `unsafe`.

mod analysis;
mod deck;
mod error;
mod model;
mod ngspice;
mod value;

pub use analysis::{Analysis, Transient};
pub use deck::{Deck, GROUND, deck};
pub use error::PreflightError;
pub use ngspice::{Engine, EngineError, EngineGuard, Plot, Vector};
pub use value::{parse_value, spice_number};
