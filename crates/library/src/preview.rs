//! Preview geometry stored in the index, so the library browser can draw a part without reading
//! KiCad files.
//!
//! Every coordinate and length is an `i64` count of nanometers in a Y-down space, the same as
//! schematic and board geometry. Points serialize as `[x, y]` to keep the index small.

use serde::{Deserialize, Serialize};
use tron_core::units::Point;
use ts_rs::TS;

/// A point in nanometers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Xy(
    #[ts(type = "number")] pub i64,
    #[ts(type = "number")] pub i64,
);

impl From<Point> for Xy {
    fn from(point: Point) -> Xy {
        Xy(point.x.0, point.y.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Fill {
    None,
    /// Filled with the stroke color.
    Outline,
    /// Filled with the symbol body color.
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Shape {
    /// Connected line segments. The path is closed only if its last point repeats the first.
    Polyline {
        points: Vec<Xy>,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    /// A closed polygon.
    Polygon {
        points: Vec<Xy>,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    Rect {
        start: Xy,
        end: Xy,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    Circle {
        center: Xy,
        #[ts(type = "number")]
        radius: i64,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    /// The circular arc from `start` through `mid` to `end`.
    Arc {
        start: Xy,
        mid: Xy,
        end: Xy,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
    /// A cubic Bézier curve given by its four control points.
    Bezier {
        points: Vec<Xy>,
        #[ts(type = "number")]
        width: i64,
        fill: Fill,
    },
}

/// The unit 1 drawing of a symbol in its normal body style, with unit 0 graphics shared by all
/// units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SymbolPreview {
    pub shapes: Vec<Shape>,
    /// Visible pins only.
    pub pins: Vec<Pin>,
    pub pin_numbers_visible: bool,
    pub pin_names_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Pin {
    /// Where a wire connects.
    pub connection: Xy,
    /// Where the pin line meets the symbol body.
    pub body: Xy,
    pub number: String,
    pub name: String,
    /// The KiCad electrical type, such as `input`, `passive`, or `power_in`.
    pub electrical: String,
}

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
