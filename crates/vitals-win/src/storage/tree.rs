//! An arena-backed directory tree sized for treemap rendering.
//!
//! The memory shape is the whole point. A naive tree stores an owned `String`
//! path on every node; on a two-million-file volume that is two million heap
//! allocations averaging well over a hundred bytes each — a quarter of a
//! gigabyte spent re-storing the substring `C:\Users\dragos\` a million times.
//!
//! Instead: nodes live in one `Vec` and are addressed by index, and each node
//! holds only *its own* name, interned so that the thousands of directories
//! called `bin`, `node_modules` or `en-US` share a single copy. A full path is
//! reconstructed by walking parents when — and only when — the UI asks for
//! one, which it does for a few dozen rows, never for a million.

use std::collections::HashMap;

use vitals_core::units::Bytes;

use super::sizing::{SkipReason, SkippedPath, top_n_by};

/// Index of a node within a [`SizeTree`].
///
/// A plain `u32`, not a pointer: it halves the size of every parent/child
/// link against a 64-bit pointer, and four billion directories is a limit no
/// filesystem will reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

/// Index of a string within the interner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NameId(u32);

/// Deduplicating store for path components.
///
/// Directory names repeat enormously across a real volume — every Rust
/// project has a `target`, every localised app has forty language folders.
/// Interning turns that from one allocation per occurrence into one per
/// distinct name.
#[derive(Debug, Default)]
struct Interner {
    names: Vec<String>,
    lookup: HashMap<String, NameId>,
}

impl Interner {
    fn intern(&mut self, name: &str) -> NameId {
        if let Some(&id) = self.lookup.get(name) {
            return id;
        }
        // Truncation is impossible in practice and saturating here keeps the
        // function total; a collision at u32::MAX distinct names would be a
        // pathological volume long past every other limit.
        let id = NameId(u32::try_from(self.names.len()).unwrap_or(u32::MAX));
        self.names.push(name.to_owned());
        self.lookup.insert(name.to_owned(), id);
        id
    }

    fn resolve(&self, id: NameId) -> &str {
        self.names.get(id.0 as usize).map_or("", String::as_str)
    }

    fn distinct(&self) -> usize {
        self.names.len()
    }
}

/// One directory in the tree.
///
/// Deliberately free of `String` and `Vec<NodeId>`: children are held as a
/// linked list through `first_child`/`next_sibling`, which costs eight bytes
/// per node instead of the twenty-four-byte header plus spare capacity a
/// `Vec` would need on every one of a million nodes.
#[derive(Debug, Clone)]
pub struct Node {
    name: NameId,
    parent: Option<NodeId>,
    first_child: Option<NodeId>,
    next_sibling: Option<NodeId>,
    /// Bytes in files directly inside this directory, on disk.
    own_allocated: u64,
    /// Logical bytes in those same files.
    own_logical: u64,
    /// Files directly inside this directory.
    own_files: u64,
    /// Including every descendant. Valid only after [`SizeTree::aggregate`].
    total_allocated: u64,
    total_logical: u64,
    total_files: u64,
    /// Set when this directory's contents could not be read, so the UI can
    /// mark the node as incomplete rather than showing a confident zero.
    skipped: Option<SkipReason>,
}

impl Node {
    /// Bytes this directory and all its descendants occupy on disk.
    ///
    /// The figure to render: it accounts for cluster rounding, so subtree
    /// totals reconcile with the volume's own used-space number.
    #[must_use]
    pub const fn allocated(&self) -> Bytes {
        Bytes(self.total_allocated)
    }

    /// Sum of file lengths in this subtree.
    ///
    /// Almost always smaller than [`Node::allocated`]. Exposed for the same
    /// reason the difference is worth showing: it is what every other tool,
    /// including Explorer's "Size:" line, reports.
    #[must_use]
    pub const fn logical(&self) -> Bytes {
        Bytes(self.total_logical)
    }

    #[must_use]
    pub const fn file_count(&self) -> u64 {
        self.total_files
    }

    /// Why this directory's contents are missing, if they are.
    #[must_use]
    pub const fn skipped(&self) -> Option<SkipReason> {
        self.skipped
    }
}

