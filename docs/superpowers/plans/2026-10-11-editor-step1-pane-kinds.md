# Editor Step 1: Pane-Kind Seam Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Panes can hold something other than a terminal. Editor and file-tree panels exist in the domain, persist with a file path, restore after a restart, and render as a placeholder card that can be split, focused, moved and closed like any pane.

**Architecture:** Terminals keep their existing `Shell.panes` map untouched. Non-terminal panes live in a second map, `Shell.documents: HashMap<PanelId, DocumentPane>`. Every terminal-only `Shell` method already looks up `self.panes` and does nothing on a miss, so it skips documents for free. Only spawn/restore, render, key routing and discard branch on the kind. The path to the file or folder is a new `PanelSnapshot.path` field stored in a new `panels.path` column (migration v4).

**Tech Stack:** Rust 1.85, GPUI 0.2.2, rusqlite 0.37, serde.

**Spec:** `docs/superpowers/specs/2026-10-11-code-editor-design.md` (sections "Step 1 — Pane-kind seam" and "Persistence")

## Global Constraints

- No code comments of any kind, including doc comments.
- Every crate keeps `#![deny(warnings)]` and `#![forbid(unsafe_code)]`.
- No new dependencies in this plan.
- Magic numbers become named `const`s at the top of the file.
- Commits: `TYPE(SCOPE): MESSAGE`, single line, no `Co-Authored-By` trailer, never `--no-verify`.
- Verification per task: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`.

## Spec deviations (decided while planning)

- `ResourceRef` derives `Copy`, so it can't hold a `PathBuf`. Instead it gains `ResourceRef::Document(DocumentId)`, and the path lives in `PanelSnapshot.path: Option<PathBuf>` next to `cwd`. The `panels.path` column is the same as in the spec.
- A file tree whose root no longer exists shows the same "not found" placeholder as an editor whose file is gone, instead of closing itself. The user closes it. This avoids layout changes during restore.

Task 4 copies both deviations into the spec.

## Review Focus

1. **An old v3 database opens after the upgrade.** Existing users' terminals restore, and `path` is NULL for them. Test: Task 2 `migration_adds_panel_path_to_v3_database`.
2. **A restored editor panel must never start a shell.** If `show_active_workspace` treats it as a terminal, a PTY gets spawned and overwrites `resource` with `ResourceRef::Terminal`, and the editor is lost on the next save. Test: Task 3 `document_panels_are_not_spawned_as_terminals`.
3. **The file was deleted between sessions.** The pane restores and shows "not found" instead of panicking or disappearing. Covered by Task 3's render branch (`path.exists()`), checked manually in Task 5 step 3.
4. **Quit with only editor panes open.** It must not count them as live shell sessions in the quit confirm. `live_session_count` reads `self.panes` only, so this is unchanged; checked manually in Task 5.
5. **Keys typed into a focused editor placeholder** must not reach the last focused terminal. `send_key_to_terminal` uses `focused_handle()`, which misses for documents. Test: Task 3 `focused_document_has_no_terminal_handle` asserts on the model-level helper.

---

### Task 1: Domain — editor and file-tree panel kinds

**Files:**
- Modify: `crates/domain/src/id.rs` (after line 49)
- Modify: `crates/domain/src/panel.rs`
- Modify: `crates/domain/src/lib.rs` (re-export `DocumentId` beside the other ids)
- Test: `crates/domain/tests/domain.rs`

**Interfaces:**
- Produces: `domain::DocumentId` (UUID v7 newtype, display prefix `doc`); `PanelKind::{Editor, FileTree}`; `ResourceRef::Document(DocumentId)`; `Panel::new_editor(id: PanelId, title: String) -> Panel`; `Panel::new_file_tree(id: PanelId, title: String) -> Panel`; `Panel::is_document(&self) -> bool`.

- [ ] **Step 1: Write the failing test** — append to `crates/domain/tests/domain.rs`:

```rust
#[test]
fn document_panels_carry_their_kind_and_a_document_resource() {
    let editor = Panel::new_editor(PanelId::new(), "main.rs".into());
    assert_eq!(editor.kind, PanelKind::Editor);
    assert!(matches!(editor.resource, ResourceRef::Document(_)));
    assert!(editor.is_document());

    let tree = Panel::new_file_tree(PanelId::new(), "src".into());
    assert_eq!(tree.kind, PanelKind::FileTree);
    assert!(tree.is_document());

    let terminal = Panel::new_terminal(PanelId::new(), "zsh".into(), TerminalSessionId::new());
    assert!(!terminal.is_document());
}
```

Add any missing names (`PanelKind`, `ResourceRef`, `PanelId`, `TerminalSessionId`) to the file's existing `use domain::{...}` line.

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p domain document_panels_carry`
Expected: compile error, `no function or associated item named new_editor`.

