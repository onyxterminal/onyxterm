use crate::error::PersistenceError;
use crate::sqlite::SqlitePool;
use domain::{Agent, LayoutNode, Panel, PanelId, Tab, Workspace, WorkspaceId};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct RestoreSnapshot {
    pub workspaces: Vec<Workspace>,
    pub tabs: Vec<Tab>,
    pub panels: Vec<Panel>,
    pub agents: Vec<Agent>,
    pub app_settings: Vec<(String, String)>,
    pub warnings: Vec<RestoreWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreWarning {
    TabDropped { workspace: WorkspaceId, tab: domain::TabId, reason: String },
}

pub(crate) type RepairOutcome = (Vec<Tab>, Vec<Panel>, Vec<RestoreWarning>);

pub(crate) fn load_with_repair(
    pool: &SqlitePool,
    raw_tabs: Vec<(WorkspaceId, Tab, Vec<Panel>)>,
) -> Result<RepairOutcome, PersistenceError> {
    let mut tabs_out: Vec<Tab> = Vec::new();
    let mut panels_out: Vec<Panel> = Vec::new();
    let mut warnings: Vec<RestoreWarning> = Vec::new();
    for (workspace, tab, panels) in raw_tabs {
        let mut repaired = tab.layout.clone();
        if repaired.validate().is_err() {
            match repair_layout(&mut repaired) {
                Ok(()) => {
                    warn!(
                        workspace = %workspace,
                        tab = %tab.id,
                        "repaired layout during restore"
                    );
                }
                Err(reason) => {
                    warn!(
                        workspace = %workspace,
                        tab = %tab.id,
                        reason = %reason,
                        "dropping tab with unrepairable layout"
                    );
                    pool.with_conn(|conn| {
                        let _ = conn.execute("DELETE FROM tabs WHERE id = ?1", [tab.id.to_string()]);
                    });
                    warnings.push(RestoreWarning::TabDropped {
                        workspace,
                        tab: tab.id,
                        reason,
                    });
                    continue;
                }
            }
        }
        let used_panels: std::collections::HashSet<PanelId> =
            repaired.leaves().into_iter().collect();
        let kept_panels: Vec<Panel> = panels
            .into_iter()
            .filter(|p| used_panels.contains(&p.id))
            .collect();
        let mut new_tab = tab;
        new_tab.layout = repaired;
        tabs_out.push(new_tab);
        panels_out.extend(kept_panels);
    }
    Ok((tabs_out, panels_out, warnings))
}

fn repair_layout(layout: &mut LayoutNode) -> Result<(), String> {
    for _ in 0..256 {
        if layout.validate().is_ok() {
            return Ok(());
        }
        let leaves = layout.leaves();
        let mut duplicate: Option<PanelId> = None;
        for (i, leaf) in leaves.iter().enumerate() {
            if leaves.iter().skip(i + 1).any(|other| other == leaf) {
                duplicate = Some(*leaf);
                break;
            }
        }
        match duplicate {
            Some(panel) => match layout.close_leaf(&mut |id| id == panel) {
                Ok(true) => continue,
                Ok(false) => return Err("close_leaf returned false".into()),
                Err(_) => return Err("close_leaf refused".into()),
            },
            None => {
                let mut bad_ratio: Option<f32> = None;
                walk_for_bad_ratio(layout, &mut bad_ratio);
                if let Some(r) = bad_ratio {
                    fix_ratios(layout, r);
                    continue;
                }
                return Err("invalid layout but nothing to close".into());
            }
        }
    }
    Err("repair iteration limit exceeded".into())
}

fn walk_for_bad_ratio(node: &LayoutNode, out: &mut Option<f32>) {
    match node {
        LayoutNode::Leaf { .. } => {}
        LayoutNode::Split { ratio, first, second, .. } => {
            if !(0.0 < *ratio && *ratio < 1.0) && out.is_none() {
                *out = Some(*ratio);
            }
            walk_for_bad_ratio(first, out);
            walk_for_bad_ratio(second, out);
        }
    }
}

fn fix_ratios(node: &mut LayoutNode, ratio: f32) {
    match node {
        LayoutNode::Leaf { .. } => {}
        LayoutNode::Split { ratio: r, first, second, .. } => {
            if (*r - ratio).abs() < f32::EPSILON {
                *r = 0.5;
            }
            fix_ratios(first, ratio);
            fix_ratios(second, ratio);
        }
    }
}