/// A scanned directory hierarchy.
#[derive(Debug)]
pub struct SizeTree {
    nodes: Vec<Node>,
    interner: Interner,
    root: NodeId,
    skipped: Vec<SkippedPath>,
    aggregated: bool,
}

impl SizeTree {
    /// Creates a tree with a single root named `root_name`.
    #[must_use]
    pub fn new(root_name: &str) -> Self {
        let mut interner = Interner::default();
        let name = interner.intern(root_name);
        Self {
            nodes: vec![Node {
                name,
                parent: None,
                first_child: None,
                next_sibling: None,
                own_allocated: 0,
                own_logical: 0,
                own_files: 0,
                total_allocated: 0,
                total_logical: 0,
                total_files: 0,
                skipped: None,
            }],
            interner,
            root: NodeId(0),
            skipped: Vec::new(),
            aggregated: false,
        }
    }

    #[must_use]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize)
    }

    /// Total directories in the tree, including the root.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1 && self.nodes[0].total_files == 0
    }

    /// Distinct interned component names — a direct measure of how much the
    /// interner saved.
    #[must_use]
    pub fn distinct_names(&self) -> usize {
        self.interner.distinct()
    }

    /// Approximate heap footprint of the tree, in bytes.
    ///
    /// Reported rather than guessed at, because "memory-efficient" is a claim
    /// that should carry a number.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        let nodes = self.nodes.capacity() * size_of::<Node>();
        let names: usize = self
            .interner
            .names
            .iter()
            .map(|n| n.capacity() + size_of::<String>())
            .sum();
        // The lookup map holds a second copy of each key plus bucket overhead;
        // approximated rather than measured, and flagged as such.
        nodes + names * 2 + self.skipped.capacity() * size_of::<SkippedPath>()
    }

    /// Adds a child directory and returns its index.
    pub fn add_child(&mut self, parent: NodeId, name: &str) -> NodeId {
        let name = self.interner.intern(name);
        let id = NodeId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));

        let previous_first = self
            .nodes
            .get(parent.0 as usize)
            .and_then(|p| p.first_child);

        self.nodes.push(Node {
            name,
            parent: Some(parent),
            first_child: None,
            // Prepending rather than appending keeps insertion O(1) without a
            // tail pointer; child order is not meaningful because every
            // consumer sorts by size anyway.
            next_sibling: previous_first,
            own_allocated: 0,
            own_logical: 0,
            own_files: 0,
            total_allocated: 0,
            total_logical: 0,
            total_files: 0,
            skipped: None,
        });

        if let Some(p) = self.nodes.get_mut(parent.0 as usize) {
            p.first_child = Some(id);
        }
        self.aggregated = false;
        id
    }

    /// Adds a file's sizes to a directory.
    ///
    /// Files are not given nodes of their own. A million file nodes would
    /// dominate both memory and render time, and a treemap only ever draws
    /// directories plus, at the leaf, a handful of individually large files.
    pub fn add_file(&mut self, dir: NodeId, allocated: u64, logical: u64) {
        if let Some(node) = self.nodes.get_mut(dir.0 as usize) {
            node.own_allocated = node.own_allocated.saturating_add(allocated);
            node.own_logical = node.own_logical.saturating_add(logical);
            node.own_files += 1;
            self.aggregated = false;
        }
    }

    /// Marks a directory as unmeasurable and records why.
    ///
    /// The node stays in the tree with zero size. Dropping it instead would
    /// make the parent's total look complete when it is not; keeping it
    /// visible and flagged lets the UI say "plus an unreadable folder".
    pub fn mark_skipped(&mut self, id: NodeId, path: String, reason: SkipReason) {
        if let Some(node) = self.nodes.get_mut(id.0 as usize) {
            node.skipped = Some(reason);
        }
        self.skipped.push(SkippedPath { path, reason });
    }

    /// Directories that were not measured, with reasons.
    #[must_use]
    pub fn skipped(&self) -> &[SkippedPath] {
        &self.skipped
    }

    /// Rolls child totals up into their parents.
    ///
    /// Iterative and bottom-up by index rather than recursive: a path like
    /// `node_modules/.pnpm/...` nests deeply enough that recursion on a
    /// pathological tree can overflow the stack, and a scanner that panics on
    /// a deep directory is worse than a slow one.
    ///
    /// Children always have a higher index than their parent because nodes
    /// are only ever appended, so a single reverse pass suffices.
    pub fn aggregate(&mut self) {
        for node in &mut self.nodes {
            node.total_allocated = node.own_allocated;
            node.total_logical = node.own_logical;
            node.total_files = node.own_files;
        }

        for i in (1..self.nodes.len()).rev() {
            let (allocated, logical, files, parent) = {
                let n = &self.nodes[i];
                (n.total_allocated, n.total_logical, n.total_files, n.parent)
            };
            if let Some(p) = parent
                && let Some(parent_node) = self.nodes.get_mut(p.0 as usize)
            {
                parent_node.total_allocated = parent_node.total_allocated.saturating_add(allocated);
                parent_node.total_logical = parent_node.total_logical.saturating_add(logical);
                parent_node.total_files = parent_node.total_files.saturating_add(files);
            }
        }

        self.aggregated = true;
    }

    /// Whether [`SizeTree::aggregate`] has run since the last mutation.
    #[must_use]
    pub const fn is_aggregated(&self) -> bool {
        self.aggregated
    }

    /// Reconstructs the full path of a node by walking to the root.
    ///
    /// O(depth) and allocating, which is why it is a method rather than a
    /// stored field: the UI needs it for the few rows it displays, not for
    /// every node in the tree.
    #[must_use]
    pub fn path_of(&self, id: NodeId) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            let Some(node) = self.nodes.get(current.0 as usize) else {
                break;
            };
            parts.push(self.interner.resolve(node.name));
            cursor = node.parent;
        }
        parts.reverse();

        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 && !out.ends_with('\\') {
                out.push('\\');
            }
            out.push_str(part);
        }
        out
    }

    /// Immediate subdirectories of a node.
    #[must_use]
    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cursor = self.nodes.get(id.0 as usize).and_then(|n| n.first_child);
        while let Some(child) = cursor {
            out.push(child);
            cursor = self
                .nodes
                .get(child.0 as usize)
                .and_then(|n| n.next_sibling);
        }
        out
    }

    /// The `n` largest directories anywhere in the tree, by allocated size.
    ///
    /// Subtree totals, so a parent will generally outrank its own children —
    /// which is what a "largest folders" list should show.
    ///
    /// Returns an empty vector if [`SizeTree::aggregate`] has not been run,
    /// rather than a plausible-looking list built from stale zeroes.
    #[must_use]
    pub fn largest_directories(&self, n: usize) -> Vec<(NodeId, Bytes)> {
        if !self.aggregated {
            return Vec::new();
        }
        let mut all: Vec<(NodeId, Bytes)> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (NodeId(i as u32), Bytes(node.total_allocated)))
            .collect();
        top_n_by(&mut all, n, |(_, bytes)| *bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_layout_is_pinned() {
        // Pinned because a million of these are allocated: a field added
        // carelessly costs megabytes, and the change should be a deliberate
        // one made against a failing test rather than an accident.
        assert_eq!(size_of::<Node>(), 88, "Node grew; was this intended?");
        assert_eq!(size_of::<NodeId>(), 4);
        assert_eq!(size_of::<Option<NodeId>>(), 8);
    }

    fn sample_tree() -> SizeTree {
        let mut tree = SizeTree::new("C:\\test");
        let root = tree.root();
        let a = tree.add_child(root, "alpha");
        let b = tree.add_child(root, "beta");
        let a_sub = tree.add_child(a, "nested");

        tree.add_file(root, 4096, 10);
        tree.add_file(a, 8192, 5000);
        tree.add_file(a_sub, 4096, 4096);
        tree.add_file(b, 65_536, 65_000);
        tree.aggregate();
        tree
    }

    #[test]
    fn totals_roll_up_through_every_level() {
        let tree = sample_tree();
        let root = tree.node(tree.root()).expect("root exists");
        assert_eq!(root.allocated().get(), 4096 + 8192 + 4096 + 65_536);
        assert_eq!(root.file_count(), 4);
    }

    #[test]
    fn a_subtree_total_excludes_its_siblings() {
        let tree = sample_tree();
        let alpha = tree
            .children(tree.root())
            .into_iter()
            .find(|&c| tree.path_of(c).ends_with("alpha"))
            .expect("alpha exists");
        assert_eq!(tree.node(alpha).expect("node").allocated().get(), 12_288);
    }

    #[test]
    fn logical_and_allocated_are_tracked_separately() {
        let tree = sample_tree();
        let root = tree.node(tree.root()).expect("root exists");
        assert_eq!(root.logical().get(), 10 + 5000 + 4096 + 65_000);
        assert!(
            root.allocated() > root.logical(),
            "cluster rounding must make disk usage exceed file lengths here"
        );
    }

    #[test]
    fn paths_are_reconstructed_with_a_single_separator() {
        let mut tree = SizeTree::new("C:\\");
        let a = tree.add_child(tree.root(), "Windows");
        let b = tree.add_child(a, "System32");
        assert_eq!(tree.path_of(b), "C:\\Windows\\System32");
    }

    #[test]
    fn repeated_names_are_interned_once() {
        let mut tree = SizeTree::new("root");
        for i in 0..100 {
            let parent = tree.add_child(tree.root(), &format!("project{i}"));
            tree.add_child(parent, "node_modules");
            tree.add_child(parent, "target");
        }
        // 1 root + 100 projects + 2 shared names = 103 distinct, against 301
        // nodes.
        assert_eq!(tree.distinct_names(), 103);
        assert_eq!(tree.len(), 301);
    }

    #[test]
    fn an_unreadable_directory_is_recorded_not_swallowed() {
        let mut tree = SizeTree::new("C:\\");
        let secret = tree.add_child(tree.root(), "System Volume Information");
        tree.mark_skipped(
            secret,
            "C:\\System Volume Information".into(),
            SkipReason::AccessDenied,
        );
        tree.add_file(tree.root(), 4096, 4096);
        tree.aggregate();

        assert_eq!(tree.skipped().len(), 1);
        assert_eq!(tree.skipped()[0].reason, SkipReason::AccessDenied);
        assert_eq!(
            tree.node(secret).expect("node").skipped(),
            Some(SkipReason::AccessDenied)
        );
        assert_eq!(
            tree.node(tree.root()).expect("root").allocated().get(),
            4096,
            "the unreadable subtree must contribute nothing, not a guess"
        );
    }

    #[test]
    fn a_skipped_reparse_point_contributes_nothing() {
        let mut tree = SizeTree::new("C:\\");
        let junction = tree.add_child(tree.root(), "Documents and Settings");
        tree.mark_skipped(
            junction,
            "C:\\Documents and Settings".into(),
            SkipReason::ReparsePoint,
        );
        tree.aggregate();
        assert_eq!(tree.node(tree.root()).expect("root").allocated().get(), 0);
        assert_eq!(tree.skipped()[0].reason, SkipReason::ReparsePoint);
    }

    #[test]
    fn largest_directories_are_ranked_by_subtree_size() {
        let tree = sample_tree();
        let top = tree.largest_directories(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, tree.root());
        assert!(top[0].1 >= top[1].1);
    }

    #[test]
    fn ranking_before_aggregation_returns_nothing_rather_than_zeroes() {
        let mut tree = SizeTree::new("C:\\");
        tree.add_file(tree.root(), 4096, 4096);
        assert!(!tree.is_aggregated());
        assert!(
            tree.largest_directories(5).is_empty(),
            "stale totals must not be presented as a ranking"
        );
    }

    #[test]
    fn aggregation_is_idempotent() {
        let mut tree = sample_tree();
        let first = tree.node(tree.root()).expect("root").allocated();
        tree.aggregate();
        tree.aggregate();
        assert_eq!(tree.node(tree.root()).expect("root").allocated(), first);
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        // node_modules/.pnpm chains reach several hundred levels in the wild.
        let mut tree = SizeTree::new("C:\\");
        let mut cursor = tree.root();
        for _ in 0..10_000 {
            cursor = tree.add_child(cursor, "deep");
            tree.add_file(cursor, 4096, 1);
        }
        tree.aggregate();
        assert_eq!(
            tree.node(tree.root()).expect("root").allocated().get(),
            10_000 * 4096
        );
    }
}