- [ ] **Step 3: Implement**

`crates/domain/src/id.rs`, after `id_newtype!(AgentId, "agent");`:

```rust
id_newtype!(DocumentId, "doc");
```

`crates/domain/src/lib.rs`: add `DocumentId` to the existing `pub use id::{...}` list.

`crates/domain/src/panel.rs`, full file:

```rust
use crate::id::{AgentId, DocumentId, PanelId, TerminalSessionId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelKind {
    Terminal,
    Agent,
    Editor,
    FileTree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceRef {
    Terminal(TerminalSessionId),
    Agent(AgentId),
    Document(DocumentId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub id: PanelId,
    pub kind: PanelKind,
    pub title: String,
    pub resource: ResourceRef,
}

impl Panel {
    pub fn new_terminal(id: PanelId, title: String, session: TerminalSessionId) -> Self {
        Self { id, kind: PanelKind::Terminal, title, resource: ResourceRef::Terminal(session) }
    }

    pub fn new_agent(id: PanelId, title: String, agent: AgentId) -> Self {
        Self { id, kind: PanelKind::Agent, title, resource: ResourceRef::Agent(agent) }
    }

    pub fn new_editor(id: PanelId, title: String) -> Self {
        Self::new_document(id, PanelKind::Editor, title)
    }

    pub fn new_file_tree(id: PanelId, title: String) -> Self {
        Self::new_document(id, PanelKind::FileTree, title)
    }

    pub fn is_document(&self) -> bool {
        matches!(self.kind, PanelKind::Editor | PanelKind::FileTree)
    }

    fn new_document(id: PanelId, kind: PanelKind, title: String) -> Self {
        Self { id, kind, title, resource: ResourceRef::Document(DocumentId::new()) }
    }
}
```

Keep the existing rustfmt style of the file. If `cargo fmt --check` was clean before this change, run `cargo fmt -p domain`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p domain`
Expected: all pass. `cargo build --workspace` now fails in `persistence/src/codec.rs` with non-exhaustive matches. Task 2 fixes that, so commit Tasks 1 and 2 together if the workspace must build at every commit (it should). Move on to Task 2 without committing.

---

### Task 2: Persistence — codec, migration v4, `PanelSnapshot.path`

**Files:**
- Modify: `crates/persistence/src/codec.rs:41-79`
- Modify: `crates/persistence/src/migrations.rs`
- Modify: `crates/persistence/src/snapshot.rs` (struct at :13, write at :58-67, load at :88-110, new `panel_paths` beside `panel_cwds` at :141)
- Modify every `PanelSnapshot { .. }` literal to add `path: None`:
  - `crates/ui/src/shell.rs:753`
  - `crates/ui/src/workspaces.rs:244`
  - `crates/ui/src/workspace_tests.rs:15`
  - `crates/workspace-service/src/restore_policy.rs:16`
  - `crates/persistence/tests/snapshot.rs:13`
- Test: `crates/persistence/tests/persistence.rs`, `crates/persistence/tests/snapshot.rs`

**Interfaces:**
- Consumes: Task 1's `PanelKind::{Editor, FileTree}`, `ResourceRef::Document`, `DocumentId`, `Panel::new_editor`.
- Produces: `PanelSnapshot { panel: Panel, cwd: PathBuf, name: Option<String>, path: Option<PathBuf> }`; `CURRENT_VERSION == 4`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/persistence/tests/persistence.rs`:

```rust
#[test]
fn migration_adds_panel_path_to_v3_database() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("v3.db");
    let conn = rusqlite::Connection::open(&path).expect("open v3");
    conn.execute_batch(SCHEMA_SQL).expect("create v1 schema");
    conn.execute_batch("ALTER TABLE panels ADD COLUMN cwd TEXT; ALTER TABLE panels ADD COLUMN name TEXT;")
        .expect("add v2 and v3 columns");
    conn.execute_batch(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (1, '2025-01-01');
         INSERT INTO schema_migrations (version, applied_at) VALUES (2, '2025-01-02');
         INSERT INTO schema_migrations (version, applied_at) VALUES (3, '2025-01-03');",
    )
    .expect("record v3");
    drop(conn);

    let repo = Repository::open(&path).expect("migrate v3");
    assert_eq!(read_max_version(&repo), CURRENT_VERSION);
    let path_count: i64 = repo.pool().with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM pragma_table_info('panels') WHERE name = 'path'", [], |row| row.get(0))
            .expect("panel path column")
    });
    assert_eq!(path_count, 1);
}
```

