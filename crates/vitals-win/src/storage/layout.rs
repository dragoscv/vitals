//! Map layouts over a [`SizeTree`]: an icicle and a squarified treemap.
//!
//! Computed here rather than in the webview because the tree lives here. A
//! full `C:` scan is 1.9 million directories; shipping them to JavaScript to
//! lay out would cost hundreds of megabytes of JSON for a picture that can
//! only ever show a few thousand rectangles. So the level of detail is decided
//! at the source: anything too narrow to see is folded into one "smaller
//! folders" cell, and the output is capped at a rectangle budget.

use std::collections::VecDeque;

use super::sizing::SkipReason;
use super::tree::{NodeId, SizeTree};

/// What a cell stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    /// A directory, which can be opened.
    Directory,
    /// The files directly inside the parent directory, as one block.
    Files,
    /// Subdirectories too small to draw individually, folded together.
    Smaller,
}

/// One rectangle of a map.
///
/// Coordinates are fractions of the drawing area (0 to 1) so the webview can
/// resize without asking again.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub kind: CellKind,
    /// Set for [`CellKind::Directory`], and for the other kinds the node whose
    /// files or small subfolders they stand for.
    pub node: NodeId,
    /// 0 for the focused directory itself.
    pub depth: u16,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub allocated: u64,
    /// Files for a directory or a files block; folders for a smaller block.
    pub count: u64,
    /// Whether a directory has subdirectories to open.
    pub openable: bool,
    pub incomplete: Option<SkipReason>,
}

/// Limits on how much detail a layout returns.
#[derive(Debug, Clone, Copy)]
pub struct Detail {
    /// Deepest level below the focus to lay out.
    pub max_depth: u16,
    /// Cells narrower than this fraction of the area are folded into
    /// [`CellKind::Smaller`]. For an icicle it is a width; for a treemap an
    /// area.
    pub min_fraction: f64,
    /// Hard cap on the number of cells.
    pub max_cells: usize,
}

impl Default for Detail {
    fn default() -> Self {
        Self {
            max_depth: 6,
            // About two pixels of a 1,200-pixel-wide map.
            min_fraction: 1.0 / 600.0,
            // Canvas 2D draws this many in well under a frame.
            max_cells: 5_000,
        }
    }
}

/// One entry to place inside a parent's span.
struct Part {
    kind: CellKind,
    node: NodeId,
    allocated: u64,
    count: u64,
    openable: bool,
    incomplete: Option<SkipReason>,
}

/// The pieces a directory divides into: its subdirectories largest first,
/// then its own files, with anything below `min_bytes` folded together.
fn parts_of(tree: &SizeTree, id: NodeId, min_bytes: f64) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut small_bytes = 0_u64;
    let mut small_count = 0_u64;
    for child in tree.children_by_size(id) {
        let Some(node) = tree.node(child) else {
            continue;
        };
        let bytes = node.allocated().get();
        // An empty but unreadable folder is kept: it is where the gap is.
        if (bytes as f64) < min_bytes && node.skipped().is_none() {
            small_bytes += bytes;
            small_count += 1;
            continue;
        }
        if bytes == 0 && node.skipped().is_none() {
            continue;
        }
        parts.push(Part {
            kind: CellKind::Directory,
            node: child,
            allocated: bytes,
            count: node.file_count(),
            openable: node.has_children(),
            incomplete: node.skipped(),
        });
    }
    if let Some(node) = tree.node(id) {
        let own = node.own_allocated().get();
        if own > 0 {
            parts.push(Part {
                kind: CellKind::Files,
                node: id,
                allocated: own,
                count: node.own_files(),
                openable: false,
                incomplete: None,
            });
        }
    }
    if small_bytes > 0 {
        parts.push(Part {
            kind: CellKind::Smaller,
            node: id,
            allocated: small_bytes,
            count: small_count,
            openable: false,
            incomplete: None,
        });
    }
    parts
}

