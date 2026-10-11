#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::pane_drop::DropTarget;
use crate::pane_layout::neighbor_toward;
use domain::{Axis, Direction, DomainError, DropZone, LayoutNode, PanelId, TabId};

pub const SPLIT_RATIO: f32 = 0.5;
pub const RESIZE_STEP: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SplitSide {
    First,
    Second,
}

pub type SplitPath = Vec<SplitSide>;

pub fn numbered_tab_label(index: usize) -> String {
    format!("#{}", index + 1)
}

pub fn clamp_split_ratio(offset: f32, extent: f32, min_pane: f32) -> f32 {
    if extent <= min_pane * 2.0 {
        return SPLIT_RATIO;
    }
    let min_ratio = min_pane / extent;
    (offset / extent).clamp(min_ratio, 1.0 - min_ratio)
}

fn split_at_mut<'a>(node: &'a mut LayoutNode, path: &[SplitSide]) -> Option<&'a mut LayoutNode> {
    let Some((side, rest)) = path.split_first() else {
        return Some(node);
    };
    let LayoutNode::Split { first, second, .. } = node else {
        return None;
    };
    let child = match side {
        SplitSide::First => first,
        SplitSide::Second => second,
    };
    split_at_mut(child, rest)
}

#[derive(Debug, Clone)]
pub struct Tab {
    id: TabId,
    layout: LayoutNode,
    focused: PanelId,
    is_maximized: bool,
    name: Option<String>,
}

impl Tab {
    pub fn new(panel: PanelId) -> Self {
        Self { id: TabId::new(), layout: LayoutNode::leaf(panel), focused: panel, is_maximized: false, name: None }
    }

    pub fn restore(id: TabId, layout: LayoutNode, focused: Option<PanelId>) -> Self {
        let leaves = layout.leaves();
        let focused = focused.filter(|panel| leaves.contains(panel)).unwrap_or(leaves[0]);
        Self { id, layout, focused, is_maximized: false, name: None }
    }

    pub fn id(&self) -> TabId {
        self.id
    }

    pub fn layout(&self) -> &LayoutNode {
        &self.layout
    }