Append to `crates/persistence/tests/snapshot.rs`:

```rust
#[test]
fn editor_panel_roundtrips_kind_and_path() {
    let dir = TempDir::new().expect("tempdir");
    let repo = Repository::open(&dir.path().join("snap.db")).expect("open");
    let mut workspace = Workspace::new(WorkspaceId::new(), "Code".to_string());
    let editor_id = PanelId::new();
    let tab = Tab::new(TabId::new(), workspace.id, LayoutNode::leaf(editor_id));
    workspace.tab_order = vec![tab.id];
    workspace.active_tab = Some(tab.id);
    let editor = PanelSnapshot {
        panel: Panel::new_editor(editor_id, "main.rs".into()),
        cwd: PathBuf::from("/code/src"),
        name: None,
        path: Some(PathBuf::from("/code/src/main.rs")),
    };
    let snapshot = WorkspaceSnapshot {
        workspaces: vec![workspace.clone()],
        tabs: vec![TabSnapshot { tab, panels: vec![editor.clone()] }],
        agents: Vec::new(),
        active_workspace: Some(workspace.id),
    };
    repo.write_snapshot(&snapshot).expect("write");

    let loaded = repo.load_snapshot(Path::new("/default")).expect("load");
    assert_eq!(loaded.snapshot.tabs[0].panels, vec![editor]);
}
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test -p persistence`
Expected: compile errors (non-exhaustive match in `codec.rs`, `PanelSnapshot` has no field `path`).

- [ ] **Step 3: Implement**

`crates/persistence/src/codec.rs`, replace the four panel/resource functions:

```rust
pub fn panel_kind_to_str(kind: PanelKind) -> &'static str {
    match kind {
        PanelKind::Terminal => "Terminal",
        PanelKind::Agent => "Agent",
        PanelKind::Editor => "Editor",
        PanelKind::FileTree => "FileTree",
    }
}

pub fn panel_kind_from_str(value: &str) -> Result<PanelKind, PersistenceError> {
    match value {
        "Terminal" => Ok(PanelKind::Terminal),
        "Agent" => Ok(PanelKind::Agent),
        "Editor" => Ok(PanelKind::Editor),
        "FileTree" => Ok(PanelKind::FileTree),
        other => Err(PersistenceError::MigrationFailed(format!(
            "unknown panel kind {other}"
        ))),
    }
}

pub fn resource_ref_columns(resource: ResourceRef) -> (&'static str, String) {
    match resource {
        ResourceRef::Terminal(session) => ("Terminal", session.to_string()),
        ResourceRef::Agent(agent) => ("Agent", agent.to_string()),
        ResourceRef::Document(document) => ("Document", document.to_string()),
    }
}

pub fn resource_ref_from(kind: &str, id: &str) -> Result<ResourceRef, PersistenceError> {
    match kind {
        "Terminal" => {
            let session: domain::TerminalSessionId = parse_uuid_id(id)?;
            Ok(ResourceRef::Terminal(session))
        }
        "Agent" => {
            let agent: AgentId = parse_uuid_id(id)?;
            Ok(ResourceRef::Agent(agent))
        }
        "Document" => {
            let document: domain::DocumentId = parse_uuid_id(id)?;
            Ok(ResourceRef::Document(document))
        }
        other => Err(PersistenceError::MigrationFailed(format!(
            "unknown resource kind {other}"
        ))),
    }
}
```

`crates/persistence/src/migrations.rs`:

```rust
pub const CURRENT_VERSION: i64 = 4;

const PANEL_CWD_SQL: &str = "ALTER TABLE panels ADD COLUMN cwd TEXT;";

const PANEL_NAME_SQL: &str = "ALTER TABLE panels ADD COLUMN name TEXT;";

const PANEL_PATH_SQL: &str = "ALTER TABLE panels ADD COLUMN path TEXT;";

const MIGRATIONS: &[(i64, &str)] =
    &[(1, SCHEMA_SQL), (2, PANEL_CWD_SQL), (3, PANEL_NAME_SQL), (4, PANEL_PATH_SQL)];
```

