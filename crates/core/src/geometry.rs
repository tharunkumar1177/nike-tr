//! Copper shapes and the distances between them, shared by connectivity, zone fill, and DRC.
//!
//! Every copper shape is a core (a point, a segment, or a filled polygon) grown by a
//! radius: a via is a point grown by half its diameter, a track is its centerline grown by
//! half its width, an oval pad is a segment, and a rounded rectangle is its rectangle shrunk
//! by the corner radius and grown back. Two shapes are then `core distance - radii` apart.
//! Intersection and containment tests are exact in integers; distances between cores that
//! don't touch are computed in `f64` from integer inputs and are accurate to well under a
//! nanometer across KiCad's coordinate range.

use crate::units::{Angle, Nm, Point};

/// The inner geometry of a [`Shape`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Core {
    Point(Point),
    Segment(Point, Point),
    /// A filled simple polygon, closed implicitly, in either winding.
    Polygon(Vec<Point>),
}

/// A core grown by `radius` in every direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub core: Core,
    pub radius: Nm,
}

/// An axis-aligned box `(min, max)`, inclusive.
pub type Bounds = (Point, Point);

impl Shape {
    /// A disc of `radius` around `center`.
    pub fn disc(center: Point, radius: Nm) -> Shape {
        Shape {
            core: Core::Point(center),
            radius,
        }
    }

    /// A segment from `a` to `b` with round ends, `width` wide.
    pub fn stroke(a: Point, b: Point, width: Nm) -> Shape {
        Shape {
            core: Core::Segment(a, b),
            radius: Nm(width.0 / 2),
        }
    }

    /// A filled polygon with sharp corners.
    pub fn polygon(points: Vec<Point>) -> Shape {
        Shape {
            core: Core::Polygon(points),
            radius: Nm::ZERO,
        }
    }

    /// The gap between the two shapes' edges, or zero when they touch or overlap.
    pub fn distance(&self, other: &Shape) -> f64 {
        let reach = (self.radius.0 + other.radius.0) as f64;
        (core_distance(&self.core, &other.core) - reach).max(0.0)
    }

    /// Whether the shapes touch or overlap.
    pub fn touches(&self, other: &Shape) -> bool {
        let reach = (self.radius.0 + other.radius.0) as f64;
        core_distance(&self.core, &other.core) <= reach
    }

    /// Whether `point` lies in the shape or on its edge.
    pub fn contains(&self, point: Point) -> bool {
        self.touches(&Shape::disc(point, Nm::ZERO))
    }

    /// The smallest box around the shape.
    pub fn bounds(&self) -> Bounds {
        let points = match &self.core {
            Core::Point(point) => vec![*point],
            Core::Segment(a, b) => vec![*a, *b],
            Core::Polygon(points) => points.clone(),
        };
        let r = self.radius;
        let (min, max) = points_bounds(&points).unwrap_or_default();
        (min.offset(-r, -r), max.offset(r, r))
    }
}

/// The smallest box around `points`, or `None` when there are none.
pub fn points_bounds(points: &[Point]) -> Option<Bounds> {
    let first = *points.first()?;
    let mut bounds = (first, first);
    for point in points {
        bounds.0 = Point::new(bounds.0.x.min(point.x), bounds.0.y.min(point.y));
        bounds.1 = Point::new(bounds.1.x.max(point.x), bounds.1.y.max(point.y));
    }
    Some(bounds)
}

/// Whether two boxes overlap or touch.
pub fn bounds_overlap(a: Bounds, b: Bounds) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}

/// The sign of the turn from `a` to `b` to `c`: positive counterclockwise in a Y-up frame,
/// zero when collinear.
pub fn orientation(a: Point, b: Point, c: Point) -> i128 {
    let abx = i128::from(b.x.0 - a.x.0);
    let aby = i128::from(b.y.0 - a.y.0);
    let acx = i128::from(c.x.0 - a.x.0);
    let acy = i128::from(c.y.0 - a.y.0);
    (abx * acy - aby * acx).signum()
}

/// Whether `p`, known to be collinear with `a` and `b`, lies within their box.
fn within(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Whether segments `ab` and `cd` share at least one point.
pub fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    if o1 * o2 < 0 && o3 * o4 < 0 {
        return true;
    }
    (o1 == 0 && within(a, b, c))
        || (o2 == 0 && within(a, b, d))
        || (o3 == 0 && within(c, d, a))
        || (o4 == 0 && within(c, d, b))
}

