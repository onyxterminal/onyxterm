#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{DropZone, PanelId};
use gpui::{Bounds, Pixels, Point};

const EDGE_BAND: f32 = 0.3;
pub const DRAG_START_DISTANCE_PX: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    Pane(PanelId, DropZone),
    Tab(usize),
}

pub fn drop_zone_at(x_fraction: f32, y_fraction: f32) -> DropZone {
    let edges = [
        (x_fraction, DropZone::Left),
        (1.0 - x_fraction, DropZone::Right),
        (y_fraction, DropZone::Top),
        (1.0 - y_fraction, DropZone::Bottom),
    ];
    let (distance, zone) = edges.into_iter().fold((f32::INFINITY, DropZone::Center), |nearest, edge| {
        if edge.0 < nearest.0 {
            edge
        } else {
            nearest
        }
    });
    match distance < EDGE_BAND {
        true => zone,
        false => DropZone::Center,
    }
}

pub fn pane_drop_target(
    cursor: Point<Pixels>,
    panes: &[(PanelId, Bounds<Pixels>)],
    source: PanelId,
) -> Option<(PanelId, DropZone)> {
    let (panel, bounds) = panes.iter().find(|(_, bounds)| bounds.contains(&cursor))?;
    if *panel == source || bounds.size.width <= Pixels::ZERO || bounds.size.height <= Pixels::ZERO {
        return None;
    }
    let local = cursor - bounds.origin;
    Some((*panel, drop_zone_at(local.x / bounds.size.width, local.y / bounds.size.height)))
}

pub fn tab_drop_target(cursor: Point<Pixels>, tabs: &[(usize, Bounds<Pixels>)], source_tab: usize) -> Option<usize> {
    tabs.iter().find(|(index, bounds)| *index != source_tab && bounds.contains(&cursor)).map(|(index, _)| *index)
}

pub fn has_left_dead_zone(press: Point<Pixels>, cursor: Point<Pixels>) -> bool {
    let delta = cursor - press;
    f32::from(delta.x).hypot(f32::from(delta.y)) >= DRAG_START_DISTANCE_PX
}
