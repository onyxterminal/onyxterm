use crate::error::DomainError;
use crate::id::PanelId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub fn opposite(self) -> Self {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
}

pub type SplitRatio = f32;

const DROP_SPLIT_RATIO: SplitRatio = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

impl DropZone {
    fn axis(self) -> Axis {
        match self {
            DropZone::Left | DropZone::Right | DropZone::Center => Axis::Horizontal,
            DropZone::Top | DropZone::Bottom => Axis::Vertical,
        }
    }

    fn places_before(self) -> bool {
        matches!(self, DropZone::Left | DropZone::Top)
    }
}

pub mod axis_helpers {
    use super::Axis;

    pub const DEFAULT_HORIZONTAL: Axis = Axis::Horizontal;
    pub const DEFAULT_VERTICAL: Axis = Axis::Vertical;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutNode {
    Leaf { panel_id: PanelId },
    Split {
        axis: Axis,
        ratio: SplitRatio,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn leaf(panel_id: PanelId) -> Self {
        Self::Leaf { panel_id }
    }

    pub fn collect_leaves(&self, out: &mut Vec<PanelId>) {
        match self {
            Self::Leaf { panel_id } => out.push(*panel_id),
            Self::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    pub fn leaves(&self) -> Vec<PanelId> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    pub fn find_leaf<F: FnMut(PanelId) -> bool>(&self, pred: &mut F) -> Option<&Self> {
        match self {
            Self::Leaf { panel_id } => {
                if pred(*panel_id) {
                    Some(self)
                } else {
                    None
                }
            }
            Self::Split { first, second, .. } => first
                .find_leaf(pred)
                .or_else(|| second.find_leaf(pred)),
        }
    }

    pub fn find_leaf_mut<F: FnMut(PanelId) -> bool>(
        &mut self,
        pred: &mut F,
    ) -> Option<&mut Self> {
        match self {
            Self::Leaf { panel_id } => {
                if pred(*panel_id) {
                    Some(self)
                } else {
                    None
                }
            }
            Self::Split { first, second, .. } => first
                .find_leaf_mut(pred)
                .or_else(|| second.find_leaf_mut(pred)),
        }
    }

    pub fn split_with(
        &mut self,
        target: PanelId,
        axis: Axis,
        ratio: SplitRatio,
        new_panel: PanelId,
    ) -> Result<(), DomainError> {
        validate_ratio(ratio)?;
        let node = self
            .find_leaf_mut(&mut |pid| pid == target)
            .ok_or_else(|| DomainError::PanelNotFound(target.to_string()))?;
        let old = std::mem::replace(node, LayoutNode::leaf(PanelId::new()));
        *node = LayoutNode::Split {
            axis,
            ratio,
            first: Box::new(old),
            second: Box::new(LayoutNode::Leaf {
                panel_id: new_panel,
            }),
        };
        Ok(())
    }

    pub fn replace_leaf<F: FnMut(PanelId) -> bool>(
        &mut self,
        pred: &mut F,
        replacement: LayoutNode,
    ) -> Result<bool, DomainError> {
        match self {
            Self::Leaf { panel_id } => {
                if pred(*panel_id) {
                    *self = replacement;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Self::Split { first, second, .. } => {
                if first.replace_leaf(pred, replacement.clone())? {
                    return Ok(true);
                }
                second.replace_leaf(pred, replacement)
            }
        }
    }

    pub fn close_leaf<F: FnMut(PanelId) -> bool>(
        &mut self,
        mut pred: &mut F,
    ) -> Result<bool, DomainError> {
        match self {
            Self::Leaf { panel_id } => {
                if pred(*panel_id) {
                    return Err(DomainError::InvalidLayout(
                        "cannot close root leaf; replace whole layout".into(),
                    ));
                }
                Ok(false)
            }
            Self::Split { first, second, .. } => {
                let first_id = first.leaf_id();
                let first_match = first_id.is_some_and(&mut pred);
                if first_match {
                    let survivor = std::mem::replace(second.as_mut(), LayoutNode::leaf(PanelId::new()));
                    *self = survivor;
                    return Ok(true);
                }
                let second_id = second.leaf_id();
                let second_match = second_id.is_some_and(&mut pred);
                if second_match {
                    let survivor = std::mem::replace(first.as_mut(), LayoutNode::leaf(PanelId::new()));
                    *self = survivor;
                    return Ok(true);
                }
                if first.close_leaf(pred)? {
                    return Ok(true);
                }
                second.close_leaf(pred)
            }
        }
    }

    pub fn move_leaf(&mut self, source: PanelId, target: PanelId, zone: DropZone) -> Result<(), DomainError> {
        let leaves = self.leaves();
        if source == target || !leaves.contains(&source) || !leaves.contains(&target) {
            return Err(DomainError::InvalidLayout(format!("cannot move {source} onto {target}")));
        }
        if zone == DropZone::Center {
            self.swap_leaves(source, target);
            return Ok(());
        }
        self.close_leaf(&mut |panel| panel == source)?;
        self.insert_beside(target, source, zone)
    }

    pub fn insert_beside(&mut self, target: PanelId, panel: PanelId, zone: DropZone) -> Result<(), DomainError> {
        let node = self
            .find_leaf_mut(&mut |id| id == target)
            .ok_or_else(|| DomainError::PanelNotFound(target.to_string()))?;
        let (first, second) = match zone.places_before() {
            true => (panel, target),
            false => (target, panel),
        };
        *node = LayoutNode::Split {
            axis: zone.axis(),
            ratio: DROP_SPLIT_RATIO,
            first: Box::new(LayoutNode::leaf(first)),
            second: Box::new(LayoutNode::leaf(second)),
        };
        Ok(())
    }

    fn swap_leaves(&mut self, a: PanelId, b: PanelId) {
        match self {
            Self::Leaf { panel_id } if *panel_id == a => *panel_id = b,
            Self::Leaf { panel_id } if *panel_id == b => *panel_id = a,
            Self::Leaf { .. } => {}
            Self::Split { first, second, .. } => {
                first.swap_leaves(a, b);
                second.swap_leaves(a, b);
            }
        }
    }

    fn leaf_id(&self) -> Option<PanelId> {
        match self {
            Self::Leaf { panel_id } => Some(*panel_id),
            Self::Split { .. } => None,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        validate_split_ratios(self)?;
        validate_no_cycle(self)
    }
}

pub fn validate_ratio(ratio: SplitRatio) -> Result<(), DomainError> {
    if !(0.0 < ratio && ratio < 1.0) {
        return Err(DomainError::InvalidRatio(ratio));
    }
    Ok(())
}

fn validate_split_ratios(node: &LayoutNode) -> Result<(), DomainError> {
    match node {
        LayoutNode::Leaf { .. } => Ok(()),
        LayoutNode::Split {
            ratio, first, second, ..
        } => {
            validate_ratio(*ratio)?;
            validate_split_ratios(first)?;
            validate_split_ratios(second)?;
            Ok(())
        }
    }
}

fn validate_no_cycle(node: &LayoutNode) -> Result<(), DomainError> {
    fn walk(node: &LayoutNode, seen: &mut Vec<PanelId>) -> bool {
        match node {
            LayoutNode::Leaf { panel_id } => {
                if seen.contains(panel_id) {
                    return false;
                }
                seen.push(*panel_id);
                true
            }
            LayoutNode::Split { first, second, .. } => {
                walk(first, seen) && walk(second, seen)
            }
        }
    }
    let mut seen = Vec::new();
    if walk(node, &mut seen) {
        Ok(())
    } else {
        Err(DomainError::LayoutCycle)
    }
}