    pub fn focused(&self) -> PanelId {
        self.focused
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn label(&self, index: usize) -> String {
        self.name().map_or_else(|| numbered_tab_label(index), str::to_string)
    }

    pub fn rename(&mut self, name: Option<String>) {
        self.name = name.map(|name| name.trim().to_string()).filter(|name| !name.is_empty());
    }

    pub fn is_maximized(&self) -> bool {
        self.is_maximized && !self.is_single_panel()
    }

    pub fn toggle_maximized(&mut self) {
        self.is_maximized = !self.is_maximized;
    }

    pub fn contains(&self, panel: PanelId) -> bool {
        self.layout.leaves().contains(&panel)
    }

    pub fn split_focused(&mut self, axis: Axis, new_panel: PanelId) -> Result<(), DomainError> {
        self.layout.split_with(self.focused, axis, SPLIT_RATIO, new_panel)?;
        self.focused = new_panel;
        self.is_maximized = false;
        Ok(())
    }

    pub fn set_split_ratio(&mut self, path: &[SplitSide], new_ratio: f32) {
        if let Some(LayoutNode::Split { ratio, .. }) = split_at_mut(&mut self.layout, path) {
            *ratio = new_ratio;
        }
    }

    pub fn move_panel(&mut self, source: PanelId, target: PanelId, zone: DropZone) -> Result<(), DomainError> {
        self.layout.move_leaf(source, target, zone)?;
        self.focused = source;
        self.is_maximized = false;
        Ok(())
    }

    pub fn insert_beside_focused(&mut self, panel: PanelId, zone: DropZone) -> Result<(), DomainError> {
        self.layout.insert_beside(self.focused, panel, zone)?;
        self.focused = panel;
        self.is_maximized = false;
        Ok(())
    }

    fn adopt_panel(&mut self, panel: PanelId) -> Result<(), DomainError> {
        self.insert_beside_focused(panel, DropZone::Right)
    }

    pub fn focus(&mut self, panel: PanelId) {
        if self.contains(panel) {
            self.focused = panel;
        }
    }

    pub fn focus_next(&mut self) {
        let leaves = self.layout.leaves();
        let current = leaves.iter().position(|id| *id == self.focused).unwrap_or(0);
        self.focused = leaves[(current + 1) % leaves.len()];
    }

    pub fn focus_toward(&mut self, direction: Direction) {
        if let Some(panel) = neighbor_toward(&self.layout, self.focused, direction) {
            self.focused = panel;
        }
    }

    pub fn swap_toward(&mut self, direction: Direction) -> Result<(), DomainError> {
        let Some(target) = neighbor_toward(&self.layout, self.focused, direction) else {
            return Ok(());
        };
        self.move_panel(self.focused, target, DropZone::Center)
    }

    pub fn resize_toward(&mut self, direction: Direction) {
        self.layout.resize_toward(self.focused, direction, RESIZE_STEP);
    }

    pub fn arrange_grid(&mut self) {
        self.layout = self.layout.to_grid();
        self.is_maximized = false;
    }

    pub fn equalize(&mut self) {
        self.layout.equalize();
    }

    fn is_single_panel(&self) -> bool {
        matches!(self.layout, LayoutNode::Leaf { .. })
    }

    fn remove_panel(&mut self, panel: PanelId) -> Result<(), DomainError> {
        self.layout.close_leaf(&mut |id| id == panel)?;
        self.is_maximized = false;
        if self.focused == panel {
            self.focused = self.layout.leaves()[0];
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
}

impl Tabs {
    pub fn restore(tabs: Vec<Tab>, active: usize) -> Self {
        let active = active.min(tabs.len().saturating_sub(1));
        Self { tabs, active }
    }

    pub fn position_of(&self, panel: PanelId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.contains(panel))
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    pub fn labels(&self) -> Vec<String> {
        self.tabs.iter().enumerate().map(|(index, tab)| tab.label(index)).collect()
    }

    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Tab> {
        self.tabs.get_mut(index)
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn open(&mut self, panel: PanelId) {
        self.tabs.push(Tab::new(panel));
        self.active = self.tabs.len() - 1;
    }

    pub fn select_next(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub fn activate(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub fn select_previous(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    pub fn drop_panel(&mut self, source: PanelId, target: DropTarget) -> Result<(), DomainError> {
        match target {
            DropTarget::Tab(index) => self.move_panel_to_tab(source, index),
            DropTarget::Pane(panel, zone) => {
                let index = self.position_of(panel).ok_or_else(|| DomainError::PanelNotFound(panel.to_string()))?;
                self.tabs[index].move_panel(source, panel, zone)
            }
        }
    }

    pub fn move_panel_to_new_tab(&mut self, panel: PanelId) -> Result<(), DomainError> {
        let source = self.position_of(panel).ok_or_else(|| DomainError::PanelNotFound(panel.to_string()))?;
        if self.tabs[source].is_single_panel() {
            return Err(DomainError::InvalidLayout(format!("{panel} is already alone in its tab")));
        }
        self.tabs[source].remove_panel(panel)?;
        self.open(panel);
        Ok(())
    }

    pub fn move_panel_to_tab(&mut self, panel: PanelId, target: usize) -> Result<(), DomainError> {
        let source = self.position_of(panel).ok_or_else(|| DomainError::PanelNotFound(panel.to_string()))?;
        if source == target || target >= self.tabs.len() {
            return Err(DomainError::InvalidLayout(format!("cannot move {panel} into tab {target}")));
        }
        let target_id = self.tabs[target].id();
        self.close_panel(panel)?;
        let index = self.tabs.iter().position(|tab| tab.id() == target_id).ok_or_else(|| {
            DomainError::TabNotFound(target_id.to_string())
        })?;
        self.tabs[index].adopt_panel(panel)?;
        self.active = index;
        Ok(())
    }

    pub fn close_panel(&mut self, panel: PanelId) -> Result<(), DomainError> {
        let Some(index) = self.tabs.iter().position(|tab| tab.contains(panel)) else {
            return Err(DomainError::PanelNotFound(panel.to_string()));
        };
        if !self.tabs[index].is_single_panel() {
            return self.tabs[index].remove_panel(panel);
        }
        self.tabs.remove(index);
        if self.active > index || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        Ok(())
    }
}
