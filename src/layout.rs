//! The arrangement of terminal panes.
//!
//! A binary tree: every leaf is a terminal session, every branch splits its
//! area in two at an adjustable ratio. Splitting the focused pane replaces
//! that leaf with a branch holding the old pane and a new one; closing a pane
//! collapses its branch back into its sibling.

use eframe::egui::{Pos2, Rect};

/// How a branch arranges its two children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Side by side — what "split right" produces.
    Row,
    /// Stacked — what "split down" produces.
    Column,
}

/// Which way focus should move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Left,
    Right,
    Up,
    Down,
}

/// A pane tree. Ids are session ids for leaves and divider ids for branches;
/// the two come from the same counter in the application, so they never clash.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Leaf(u64),
    Split {
        id: u64,
        dir: Dir,
        /// Share of the space given to `first`, clamped to sane bounds.
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// Smallest share a pane may be dragged down to.
const MIN_RATIO: f32 = 0.1;

impl Node {
    pub fn leaf(id: u64) -> Self {
        Node::Leaf(id)
    }

    /// Splits the pane showing `target`, putting `new_leaf` beside or below
    /// it. Returns false when `target` is not in the tree.
    pub fn split(&mut self, target: u64, dir: Dir, split_id: u64, new_leaf: u64) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                *self = Node::Split {
                    id: split_id,
                    dir,
                    ratio: 0.5,
                    first: Box::new(Node::Leaf(target)),
                    second: Box::new(Node::Leaf(new_leaf)),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { first, second, .. } => {
                first.split(target, dir, split_id, new_leaf)
                    || second.split(target, dir, split_id, new_leaf)
            }
        }
    }

    /// Removes the pane showing `target`, collapsing its branch into the
    /// sibling. Returns false when `target` is the only pane, or absent.
    pub fn remove(&mut self, target: u64) -> bool {
        match self {
            // The root leaf cannot be removed: something has to be on screen.
            Node::Leaf(_) => false,
            Node::Split { first, second, .. } => {
                if matches!(**first, Node::Leaf(id) if id == target) {
                    *self = (**second).clone();
                    return true;
                }
                if matches!(**second, Node::Leaf(id) if id == target) {
                    *self = (**first).clone();
                    return true;
                }
                first.remove(target) || second.remove(target)
            }
        }
    }

    pub fn contains(&self, target: u64) -> bool {
        match self {
            Node::Leaf(id) => *id == target,
            Node::Split { first, second, .. } => first.contains(target) || second.contains(target),
        }
    }

    /// Every pane, left to right and top to bottom.
    pub fn leaves(&self) -> Vec<u64> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<u64>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    pub fn pane_count(&self) -> usize {
        self.leaves().len()
    }

    /// Moves a divider. `ratio` is clamped so neither side can vanish.
    pub fn set_ratio(&mut self, split_id: u64, ratio: f32) -> bool {
        match self {
            Node::Leaf(_) => false,
            Node::Split {
                id,
                ratio: current,
                first,
                second,
                ..
            } => {
                if *id == split_id {
                    *current = ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
                    true
                } else {
                    first.set_ratio(split_id, ratio) || second.set_ratio(split_id, ratio)
                }
            }
        }
    }

    /// Where each pane goes inside `rect`, leaving `gap` between neighbours.
    pub fn pane_rects(&self, rect: Rect, gap: f32) -> Vec<(u64, Rect)> {
        let mut out = Vec::new();
        self.walk(rect, gap, &mut out, &mut Vec::new());
        out
    }

    /// Where each divider goes, for dragging. Returns `(split id, direction,
    /// the strip to interact with)`.
    pub fn divider_rects(&self, rect: Rect, gap: f32) -> Vec<(u64, Dir, Rect)> {
        let mut dividers = Vec::new();
        self.walk(rect, gap, &mut Vec::new(), &mut dividers);
        dividers
    }

    fn walk(
        &self,
        rect: Rect,
        gap: f32,
        panes: &mut Vec<(u64, Rect)>,
        dividers: &mut Vec<(u64, Dir, Rect)>,
    ) {
        match self {
            Node::Leaf(id) => panes.push((*id, rect)),
            Node::Split {
                id,
                dir,
                ratio,
                first,
                second,
            } => {
                let (a, b, divider) = split_rect(rect, *dir, *ratio, gap);
                dividers.push((*id, *dir, divider));
                first.walk(a, gap, panes, dividers);
                second.walk(b, gap, panes, dividers);
            }
        }
    }

    /// The pane to focus when moving `nav` from `from`.
    ///
    /// Worked out from the laid-out rectangles rather than the tree, so it
    /// behaves the way it looks: the nearest pane whose centre lies in that
    /// direction and whose span overlaps the current one.
    pub fn neighbour(&self, from: u64, nav: Nav, rect: Rect, gap: f32) -> Option<u64> {
        let rects = self.pane_rects(rect, gap);
        let current = rects.iter().find(|(id, _)| *id == from)?.1;

        rects
            .iter()
            .filter(|(id, _)| *id != from)
            .filter(|(_, r)| match nav {
                Nav::Left => r.center().x < current.center().x && overlaps_y(*r, current),
                Nav::Right => r.center().x > current.center().x && overlaps_y(*r, current),
                Nav::Up => r.center().y < current.center().y && overlaps_x(*r, current),
                Nav::Down => r.center().y > current.center().y && overlaps_x(*r, current),
            })
            .min_by(|(_, a), (_, b)| {
                let da = distance(current.center(), a.center(), nav);
                let db = distance(current.center(), b.center(), nav);
                da.total_cmp(&db)
            })
            .map(|(id, _)| *id)
    }
}

