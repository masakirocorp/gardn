//! BSP tree layout for tiling panes within a workspace.

use ratatui::layout::{Direction, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PaneId(u32);

/// Global atomic counter for unique PaneId generation across all workspaces.
static NEXT_PANE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

impl PaneId {
    /// Allocate a globally unique PaneId.
    pub fn alloc() -> Self {
        Self(NEXT_PANE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    /// Reconstruct from a saved u32 (persistence only).
    pub fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

/// Snapshot of a pane's position and focus state after layout.
#[derive(Clone)]
pub struct PaneInfo {
    pub id: PaneId,
    /// Outer rect (including borders if present).
    pub rect: Rect,
    /// Inner rect (content area, excluding borders). Used for selection.
    pub inner_rect: Rect,
    /// Visible scrollbar lane, when scrollback is present. `inner_rect` may still
    /// exclude a stable hidden gutter when this is `None`.
    pub scrollbar_rect: Option<Rect>,
    pub is_focused: bool,
}

/// Info about a split boundary, used for mouse drag resize.
#[derive(Clone)]
pub struct SplitBorder {
    /// Position of the divider line (x for horizontal split, y for vertical).
    pub pos: u16,
    /// Direction of the split that created this border.
    pub direction: Direction,
    /// Total area of the split node.
    pub area: Rect,
    /// Path from root to this split node (false=first, true=second).
    pub path: Vec<bool>,
}

/// Cardinal direction for pane navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDirection {
    Left,
    Right,
    Up,
    Down,
}

/// A node in the BSP tree. Public for serialization.
#[derive(Clone)]
pub enum Node {
    Pane(PaneId),
    Split {
        direction: Direction,
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// BSP tiling layout. Tracks a tree of splits and a focused pane.
#[derive(Clone)]
pub struct TileLayout {
    root: Node,
    focus: PaneId,
}

impl TileLayout {
    /// Create a new layout with a single pane (globally unique ID).
    /// Returns (layout, root_pane_id) so the caller can create the pane.
    pub fn new() -> (Self, PaneId) {
        let root_id = PaneId::alloc();
        (
            Self {
                root: Node::Pane(root_id),
                focus: root_id,
            },
            root_id,
        )
    }

    pub fn focused(&self) -> PaneId {
        self.focus
    }

    pub fn pane_count(&self) -> usize {
        count_panes(&self.root)
    }

    pub fn pane_ordinal(&self, pane_id: PaneId) -> Option<usize> {
        let mut ordinal = 0;
        find_pane_ordinal(&self.root, pane_id, &mut ordinal)
    }

    /// Compute rects for all panes given the available area.
    pub fn panes(&self, area: Rect) -> Vec<PaneInfo> {
        let mut result = Vec::new();
        collect_panes(&self.root, area, self.focus, &mut result);
        result
    }

    /// Collect all split boundaries for mouse drag resize.
    pub fn splits(&self, area: Rect) -> Vec<SplitBorder> {
        let mut result = Vec::new();
        collect_splits(&self.root, area, vec![], &mut result);
        result
    }

    /// Split the focused pane. Returns the new pane's id.
    pub fn split_focused(&mut self, direction: Direction) -> PaneId {
        let new_id = PaneId::alloc();
        let placeholder = PaneId::from_raw(0);
        let old = std::mem::replace(&mut self.root, Node::Pane(placeholder));
        self.root = split_at(old, self.focus, direction, new_id);
        self.focus = new_id;
        new_id
    }

    /// Split the focused pane with an explicit first-pane ratio.
    pub fn split_focused_with_ratio(&mut self, direction: Direction, ratio: f32) -> PaneId {
        let new_id = PaneId::alloc();
        let placeholder = PaneId::from_raw(0);
        let old = std::mem::replace(&mut self.root, Node::Pane(placeholder));
        self.root = split_at_with_ratio(old, self.focus, direction, new_id, ratio.clamp(0.1, 0.9));
        self.focus = new_id;
        new_id
    }

    /// Insert an existing pane id next to a target pane without allocating a new pane.
    pub fn insert_pane_near(
        &mut self,
        target: PaneId,
        moved: PaneId,
        direction: Direction,
        ratio: f32,
    ) -> bool {
        let ids = self.pane_ids();
        if !ids.contains(&target) || ids.contains(&moved) {
            return false;
        }

        let placeholder = PaneId::from_raw(0);
        let old = std::mem::replace(&mut self.root, Node::Pane(placeholder));
        let (new_root, inserted) =
            insert_existing_pane(old, target, moved, direction, ratio.clamp(0.1, 0.9));
        self.root = new_root;
        if inserted {
            self.focus = moved;
        }
        inserted
    }

    /// Close the focused pane. Returns false if it's the last pane.
    pub fn close_focused(&mut self) -> bool {
        if self.pane_count() <= 1 {
            return false;
        }
        let target = self.focus;
        let ids = self.pane_ids();
        let pos = ids.iter().position(|id| *id == target).unwrap();
        let new_focus = if pos + 1 < ids.len() {
            ids[pos + 1]
        } else {
            ids[pos - 1]
        };
        let placeholder = PaneId::from_raw(0);
        let old = std::mem::replace(&mut self.root, Node::Pane(placeholder));
        if let Some(new_root) = remove_pane(old, target) {
            self.root = new_root;
            self.focus = new_focus;
            true
        } else {
            false
        }
    }

    pub fn focus_pane(&mut self, id: PaneId) {
        if self.pane_ids().contains(&id) {
            self.focus = id;
        }
    }

    /// Set the ratio of a split node at the given path.
    pub fn set_ratio_at(&mut self, path: &[bool], ratio: f32) {
        set_ratio_at(&mut self.root, path, ratio.clamp(0.1, 0.9));
    }

    /// Adjust the nearest split in the given direction for the focused pane.
    /// `delta` is positive to grow, negative to shrink.
    pub fn resize_focused(&mut self, nav: NavDirection, delta: f32, area: Rect) {
        let panes = self.panes(area);
        let Some(focused) = panes.iter().find(|p| p.is_focused) else {
            return;
        };
        let focused_rect = focused.rect;
        let splits = self.splits(area);

        let target_dir = match nav {
            NavDirection::Left | NavDirection::Right => Direction::Horizontal,
            NavDirection::Up | NavDirection::Down => Direction::Vertical,
        };
        let grows = matches!(nav, NavDirection::Right | NavDirection::Down);

        let best = nearest_resize_split(&splits, target_dir, focused_rect, nav).or_else(|| {
            nearest_resize_split(&splits, target_dir, focused_rect, opposite_direction(nav))
        });

        if let Some(split) = best {
            let path = split.path.clone();
            let current_ratio = get_ratio_at(&self.root, &path).unwrap_or(0.5);
            let adj = if grows { delta } else { -delta };
            self.set_ratio_at(&path, current_ratio + adj);
        }
    }

    pub fn resize_pane(&mut self, id: PaneId, nav: NavDirection, delta: f32, area: Rect) {
        if !self.pane_ids().contains(&id) {
            return;
        }
        let previous_focus = self.focus;
        self.focus = id;
        self.resize_focused(nav, delta, area);
        self.focus = previous_focus;
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        collect_ids(&self.root, &mut ids);
        ids
    }

    /// Access the tree root for serialization.
    pub fn root(&self) -> &Node {
        &self.root
    }

    /// Reconstruct a layout from a saved tree.
    /// Reconstruct a layout from a saved tree.
    pub fn from_saved(root: Node, focus: PaneId) -> Self {
        Self { root, focus }
    }
}

// --- Directional pane navigation ---

/// Find the nearest pane in the given direction from `focused`.
pub fn find_in_direction(
    focused: &PaneInfo,
    direction: NavDirection,
    panes: &[PaneInfo],
) -> Option<PaneId> {
    let fr = focused.rect;

    panes
        .iter()
        .filter(|p| p.id != focused.id)
        .filter(|p| {
            let r = p.rect;
            match direction {
                NavDirection::Left => {
                    r.x + r.width <= fr.x && ranges_overlap(r.y, r.height, fr.y, fr.height)
                }
                NavDirection::Right => {
                    r.x >= fr.x + fr.width && ranges_overlap(r.y, r.height, fr.y, fr.height)
                }
                NavDirection::Up => {
                    r.y + r.height <= fr.y && ranges_overlap(r.x, r.width, fr.x, fr.width)
                }
                NavDirection::Down => {
                    r.y >= fr.y + fr.height && ranges_overlap(r.x, r.width, fr.x, fr.width)
                }
            }
        })
        .min_by_key(|p| {
            let r = p.rect;
            match direction {
                NavDirection::Left => fr.x.saturating_sub(r.x + r.width),
                NavDirection::Right => r.x.saturating_sub(fr.x + fr.width),
                NavDirection::Up => fr.y.saturating_sub(r.y + r.height),
                NavDirection::Down => r.y.saturating_sub(fr.y + fr.height),
            }
        })
        .map(|p| p.id)
}

fn ranges_overlap(a_start: u16, a_len: u16, b_start: u16, b_len: u16) -> bool {
    a_start < b_start + b_len && a_start + a_len > b_start
}

fn split_on_requested_edge(split: &SplitBorder, focused: Rect, nav: NavDirection) -> bool {
    split_edge_distance(split, focused, nav) <= 1
}

fn split_area_overlaps_focused_pane(split: &SplitBorder, focused: Rect, nav: NavDirection) -> bool {
    match nav {
        NavDirection::Left | NavDirection::Right => {
            ranges_overlap(split.area.y, split.area.height, focused.y, focused.height)
        }
        NavDirection::Up | NavDirection::Down => {
            ranges_overlap(split.area.x, split.area.width, focused.x, focused.width)
        }
    }
}

fn nearest_resize_split(
    splits: &[SplitBorder],
    target_dir: Direction,
    focused: Rect,
    nav: NavDirection,
) -> Option<&SplitBorder> {
    splits
        .iter()
        .filter(|s| s.direction == target_dir)
        .filter(|s| split_area_overlaps_focused_pane(s, focused, nav))
        .filter(|s| split_on_requested_edge(s, focused, nav))
        .min_by_key(|s| split_edge_distance(s, focused, nav))
}

fn opposite_direction(nav: NavDirection) -> NavDirection {
    match nav {
        NavDirection::Left => NavDirection::Right,
        NavDirection::Right => NavDirection::Left,
        NavDirection::Up => NavDirection::Down,
        NavDirection::Down => NavDirection::Up,
    }
}

fn split_edge_distance(split: &SplitBorder, focused: Rect, nav: NavDirection) -> u32 {
    match nav {
        NavDirection::Left => (split.pos as i32 - focused.x as i32).unsigned_abs(),
        NavDirection::Right => {
            (split.pos as i32 - (focused.x + focused.width) as i32).unsigned_abs()
        }
        NavDirection::Up => (split.pos as i32 - focused.y as i32).unsigned_abs(),
        NavDirection::Down => {
            (split.pos as i32 - (focused.y + focused.height) as i32).unsigned_abs()
        }
    }
}

// --- Tree operations ---

fn count_panes(node: &Node) -> usize {
    match node {
        Node::Pane(_) => 1,
        Node::Split { first, second, .. } => count_panes(first) + count_panes(second),
    }
}

fn find_pane_ordinal(node: &Node, target: PaneId, ordinal: &mut usize) -> Option<usize> {
    match node {
        Node::Pane(pane_id) => {
            *ordinal += 1;
            (*pane_id == target).then_some(*ordinal)
        }
        Node::Split { first, second, .. } => {
            if let Some(found) = find_pane_ordinal(first, target, ordinal) {
                return Some(found);
            }
            find_pane_ordinal(second, target, ordinal)
        }
    }
}

fn collect_panes(node: &Node, area: Rect, focus: PaneId, result: &mut Vec<PaneInfo>) {
    match node {
        Node::Pane(id) => {
            result.push(PaneInfo {
                id: *id,
                rect: area,
                // inner_rect is set during render when we know if borders are shown
                inner_rect: area,
                scrollbar_rect: None,
                is_focused: *id == focus,
            });
        }
        Node::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let (a, b) = split_rect(area, *direction, *ratio);
            collect_panes(first, a, focus, result);
            collect_panes(second, b, focus, result);
        }
    }
}

fn collect_splits(node: &Node, area: Rect, path: Vec<bool>, result: &mut Vec<SplitBorder>) {
    if let Node::Split {
        direction,
        ratio,
        first,
        second,
    } = node
    {
        let (a, b) = split_rect(area, *direction, *ratio);
        let pos = match direction {
            Direction::Horizontal => a.x + a.width,
            Direction::Vertical => a.y + a.height,
        };
        result.push(SplitBorder {
            pos,
            direction: *direction,
            area,
            path: path.clone(),
        });
        let mut lp = path.clone();
        lp.push(false);
        collect_splits(first, a, lp, result);
        let mut rp = path;
        rp.push(true);
        collect_splits(second, b, rp, result);
    }
}

fn collect_ids(node: &Node, ids: &mut Vec<PaneId>) {
    match node {
        Node::Pane(id) => ids.push(*id),
        Node::Split { first, second, .. } => {
            collect_ids(first, ids);
            collect_ids(second, ids);
        }
    }
}

fn split_at(node: Node, target: PaneId, direction: Direction, new_id: PaneId) -> Node {
    match node {
        Node::Pane(id) if id == target => Node::Split {
            direction,
            ratio: 0.5,
            first: Box::new(Node::Pane(id)),
            second: Box::new(Node::Pane(new_id)),
        },
        Node::Pane(_) => node,
        Node::Split {
            direction: d,
            ratio,
            first,
            second,
        } => Node::Split {
            direction: d,
            ratio,
            first: Box::new(split_at(*first, target, direction, new_id)),
            second: Box::new(split_at(*second, target, direction, new_id)),
        },
    }
}

fn split_at_with_ratio(
    node: Node,
    target: PaneId,
    direction: Direction,
    new_id: PaneId,
    split_ratio: f32,
) -> Node {
    match node {
        Node::Pane(id) if id == target => Node::Split {
            direction,
            ratio: split_ratio,
            first: Box::new(Node::Pane(id)),
            second: Box::new(Node::Pane(new_id)),
        },
        Node::Pane(_) => node,
        Node::Split {
            direction: d,
            ratio,
            first,
            second,
        } => Node::Split {
            direction: d,
            ratio,
            first: Box::new(split_at_with_ratio(
                *first,
                target,
                direction,
                new_id,
                split_ratio,
            )),
            second: Box::new(split_at_with_ratio(
                *second,
                target,
                direction,
                new_id,
                split_ratio,
            )),
        },
    }
}

fn insert_existing_pane(
    node: Node,
    target: PaneId,
    moved: PaneId,
    direction: Direction,
    new_ratio: f32,
) -> (Node, bool) {
    match node {
        Node::Pane(id) if id == target => (
            Node::Split {
                direction,
                ratio: new_ratio,
                first: Box::new(Node::Pane(id)),
                second: Box::new(Node::Pane(moved)),
            },
            true,
        ),
        Node::Pane(_) => (node, false),
        Node::Split {
            direction: d,
            ratio,
            first,
            second,
        } => {
            let (first_node, inserted) =
                insert_existing_pane(*first, target, moved, direction, new_ratio);
            if inserted {
                return (
                    Node::Split {
                        direction: d,
                        ratio,
                        first: Box::new(first_node),
                        second,
                    },
                    true,
                );
            }
            let (second_node, inserted) =
                insert_existing_pane(*second, target, moved, direction, new_ratio);
            (
                Node::Split {
                    direction: d,
                    ratio,
                    first: Box::new(first_node),
                    second: Box::new(second_node),
                },
                inserted,
            )
        }
    }
}

fn remove_pane(node: Node, target: PaneId) -> Option<Node> {
    match node {
        Node::Pane(id) if id == target => None,
        Node::Pane(_) => Some(node),
        Node::Split {
            direction,
            ratio,
            first,
            second,
        } => match (remove_pane(*first, target), remove_pane(*second, target)) {
            (None, Some(s)) => Some(s),
            (Some(f), None) => Some(f),
            (Some(f), Some(s)) => Some(Node::Split {
                direction,
                ratio,
                first: Box::new(f),
                second: Box::new(s),
            }),
            (None, None) => None,
        },
    }
}

fn set_ratio_at(node: &mut Node, path: &[bool], new_ratio: f32) {
    if let Node::Split {
        ratio,
        first,
        second,
        ..
    } = node
    {
        if path.is_empty() {
            *ratio = new_ratio;
        } else if path[0] {
            set_ratio_at(second, &path[1..], new_ratio);
        } else {
            set_ratio_at(first, &path[1..], new_ratio);
        }
    }
}

fn get_ratio_at(node: &Node, path: &[bool]) -> Option<f32> {
    if let Node::Split {
        ratio,
        first,
        second,
        ..
    } = node
    {
        if path.is_empty() {
            Some(*ratio)
        } else if path[0] {
            get_ratio_at(second, &path[1..])
        } else {
            get_ratio_at(first, &path[1..])
        }
    } else {
        None
    }
}

fn split_rect(area: Rect, direction: Direction, ratio: f32) -> (Rect, Rect) {
    match direction {
        Direction::Horizontal => {
            let first_w = ((area.width as f32) * ratio).round() as u16;
            let second_w = area.width.saturating_sub(first_w);
            (
                Rect::new(area.x, area.y, first_w, area.height),
                Rect::new(area.x + first_w, area.y, second_w, area.height),
            )
        }
        Direction::Vertical => {
            let first_h = ((area.height as f32) * ratio).round() as u16;
            let second_h = area.height.saturating_sub(first_h);
            (
                Rect::new(area.x, area.y, area.width, first_h),
                Rect::new(area.x, area.y + first_h, area.width, second_h),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u32) -> PaneId {
        PaneId::from_raw(id)
    }

    fn pane_rect(layout: &TileLayout, pane_id: PaneId) -> Rect {
        layout
            .panes(Rect::new(0, 0, 100, 40))
            .into_iter()
            .find_map(|info| (info.id == pane_id).then_some(info.rect))
            .expect("pane should exist")
    }

    fn split_snapshot(layout: &TileLayout) -> Vec<(Direction, f32)> {
        fn collect(node: &Node, out: &mut Vec<(Direction, f32)>) {
            match node {
                Node::Pane(_) => {}
                Node::Split {
                    direction,
                    ratio,
                    first,
                    second,
                } => {
                    out.push((*direction, *ratio));
                    collect(first, out);
                    collect(second, out);
                }
            }
        }

        let mut out = Vec::new();
        collect(layout.root(), &mut out);
        out
    }

    #[test]
    fn resize_outer_edges_shrink_focused_pane() {
        let (mut horizontal, left) = TileLayout::new();
        horizontal.split_focused(Direction::Horizontal);
        horizontal.focus_pane(left);
        horizontal.resize_focused(NavDirection::Left, 0.05, Rect::new(0, 0, 100, 40));
        let split = split_snapshot(&horizontal)[0];
        assert_eq!(split.0, Direction::Horizontal);
        assert!((split.1 - 0.45).abs() < f32::EPSILON);

        let (mut horizontal, _left) = TileLayout::new();
        let right = horizontal.split_focused(Direction::Horizontal);
        horizontal.focus_pane(right);
        horizontal.resize_focused(NavDirection::Right, 0.05, Rect::new(0, 0, 100, 40));
        let split = split_snapshot(&horizontal)[0];
        assert_eq!(split.0, Direction::Horizontal);
        assert!((split.1 - 0.55).abs() < f32::EPSILON);

        let (mut vertical, top) = TileLayout::new();
        vertical.split_focused(Direction::Vertical);
        vertical.focus_pane(top);
        vertical.resize_focused(NavDirection::Up, 0.05, Rect::new(0, 0, 100, 40));
        let split = split_snapshot(&vertical)[0];
        assert_eq!(split.0, Direction::Vertical);
        assert!((split.1 - 0.45).abs() < f32::EPSILON);

        let (mut vertical, _top) = TileLayout::new();
        let bottom = vertical.split_focused(Direction::Vertical);
        vertical.focus_pane(bottom);
        vertical.resize_focused(NavDirection::Down, 0.05, Rect::new(0, 0, 100, 40));
        let split = split_snapshot(&vertical)[0];
        assert_eq!(split.0, Direction::Vertical);
        assert!((split.1 - 0.55).abs() < f32::EPSILON);
    }

    #[test]
    fn resize_outer_edge_falls_back_to_horizontal_ancestor_split() {
        let mut layout = TileLayout::from_saved(
            Node::Split {
                direction: Direction::Horizontal,
                ratio: 0.6,
                first: Box::new(Node::Split {
                    direction: Direction::Vertical,
                    ratio: 0.5,
                    first: Box::new(Node::Pane(pane(1))),
                    second: Box::new(Node::Pane(pane(2))),
                }),
                second: Box::new(Node::Pane(pane(3))),
            },
            pane(1),
        );
        let before = pane_rect(&layout, pane(1));

        layout.resize_focused(NavDirection::Left, 0.05, Rect::new(0, 0, 100, 40));

        let after = pane_rect(&layout, pane(1));
        assert_eq!(after.height, before.height);
        assert!(after.width < before.width);
        let splits = split_snapshot(&layout);
        assert_eq!(splits[0].0, Direction::Horizontal);
        assert!((splits[0].1 - 0.55).abs() < f32::EPSILON);
        assert_eq!(splits[1], (Direction::Vertical, 0.5));
    }

    #[test]
    fn resize_outer_edge_falls_back_to_vertical_ancestor_split() {
        let mut layout = TileLayout::from_saved(
            Node::Split {
                direction: Direction::Vertical,
                ratio: 0.6,
                first: Box::new(Node::Split {
                    direction: Direction::Horizontal,
                    ratio: 0.5,
                    first: Box::new(Node::Pane(pane(1))),
                    second: Box::new(Node::Pane(pane(2))),
                }),
                second: Box::new(Node::Pane(pane(3))),
            },
            pane(1),
        );
        let before = pane_rect(&layout, pane(1));

        layout.resize_focused(NavDirection::Up, 0.05, Rect::new(0, 0, 100, 40));

        let after = pane_rect(&layout, pane(1));
        assert_eq!(after.width, before.width);
        assert!(after.height < before.height);
        let splits = split_snapshot(&layout);
        assert_eq!(splits[0].0, Direction::Vertical);
        assert!((splits[0].1 - 0.55).abs() < f32::EPSILON);
        assert_eq!(splits[1], (Direction::Horizontal, 0.5));
    }
}
