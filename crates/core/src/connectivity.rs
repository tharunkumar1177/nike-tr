//! Which copper touches which. An R-Tree over the items' bounding boxes finds the
//! candidates, and the exact shape test decides; union-find then groups the touching items
//! of each net into clusters. DRC reuses the same index for its broad phase.

use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{AABB, RTree};

use crate::copper::CopperItem;
use crate::geometry::Bounds;
use crate::netlist::DisjointSet;

/// One item's box in the tree, carrying the item's index.
type Entry = GeomWithData<Rectangle<[i64; 2]>, usize>;

/// An R-Tree of copper items by their bounding boxes.
pub struct CopperIndex {
    tree: RTree<Entry>,
}

impl CopperIndex {
    /// Indexes `items` by position in the slice.
    pub fn new(items: &[CopperItem]) -> CopperIndex {
        let mut entries = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let (min, max) = item.shape.bounds();
            let rectangle = Rectangle::from_corners([min.x.0, min.y.0], [max.x.0, max.y.0]);
            entries.push(GeomWithData::new(rectangle, index));
        }
        CopperIndex {
            tree: RTree::bulk_load(entries),
        }
    }

    /// The indices of the items whose boxes touch `bounds`, in no particular order.
    pub fn near(&self, bounds: Bounds) -> Vec<usize> {
        let (min, max) = bounds;
        let envelope = AABB::from_corners([min.x.0, min.y.0], [max.x.0, max.y.0]);
        self.tree
            .locate_in_envelope_intersecting(envelope)
            .map(|entry| entry.data)
            .collect()
    }
}

/// The cluster of each item: items of the same net that touch, directly or through other
/// items of that net, share a cluster id, which is the index of one of them. Items without
/// a net are clusters of their own.
pub fn clusters(items: &[CopperItem]) -> Vec<usize> {
    let index = CopperIndex::new(items);
    let mut sets = DisjointSet::default();
    for _ in items {
        sets.add();
    }
    for (position, item) in items.iter().enumerate() {
        let Some(net) = &item.net else {
            continue;
        };
        for other in index.near(item.shape.bounds()) {
            let candidate = &items[other];
            let same_net = candidate.net.as_ref() == Some(net);
            if other > position && same_net && item.touches(candidate) {
                sets.union(position, other);
            }
        }
    }
    (0..items.len())
        .map(|position| sets.find(position))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copper::{ItemId, LayerSet};
    use crate::geometry::Shape;
    use crate::units::{Nm, Point};
    use uuid::Uuid;

    fn track(n: u128, net: &str, layer: &str, from: (i64, i64), to: (i64, i64)) -> CopperItem {
        let a = Point::new(Nm(from.0), Nm(from.1));
        let b = Point::new(Nm(to.0), Nm(to.1));
        CopperItem {
            id: ItemId::Track(Uuid::from_u128(n)),
            net: Some(net.to_owned()),
            layers: LayerSet::named([layer]),
            shape: Shape::stroke(a, b, Nm(10)),
            anchors: vec![a, b],
        }
    }

    #[test]
    fn touching_items_of_one_net_on_one_layer_join() {
        let items = [
            track(1, "A", "F.Cu", (0, 0), (100, 0)),
            track(2, "A", "F.Cu", (100, 0), (100, 100)),
            track(3, "A", "B.Cu", (100, 100), (200, 100)),
            track(4, "B", "F.Cu", (100, 100), (100, 200)),
            track(5, "A", "F.Cu", (0, 5), (0, 50)),
        ];
        let found = clusters(&items);
        assert_eq!(found[0], found[1]);
        assert_eq!(found[0], found[4]);
        assert_ne!(found[1], found[2]);
        assert_ne!(found[1], found[3]);
    }
}
