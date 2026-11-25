//! Routed copper as the UI sees it: tracks, vias, and zones, the design rules, and the
//! finished interactive route the UI sends back to be committed.

use serde::{Deserialize, Serialize};
use tron_core::{Board, DesignRules, Nm, Point, RouteRequest, RouteSegment, Track, Via, Xy, Zone};
use ts_rs::TS;
use uuid::Uuid;

/// A straight track on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardTrack {
    pub uuid: Uuid,
    pub start: Xy,
    pub end: Xy,
    #[ts(type = "number")]
    pub width_nm: i64,
    pub layer: String,
    pub net: Option<String>,
}

impl From<&Track> for BoardTrack {
    /// The IPC form of a track.
    fn from(track: &Track) -> BoardTrack {
        BoardTrack {
            uuid: track.uuid,
            start: Xy::from(track.start),
            end: Xy::from(track.end),
            width_nm: track.width.0,
            layer: track.layer.clone(),
            net: track.net.clone(),
        }
    }
}

/// A via on the board, joining the copper layers from its first to its last layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardVia {
    pub uuid: Uuid,
    pub position: Xy,
    #[ts(type = "number")]
    pub diameter_nm: i64,
    #[ts(type = "number")]
    pub drill_nm: i64,
    pub layers: Vec<String>,
    pub net: Option<String>,
}

impl From<&Via> for BoardVia {
    /// The IPC form of a via.
    fn from(via: &Via) -> BoardVia {
        BoardVia {
            uuid: via.uuid,
            position: Xy::from(via.position),
            diameter_nm: via.size.0,
            drill_nm: via.drill.0,
            layers: vec![via.layers.0.clone(), via.layers.1.clone()],
            net: via.net.clone(),
        }
    }
}

/// One filled polygon of a zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardZoneFill {
    pub layer: String,
    pub points: Vec<Xy>,
}

/// A copper zone or rule area on the board, with its fill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardZone {
    pub uuid: Uuid,
    pub net: Option<String>,
    pub name: Option<String>,
    pub layers: Vec<String>,
    pub outline: Vec<Xy>,
    pub priority: u32,
    pub keepout: bool,
    pub fills: Vec<BoardZoneFill>,
}

impl From<&Zone> for BoardZone {
    /// The IPC form of a zone.
    fn from(zone: &Zone) -> BoardZone {
        let fills = zone
            .fills
            .iter()
            .map(|fill| BoardZoneFill {
                layer: fill.layer.clone(),
                points: fill.points.iter().copied().map(Xy::from).collect(),
            })
            .collect();
        BoardZone {
            uuid: zone.uuid,
            net: zone.net.clone(),
            name: zone.name.clone(),
            layers: zone.layers.clone(),
            outline: zone.outline.iter().copied().map(Xy::from).collect(),
            priority: zone.priority,
            keepout: zone.keepout,
            fills,
        }
    }
}

/// The board design rules, in nanometers: the constraints DRC checks and the sizes new
/// tracks and vias get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardDesignRules {
    #[ts(type = "number")]
    pub clearance_nm: i64,
    #[ts(type = "number")]
    pub min_track_width_nm: i64,
    #[ts(type = "number")]
    pub min_via_diameter_nm: i64,
    #[ts(type = "number")]
    pub min_via_drill_nm: i64,
    #[ts(type = "number")]
    pub min_annular_ring_nm: i64,
    #[ts(type = "number")]
    pub edge_clearance_nm: i64,
    #[ts(type = "number")]
    pub track_width_nm: i64,
    #[ts(type = "number")]
    pub via_diameter_nm: i64,
    #[ts(type = "number")]
    pub via_drill_nm: i64,
}

impl From<DesignRules> for BoardDesignRules {
    /// The IPC form of the design rules.
    fn from(rules: DesignRules) -> BoardDesignRules {
        BoardDesignRules {
            clearance_nm: rules.clearance.0,
            min_track_width_nm: rules.min_track_width.0,
            min_via_diameter_nm: rules.min_via_diameter.0,
            min_via_drill_nm: rules.min_via_drill.0,
            min_annular_ring_nm: rules.min_annular_ring.0,
            edge_clearance_nm: rules.edge_clearance.0,
            track_width_nm: rules.track_width.0,
            via_diameter_nm: rules.via_diameter.0,
            via_drill_nm: rules.via_drill.0,
        }
    }
}