fn overlaps_y(a: Rect, b: Rect) -> bool {
    a.min.y < b.max.y && b.min.y < a.max.y
}

fn overlaps_x(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x
}

fn distance(from: Pos2, to: Pos2, nav: Nav) -> f32 {
    match nav {
        Nav::Left | Nav::Right => (to.x - from.x).abs(),
        Nav::Up | Nav::Down => (to.y - from.y).abs(),
    }
}

/// Divides `rect` in two, reserving `gap` between the halves and reporting
/// the strip that sits in that gap.
fn split_rect(rect: Rect, dir: Dir, ratio: f32, gap: f32) -> (Rect, Rect, Rect) {
    let ratio = ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
    match dir {
        Dir::Row => {
            let usable = (rect.width() - gap).max(0.0);
            let cut = rect.min.x + usable * ratio;
            (
                Rect::from_min_max(rect.min, Pos2::new(cut, rect.max.y)),
                Rect::from_min_max(Pos2::new(cut + gap, rect.min.y), rect.max),
                Rect::from_min_max(
                    Pos2::new(cut, rect.min.y),
                    Pos2::new(cut + gap, rect.max.y),
                ),
            )
        }
        Dir::Column => {
            let usable = (rect.height() - gap).max(0.0);
            let cut = rect.min.y + usable * ratio;
            (
                Rect::from_min_max(rect.min, Pos2::new(rect.max.x, cut)),
                Rect::from_min_max(Pos2::new(rect.min.x, cut + gap), rect.max),
                Rect::from_min_max(
                    Pos2::new(rect.min.x, cut),
                    Pos2::new(rect.max.x, cut + gap),
                ),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    fn area() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 600.0))
    }

    #[test]
    fn a_lone_pane_fills_the_area() {
        let tree = Node::leaf(1);
        assert_eq!(tree.pane_count(), 1);
        assert_eq!(tree.pane_rects(area(), 8.0), vec![(1, area())]);
        assert!(tree.divider_rects(area(), 8.0).is_empty());
    }

    #[test]
    fn splitting_right_puts_the_new_pane_beside_the_old() {
        let mut tree = Node::leaf(1);
        assert!(tree.split(1, Dir::Row, 100, 2));
        assert_eq!(tree.leaves(), vec![1, 2]);

        let rects = tree.pane_rects(area(), 10.0);
        let (a, b) = (rects[0].1, rects[1].1);
        assert_eq!(rects[0].0, 1);
        assert_eq!(rects[1].0, 2);
        // Equal halves, with the gap taken out of the middle.
        assert!((a.width() - b.width()).abs() < 0.01, "{a:?} {b:?}");
        assert!((b.min.x - a.max.x - 10.0).abs() < 0.01, "gap between halves");
        assert_eq!(a.height(), area().height(), "a row split keeps full height");
    }

    #[test]
    fn splitting_down_stacks_the_new_pane() {
        let mut tree = Node::leaf(1);
        tree.split(1, Dir::Column, 100, 2);
        let rects = tree.pane_rects(area(), 10.0);
        let (a, b) = (rects[0].1, rects[1].1);
        assert!((a.height() - b.height()).abs() < 0.01);
        assert!((b.min.y - a.max.y - 10.0).abs() < 0.01);
        assert_eq!(a.width(), area().width());
    }

    #[test]
    fn splitting_an_unknown_pane_changes_nothing() {
        let mut tree = Node::leaf(1);
        assert!(!tree.split(99, Dir::Row, 100, 2));
        assert_eq!(tree, Node::leaf(1));
    }

    #[test]
    fn closing_a_pane_gives_its_room_to_the_sibling() {
        let mut tree = Node::leaf(1);
        tree.split(1, Dir::Row, 100, 2);
        tree.split(2, Dir::Column, 101, 3);
        assert_eq!(tree.leaves(), vec![1, 2, 3]);

        assert!(tree.remove(3));
        assert_eq!(tree.leaves(), vec![1, 2]);
        // Pane 2 inherits the whole right half.
        let rects = tree.pane_rects(area(), 10.0);
        assert_eq!(rects[1].1.height(), area().height());

        assert!(tree.remove(2));
        assert_eq!(tree, Node::leaf(1));
        // The last pane cannot be removed.
        assert!(!tree.remove(1));
        assert!(!tree.remove(42));
    }

    #[test]
    fn dividers_sit_in_the_gaps_and_move_the_split() {
        let mut tree = Node::leaf(1);
        tree.split(1, Dir::Row, 100, 2);

        let dividers = tree.divider_rects(area(), 10.0);
        assert_eq!(dividers.len(), 1);
        let (id, dir, strip) = dividers[0];
        assert_eq!((id, dir), (100, Dir::Row));
        assert!((strip.width() - 10.0).abs() < 0.01);

        assert!(tree.set_ratio(100, 0.25));
        let rects = tree.pane_rects(area(), 10.0);
        assert!((rects[0].1.width() - (1000.0 - 10.0) * 0.25).abs() < 0.01);

        // Neither side can be dragged away entirely.
        tree.set_ratio(100, -5.0);
        assert!(tree.pane_rects(area(), 10.0)[0].1.width() > 1.0);
        tree.set_ratio(100, 5.0);
        assert!(tree.pane_rects(area(), 10.0)[1].1.width() > 1.0);

        assert!(!tree.set_ratio(999, 0.5), "unknown divider");
    }

    #[test]
    fn focus_moves_the_way_the_panes_look() {
        // 1 | 2
        //   | 3
        let mut tree = Node::leaf(1);
        tree.split(1, Dir::Row, 100, 2);
        tree.split(2, Dir::Column, 101, 3);
        let (r, gap) = (area(), 10.0);

        assert_eq!(tree.neighbour(1, Nav::Right, r, gap), Some(2));
        assert_eq!(tree.neighbour(2, Nav::Left, r, gap), Some(1));
        assert_eq!(tree.neighbour(3, Nav::Left, r, gap), Some(1));
        assert_eq!(tree.neighbour(2, Nav::Down, r, gap), Some(3));
        assert_eq!(tree.neighbour(3, Nav::Up, r, gap), Some(2));

        // Nothing to the left of the leftmost pane, or above the top row.
        assert_eq!(tree.neighbour(1, Nav::Left, r, gap), None);
        assert_eq!(tree.neighbour(1, Nav::Up, r, gap), None);
        assert_eq!(tree.neighbour(2, Nav::Up, r, gap), None);
        assert_eq!(tree.neighbour(99, Nav::Left, r, gap), None);
    }

    #[test]
    fn panes_never_overlap_however_they_are_split() {
        let mut tree = Node::leaf(1);
        tree.split(1, Dir::Row, 100, 2);
        tree.split(2, Dir::Column, 101, 3);
        tree.split(1, Dir::Column, 102, 4);
        tree.split(3, Dir::Row, 103, 5);
        tree.set_ratio(100, 0.3);
        tree.set_ratio(101, 0.7);

        let rects = tree.pane_rects(area(), 10.0);
        assert_eq!(rects.len(), 5);
        for (i, (_, a)) in rects.iter().enumerate() {
            assert!(a.width() > 0.0 && a.height() > 0.0, "empty pane {a:?}");
            assert!(area().contains_rect(*a), "{a:?} escapes the area");
            for (_, b) in rects.iter().skip(i + 1) {
                assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
            }
        }
    }
}
