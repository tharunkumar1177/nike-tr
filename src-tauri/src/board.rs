//! The board as the UI sees it: footprints, the outline, the stackup, and the ratsnest, plus
//! the Update Board from Schematic review the UI shows before the update is applied.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tron_core::sexpr::SExpr;
use tron_core::{
    Board, BoardGraphic, BoardUpdateIssue, BoardUpdateOptions, BoardUpdatePlan, FieldChange,
    Footprint, FootprintField, FootprintReplacement, Nm, OutlineShape, Pad, PadNetChange,
    PlannedFootprint, Point, RatsnestLine, Side, StackupLayer, Xy,
};
use tron_library::kicad_mod::placed_geometry;
use tron_library::preview::{Pad as LibraryPad, PadShape};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::AppError;
use crate::library::{LibraryState, read_index};

/// The side of the board a footprint is mounted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BoardSide {
    Front,
    Back,
}

impl From<Side> for BoardSide {
    /// The IPC name of a core side.
    fn from(side: Side) -> BoardSide {
        match side {
            Side::Front => BoardSide::Front,
            Side::Back => BoardSide::Back,
        }
    }
}

/// A pad of a board footprint, at its position on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardPad {
    pub number: String,
    /// Board nanometers, after the footprint's rotation and side are applied.
    pub position: Xy,
    pub net: Option<String>,
    pub shape: PadShape,
    /// Width and height before rotation; zero when the pad's shape doesn't read.
    pub size: Xy,
    /// Tenths of a degree, counterclockwise, the footprint's rotation included.
    pub rotation: i32,
    /// Zero unless the shape is a rounded rectangle.
    #[ts(type = "number")]
    pub corner_radius_nm: i64,
    /// Drill width and height for a pad with a hole.
    pub drill: Option<Xy>,
}

/// The smallest axis-aligned box around a footprint on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardBounds {
    pub min: Xy,
    pub max: Xy,
}

/// A footprint placed on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardFootprint {
    pub uuid: Uuid,
    /// `<library>:<footprint>`.
    pub lib_id: String,
    pub position: Xy,
    /// Tenths of a degree, counterclockwise, in `[0, 3600)`.
    pub rotation: i32,
    pub side: BoardSide,
    pub reference: String,
    pub value: String,
    pub pads: Vec<BoardPad>,
    /// Around the courtyard, or the pads and graphics without one; just the origin when the
    /// footprint has neither.
    pub bounds: BoardBounds,
}

impl BoardFootprint {
    /// The IPC form of `footprint`, read from the board file node `node`.
    fn new(footprint: &Footprint, node: &SExpr) -> BoardFootprint {
        let geometry = placed_geometry(node);
        let pads = footprint
            .pads
            .iter()
            .map(|pad| board_pad(footprint, pad, &geometry.pads))
            .collect();
        BoardFootprint {
            uuid: footprint.uuid,
            lib_id: footprint.lib_id.clone(),
            position: Xy::from(footprint.position),
            rotation: footprint.rotation.decidegrees(),
            side: BoardSide::from(footprint.side),
            reference: footprint.reference.clone(),
            value: footprint.value.clone(),
            pads,
            bounds: board_bounds(footprint, geometry.extent),
        }
    }
}

/// The IPC form of `pad`, with the shape of the read pad at the same number and offset.
fn board_pad(footprint: &Footprint, pad: &Pad, shapes: &[LibraryPad]) -> BoardPad {
    let offset = Xy::from(pad.position);
    let shape = shapes
        .iter()
        .find(|shape| shape.number == pad.number && shape.center == offset);
    BoardPad {
        number: pad.number.clone(),
        position: Xy::from(footprint.pad_position(pad)),
        net: pad.net.clone(),
        shape: shape.map_or(PadShape::Rect, |shape| shape.shape),
        size: shape.map_or(Xy(0, 0), |shape| shape.size),
        rotation: pad.rotation.decidegrees(),
        corner_radius_nm: shape.map_or(0, |shape| shape.corner_radius),
        drill: shape.and_then(|shape| shape.drill),
    }
}