`crates/persistence/src/snapshot.rs`:

1. The struct:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PanelSnapshot {
    pub panel: Panel,
    pub cwd: PathBuf,
    pub name: Option<String>,
    pub path: Option<PathBuf>,
}
```

2. In `write_snapshot`, change the panels insert to write `path` as `?9`:

```rust
tx.execute(
    "INSERT INTO panels (id, tab_id, kind, title, resource_kind, resource_id, cwd, name, path) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
     ON CONFLICT(id) DO UPDATE SET tab_id=excluded.tab_id, kind=excluded.kind, title=excluded.title, \
     resource_kind=excluded.resource_kind, resource_id=excluded.resource_id, cwd=excluded.cwd, name=excluded.name, \
     path=excluded.path",
    rusqlite::params![panel.panel.id.to_string(), tab.tab.id.to_string(), panel_kind_to_str(panel.panel.kind),
        panel.panel.title, resource_kind, resource_id, path_to_string_lossy(&panel.cwd), panel.name,
        panel.path.as_deref().map(path_to_string_lossy)],
)?;
```

3. In `load_snapshot`, after `let mut names = self.panel_names()?;` add `let mut paths = self.panel_paths()?;`, and in the `PanelSnapshot { .. }` literal add `path: paths.remove(&panel.id),`.

4. Next to `panel_cwds`:

```rust
fn panel_paths(&self) -> Result<HashMap<PanelId, PathBuf>, PersistenceError> {
    self.pool.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, path FROM panels WHERE path IS NOT NULL")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        let mut out = HashMap::new();
        for row in rows {
            let (id, path) = row?;
            out.insert(parse_uuid_id(&id)?, PathBuf::from(path));
        }
        Ok(out)
    })
}
```

5. Add `path: None` to each `PanelSnapshot { .. }` literal listed under **Files**.

- [ ] **Step 4: Run the tests**

Run: `cargo build --workspace && cargo test --workspace`
Expected: build succeeds, all tests pass, including the two new ones.

- [ ] **Step 5: Lint and commit Tasks 1 and 2**

```bash
cargo clippy --workspace -- -D warnings
git add crates/domain crates/persistence crates/workspace-service/src/restore_policy.rs crates/ui/src/shell.rs crates/ui/src/workspaces.rs crates/ui/src/workspace_tests.rs
git commit -m "feat(panels): add editor and file tree panel kinds with persisted path"
```

---

### Task 3: UI — document panes in `Shell`

**Files:**
- Create: `crates/ui/src/document.rs`
- Modify: `crates/ui/src/lib.rs` (add `mod document;` beside the other modules)
- Modify: `crates/ui/src/workspaces.rs` (two helpers)
- Modify: `crates/ui/src/shell.rs`:
  - `Shell` struct (:101) and `new` (:135)
  - `show_active_workspace` (:189)
  - `consume_key` (:286)
  - `discard_pane` (:734)
  - `render_pane` (:823)
- Test: `crates/ui/src/workspace_tests.rs`

**Interfaces:**
- Consumes: `Panel::is_document`, `PanelSnapshot.path`.
- Produces:
  - `WorkspaceModel::is_document(&self, panel: PanelId) -> bool`
  - `WorkspaceModel::focused_document(&self) -> Option<PanelId>`
  - `document::DocumentPane { kind: PanelKind, path: PathBuf }`
  - `document::DocumentPane::from_snapshot(&PanelSnapshot) -> Option<DocumentPane>`
  - `document::render_document(&DocumentPane, &Theme) -> AnyElement`
  - `Shell.documents: HashMap<PanelId, DocumentPane>`
  - `Shell::open_document(&mut self, panel: Panel, path: PathBuf, cx)`, which Task 4 uses.

- [ ] **Step 1: Write the failing tests** — append to `crates/ui/src/workspace_tests.rs`:

```rust
fn editor_snapshot(panel: PanelId, file: &Path) -> PanelSnapshot {
    PanelSnapshot {
        panel: Panel::new_editor(panel, "main.rs".to_string()),
        cwd: file.parent().map(Path::to_path_buf).unwrap_or_default(),
        name: None,
        path: Some(file.to_path_buf()),
    }
}

