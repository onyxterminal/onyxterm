#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::tabs::{SplitPath, SplitSide};
use domain::{Axis, Direction, LayoutNode, PanelId};
use gpui::{point, px, size, Bounds, Pixels, Point};
use terminal_core::GridSize;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn to_bounds(self, origin: Point<Pixels>) -> Bounds<Pixels> {
        Bounds::new(origin + point(px(self.x), px(self.y)), size(px(self.width), px(self.height)))
    }

    fn start(self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.x,
            Axis::Vertical => self.y,
        }
    }

    fn end(self, axis: Axis) -> f32 {
        self.start(axis) + self.extent(axis)
    }

    fn extent(self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }

    fn slice(self, axis: Axis, offset: f32, extent: f32) -> Rect {
        match axis {
            Axis::Horizontal => Rect { x: self.x + offset, width: extent, ..self },
            Axis::Vertical => Rect { y: self.y + offset, height: extent, ..self },
        }
    }

    fn outer_edge(self, axis: Axis, side: SplitSide) -> Rect {
        let offset = match side {
            SplitSide::First => 0.0,
            SplitSide::Second => self.extent(axis),
        };
        self.slice(axis, offset, 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Island {
    pub panel: PanelId,
    pub rect: Rect,
    pub collapsed: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub path: SplitPath,
    pub axis: Axis,
    pub rect: Rect,
    pub container: Rect,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaneLayout {
    pub islands: Vec<Island>,
    pub gaps: Vec<Gap>,
}

#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    pub gap: f32,
    pub min_pane: f32,
}

struct Slot {
    rect: Rect,
    collapsed: Rect,
    path: SplitPath,
}

struct Placer {
    spacing: Spacing,
    layout: PaneLayout,
}

pub fn compute_layout(node: &LayoutNode, area: Rect, spacing: Spacing) -> PaneLayout {
    let mut placer = Placer { spacing, layout: PaneLayout::default() };
    placer.place(node, Slot { rect: area, collapsed: area, path: Vec::new() });
    placer.layout
}

pub type PinnedWidth<'a> = &'a dyn Fn(PanelId) -> Option<f32>;

pub fn pin_widths(node: &LayoutNode, area: Rect, spacing: Spacing, pinned: PinnedWidth) -> LayoutNode {
    let LayoutNode::Split { axis, ratio, first, second } = node else {
        return node.clone();
    };
    let extent = area.extent(*axis);
    let ratio = match axis {
        Axis::Horizontal => pinned_ratio(first, second, extent, spacing, pinned).unwrap_or(*ratio),
        Axis::Vertical => *ratio,
    };
    let lead = first_extent(extent, ratio, spacing);
    let gap = spacing.gap.min(extent - lead).max(0.0);
    let first_rect = area.slice(*axis, 0.0, lead);
    let second_rect = area.slice(*axis, lead + gap, (extent - lead - gap).max(0.0));
    LayoutNode::Split {
        axis: *axis,
        ratio,
        first: Box::new(pin_widths(first, first_rect, spacing, pinned)),
        second: Box::new(pin_widths(second, second_rect, spacing, pinned)),
    }
}

fn pinned_ratio(
    first: &LayoutNode,
    second: &LayoutNode,
    extent: f32,
    spacing: Spacing,
    pinned: PinnedWidth,
) -> Option<f32> {
    if extent <= 0.0 {
        return None;
    }
    let width_of = |node: &LayoutNode| match node {
        LayoutNode::Leaf { panel_id } => pinned(*panel_id),
        LayoutNode::Split { .. } => None,
    };
    if let Some(width) = width_of(first) {
        return Some(width / extent);
    }
    width_of(second).map(|width| (extent - spacing.gap - width) / extent)
}

const UNIT_AREA: Rect = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
const UNSPACED: Spacing = Spacing { gap: 0.0, min_pane: 0.0 };
const EDGE_TOLERANCE: f32 = 1e-4;

pub fn neighbor_toward(node: &LayoutNode, from: PanelId, direction: Direction) -> Option<PanelId> {
    let islands = compute_layout(node, UNIT_AREA, UNSPACED).islands;
    let origin = islands.iter().find(|island| island.panel == from)?.rect;
    islands
        .iter()
        .filter(|island| island.panel != from)
        .filter_map(|island| {
            let distance = distance_toward(origin, island.rect, direction)?;
            let overlap = cross_overlap(origin, island.rect, direction.axis());
            (overlap > EDGE_TOLERANCE).then_some((island.panel, distance, overlap))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(b.2.total_cmp(&a.2)))
        .map(|(panel, _, _)| panel)
}

fn distance_toward(origin: Rect, candidate: Rect, direction: Direction) -> Option<f32> {
    let axis = direction.axis();
    let distance = match direction.is_forward() {
        true => candidate.start(axis) - origin.end(axis),
        false => origin.start(axis) - candidate.end(axis),
    };
    (distance > -EDGE_TOLERANCE).then_some(distance)
}

fn cross_overlap(a: Rect, b: Rect, axis: Axis) -> f32 {
    let cross = axis.opposite();
    a.end(cross).min(b.end(cross)) - a.start(cross).max(b.start(cross))
}

pub fn first_extent(extent: f32, ratio: f32, spacing: Spacing) -> f32 {
    let available = (extent - spacing.gap).max(0.0);
    let wanted = extent * ratio;
    if available < spacing.min_pane * 2.0 {
        return wanted.clamp(0.0, available);
    }
    wanted.clamp(spacing.min_pane, available - spacing.min_pane)
}

impl Placer {
    fn place(&mut self, node: &LayoutNode, slot: Slot) {
        let (axis, ratio, first, second) = match node {
            LayoutNode::Leaf { panel_id } => {
                let island = Island { panel: *panel_id, rect: slot.rect, collapsed: slot.collapsed };
                self.layout.islands.push(island);
                return;
            }
            LayoutNode::Split { axis, ratio, first, second } => (*axis, *ratio, first, second),
        };
        let extent = slot.rect.extent(axis);
        let lead = first_extent(extent, ratio, self.spacing);
        let gap = self.spacing.gap.min(extent - lead).max(0.0);
        let first_rect = slot.rect.slice(axis, 0.0, lead);
        let gap_rect = slot.rect.slice(axis, lead, gap);
        let second_rect = slot.rect.slice(axis, lead + gap, (extent - lead - gap).max(0.0));
        self.layout.gaps.push(Gap { path: slot.path.clone(), axis, rect: gap_rect, container: slot.rect });
        self.place(first, child_slot(first_rect, axis, &slot.path, SplitSide::First));
        self.place(second, child_slot(second_rect, axis, &slot.path, SplitSide::Second));
    }
}

fn child_slot(rect: Rect, axis: Axis, parent: &[SplitSide], side: SplitSide) -> Slot {
    let path = parent.iter().copied().chain(std::iter::once(side)).collect();
    Slot { rect, collapsed: rect.outer_edge(axis, side), path }
}

pub fn grid_changes<C, G>(islands: &[Island], current: C, grid_for: G) -> Vec<(PanelId, GridSize)>
where
    C: Fn(PanelId) -> Option<GridSize>,
    G: Fn(Rect) -> GridSize,
{
    islands
        .iter()
        .filter_map(|island| {
            let size = current(island.panel)?;
            let wanted = grid_for(island.rect);
            (size != wanted).then_some((island.panel, wanted))
        })
        .collect()
}