/// The board box around the footprint-frame `extent` turned and moved with the footprint.
fn board_bounds(footprint: &Footprint, extent: Option<(Xy, Xy)>) -> BoardBounds {
    let Some((min, max)) = extent else {
        let origin = Xy::from(footprint.position);
        return BoardBounds {
            min: origin,
            max: origin,
        };
    };
    let corners = [min, Xy(max.0, min.1), max, Xy(min.0, max.1)];
    let point = |Xy(x, y): Xy| footprint.board_point(Point::new(Nm(x), Nm(y)));
    let points = corners.map(point);
    let xs = points.map(|point| point.x.0);
    let ys = points.map(|point| point.y.0);
    BoardBounds {
        min: Xy(min_of(xs), min_of(ys)),
        max: Xy(max_of(xs), max_of(ys)),
    }
}

/// The smallest of four values.
fn min_of(values: [i64; 4]) -> i64 {
    values.into_iter().fold(i64::MAX, i64::min)
}

/// The largest of four values.
fn max_of(values: [i64; 4]) -> i64 {
    values.into_iter().fold(i64::MIN, i64::max)
}

/// The geometry of one `Edge.Cuts` graphic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum OutlineGeometry {
    Line {
        start: Xy,
        end: Xy,
    },
    /// A circular arc through three points.
    Arc {
        start: Xy,
        mid: Xy,
        end: Xy,
    },
    /// An axis-aligned rectangle between opposite corners.
    Rect {
        start: Xy,
        end: Xy,
    },
    /// A circle around `center` through `end`.
    Circle {
        center: Xy,
        end: Xy,
    },
    /// A closed polygon.
    Polygon {
        points: Vec<Xy>,
    },
}

/// One graphic of the board outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutlineGraphic {
    pub uuid: Option<Uuid>,
    pub geometry: OutlineGeometry,
    #[ts(type = "number")]
    pub width_nm: i64,
}

impl From<&BoardGraphic> for OutlineGraphic {
    /// The IPC form of an `Edge.Cuts` graphic.
    fn from(graphic: &BoardGraphic) -> OutlineGraphic {
        let geometry = match &graphic.shape {
            OutlineShape::Line { start, end } => OutlineGeometry::Line {
                start: Xy::from(*start),
                end: Xy::from(*end),
            },
            OutlineShape::Arc { start, mid, end } => OutlineGeometry::Arc {
                start: Xy::from(*start),
                mid: Xy::from(*mid),
                end: Xy::from(*end),
            },
            OutlineShape::Rect { start, end } => OutlineGeometry::Rect {
                start: Xy::from(*start),
                end: Xy::from(*end),
            },
            OutlineShape::Circle { center, end } => OutlineGeometry::Circle {
                center: Xy::from(*center),
                end: Xy::from(*end),
            },
            OutlineShape::Polygon { points } => OutlineGeometry::Polygon {
                points: points.iter().copied().map(Xy::from).collect(),
            },
        };
        OutlineGraphic {
            uuid: graphic.uuid,
            geometry,
            width_nm: graphic.width.0,
        }
    }
}

/// One layer of the physical stackup, from the top.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StackupEntry {
    /// `F.Cu`, `dielectric 1`, and so on.
    pub name: String,
    /// KiCad's layer type, such as `copper`, `core`, `prepreg`, or `Top Solder Mask`.
    pub kind: String,
    #[ts(type = "number | null")]
    pub thickness_nm: Option<i64>,
    pub material: Option<String>,
    pub epsilon_r: Option<f64>,
    pub loss_tangent: Option<f64>,
}

impl From<&StackupLayer> for StackupEntry {
    /// The IPC form of a stackup layer.
    fn from(layer: &StackupLayer) -> StackupEntry {
        StackupEntry {
            name: layer.name.clone(),
            kind: layer.kind.clone(),
            thickness_nm: layer.thickness.map(|thickness| thickness.0),
            material: layer.material.clone(),
            epsilon_r: layer.epsilon_r,
            loss_tangent: layer.loss_tangent,
        }
    }
}

/// The board's copper layer count and physical stackup.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardStackup {
    pub copper_layers: u32,
    /// The total thickness, or 0 when the board has no stackup.
    #[ts(type = "number")]
    pub thickness_nm: i64,
    /// Empty when the board has no `setup.stackup`.
    pub layers: Vec<StackupEntry>,
    pub copper_finish: Option<String>,
}

/// One ratsnest line: an unrouted connection of a net.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RatsnestSegment {
    pub net: String,
    pub from: Xy,
    pub to: Xy,
}