#[test]
fn document_panels_are_not_spawned_as_terminals() {
    let home = TempDir::new().expect("home");
    let mut model = seeded_model(home.path());
    let terminal = model.tabs().active().expect("tab").focused();
    let editor = PanelId::new();
    model.add_panel(editor_snapshot(editor, &home.path().join("main.rs")));
    model.tabs_mut().active_mut().expect("tab").split_focused(Axis::Horizontal, editor).expect("split");

    let to_spawn: Vec<PanelId> =
        model.panels_to_spawn(|_| false).into_iter().filter(|panel| !model.is_document(*panel)).collect();

    assert_eq!(to_spawn, vec![terminal]);
    assert!(model.is_document(editor));
    assert!(!model.is_document(terminal));
}

#[test]
fn focused_document_has_no_terminal_handle() {
    let home = TempDir::new().expect("home");
    let mut model = seeded_model(home.path());
    let editor = PanelId::new();
    model.add_panel(editor_snapshot(editor, &home.path().join("main.rs")));
    model.tabs_mut().active_mut().expect("tab").split_focused(Axis::Horizontal, editor).expect("split");

    assert_eq!(model.focused_document(), Some(editor));
    model.tabs_mut().active_mut().expect("tab").focus_next();
    assert_eq!(model.focused_document(), None);
}
```

If `seeded_model` leaves the active tab without a focused terminal, start from `seeded_model(..)` plus `model.tabs_mut().open(..)` with a `titled(..)` terminal first, matching how the existing tests in this file set up a tab.

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test -p ui document`
Expected: compile error, `no method named is_document found for WorkspaceModel`.

- [ ] **Step 3: Model helpers** — in `crates/ui/src/workspaces.rs`, next to `panel`:

```rust
pub fn is_document(&self, id: PanelId) -> bool {
    self.panels.get(&id).is_some_and(|record| record.panel.is_document())
}

pub fn focused_document(&self) -> Option<PanelId> {
    self.tabs().active().map(|tab| tab.focused()).filter(|panel| self.is_document(*panel))
}
```

Run: `cargo test -p ui document`
Expected: PASS.

- [ ] **Step 4: Placeholder view** — create `crates/ui/src/document.rs`:

```rust
use crate::theme::Theme;
use domain::PanelKind;
use gpui::{div, prelude::*, px, AnyElement, IntoElement, ParentElement, SharedString, Styled};
use std::path::{Path, PathBuf};
use workspace_service::PanelSnapshot;

const NOT_FOUND_SUFFIX: &str = " not found";
const LABEL_GAP_PX: f32 = 6.0;

pub struct DocumentPane {
    pub kind: PanelKind,
    pub path: PathBuf,
}

impl DocumentPane {
    pub fn from_snapshot(record: &PanelSnapshot) -> Option<Self> {
        let path = record.path.clone()?;
        record.panel.is_document().then_some(Self { kind: record.panel.kind, path })
    }
}

pub fn document_title(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

pub fn render_document(pane: &DocumentPane, theme: &Theme) -> AnyElement {
    let name = document_title(&pane.path);
    let label: SharedString =
        if pane.path.exists() { name.into() } else { format!("{name}{NOT_FOUND_SUFFIX}").into() };
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(LABEL_GAP_PX))
        .text_color(theme.text.muted)
        .child(label)
        .into_any_element()
}
```

Check the real names of the muted-text token on `Theme` (`crates/ui/src/theme.rs:244`) and the path of `Theme` before writing this. Use whatever the terminal empty states use, and don't add new tokens in this task.

Add `mod document;` to `crates/ui/src/lib.rs`, and `pub use` it only if `shell.rs` reaches it through `crate::document`.

- [ ] **Step 5: Wire it into `Shell`** — `crates/ui/src/shell.rs`:

1. Add `use crate::document::{render_document, DocumentPane};` to the imports.
2. Add a field after `panes`: `documents: HashMap<PanelId, DocumentPane>,`. In `new`, initialise it with `documents: HashMap::new(),`.
3. Replace `show_active_workspace`:

