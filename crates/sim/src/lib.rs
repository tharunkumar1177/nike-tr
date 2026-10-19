//! Circuit simulation: the SPICE deck for a schematic, checked before ngspice runs, the
//! ngspice engine that runs it, and the reduction of its results for plotting. Only
//! `ngspice` uses `unsafe`.

mod analysis;
mod deck;
mod error;
mod model;
mod ngspice;
mod op;
mod value;
mod waveform;

pub use analysis::{Analysis, Transient};
pub use deck::{Deck, GROUND, deck};
pub use error::PreflightError;
pub use ngspice::{Engine, EngineError, EngineGuard, Plot, Vector};
pub use op::{OpAnchor, OpAnchors, OpKind, OpValue, Probe, ProbeMap, op_anchors, probe_map};
pub use value::{parse_value, spice_number};
pub use waveform::{Sample, decimate};