impl From<RatsnestLine> for RatsnestSegment {
    /// The IPC form of a ratsnest line.
    fn from(line: RatsnestLine) -> RatsnestSegment {
        RatsnestSegment {
            net: line.net,
            from: Xy::from(line.from),
            to: Xy::from(line.to),
        }
    }
}

/// The ratsnest of the nets that changed. The UI drops its lines for every net in `nets`
/// and draws `lines` in their place; a net with no lines left has no unrouted copper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RatsnestUpdate {
    pub nets: Vec<String>,
    pub lines: Vec<RatsnestSegment>,
}

/// Every footprint on the board, in file order.
pub(crate) fn board_footprints(board: &Board) -> Vec<BoardFootprint> {
    let footprints = board.footprint_nodes();
    footprints
        .iter()
        .map(|(footprint, node)| BoardFootprint::new(footprint, node))
        .collect()
}

/// Every graphic of the board outline, in file order.
pub(crate) fn board_outline(board: &Board) -> Vec<OutlineGraphic> {
    let graphics = board.edge_cuts();
    graphics.iter().map(OutlineGraphic::from).collect()
}

/// The board's copper layer count and stackup.
pub(crate) fn board_stackup(board: &Board) -> BoardStackup {
    let copper_layers = u32::try_from(board.copper_layer_count()).unwrap_or(u32::MAX);
    let Some(stackup) = board.stackup() else {
        return BoardStackup {
            copper_layers,
            thickness_nm: 0,
            layers: Vec::new(),
            copper_finish: None,
        };
    };
    BoardStackup {
        copper_layers,
        thickness_nm: stackup.thickness().0,
        layers: stackup.layers.iter().map(StackupEntry::from).collect(),
        copper_finish: stackup.copper_finish,
    }
}

/// Ratsnest lines in their IPC form.
pub(crate) fn ratsnest_segments(lines: Vec<RatsnestLine>) -> Vec<RatsnestSegment> {
    lines.into_iter().map(RatsnestSegment::from).collect()
}

/// The choices of the Update Board from Schematic dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardUpdateSettings {
    /// Delete footprints with no symbols.
    pub delete_extra: bool,
    /// Replace footprints with those specified by symbols.
    pub replace_footprints: bool,
}

impl From<BoardUpdateSettings> for BoardUpdateOptions {
    /// The core options for the dialog's choices.
    fn from(settings: BoardUpdateSettings) -> BoardUpdateOptions {
        BoardUpdateOptions {
            delete_extra: settings.delete_extra,
            replace_footprints: settings.replace_footprints,
        }
    }
}

/// A footprint the update adds or removes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdatedFootprint {
    /// `null` for a footprint the update adds.
    pub uuid: Option<Uuid>,
    pub reference: String,
    pub value: String,
    pub lib_id: String,
}

impl From<PlannedFootprint> for UpdatedFootprint {
    /// The IPC form of a planned addition or removal.
    fn from(planned: PlannedFootprint) -> UpdatedFootprint {
        UpdatedFootprint {
            uuid: planned.uuid,
            reference: planned.reference,
            value: planned.value,
            lib_id: planned.lib_id,
        }
    }
}

/// A footprint swapped for the one its symbol now names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReplacedFootprint {
    pub uuid: Uuid,
    pub reference: String,
    pub from: String,
    pub to: String,
}

impl From<FootprintReplacement> for ReplacedFootprint {
    /// The IPC form of a planned replacement.
    fn from(replacement: FootprintReplacement) -> ReplacedFootprint {
        ReplacedFootprint {
            uuid: replacement.uuid,
            reference: replacement.reference,
            from: replacement.from,
            to: replacement.to,
        }
    }
}

/// The footprint text an update changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FootprintText {
    Reference,
    Value,
}

/// A footprint's Reference or Value changing to match its symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FootprintTextChange {
    pub uuid: Uuid,
    /// The reference the footprint has after the update.
    pub reference: String,
    pub field: FootprintText,
    pub from: String,
    pub to: String,
}

impl From<FieldChange> for FootprintTextChange {
    /// The IPC form of a planned Reference or Value change.
    fn from(change: FieldChange) -> FootprintTextChange {
        FootprintTextChange {
            uuid: change.uuid,
            reference: change.reference,
            field: match change.field {
                FootprintField::Reference => FootprintText::Reference,
                FootprintField::Value => FootprintText::Value,
            },
            from: change.from,
            to: change.to,
        }
    }
}