```rust
fn show_active_workspace(&mut self, cx: &mut Context<Self>) {
    if self.model.tabs().is_empty() {
        self.open_tab(cx);
    }
    let missing = self.model.panels_to_spawn(|panel| self.panes.contains_key(&panel) || self.documents.contains_key(&panel));
    for panel in missing {
        if self.model.is_document(panel) {
            self.restore_document(panel);
        } else {
            self.spawn_restored(panel, cx);
        }
    }
}

fn restore_document(&mut self, panel: PanelId) {
    if let Some(document) = self.model.panel(panel).and_then(DocumentPane::from_snapshot) {
        self.documents.insert(panel, document);
    }
}

fn open_document(&mut self, panel: Panel, path: PathBuf, cx: &mut Context<Self>) {
    let id = panel.id;
    let cwd = path.parent().map(Path::to_path_buf).unwrap_or_else(|| self.model.active_root());
    self.model.add_panel(PanelSnapshot { panel, cwd, name: None, path: Some(path) });
    self.restore_document(id);
    let Some(tab) = self.model.tabs_mut().active_mut() else {
        return;
    };
    if let Err(err) = tab.split_focused(Axis::Horizontal, id) {
        tracing::warn!(?err, "open document failed");
        self.discard_pane(id, cx);
        return;
    }
    self.persist();
}
```

4. In `consume_key`, directly before `if self.is_copy_mode_active() {`:

```rust
if self.model.focused_document().is_some() {
    return false;
}
```

5. In `discard_pane`, add as the first line:

```rust
self.documents.remove(&panel);
```

6. In `render_pane`, replace the opening `let Some(pane) = ... else { return div()... };` and the `let terminal = terminal_canvas(...)` line with:

```rust
let content = if let Some(document) = self.documents.get(&panel) {
    render_document(document, &self.state.theme)
} else if let Some(pane) = self.panes.get(&panel) {
    terminal_canvas(pane.terminal.clone(), self.style.clone(), is_focused.then(|| self.pane_focus(cx))).into_any_element()
} else {
    return div().into_any_element();
};
```

Compute `is_focused` before this block, and pass `content` to `islands::island_content(look, content)`. If `island_content` takes `impl IntoElement`, `AnyElement` works as is.

- [ ] **Step 6: Build and test**

Run: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings`
Expected: all green. `open_document` is unused until Task 4, so `#![deny(warnings)]` will fail on dead code. Do Task 4 before committing, and commit them together.

---

### Task 4: Open File… and Open Folder… commands, ADR, spec update

**Files:**
- Modify: `crates/ui/src/command.rs` (`CommandId` at :9, `default_commands` at :208)
- Modify: `crates/ui/src/keymap.rs` (`bind_application`, around :181)
- Modify: `crates/ui/src/shell.rs` (`run_command` match at :219, plus a new `prompt_open` method)
- Create: `docs/adr/0002-editor-and-file-tree-panels.md`
- Modify: `docs/superpowers/specs/2026-10-11-code-editor-design.md` (Step 1 and Persistence sections)
- Test: `crates/ui/src/tests.rs`

**Interfaces:**
- Consumes: `Shell::open_document`, `Panel::new_editor`, `Panel::new_file_tree`, `document::document_title`.
- Produces: `CommandId::OpenFile`, `CommandId::OpenFolder`. Cmd-Shift-O is bound to `OpenFolder`. `OpenFile` is palette-only.

- [ ] **Step 1: Write the failing test** — find the existing keymap tests in `crates/ui/src/tests.rs` (search for `resolve_keystroke` or `Keymap::default`) and add one in the same style:

```rust
#[test]
fn cmd_shift_o_opens_folder() {
    let keymap = Keymap::default();
    let combo = Combo::new(Modifiers::command_shift(), "o");
    assert_eq!(keymap.resolve(&combo), Some(CommandId::OpenFolder));
}
```