fn focus_cell(tree: &SizeTree, focus: NodeId) -> Option<Cell> {
    let node = tree.node(focus)?;
    Some(Cell {
        kind: CellKind::Directory,
        node: focus,
        depth: 0,
        x0: 0.0,
        y0: 0.0,
        x1: 1.0,
        y1: 1.0,
        allocated: node.allocated().get(),
        count: node.file_count(),
        openable: node.has_children(),
        incomplete: node.skipped(),
    })
}

/// An icicle: the focus as the top row, each level below it one row, each
/// directory as wide as its share of the focus.
///
/// `y0`/`y1` are row fractions of `max_depth + 1` rows. Breadth-first, so when
/// the cap is reached it is the deepest, smallest detail that is dropped.
#[must_use]
pub fn icicle(tree: &SizeTree, focus: NodeId, detail: Detail) -> Vec<Cell> {
    let Some(root) = focus_cell(tree, focus) else {
        return Vec::new();
    };
    let rows = f64::from(detail.max_depth) + 1.0;
    let total = root.allocated.max(1) as f64;
    let min_bytes = detail.min_fraction * total;
    let mut cells = vec![Cell {
        y1: 1.0 / rows,
        ..root
    }];
    let mut queue = VecDeque::from([(focus, 0.0_f64, 1.0_f64, 0_u16)]);

    while let Some((id, x0, x1, depth)) = queue.pop_front() {
        if depth >= detail.max_depth {
            continue;
        }
        let span = x1 - x0;
        let parent_bytes = tree.node(id).map_or(0, |n| n.allocated().get()).max(1) as f64;
        let mut cursor = x0;
        let child_depth = depth + 1;
        for part in parts_of(tree, id, min_bytes) {
            if cells.len() >= detail.max_cells {
                return cells;
            }
            let width = span * part.allocated as f64 / parent_bytes;
            let (left, right) = (cursor, (cursor + width).min(x1));
            cursor = right;
            if part.kind == CellKind::Directory && part.openable {
                queue.push_back((part.node, left, right, child_depth));
            }
            cells.push(Cell {
                kind: part.kind,
                node: part.node,
                depth: child_depth,
                x0: left,
                x1: right,
                y0: f64::from(child_depth) / rows,
                y1: (f64::from(child_depth) + 1.0) / rows,
                allocated: part.allocated,
                count: part.count,
                openable: part.openable,
                incomplete: part.incomplete,
            });
        }
    }
    cells
}

/// A squarified treemap of the focus, nested `max_depth` levels.
///
/// `aspect` is the drawing area's width over its height; rectangles are
/// squarified in real proportions, then stored as fractions. Each nested
/// level is inset by `inset` (a fraction of the shorter side) so a parent's
/// edge stays visible around its children.
#[must_use]
pub fn treemap(tree: &SizeTree, focus: NodeId, aspect: f64, detail: Detail) -> Vec<Cell> {
    let Some(root) = focus_cell(tree, focus) else {
        return Vec::new();
    };
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    let total = root.allocated.max(1) as f64;
    // An area threshold: a cell below it is too small to see at all.
    let min_bytes = detail.min_fraction * detail.min_fraction * total;
    let inset = 0.004;
    let mut cells = vec![root];
    // Rectangles in real units (width `aspect`, height 1), converted on push.
    let mut queue = VecDeque::from([(focus, [0.0, 0.0, aspect, 1.0], 0_u16)]);

    while let Some((id, rect, depth)) = queue.pop_front() {
        if depth >= detail.max_depth {
            continue;
        }
        let [x, y, w, h] = rect;
        let pad = if depth == 0 { 0.0 } else { inset };
        let inner = [
            x + pad,
            y + pad,
            (w - 2.0 * pad).max(0.0),
            (h - 2.0 * pad).max(0.0),
        ];
        let parts = parts_of(tree, id, min_bytes);
        let weights: Vec<f64> = parts.iter().map(|p| p.allocated as f64).collect();
        let rects = squarify(&weights, inner);
        for (part, r) in parts.into_iter().zip(rects) {
            if cells.len() >= detail.max_cells {
                return cells;
            }
            let child_depth = depth + 1;
            if part.kind == CellKind::Directory && part.openable {
                queue.push_back((part.node, r, child_depth));
            }
            cells.push(Cell {
                kind: part.kind,
                node: part.node,
                depth: child_depth,
                x0: r[0] / aspect,
                y0: r[1],
                x1: (r[0] + r[2]) / aspect,
                y1: r[1] + r[3],
                allocated: part.allocated,
                count: part.count,
                openable: part.openable,
                incomplete: part.incomplete,
            });
        }
    }
    cells
}

