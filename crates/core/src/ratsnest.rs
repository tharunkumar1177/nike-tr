//! The ratsnest: the unrouted connections of each net, drawn as the shortest lines that
//! join all of its copper.
//!
//! Each net's connected copper forms clusters. Until tracks exist every pad is its own
//! cluster. The clusters' anchors are triangulated (Delaunay), and Kruskal's algorithm picks
//! the minimum spanning tree from the triangulation's edges, so a net with `n` clusters
//! gets `n - 1` lines. Ties break on length, then on the anchors' order, which sorts by
//! position, so the result depends only on the geometry.

use std::collections::{BTreeMap, HashMap};

use spade::{DelaunayTriangulation, Point2, Triangulation};

use crate::board::Board;
use crate::netlist::DisjointSet;
use crate::units::Point;

/// One unrouted connection between two anchors of a net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatsnestLine {
    pub net: String,
    pub from: Point,
    pub to: Point,
}

/// The ratsnest of every net, kept so that edits recompute only the nets whose anchors
/// changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ratsnest {
    nets: BTreeMap<String, NetRatsnest>,
}

/// One net's anchors, sorted, and the lines that join them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NetRatsnest {
    anchors: Vec<Point>,
    lines: Vec<RatsnestLine>,
}

impl Ratsnest {
    /// The ratsnest of every net on `board`.
    pub fn new(board: &Board) -> Ratsnest {
        let mut ratsnest = Ratsnest::default();
        ratsnest.update(board);
        ratsnest
    }

    /// Recomputes the nets whose anchors moved, appeared, or disappeared, and returns their
    /// names in order. A net that no longer has anchors is dropped and still listed.
    pub fn update(&mut self, board: &Board) -> Vec<String> {
        let current = net_anchors(board);
        let mut changed = Vec::new();
        self.nets.retain(|net, _| {
            let kept = current.contains_key(net);
            if !kept {
                changed.push(net.clone());
            }
            kept
        });
        for (net, anchors) in current {
            if self
                .nets
                .get(&net)
                .is_some_and(|known| known.anchors == anchors)
            {
                continue;
            }
            let lines = spanning_tree(&anchors)
                .into_iter()
                .map(|(a, b)| RatsnestLine {
                    net: net.clone(),
                    from: anchors[a],
                    to: anchors[b],
                })
                .collect();
            changed.push(net.clone());
            self.nets.insert(net, NetRatsnest { anchors, lines });
        }
        changed.sort();
        changed
    }

    /// Every line, by net name.
    pub fn lines(&self) -> Vec<RatsnestLine> {
        self.nets
            .values()
            .flat_map(|net| net.lines.iter().cloned())
            .collect()
    }

    /// The lines of the nets in `nets`, by net name.
    pub fn net_lines(&self, nets: &[String]) -> Vec<RatsnestLine> {
        nets.iter()
            .filter_map(|net| self.nets.get(net))
            .flat_map(|net| net.lines.iter().cloned())
            .collect()
    }
}

/// Every pad's board position, grouped by net and sorted by position. Pads without a net
/// are left out.
fn net_anchors(board: &Board) -> BTreeMap<String, Vec<Point>> {
    let mut nets: BTreeMap<String, Vec<Point>> = BTreeMap::new();
    for footprint in board.footprints() {
        for pad in &footprint.pads {
            if let Some(net) = &pad.net {
                let position = footprint.pad_position(pad);
                nets.entry(net.clone()).or_default().push(position);
            }
        }
    }
    for anchors in nets.values_mut() {
        anchors.sort_by_key(|point| (point.x, point.y));
    }
    nets
}

/// The minimum spanning tree over `anchors`, which must be sorted by position, as pairs of
/// indices into it. Anchors at the same point join with zero-length lines.
pub(crate) fn spanning_tree(anchors: &[Point]) -> Vec<(usize, usize)> {
    let mut candidates: Vec<(i128, usize, usize)> = Vec::new();
    let mut unique: Vec<usize> = Vec::new();
    for (index, anchor) in anchors.iter().enumerate() {
        match unique.last() {
            Some(&last) if anchors[last] == *anchor => candidates.push((0, last, index)),
            _ => unique.push(index),
        }
    }
    let length = |a: usize, b: usize| {
        let dx = i128::from(anchors[a].x.0 - anchors[b].x.0);
        let dy = i128::from(anchors[a].y.0 - anchors[b].y.0);
        dx * dx + dy * dy
    };
    for (a, b) in triangulation_edges(anchors, &unique) {
        let (a, b) = (a.min(b), a.max(b));
        candidates.push((length(a, b), a, b));
    }
    candidates.sort_unstable();
    let mut clusters = DisjointSet::default();
    for _ in anchors {
        clusters.add();
    }
    let mut tree = Vec::with_capacity(anchors.len().saturating_sub(1));
    for (_, a, b) in candidates {
        if clusters.find(a) != clusters.find(b) {
            clusters.union(a, b);
            tree.push((a, b));
        }
    }
    tree
}