Use whichever lookup method the neighbouring keymap tests use (`resolve`, `command_for` or similar), and the same construction of `Keymap`.

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p ui cmd_shift_o_opens_folder`
Expected: compile error, `no variant OpenFolder`.

- [ ] **Step 3: Implement**

`command.rs`: add `OpenFile,` and `OpenFolder,` to `CommandId`, then add these to `default_commands()` after `MoveToNewTab`:

```rust
Command::builder(OpenFile, "Open File…", "Files", no_op).build(),
Command::builder(OpenFolder, "Open Folder…", "Files", no_op).build(),
```

`keymap.rs`, in `bind_application`:

```rust
map.bind(Combo::new(Modifiers::command_shift(), "o"), OpenFolder);
```

`shell.rs`, in the `run_command` match:

```rust
CommandId::OpenFile => self.prompt_open(false, cx),
CommandId::OpenFolder => self.prompt_open(true, cx),
```

Then the new method:

```rust
fn prompt_open(&mut self, is_folder: bool, cx: &mut Context<Self>) {
    let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: !is_folder,
        directories: is_folder,
        multiple: false,
        prompt: None,
    });
    cx.spawn(async move |shell, cx| {
        let Ok(Ok(Some(mut picked))) = paths.await else {
            return;
        };
        let Some(path) = picked.pop() else {
            return;
        };
        let updated = shell.update(cx, |shell, cx| {
            let title = crate::document::document_title(&path);
            let panel = if is_folder {
                Panel::new_file_tree(PanelId::new(), title)
            } else {
                Panel::new_editor(PanelId::new(), title)
            };
            shell.open_document(panel, path, cx);
            cx.notify();
        });
        if let Err(err) = updated {
            tracing::debug!(?err, "shell dropped before open finished");
        }
    })
    .detach();
}
```

If the menu bar lists commands from `ui/src/menus.rs:26` `app_menus()` by category, add both to the File menu following the existing items.

- [ ] **Step 4: ADR** — create `docs/adr/0002-editor-and-file-tree-panels.md`:

```markdown
# ADR-0002: Editor and file tree panels

## Status

Accepted — 2026-10-11.

## Context

The blueprint (`docs/blueprint.md:353,373`) schedules a code editor for Phase 3 and forbids editors in Phase 1. The owner has pulled a native editor and file manager forward. Design: `docs/superpowers/specs/2026-10-11-code-editor-design.md`.

## Decision

- Two new registered panel kinds, `PanelKind::Editor` and `PanelKind::FileTree`, both referencing `ResourceRef::Document(DocumentId)`.
- The file or folder path is stored per panel in a new nullable column, `panels.path`, added by schema migration v4. This satisfies ADR-0001's rule that schema changes need a migration plus an ADR addendum.
- The UI keeps terminals and documents in separate maps so terminal code paths are untouched.
- Later steps add these dependencies: `ropey`, `tree-sitter` and built-in grammars, `regex-cursor`, `ignore`, `notify`, `nucleo`, `trash`. Each is justified in the design spec's dependency table.

## Consequences

- A database written by this version can't be opened by older builds (downgrade is rejected, as before).
- Editors and file trees never spawn processes. Restore re-reads the file from disk, and unsaved edits are not persisted.
```

- [ ] **Step 5: Spec update** — in `docs/superpowers/specs/2026-10-11-code-editor-design.md`:
  - Replace "`ResourceRef` gains `Path(PathBuf)`" with "`ResourceRef` gains `Document(DocumentId)`; the path is `PanelSnapshot.path: Option<PathBuf>`".
  - Replace "`ResourceRef::Path(p)` stores `resource_kind = 'path'`, the panel id as `resource_id`, and `p` in `path`" with "`ResourceRef::Document(id)` stores `resource_kind = 'Document'` and the document id; the path is stored in `panels.path`".
  - Replace "a file tree whose root is gone closes" with "a file tree whose root is gone shows a 'not found' placeholder".
  - Replace "`Pane` becomes `enum Pane { ... }`" with "non-terminal panes live in `Shell.documents: HashMap<PanelId, DocumentPane>`; terminal code paths are unchanged".

- [ ] **Step 6: Build, test, lint, commit Tasks 3 and 4**

```bash
cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings
git add crates/ui docs/adr/0002-editor-and-file-tree-panels.md docs/superpowers/specs/2026-10-11-code-editor-design.md
git commit -m "feat(panels): open files and folders as document panes"
```

---

### Task 5: Manual verification in the running app

- [ ] **Step 1:** `cargo run -p app`. Press Cmd-Shift-O and pick a folder. A card with the folder name appears to the right of the focused terminal.
- [ ] **Step 2:** Open the palette, run "Open File…" and pick a file. Cmd-h/l moves focus between it and the terminal, Cmd-Shift-h/l swaps them, Cmd-W closes the card, and typing while the card is focused does nothing in the terminal.
- [ ] **Step 3:** Open a file in a scratch directory, quit, delete that file, and relaunch. The card restores with "<name> not found", and terminals restore normally.
- [ ] **Step 4:** With only document cards left in a tab, plus one terminal elsewhere, press Cmd-Q. The confirm counts 1 session.
- [ ] **Step 5:** Record what was observed in the final report. No commit for this task.