impl From<BoardDesignRules> for DesignRules {
    /// The core form of the rules the UI sent.
    fn from(rules: BoardDesignRules) -> DesignRules {
        DesignRules {
            clearance: Nm(rules.clearance_nm),
            min_track_width: Nm(rules.min_track_width_nm),
            min_via_diameter: Nm(rules.min_via_diameter_nm),
            min_via_drill: Nm(rules.min_via_drill_nm),
            min_annular_ring: Nm(rules.min_annular_ring_nm),
            edge_clearance: Nm(rules.edge_clearance_nm),
            track_width: Nm(rules.track_width_nm),
            via_diameter: Nm(rules.via_diameter_nm),
            via_drill: Nm(rules.via_drill_nm),
        }
    }
}

/// One straight piece of a finished route.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoutePiece {
    pub start: Xy,
    pub end: Xy,
    pub layer: String,
}

/// A finished interactive route: its connected pieces in drawing order, and a via at each
/// point where it changes layer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrackRoute {
    pub net: Option<String>,
    #[ts(type = "number")]
    pub width_nm: i64,
    #[ts(type = "number")]
    pub via_diameter_nm: i64,
    #[ts(type = "number")]
    pub via_drill_nm: i64,
    pub pieces: Vec<RoutePiece>,
    pub vias: Vec<Xy>,
}

impl From<TrackRoute> for RouteRequest {
    /// The core form of the route the UI sent.
    fn from(route: TrackRoute) -> RouteRequest {
        let segments = route
            .pieces
            .into_iter()
            .map(|piece| RouteSegment {
                start: point(piece.start),
                end: point(piece.end),
                layer: piece.layer,
            })
            .collect();
        RouteRequest {
            net: route.net,
            width: Nm(route.width_nm),
            via_diameter: Nm(route.via_diameter_nm),
            via_drill: Nm(route.via_drill_nm),
            segments,
            vias: route.vias.into_iter().map(point).collect(),
        }
    }
}

/// A point the UI sent.
fn point(Xy(x, y): Xy) -> Point {
    Point::new(Nm(x), Nm(y))
}

/// Every track on the board, in file order.
pub(crate) fn board_tracks(board: &Board) -> Vec<BoardTrack> {
    board.tracks().iter().map(BoardTrack::from).collect()
}

/// Every via on the board, in file order.
pub(crate) fn board_vias(board: &Board) -> Vec<BoardVia> {
    board.vias().iter().map(BoardVia::from).collect()
}

/// Every zone on the board, in file order.
pub(crate) fn board_zones(board: &Board) -> Vec<BoardZone> {
    board.zones().iter().map(BoardZone::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_serialize_and_deserialize_in_nanometers() {
        let board = Board::parse(
            "(kicad_pcb (version 20260206) \
             (segment (start 0 0) (end 1 1) (width 0.25) (layer \"B.Cu\") (net \"A\") \
             (uuid \"00000000-0000-4000-8000-000000000001\")) \
             (via (at 1 1) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") (net \"A\") \
             (uuid \"00000000-0000-4000-8000-000000000002\")))",
        )
        .unwrap();
        let tracks = serde_json::to_value(board_tracks(&board)).unwrap();
        let end = serde_json::json!([1_000_000, 1_000_000]);
        assert_eq!(tracks[0]["end"], end);
        assert_eq!(tracks[0]["widthNm"], 250_000);
        let vias = serde_json::to_value(board_vias(&board)).unwrap();
        assert_eq!(vias[0]["diameterNm"], 600_000);
        assert_eq!(vias[0]["layers"], serde_json::json!(["F.Cu", "B.Cu"]));

        let payload = serde_json::json!({
            "net": "A",
            "widthNm": 200_000,
            "viaDiameterNm": 600_000,
            "viaDrillNm": 300_000,
            "pieces": [{"start": [0, 0], "end": [0, 1_000_000], "layer": "F.Cu"}],
            "vias": [[0, 1_000_000]]
        });
        let route: TrackRoute = serde_json::from_value(payload).unwrap();
        let request = RouteRequest::from(route);
        assert_eq!(request.segments[0].end, Point::new(Nm(0), Nm(1_000_000)));
        assert_eq!(request.vias.len(), 1);
        let rules = BoardDesignRules::from(DesignRules::default());
        assert_eq!(DesignRules::from(rules), DesignRules::default());
    }
}