/// A pad joining, leaving, or switching nets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PadNetUpdate {
    pub uuid: Uuid,
    pub reference: String,
    pub pad: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl From<PadNetChange> for PadNetUpdate {
    /// The IPC form of a planned pad net change.
    fn from(change: PadNetChange) -> PadNetUpdate {
        PadNetUpdate {
            uuid: change.uuid,
            reference: change.reference,
            pad: change.pad,
            from: change.from,
            to: change.to,
        }
    }
}

/// A symbol the update can't carry to the board, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardUpdateProblem {
    pub reference: String,
    pub message: String,
}

impl From<BoardUpdateIssue> for BoardUpdateProblem {
    /// The IPC form of a planned update issue.
    fn from(issue: BoardUpdateIssue) -> BoardUpdateProblem {
        BoardUpdateProblem {
            reference: issue.reference,
            message: issue.message,
        }
    }
}

/// Everything an update would change, for the user to review. Applying it needs the same
/// `revision`, so a review the document has moved past can't be applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardUpdateReview {
    pub revision: u32,
    /// Whether applying the review changes the board.
    pub has_changes: bool,
    pub additions: Vec<UpdatedFootprint>,
    pub removals: Vec<UpdatedFootprint>,
    pub replacements: Vec<ReplacedFootprint>,
    pub text_changes: Vec<FootprintTextChange>,
    pub net_changes: Vec<PadNetUpdate>,
    pub problems: Vec<BoardUpdateProblem>,
}

impl BoardUpdateReview {
    /// The review of `plan`, made at `revision`.
    pub(crate) fn new(plan: BoardUpdatePlan, revision: u32) -> BoardUpdateReview {
        /// Converts every item of a plan list.
        fn all<T, U: From<T>>(items: Vec<T>) -> Vec<U> {
            items.into_iter().map(U::from).collect()
        }
        BoardUpdateReview {
            revision,
            has_changes: plan.has_changes(),
            additions: all(plan.additions),
            removals: all(plan.removals),
            replacements: all(plan.replacements),
            text_changes: all(plan.field_changes),
            net_changes: all(plan.net_changes),
            problems: all(plan.issues),
        }
    }
}

