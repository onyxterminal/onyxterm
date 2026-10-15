use super::toast::ToastTone;
use super::Shell;
use crate::editor_pane::LineJump;
use crate::file_links::FileLink;
use crate::terminal_view::LinkOpener;
use domain::PanelId;
use gpui::Context;
use std::path::Path;
use std::rc::Rc;

impl Shell {
    pub(super) fn link_opener(&self, panel: PanelId, cx: &mut Context<Self>) -> LinkOpener {
        let shell = cx.entity().downgrade();
        Rc::new(move |link, _window, cx| {
            let opened = shell.update(cx, |shell, cx| shell.open_file_link(panel, link, cx));
            if let Err(err) = opened {
                tracing::debug!(?err, "shell dropped before opening a file link");
            }
        })
    }

    fn open_file_link(&mut self, panel: PanelId, link: &FileLink, cx: &mut Context<Self>) {
        let cwd = self.panel_cwd(panel);
        let path = cwd.join(&link.path);
        if !path.is_file() {
            self.show_toast(format!("Couldn't find {}", link.path), ToastTone::Failure, cx);
            return;
        }
        match self.find_bundle() {
            Some(_) => self.open_file(path.clone(), cx),
            None => self.spawn_bundle_showing(cwd, path.clone(), cx),
        }
        self.jump_to_document(&path, line_jump(link));
        self.remember_focused_editor();
        self.persist();
        cx.notify();
    }

    fn jump_to_document(&mut self, path: &Path, jump: LineJump) {
        let leaves = self.model.tabs().active().map(|tab| tab.layout().leaves()).unwrap_or_default();
        for panel in leaves {
            let Some(pane) = self.editor_pane_mut(panel) else {
                continue;
            };
            let Some(index) = pane.position_of(path) else {
                continue;
            };
            pane.activate(index);
            pane.active_mut().jump_to(jump);
            self.focus_pane(panel);
            return;
        }
    }
}

fn line_jump(link: &FileLink) -> LineJump {
    let to_index = |number: u32| number.saturating_sub(1) as usize;
    LineJump { line: to_index(link.line), column: link.column.map_or(0, to_index) }
}
