//! The board outline as one simple closed polygon.
//!
//! Tron edits the `Edge.Cuts` outline as a single `gr_poly`. A polygon is accepted when it
//! has at least three distinct corners, encloses an area, stays inside KiCad's 32-bit
//! nanometer range, and no edge crosses or touches an edge it doesn't share a corner with.
//! All geometry uses exact integer arithmetic.

use uuid::Uuid;

use crate::board::MAX_COORDINATE;
use crate::kicad::mm_text;
use crate::sexpr::SExpr;
use crate::units::{Nm, Point};
use crate::wiring::{uuid_node, xy_node};

/// The most corners an edited outline can have.
pub const MAX_OUTLINE_POINTS: usize = 1024;

/// KiCad's default line width on `Edge.Cuts`.
pub const EDGE_CUTS_WIDTH: Nm = Nm(50_000);

/// The corners of the polygon through `points`, without a closing point that repeats the
/// first or consecutive duplicates. Fails with a message for the user when the polygon is
/// not one simple closed shape.
pub fn validate_polygon(points: &[Point]) -> Result<Vec<Point>, String> {
    let mut corners: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        if corners.last() != Some(point) {
            corners.push(*point);
        }
    }
    while corners.len() > 1 && corners.first() == corners.last() {
        corners.pop();
    }
    if corners.len() < 3 {
        return Err("The board outline needs at least three corners.".to_owned());
    }
    if corners.len() > MAX_OUTLINE_POINTS {
        return Err(format!(
            "The board outline can have at most {MAX_OUTLINE_POINTS} points."
        ));
    }
    if let Some(point) = corners.iter().find(|point| !fits(**point)) {
        return Err(format!(
            "The board outline at ({}, {}) mm is outside the ±{} mm area KiCad boards can hold",
            mm_text(point.x),
            mm_text(point.y),
            mm_text(MAX_COORDINATE)
        ));
    }
    if twice_area(&corners) == 0 {
        return Err("The board outline must enclose an area.".to_owned());
    }
    if self_intersects(&corners) {
        return Err("The board outline must not cross itself.".to_owned());
    }
    Ok(corners)
}

/// The `Edge.Cuts` polygon through `corners`, as KiCad 10 writes it.
pub fn outline_node(corners: &[Point], uuid: Uuid) -> SExpr {
    let width = SExpr::atom(mm_text(EDGE_CUTS_WIDTH));
    SExpr::node(
        "gr_poly",
        vec![
            SExpr::node("pts", corners.iter().copied().map(xy_node).collect()),
            SExpr::node(
                "stroke",
                vec![
                    SExpr::node("width", vec![width]),
                    SExpr::node("type", vec![SExpr::atom("default")]),
                ],
            ),
            SExpr::node("fill", vec![SExpr::atom("no")]),
            SExpr::node("layer", vec![SExpr::string("Edge.Cuts")]),
            uuid_node(uuid),
        ],
    )
}

/// Whether both coordinates of `point` fit KiCad's 32-bit nanometer range.
fn fits(point: Point) -> bool {
    let limit = MAX_COORDINATE.0.unsigned_abs();
    point.x.0.unsigned_abs() <= limit && point.y.0.unsigned_abs() <= limit
}

/// Twice the signed area of the polygon through `corners`.
fn twice_area(corners: &[Point]) -> i128 {
    let mut sum = 0i128;
    for (index, a) in corners.iter().enumerate() {
        let b = corners[(index + 1) % corners.len()];
        sum += i128::from(a.x.0) * i128::from(b.y.0) - i128::from(b.x.0) * i128::from(a.y.0);
    }
    sum
}

/// The cross product of `a - o` and `b - o`: positive when `o`, `a`, `b` turn
/// counterclockwise in a Y-up frame, zero when they are collinear.
fn cross(o: Point, a: Point, b: Point) -> i128 {
    let (ox, oy) = (i128::from(o.x.0), i128::from(o.y.0));
    let (ax, ay) = (i128::from(a.x.0) - ox, i128::from(a.y.0) - oy);
    let (bx, by) = (i128::from(b.x.0) - ox, i128::from(b.y.0) - oy);
    ax * by - ay * bx
}

/// The dot product of `a - o` and `b - o`.
fn dot(o: Point, a: Point, b: Point) -> i128 {
    let (ox, oy) = (i128::from(o.x.0), i128::from(o.y.0));
    let (ax, ay) = (i128::from(a.x.0) - ox, i128::from(a.y.0) - oy);
    let (bx, by) = (i128::from(b.x.0) - ox, i128::from(b.y.0) - oy);
    ax * bx + ay * by
}

/// Whether `p`, already known to be collinear with `a` and `b`, lies on the segment `ab`.
fn within(a: Point, b: Point, p: Point) -> bool {
    a.x.min(b.x) <= p.x && p.x <= a.x.max(b.x) && a.y.min(b.y) <= p.y && p.y <= a.y.max(b.y)
}