/// Looks up each `<library>:<footprint>` in `required` in the component library. IDs the
/// library lacks are left out, and the plan reports them. With nothing required the library
/// isn't read, so footprints can still be removed before it is downloaded.
pub(crate) fn footprint_definitions(
    app: &AppHandle,
    library: &LibraryState,
    required: &BTreeSet<String>,
) -> Result<HashMap<String, SExpr>, AppError> {
    if required.is_empty() {
        return Ok(HashMap::new());
    }
    read_index(app, library, |index| {
        let mut definitions = HashMap::new();
        for lib_id in required {
            let Some((lib, name)) = lib_id.split_once(':') else {
                continue;
            };
            if let Some(definition) = index.footprint_definition_named(lib, name)? {
                definitions.insert(lib_id.clone(), definition);
            }
        }
        Ok(definitions)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A back-side `R_0603_1608Metric` turned a quarter, as a board file holds it.
    const BACK_RESISTOR: &str = "(kicad_pcb (version 20260206) \
        (footprint \"Resistor_SMD:R_0603_1608Metric\" (layer \"B.Cu\") \
        (uuid \"00000000-0000-0000-0000-000000000000\") (at 100 50 90) \
        (property \"Reference\" \"R1\" (at 0 1.43 90) (layer \"B.SilkS\")) \
        (property \"Value\" \"10k\" (at 0 -1.43 90) (layer \"B.Fab\")) \
        (fp_rect (start -1.48 -0.73) (end 1.48 0.73) (stroke (width 0.05)) (layer \"B.CrtYd\")) \
        (pad \"1\" smd roundrect (at -0.825 0 90) (size 0.8 0.95) (layers \"B.Cu\") \
        (roundrect_rratio 0.25) (net \"GND\")) \
        (pad \"2\" thru_hole circle (at 0.825 0 90) (size 1.6 1.6) (drill 0.8) \
        (layers \"*.Cu\"))))";

    #[test]
    fn footprints_serialize_with_board_pad_geometry_and_bounds() {
        let board = Board::parse(BACK_RESISTOR).unwrap();
        let footprint = board.footprint(Uuid::nil()).unwrap();
        let json = serde_json::to_value(board_footprints(&board)).unwrap();
        let json = &json[0];
        assert_eq!(json["libId"], "Resistor_SMD:R_0603_1608Metric");
        assert_eq!(
            json["position"],
            serde_json::json!([100_000_000, 50_000_000])
        );
        assert_eq!(json["rotation"], 900);
        assert_eq!(json["side"], "back");
        let (min, max) = (
            serde_json::json!([99_270_000, 48_520_000]),
            serde_json::json!([100_730_000, 51_480_000]),
        );
        assert_eq!(json["bounds"]["min"], min);
        assert_eq!(json["bounds"]["max"], max);
        let pad = &json["pads"][0];
        assert_eq!(pad["net"], "GND");
        let expected = Xy::from(footprint.pad_position(&footprint.pads[0]));
        assert_eq!(pad["position"], serde_json::to_value(expected).unwrap());
        assert_eq!(pad["shape"], "round_rect");
        assert_eq!(pad["size"], serde_json::json!([800_000, 950_000]));
        assert_eq!(pad["rotation"], 900);
        assert_eq!(pad["cornerRadiusNm"], 200_000);
        assert_eq!(pad["drill"], serde_json::Value::Null);
        let hole = &json["pads"][1];
        assert_eq!(hole["shape"], "circle");
        assert_eq!(hole["drill"], serde_json::json!([800_000, 800_000]));
    }

    #[test]
    fn footprints_without_geometry_are_bounded_by_their_origin() {
        let board = Board::parse(
            "(kicad_pcb (version 20260206) (footprint \"X:Y\" (layer \"F.Cu\") \
            (uuid \"00000000-0000-0000-0000-000000000000\") (at 3 4)))",
        )
        .unwrap();
        let json = serde_json::to_value(board_footprints(&board)).unwrap();
        let origin = serde_json::json!([3_000_000, 4_000_000]);
        assert_eq!(json[0]["bounds"]["min"], origin);
        assert_eq!(json[0]["bounds"]["max"], origin);
    }

    #[test]
    fn a_new_board_has_two_copper_layers_and_no_outline() {
        let board = Board::new_empty();
        let stackup = serde_json::to_value(board_stackup(&board)).unwrap();
        assert_eq!(stackup["copperLayers"], 2);
        assert_eq!(stackup["thicknessNm"], 1_600_000);
        assert_eq!(stackup["layers"][2]["name"], "F.Mask");
        assert!(board_outline(&board).is_empty());
        assert!(board_footprints(&board).is_empty());
    }

    #[test]
    fn outline_graphics_are_tagged_by_type() {
        let board = Board::parse(
            "(kicad_pcb (version 20260206) \
            (gr_rect (start 100 60) (end 150 100) (stroke (width 0.05)) (layer \"Edge.Cuts\")))",
        )
        .unwrap();
        let json = serde_json::to_value(board_outline(&board)).unwrap();
        assert_eq!(json[0]["geometry"]["type"], "rect");
        let start = serde_json::json!([100_000_000, 60_000_000]);
        assert_eq!(json[0]["geometry"]["start"], start);
        assert_eq!(json[0]["widthNm"], 50_000);
    }

    #[test]
    fn a_review_carries_every_change_and_the_revision() {
        let plan = BoardUpdatePlan {
            field_changes: vec![FieldChange {
                uuid: Uuid::nil(),
                reference: "R2".to_owned(),
                field: FootprintField::Value,
                from: "10k".to_owned(),
                to: "22k".to_owned(),
            }],
            issues: vec![BoardUpdateIssue {
                reference: "R3".to_owned(),
                message: "missing".to_owned(),
            }],
            ..BoardUpdatePlan::default()
        };
        let json = serde_json::to_value(BoardUpdateReview::new(plan, 4)).unwrap();
        assert_eq!(json["revision"], 4);
        assert_eq!(json["hasChanges"], true);
        assert_eq!(json["textChanges"][0]["field"], "value");
        assert_eq!(json["problems"][0]["reference"], "R3");
        assert_eq!(json["additions"], serde_json::json!([]));
        let payload = serde_json::json!({ "deleteExtra": false, "replaceFootprints": true });
        let settings: BoardUpdateSettings = serde_json::from_value(payload).unwrap();
        assert!(!BoardUpdateOptions::from(settings).delete_extra);
    }
}
