//! Preview geometry stored in the index, so the library browser can draw a part without reading
//! KiCad files.
//!
//! Symbol preview geometry is extracted by `tron_core::symbol` and re-exported here unchanged.
//! Every coordinate and length is an `i64` count of nanometers in a Y-down space, the same as
//! schematic and board geometry. Points serialize as `[x, y]` to keep the index small.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use tron_core::symbol::{Fill, Pin, Shape, SymbolPreview, Xy};

/// The front side of a footprint: its pads plus silkscreen, fabrication, and courtyard graphics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FootprintPreview {
    pub graphics: Vec<FootprintGraphic>,
    pub pads: Vec<Pad>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FootprintGraphic {
    pub layer: Layer,
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Layer {
    Silkscreen,
    Fabrication,
    Courtyard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Pad {
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    pub center: Xy,
    /// Width and height before rotation.
    pub size: Xy,
    /// Tenths of a degree, counterclockwise, in `[0, 3600)`.
    pub rotation: i32,
    /// Zero unless the shape is a rounded rectangle.
    #[ts(type = "number")]
    pub corner_radius: i64,
    /// Drill width and height for through-hole pads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drill: Option<Xy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PadKind {
    Smd,
    ThroughHole,
    NonPlatedHole,
    /// Copper with no paste or hole, such as an edge connector finger.
    Connect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PadShape {
    Rect,
    RoundRect,
    Circle,
    Oval,
    Trapezoid,
    /// A custom outline, previewed as its anchor rectangle.
    Custom,
}