/// The distance from `p` to the segment `ab`.
pub fn point_segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = (b.x.0 - a.x.0) as f64;
    let dy = (b.y.0 - a.y.0) as f64;
    let px = (p.x.0 - a.x.0) as f64;
    let py = (p.y.0 - a.y.0) as f64;
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        ((px * dx + py * dy) / length).clamp(0.0, 1.0)
    };
    (px - t * dx).hypot(py - t * dy)
}

/// The distance between segments `ab` and `cd`, zero when they cross or touch.
pub fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    if segments_intersect(a, b, c, d) {
        return 0.0;
    }
    point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b))
}

/// Whether `p` lies inside the polygon or on its boundary. Exact.
pub fn point_in_polygon(p: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    for (a, b) in edges(polygon) {
        let side = orientation(a, b, p);
        if side == 0 && within(a, b, p) {
            return true;
        }
        // An edge that crosses the horizontal line through `p` counts when it crosses to
        // the right of `p`, which the turn direction tells without dividing.
        if (a.y > p.y) != (b.y > p.y) && (side > 0) == (b.y > a.y) {
            inside = !inside;
        }
    }
    inside
}

/// The edges of a closed polygon, the closing edge last.
pub fn edges(polygon: &[Point]) -> Vec<(Point, Point)> {
    let mut edges = Vec::with_capacity(polygon.len());
    for (index, &a) in polygon.iter().enumerate() {
        edges.push((a, polygon[(index + 1) % polygon.len()]));
    }
    edges
}

/// The distance between two cores, zero when they overlap.
pub fn core_distance(a: &Core, b: &Core) -> f64 {
    match (a, b) {
        (Core::Point(p), Core::Point(q)) => point_segment_distance(*p, *q, *q),
        (Core::Point(p), Core::Segment(c, d)) | (Core::Segment(c, d), Core::Point(p)) => {
            point_segment_distance(*p, *c, *d)
        }
        (Core::Segment(a, b), Core::Segment(c, d)) => segment_distance(*a, *b, *c, *d),
        (Core::Point(p), Core::Polygon(polygon)) | (Core::Polygon(polygon), Core::Point(p)) => {
            polygon_segment_distance(polygon, *p, *p)
        }
        (Core::Segment(a, b), Core::Polygon(polygon))
        | (Core::Polygon(polygon), Core::Segment(a, b)) => {
            polygon_segment_distance(polygon, *a, *b)
        }
        (Core::Polygon(first), Core::Polygon(second)) => polygon_distance(first, second),
    }
}

