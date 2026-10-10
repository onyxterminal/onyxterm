#![deny(warnings)]
#![forbid(unsafe_code)]

use domain::{Axis, DomainError, LayoutNode, PanelId};

const SPLIT_RATIO: f32 = 0.5;

#[derive(Debug, Clone)]
pub struct Tab {
    layout: LayoutNode,
    focused: PanelId,
}

impl Tab {
    pub fn new(panel: PanelId) -> Self {
        Self { layout: LayoutNode::leaf(panel), focused: panel }
    }

    pub fn layout(&self) -> &LayoutNode {
        &self.layout
    }

    pub fn focused(&self) -> PanelId {
        self.focused
    }

    pub fn contains(&self, panel: PanelId) -> bool {
        self.layout.leaves().contains(&panel)
    }

    pub fn split_focused(&mut self, axis: Axis, new_panel: PanelId) -> Result<(), DomainError> {
        self.layout.split_with(self.focused, axis, SPLIT_RATIO, new_panel)?;
        self.focused = new_panel;
        Ok(())
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

    fn is_single_panel(&self) -> bool {
        matches!(self.layout, LayoutNode::Leaf { .. })
    }

    fn remove_panel(&mut self, panel: PanelId) -> Result<(), DomainError> {
        self.layout.close_leaf(&mut |id| id == panel)?;
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

    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
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

    pub fn select_previous(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
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
