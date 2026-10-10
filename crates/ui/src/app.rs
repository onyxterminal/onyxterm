#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::CommandRegistry;
use crate::keymap::Keymap;
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct WorkspaceStub {
    pub id: String,
    pub name: String,
}

#[derive(Debug)]
pub struct AppState {
    pub theme: Theme,
    pub registry: CommandRegistry,
    pub keymap: Keymap,
    pub palette_visible: bool,
    pub palette_query: String,
    pub palette_selection: usize,
    pub workspaces: Vec<WorkspaceStub>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            theme: Theme::load_default().expect("github-light theme must load"),
            registry: CommandRegistry::with_defaults(),
            keymap: Keymap::default(),
            palette_visible: false,
            palette_query: String::new(),
            palette_selection: 0,
            workspaces: vec![
                WorkspaceStub {
                    id: "ws_personal".into(),
                    name: "Personal".into(),
                },
                WorkspaceStub {
                    id: "ws_infra".into(),
                    name: "Infrastructure".into(),
                },
                WorkspaceStub {
                    id: "ws_project".into(),
                    name: "Project".into(),
                },
            ],
        }
    }

    pub fn filtered_commands(&self) -> Vec<crate::command::CommandId> {
        self.registry
            .search(&self.palette_query)
            .into_iter()
            .map(|c| c.id)
            .collect()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