/// Whether the segments `ab` and `cd` share any point.
fn segments_touch(a: Point, b: Point, c: Point, d: Point) -> bool {
    let d1 = cross(c, d, a).signum();
    let d2 = cross(c, d, b).signum();
    let d3 = cross(a, b, c).signum();
    let d4 = cross(a, b, d).signum();
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    (d1 == 0 && within(c, d, a))
        || (d2 == 0 && within(c, d, b))
        || (d3 == 0 && within(a, b, c))
        || (d4 == 0 && within(a, b, d))
}

/// Whether any two edges of the polygon through `corners` meet anywhere other than the
/// corner they share. Adjacent edges overlap when the second folds back along the first.
fn self_intersects(corners: &[Point]) -> bool {
    let count = corners.len();
    let edge = |index: usize| (corners[index], corners[(index + 1) % count]);
    for i in 0..count {
        let (a, b) = edge(i);
        let (_, next) = edge((i + 1) % count);
        if cross(b, a, next) == 0 && dot(b, a, next) > 0 {
            return true;
        }
        for j in (i + 2)..count {
            if i == 0 && j == count - 1 {
                continue;
            }
            let (c, d) = edge(j);
            if segments_touch(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point at whole millimeters.
    fn mm(x: i64, y: i64) -> Point {
        Point::new(Nm(x * 1_000_000), Nm(y * 1_000_000))
    }

    fn square() -> Vec<Point> {
        vec![mm(0, 0), mm(10, 0), mm(10, 10), mm(0, 10)]
    }

    #[test]
    fn a_closing_point_and_repeated_corners_are_dropped() {
        let points = [
            mm(0, 0),
            mm(10, 0),
            mm(10, 0),
            mm(10, 10),
            mm(0, 10),
            mm(0, 0),
        ];
        assert_eq!(validate_polygon(&points), Ok(square()));
    }

    #[test]
    fn either_winding_is_accepted() {
        let mut points = square();
        points.reverse();
        assert_eq!(validate_polygon(&points).map(|c| c.len()), Ok(4));
    }

    #[test]
    fn polygons_need_three_corners_and_an_area() {
        let two = [mm(0, 0), mm(5, 0), mm(0, 0)];
        let error = validate_polygon(&two).unwrap_err();
        assert_eq!(error, "The board outline needs at least three corners.");
        let flat = [mm(0, 0), mm(5, 0), mm(10, 0)];
        let error = validate_polygon(&flat).unwrap_err();
        assert_eq!(error, "The board outline must enclose an area.");
    }

    #[test]
    fn too_many_corners_are_refused() {
        let points: Vec<Point> = (0..=MAX_OUTLINE_POINTS as i64)
            .map(|i| mm(i, i * i))
            .collect();
        let error = validate_polygon(&points).unwrap_err();
        assert_eq!(error, "The board outline can have at most 1024 points.");
    }

    #[test]
    fn crossing_and_touching_edges_are_refused() {
        let crossing = "The board outline must not cross itself.";
        let bowtie = [mm(0, 0), mm(10, 10), mm(10, 0), mm(0, 10)];
        assert_eq!(validate_polygon(&bowtie).unwrap_err(), crossing);
        let folded = [mm(0, 0), mm(10, 0), mm(10, 10), mm(5, 0)];
        assert_eq!(validate_polygon(&folded).unwrap_err(), crossing);
        let pinched = [
            mm(0, 0),
            mm(10, 0),
            mm(5, 5),
            mm(10, 10),
            mm(0, 10),
            mm(5, 5),
        ];
        assert_eq!(validate_polygon(&pinched).unwrap_err(), crossing);
        let on_edge = [mm(0, 0), mm(10, 0), mm(10, 10), mm(5, 10), mm(5, 0)];
        assert_eq!(validate_polygon(&on_edge).unwrap_err(), crossing);
    }

    #[test]
    fn concave_polygons_are_accepted() {
        let notch = [mm(0, 0), mm(10, 0), mm(10, 10), mm(5, 5), mm(0, 10)];
        assert_eq!(validate_polygon(&notch).map(|c| c.len()), Ok(5));
    }

    #[test]
    fn coordinates_must_fit_kicads_range() {
        let edge = MAX_COORDINATE.0;
        let largest = [
            Point::new(Nm(-edge), Nm(-edge)),
            Point::new(Nm(edge), Nm(-edge)),
            Point::new(Nm(edge), Nm(edge)),
        ];
        assert!(validate_polygon(&largest).is_ok());
        let far = [mm(0, 0), Point::new(Nm(i64::MIN), Nm(0)), mm(0, 10)];
        let error = validate_polygon(&far).unwrap_err();
        assert!(error.starts_with("The board outline at ("), "{error}");
    }

    #[test]
    fn the_outline_node_is_a_kicad_10_polygon() {
        let uuid = Uuid::from_u128(7);
        let text = outline_node(&square(), uuid).to_pretty_string();
        assert!(text.starts_with("(gr_poly"), "{text}");
        assert!(text.contains("(xy 10 10)"), "{text}");
        assert!(text.contains("(width 0.05)"), "{text}");
        assert!(text.contains("(fill no)"), "{text}");
        assert!(text.contains("(layer \"Edge.Cuts\")"), "{text}");
        assert!(text.contains(&uuid.to_string()), "{text}");
    }
}