/// The distance from the filled `polygon` to the segment `ab`.
fn polygon_segment_distance(polygon: &[Point], a: Point, b: Point) -> f64 {
    if polygon.is_empty() {
        return f64::INFINITY;
    }
    if point_in_polygon(a, polygon) || point_in_polygon(b, polygon) {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for (c, d) in edges(polygon) {
        best = best.min(segment_distance(a, b, c, d));
    }
    best
}

/// The distance between two filled polygons.
fn polygon_distance(first: &[Point], second: &[Point]) -> f64 {
    if first.is_empty() || second.is_empty() {
        return f64::INFINITY;
    }
    if point_in_polygon(second[0], first) {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for (a, b) in edges(first) {
        best = best.min(polygon_segment_distance(second, a, b));
    }
    best
}

/// `point` turned by `decidegrees` about the origin with KiCad's rotation convention,
/// rounded to the nearest nanometer. Quarter turns are exact.
pub fn rotate(point: Point, decidegrees: i32) -> Point {
    let angle = Angle::from_decidegrees(decidegrees);
    let (x, y) = crate::footprint::rotate((point.x.0, point.y.0), angle);
    Point::new(Nm(x), Nm(y))
}

/// The corners of a `width` by `height` rectangle centered on the origin, turned by
/// `decidegrees` and moved to `center`.
pub fn rectangle(center: Point, width: Nm, height: Nm, decidegrees: i32) -> Vec<Point> {
    let (hw, hh) = (width.0 / 2, height.0 / 2);
    let mut corners = Vec::with_capacity(4);
    for (x, y) in [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)] {
        let turned = rotate(Point::new(Nm(x), Nm(y)), decidegrees);
        corners.push(turned.offset(center.x, center.y));
    }
    corners
}

/// A regular polygon with `sides` corners around a circle of `radius`, circumscribed so
/// the circle fits inside it: shapes built from it never fall short of the circle.
pub fn circle_polygon(center: Point, radius: Nm, sides: usize) -> Vec<Point> {
    let sides = sides.max(3);
    let step = std::f64::consts::TAU / sides as f64;
    let outer = radius.0 as f64 / (step / 2.0).cos();
    let mut points = Vec::with_capacity(sides);
    for index in 0..sides {
        let angle = step * index as f64;
        let x = (outer * angle.cos()).round() as i64;
        let y = (outer * angle.sin()).round() as i64;
        points.push(center.offset(Nm(x), Nm(y)));
    }
    points
}

/// The doubled signed area of a polygon: positive counterclockwise in a Y-up frame.
pub fn doubled_area(polygon: &[Point]) -> i128 {
    let mut sum = 0;
    for (a, b) in edges(polygon) {
        let forward = i128::from(a.x.0) * i128::from(b.y.0);
        let backward = i128::from(b.x.0) * i128::from(a.y.0);
        sum += forward - backward;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    fn square(x: i64, y: i64, size: i64) -> Vec<Point> {
        let far = (x + size, y + size);
        vec![at(x, y), at(far.0, y), at(far.0, far.1), at(x, far.1)]
    }

    #[test]
    fn crossing_touching_and_collinear_segments_intersect() {
        let (a, b) = (at(0, 0), at(10, 0));
        assert!(segments_intersect(a, b, at(5, -5), at(5, 5)));
        assert!(segments_intersect(a, b, b, at(10, 5)));
        assert!(segments_intersect(a, b, at(5, 0), at(20, 0)));
        assert!(!segments_intersect(a, b, at(11, 0), at(20, 0)));
        assert!(!segments_intersect(a, b, at(0, 1), at(10, 1)));
    }

    #[test]
    fn distances_between_cores() {
        let (a, b) = (at(0, 0), at(10, 0));
        assert_eq!(point_segment_distance(at(5, 3), a, b), 3.0);
        assert_eq!(point_segment_distance(at(13, 4), a, b), 5.0);
        assert_eq!(segment_distance(a, b, at(0, 7), at(10, 7)), 7.0);
        let first = Core::Polygon(square(0, 0, 10));
        let second = Core::Polygon(square(14, 0, 10));
        assert_eq!(core_distance(&first, &second), 4.0);
        let inner = Core::Polygon(square(2, 2, 2));
        assert_eq!(core_distance(&first, &inner), 0.0);
        let centre = Core::Point(at(5, 5));
        assert_eq!(core_distance(&first, &centre), 0.0);
        let outside = Core::Point(at(13, 14));
        assert_eq!(core_distance(&first, &outside), 5.0);
    }

    #[test]
    fn shapes_subtract_their_radii() {
        let track = Shape::stroke(at(0, 0), at(100, 0), Nm(20));
        let via = Shape::disc(at(50, 40), Nm(10));
        assert_eq!(track.distance(&via), 20.0);
        assert!(!track.touches(&via));
        let close = Shape::disc(at(50, 20), Nm(10));
        assert!(track.touches(&close));
        assert_eq!(track.distance(&close), 0.0);
        assert!(track.contains(at(100, 10)));
        assert!(!track.contains(at(111, 0)));
        assert_eq!(track.bounds(), (at(-10, -10), at(110, 10)));
    }

    #[test]
    fn points_in_polygons_including_the_boundary() {
        let l_shape = vec![
            at(0, 0),
            at(20, 0),
            at(20, 10),
            at(10, 10),
            at(10, 20),
            at(0, 20),
        ];
        assert!(point_in_polygon(at(5, 15), &l_shape));
        assert!(!point_in_polygon(at(15, 15), &l_shape));
        assert!(point_in_polygon(at(20, 5), &l_shape));
        assert!(point_in_polygon(at(10, 15), &l_shape));
        let reversed: Vec<Point> = l_shape.iter().rev().copied().collect();
        assert!(point_in_polygon(at(5, 15), &reversed));
        assert!(!point_in_polygon(at(15, 15), &reversed));
    }

    #[test]
    fn rotation_is_exact_for_quarter_turns() {
        assert_eq!(rotate(at(10, 0), 900), at(0, -10));
        assert_eq!(rotate(at(10, 0), 2700), at(0, 10));
        assert_eq!(rotate(at(1_000_000, 0), 450), at(707_107, -707_107));
        let corners = rectangle(at(100, 100), Nm(20), Nm(10), 900);
        let expected = [at(95, 110), at(95, 90), at(105, 90), at(105, 110)];
        assert_eq!(corners, expected);
    }

    #[test]
    fn circle_polygons_enclose_their_circle() {
        let polygon = circle_polygon(at(0, 0), Nm(1_000_000), 16);
        assert_eq!(polygon.len(), 16);
        for (a, b) in edges(&polygon) {
            let gap = point_segment_distance(at(0, 0), a, b);
            assert!(gap >= 999_999.0, "{gap}");
        }
        assert_eq!(doubled_area(&square(0, 0, 10)), 200);
    }
}