/// Squarified layout (Bruls, Huizing, van Wijk 2000) of `weights`, which
/// must be sorted largest first, into `[x, y, w, h]`.
///
/// Returns one rectangle per weight, in order.
fn squarify(weights: &[f64], rect: [f64; 4]) -> Vec<[f64; 4]> {
    let mut out = Vec::with_capacity(weights.len());
    let total: f64 = weights.iter().sum();
    let [mut x, mut y, mut w, mut h] = rect;
    if total <= 0.0 || w <= 0.0 || h <= 0.0 {
        out.resize(weights.len(), [x, y, 0.0, 0.0]);
        return out;
    }
    // Weights scaled to areas of the rectangle.
    let scale = w * h / total;
    let areas: Vec<f64> = weights.iter().map(|v| v * scale).collect();

    let mut start = 0;
    while start < areas.len() {
        let side = w.min(h);
        let mut end = start + 1;
        let mut best = worst(&areas[start..end], side);
        while end < areas.len() {
            let next = worst(&areas[start..=end], side);
            if next > best {
                break;
            }
            best = next;
            end += 1;
        }
        let row = &areas[start..end];
        let sum: f64 = row.iter().sum();
        if w >= h {
            // A column along the left edge.
            let col = if h > 0.0 { sum / h } else { 0.0 };
            let mut cy = y;
            for a in row {
                let rh = if col > 0.0 { a / col } else { 0.0 };
                out.push([x, cy, col, rh]);
                cy += rh;
            }
            x += col;
            w -= col;
        } else {
            let band = if w > 0.0 { sum / w } else { 0.0 };
            let mut cx = x;
            for a in row {
                let rw = if band > 0.0 { a / band } else { 0.0 };
                out.push([cx, y, rw, band]);
                cx += rw;
            }
            y += band;
            h -= band;
        }
        start = end;
    }
    out
}