/// The edges of the Delaunay triangulation of the anchors at `unique`, as anchor indices.
/// Every pair is a candidate when the points can't be triangulated.
fn triangulation_edges(anchors: &[Point], unique: &[usize]) -> Vec<(usize, usize)> {
    let mut triangulation: DelaunayTriangulation<Point2<f64>> = DelaunayTriangulation::new();
    let mut by_vertex = HashMap::with_capacity(unique.len());
    for &index in unique {
        let at = anchors[index];
        let point = Point2::new(at.x.0 as f64, at.y.0 as f64);
        match triangulation.insert(point) {
            Ok(vertex) => {
                by_vertex.insert(vertex.index(), index);
            }
            Err(_) => return all_pairs(unique),
        }
    }
    triangulation
        .undirected_edges()
        .filter_map(|edge| {
            let [a, b] = edge.vertices();
            let a = by_vertex.get(&a.fix().index())?;
            let b = by_vertex.get(&b.fix().index())?;
            Some((*a, *b))
        })
        .collect()
}

/// Every pair of the anchors at `unique`.
fn all_pairs(unique: &[usize]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (position, &a) in unique.iter().enumerate() {
        for &b in &unique[position + 1..] {
            pairs.push((a, b));
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Nm;

    fn at(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn a_square_gets_three_sides_chosen_deterministically() {
        let anchors = [at(0, 0), at(0, 10), at(10, 0), at(10, 10)];
        assert_eq!(spanning_tree(&anchors), [(0, 1), (0, 2), (1, 3)]);
    }

    #[test]
    fn n_anchors_get_n_minus_one_lines() {
        let mut anchors: Vec<Point> = (0..40)
            .map(|i| at((i * 7919) % 101 * 1_000, (i * 104_729) % 97 * 1_000))
            .collect();
        anchors.sort_by_key(|point| (point.x, point.y));
        let tree = spanning_tree(&anchors);
        assert_eq!(tree.len(), anchors.len() - 1);
        let mut clusters = DisjointSet::default();
        for _ in &anchors {
            clusters.add();
        }
        for (a, b) in tree {
            clusters.union(a, b);
        }
        let root = clusters.find(0);
        assert!((0..anchors.len()).all(|i| clusters.find(i) == root));
    }

    #[test]
    fn collinear_and_coincident_anchors_still_span() {
        let line = [at(0, 0), at(5, 0), at(10, 0)];
        assert_eq!(spanning_tree(&line), [(0, 1), (1, 2)]);
        let stacked = [at(0, 0), at(0, 0), at(3, 4)];
        assert_eq!(spanning_tree(&stacked), [(0, 1), (0, 2)]);
        assert!(spanning_tree(&[at(1, 1)]).is_empty());
        assert!(spanning_tree(&[]).is_empty());
    }

    /// A board with two resistors whose pads share the nets `A` and `B`.
    fn board(second_x: &str) -> Board {
        let resistor = |uuid: &str, x: &str| {
            format!(
                "(footprint \"R\" (layer \"F.Cu\") (uuid \"{uuid}\") (at {x} 0) \
                (pad \"1\" smd rect (at -1 0) (size 1 1) (net \"A\")) \
                (pad \"2\" smd rect (at 1 0) (size 1 1) (net \"B\")))"
            )
        };
        Board::parse(&format!(
            "(kicad_pcb (version 20260206) {} {})",
            resistor("00000000-0000-4000-8000-000000000001", "0"),
            resistor("00000000-0000-4000-8000-000000000002", second_x),
        ))
        .unwrap()
    }

    #[test]
    fn only_nets_whose_pads_moved_are_recomputed() {
        let mut ratsnest = Ratsnest::new(&board("10"));
        let lines = ratsnest.lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            RatsnestLine {
                net: "A".to_owned(),
                from: at(-1_000_000, 0),
                to: at(9_000_000, 0),
            }
        );
        assert!(ratsnest.update(&board("10")).is_empty());
        assert_eq!(ratsnest.update(&board("20")), ["A", "B"]);
        let moved = ratsnest.net_lines(&["B".to_owned()]);
        assert_eq!(moved[0].to, at(21_000_000, 0));
        let empty = Board::parse("(kicad_pcb (version 20260206))").unwrap();
        assert_eq!(ratsnest.update(&empty), ["A", "B"]);
        assert!(ratsnest.lines().is_empty());
    }
}
