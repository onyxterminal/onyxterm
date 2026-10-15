#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::theme::Theme;
use domain::PanelId;
use gpui::Hsla;
use std::collections::HashMap;

const FULL_OPACITY: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IslandRole {
    pub is_focused: bool,
    pub is_split: bool,
}

impl IslandRole {
    fn shows_focus_ring(self) -> bool {
        self.is_split && self.is_focused
    }

    fn is_dimmed(self, theme: &Theme) -> bool {
        theme.island.dim_unfocused && self.is_split && !self.is_focused
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IslandLook {
    pub edge_px: f32,
    pub edge_color: Hsla,
    pub content_opacity: f32,
}

impl IslandLook {
    pub fn for_role(theme: &Theme, role: IslandRole) -> Self {
        let (edge_px, edge_color) = match role.shows_focus_ring() {
            true => (theme.island.focus_ring_px, theme.border_focus()),
            false => (theme.island.unfocused_border_px, theme.border_default()),
        };
        let content_opacity = match role.is_dimmed(theme) {
            true => theme.island.unfocused_opacity,
            false => FULL_OPACITY,
        };
        Self { edge_px, edge_color, content_opacity }
    }

    pub fn is_dimmed(&self) -> bool {
        self.content_opacity < FULL_OPACITY
    }
}

#[derive(Debug, Default)]
pub struct IslandLooks {
    looks: HashMap<PanelId, IslandLook>,
}

impl IslandLooks {
    pub fn remember(&mut self, current: HashMap<PanelId, IslandLook>, ghosts: &[PanelId]) {
        self.looks.retain(|panel, _| ghosts.contains(panel));
        self.looks.extend(current);
    }

    pub fn get(&self, panel: PanelId) -> Option<IslandLook> {
        self.looks.get(&panel).copied()
    }
}