/// The worst aspect ratio in a row laid along a side of length `side`.
fn worst(row: &[f64], side: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    if sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let (min, max) = row.iter().fold((f64::INFINITY, 0.0_f64), |(lo, hi), &a| {
        (lo.min(a), hi.max(a))
    });
    let s2 = side * side;
    let sum2 = sum * sum;
    (s2 * max / sum2).max(sum2 / (s2 * min))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// root(files 100) -> big(900) -> deep(600); small1(1), small2(1)
    fn sample() -> (SizeTree, NodeId, NodeId) {
        let mut tree = SizeTree::new("R:\\");
        let root = tree.root();
        tree.add_file(root, 100, 100);
        let big = tree.add_child(root, "big");
        tree.add_file(big, 300, 300);
        let deep = tree.add_child(big, "deep");
        tree.add_file(deep, 600, 600);
        for name in ["small1", "small2"] {
            let small = tree.add_child(root, name);
            tree.add_file(small, 1, 1);
        }
        tree.aggregate();
        (tree, big, deep)
    }

    #[test]
    fn an_icicle_row_spans_exactly_its_parent_and_widths_follow_size() {
        let (tree, big, deep) = sample();
        let cells = icicle(&tree, tree.root(), Detail::default());
        let total = 1002.0;
        let big_cell = cells
            .iter()
            .find(|c| c.node == big && c.kind == CellKind::Directory);
        let big_cell = big_cell.expect("big is drawn");
        assert!((big_cell.x1 - big_cell.x0 - 900.0 / total).abs() < 1e-9);
        let deep_cell = cells
            .iter()
            .find(|c| c.node == deep)
            .expect("deep is drawn");
        assert_eq!(deep_cell.depth, 2);
        assert!(deep_cell.x0 >= big_cell.x0 && deep_cell.x1 <= big_cell.x1 + 1e-12);

        // Every row's cells, together, never exceed the focus.
        for depth in 1..=2 {
            let width: f64 = cells
                .iter()
                .filter(|c| c.depth == depth)
                .map(|c| c.x1 - c.x0)
                .sum();
            assert!(width <= 1.0 + 1e-9, "row {depth}: {width}");
        }
    }

    #[test]
    fn folders_too_small_to_see_are_folded_into_one_cell_not_dropped() {
        let (tree, _, _) = sample();
        let cells = icicle(
            &tree,
            tree.root(),
            Detail {
                min_fraction: 0.01,
                ..Detail::default()
            },
        );
        let smaller: Vec<_> = cells
            .iter()
            .filter(|c| c.kind == CellKind::Smaller)
            .collect();
        assert_eq!(smaller.len(), 1);
        assert_eq!(smaller[0].count, 2);
        assert_eq!(
            smaller[0].allocated, 2,
            "their bytes are still in the picture"
        );
        let row: u64 = cells
            .iter()
            .filter(|c| c.depth == 1)
            .map(|c| c.allocated)
            .sum();
        assert_eq!(row, 1002, "row one accounts for every byte of the focus");
    }

    #[test]
    fn the_cell_budget_is_a_hard_cap() {
        let mut tree = SizeTree::new("R:\\");
        let root = tree.root();
        for i in 0..10_000 {
            let child = tree.add_child(root, &format!("d{i}"));
            tree.add_file(child, 1_000, 1_000);
        }
        tree.aggregate();
        let detail = Detail {
            min_fraction: 0.0,
            max_cells: 300,
            ..Detail::default()
        };
        assert_eq!(icicle(&tree, root, detail).len(), 300);
        assert_eq!(treemap(&tree, root, 1.6, detail).len(), 300);
    }

    #[test]
    fn treemap_cells_tile_the_area_without_overlap() {
        let weights = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let rects = squarify(&weights, [0.0, 0.0, 6.0, 4.0]);
        let area: f64 = rects.iter().map(|r| r[2] * r[3]).sum();
        assert!((area - 24.0).abs() < 1e-9);
        for (i, a) in rects.iter().enumerate() {
            assert!(a[0] >= -1e-9 && a[1] >= -1e-9);
            assert!(a[0] + a[2] <= 6.0 + 1e-9 && a[1] + a[3] <= 4.0 + 1e-9);
            for b in &rects[i + 1..] {
                let overlap_w = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
                let overlap_h = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
                assert!(
                    overlap_w <= 1e-9 || overlap_h <= 1e-9,
                    "{a:?} overlaps {b:?}"
                );
            }
        }
        // Squarified means nearly square: the paper's own example stays under 3:1.
        for r in &rects {
            let ratio = (r[2] / r[3]).max(r[3] / r[2]);
            assert!(ratio < 3.0, "{r:?}");
        }
    }

    #[test]
    fn treemap_children_sit_inside_their_parent() {
        let (tree, big, deep) = sample();
        let cells = treemap(&tree, tree.root(), 1.5, Detail::default());
        let parent = cells
            .iter()
            .find(|c| c.node == big && c.kind == CellKind::Directory)
            .expect("big");
        let child = cells.iter().find(|c| c.node == deep).expect("deep");
        assert!(child.x0 >= parent.x0 && child.x1 <= parent.x1);
        assert!(child.y0 >= parent.y0 && child.y1 <= parent.y1);
    }

    #[test]
    fn an_unknown_focus_yields_no_cells() {
        let (tree, _, _) = sample();
        assert!(icicle(&tree, NodeId(9_999), Detail::default()).is_empty());
    }
}
