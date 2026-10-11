# Editor Step 3: File Tree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The file-tree pane is real. It lists the folder lazily (respecting `.gitignore`), and you move through it with the keyboard or mouse. Opening a file shows it in an editor. You can create, rename and delete (to the OS trash) inline. The tree stays current through a file watcher. Open editors reload when their file changes on disk, or show a "changed on disk" badge if they have unsaved edits.

**Architecture:**
- New pure-Rust crate `crates/fs-service`, with no gpui:
  - `listing` reads one directory: gitignore-aware, sorted.
  - `ops` creates, renames and trashes.
  - `watch` wraps a `notify` watcher and delivers coalesced batches of changed paths over a channel.
- In `ui`:
  - `file_tree.rs` holds a pure `FileTree` model: root, expanded set, loaded children per directory, flat visible rows, selection. It is fully unit-tested.
  - `FileTreePane` wraps the model, a scroll handle and an optional inline name field.
  - `shell/file_tree_input.rs` holds the `impl Shell` for loading, keys, mouse, open rules and file operations.
  - Rendering uses `uniform_list`, as the palette does.
- All filesystem work runs on `cx.background_executor()`.

**Tech Stack:** Rust 1.85, GPUI 0.2.2, `ignore =0.4.23`, `notify =8.2.0`, `trash =5.2.9`.

**Spec:** `docs/superpowers/specs/2026-10-11-code-editor-design.md`, sections "Step 3 — File tree pane", "External changes" (moved here from step 2), and "Error handling".

## Global Constraints

- No code comments of any kind, including doc comments. No `allow` attributes.
- Keep `#![deny(warnings)]` and `#![forbid(unsafe_code)]` on every crate root.
- `fs-service` depends on no workspace crate and no gpui. Its dependencies are pinned with `=`: `ignore = "=0.4.23"`, `notify = "=8.2.0"`, `trash = "=5.2.9"`, plus `thiserror` and `tracing` from the workspace, and `tempfile = "=3.20.0"` as a dev-dependency only. If a pinned version fails to resolve or build on Rust 1.85, take the nearest older patch release and say so in the report.
- The UI thread never touches the filesystem: listing, ops and watch setup all run in the background. There is no per-render stat.
- Tree rows render through `uniform_list`, so cost depends on the viewport, not on how many rows are expanded.
- Delete always confirms first and always goes to the OS trash. Nothing in this plan calls `std::fs::remove_*`.
- Magic numbers become named consts. Follow the existing patterns:
  - `impl Shell` blocks go in `shell/*.rs` with `pub(super)`.
  - Confirms go through `confirm.rs`.
  - Inline fields use `TextField` and `FieldTarget`.
- Commits use `TYPE(SCOPE): MESSAGE`, single line, no trailer, never `--no-verify`, on `main`.
- Verify every task with `cargo build --workspace`, `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`.

## Spec deviations (decided while planning)

- **Fuzzy index (`index.rs`) moves to step 7.** Cmd-P is the only thing that needs it.
- **Reloading from disk is a palette command and a pane-menu entry,** "Reload From Disk". The pane menu shows it only for editor panes. "Keep mine" isn't a command: you just keep editing, and the badge stays until you save or reload.
- **One watcher per tree root.** Open editors outside any tree root get a non-recursive watcher on their parent directory.

## Review Focus

1. **Deleting must never hard-delete.** `ops::trash` is the only delete path, and it runs only after a confirm. Test: Task 2 `trash_moves_the_file_out_of_the_folder` (ignored by default, because it really uses the OS trash), plus a model test that delete is gated on a `Confirmation::TrashPath`.
2. **Renaming onto an existing name must fail,** not overwrite. Test: Task 2 `rename_refuses_to_overwrite`.
3. **A deep tree or a folder with 50k entries must not freeze anything.** Listing runs in the background and rows are virtualized. Test: Task 4 `rows_flatten_only_expanded_directories`, plus the manual check in Task 8.
4. **A watcher event storm (git checkout, npm install) is coalesced** into one refresh per debounce window. Test: Task 3 `bursts_coalesce_into_one_batch`.
5. **External change to a dirty buffer never overwrites the user's edits.** Clean buffers reload. Dirty buffers only get the badge. Test: Task 7 `external_change_decision`.

---

### Task 1: `fs-service`: directory listing

**Files:** Create `crates/fs-service/{Cargo.toml,src/lib.rs,src/listing.rs}`. Add `"crates/fs-service"` to the root workspace members.

**Interfaces:**
- `pub struct Entry { pub path: PathBuf, pub name: String, pub is_dir: bool, pub is_ignored: bool }`
- `pub fn list_dir(dir: &Path) -> std::io::Result<Vec<Entry>>`:
  - immediate children only;
  - `.git` is always skipped;
  - gitignored entries are included with `is_ignored = true`, so the UI can dim them;
  - directories come first, then files, each group in natural, case-insensitive order (`file2` before `file10`).
- `pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering`

Use `ignore::gitignore::GitignoreBuilder` to find ignored entries. Walk upward from `dir` to the filesystem root, collecting `.gitignore` files on the way, and stop after the first directory that contains `.git`. Read the directory with `std::fs::read_dir`.

- [ ] **Tests first** (tempdir):
  - `lists_dirs_first_then_files_in_natural_order`: files `b.txt`, `a10.txt`, `a2.txt` and dirs `zdir`, `Adir` give `[Adir, zdir, a2.txt, a10.txt, b.txt]`.
  - `skips_dot_git_and_marks_ignored`: a `.git/` dir, a `.gitignore` containing `target/\n*.log`, and `target/`, `x.log`, `src/`. The result has no `.git`; `target` and `x.log` have `is_ignored`; `src` and `.gitignore` don't.
  - `missing_directory_is_an_error`.
  - `natural_cmp` cases: `"a2" < "a10"`, `"A" == "a"` ordering-wise falls back to byte order for stability, `"img_9.png" < "img_10.png"`.
- [ ] RED, implement, GREEN, verify.
- [ ] Commit `feat(fs-service): list directories with gitignore and natural sort`.

### Task 2: `fs-service`: create, rename, trash

**Files:** `crates/fs-service/src/ops.rs`

**Interfaces:**
- `create_file(dir: &Path, name: &str) -> Result<PathBuf, OpError>` uses `OpenOptions::create_new`.
- `create_dir(dir: &Path, name: &str) -> Result<PathBuf, OpError>`
- `rename(from: &Path, new_name: &str) -> Result<PathBuf, OpError>` stays in the same parent and errors if the target exists.
- `trash(path: &Path) -> Result<(), OpError>` calls `trash::delete`.
- `OpError` is a `thiserror` enum: `Io(#[from] std::io::Error)`, `AlreadyExists(PathBuf)`, `InvalidName(String)`, `Trash(String)`.
- `validate_name(name) -> Result<&str, OpError>` rejects empty names, `.`, `..`, and names containing `/` or `\0`. It trims surrounding whitespace.

- [ ] **Tests first:**
  - `create_file_and_dir_inside_the_folder`
  - `create_refuses_existing_name`
  - `rename_moves_within_the_same_parent`
  - `rename_refuses_to_overwrite`
  - `invalid_names_are_rejected`, covering `""`, `"  "`, `"."`, `".."`, `"a/b"`
  - `trash_moves_the_file_out_of_the_folder`, marked `#[ignore]` because it uses the real OS trash. Run it once locally with `--ignored` and report the result.
- [ ] RED, implement, GREEN, verify.
- [ ] Commit `feat(fs-service): add safe create, rename and trash`.

### Task 3: `fs-service`: coalescing watcher

**Files:** `crates/fs-service/src/watch.rs`

**Interfaces:**
- `pub struct Watcher` owns a `notify::RecommendedWatcher` and a background thread.
- `Watcher::new(debounce: Duration) -> Result<(Watcher, std::sync::mpsc::Receiver<Vec<PathBuf>>), notify::Error>`
- `watch(&mut self, path: &Path, recursive: bool) -> Result<(), notify::Error>`
- `unwatch(&mut self, path: &Path)`
- The thread collects event paths. After the first event it waits `debounce` while collecting more, then sends one deduplicated, sorted `Vec<PathBuf>`.
- `pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(50)`
- Extract the coalescing as a pure `coalesce(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf>` (dedup and sort) so it can be unit-tested.

- [ ] **Tests first:**
  - `coalesce_dedups_and_sorts` (pure).
  - `bursts_coalesce_into_one_batch` (tempdir, real watcher): write 20 files in a loop, then `recv_timeout(2s)` returns one batch containing them all. A second `recv_timeout(300ms)` returns `Timeout`, or at most one extra batch on macOS FSEvents. Assert "≤ 2 batches total" to stay robust.
  - `unwatched_path_sends_nothing`.
- [ ] RED, implement, GREEN, verify.
- [ ] Commit `feat(fs-service): add debounced filesystem watcher`.

### Task 4: `FileTree` model (pure, in `ui`)

**Files:** Create `crates/ui/src/file_tree.rs` and register `pub mod file_tree;` in `lib.rs`. Add `fs-service = { path = "../fs-service" }` to `crates/ui/Cargo.toml`.

**Interfaces:**
- `pub struct Row { pub path: PathBuf, pub name: String, pub depth: usize, pub is_dir: bool, pub is_expanded: bool, pub is_ignored: bool }`
- `pub struct FileTree { root: PathBuf, expanded: HashSet<PathBuf>, children: HashMap<PathBuf, Vec<fs_service::Entry>>, rows: Vec<Row>, selected: usize }`
- `new(root)` expands the root.
- `set_children(dir, entries)` rebuilds the rows and keeps the selection on the same path when it still exists.
- `rows()`, `selected_index()`, `selected_row() -> Option<&Row>`
- `move_selection(delta: isize)` clamps.
- `select_path(&Path) -> bool`
- `toggle_selected() -> TreeAction`, where `TreeAction { None, Load(PathBuf), Open(PathBuf) }`:
  - a collapsed directory with no loaded children gives `Load(dir)` and marks it expanded;
  - a loaded collapsed directory just expands, giving `None`;
  - an expanded directory collapses;
  - a file gives `Open`.
- `collapse_or_parent()`: an expanded directory collapses; otherwise selection moves to the parent row.
- `dirs_needing_refresh(changed: &[PathBuf]) -> Vec<PathBuf>` returns the loaded or expanded directories that contain any changed path.
- `expanded_dirs()` is used for watcher setup.

- [ ] **Tests first:**
  - `rows_flatten_only_expanded_directories`
  - `toggle_loads_then_expands_then_collapses`
  - `collapse_or_parent_moves_to_the_parent_row`
  - `selection_survives_a_refresh_and_clamps_when_its_row_disappears`
  - `changed_paths_map_to_their_loaded_parent_directories`
  - `depth_increases_with_nesting`
- [ ] RED, implement, GREEN, verify.
- [ ] Commit `feat(file-tree): add pure tree model with lazy expansion`.

### Task 5: Tree pane: background loading and rendering

**Files:**
- `crates/ui/src/document.rs`: `FileTreePane` becomes `{ root, exists, tree: FileTree, scroll: UniformListScrollHandle, field: Option<TreeField> }`.
- Create `crates/ui/src/shell/file_tree_input.rs`.
- `crates/ui/src/shell.rs`: restore and open start the root load; `render_pane` routes trees to the new renderer.
- Create `crates/ui/src/file_tree_view.rs` for the row renderer.

**Behaviour:**
- **Loading.** `Shell::load_tree_dir(panel, dir, cx)` spawns `fs_service::listing::list_dir` on the background executor, then applies the result with `tree.set_children` and `cx.notify()`. On restore or open, the root is loaded.
- **Errors.** A listing error for the root sets `exists = false`, which shows the existing "not found" label. An error for a subdirectory logs `tracing::warn!` and leaves it empty.
- **Rows.** Rendered with `uniform_list`:
  - `depth * INDENT_PX` padding;
  - a chevron from `theme/icons.rs` for directories (pick existing icons; don't add SVGs unless none fit, and record which);
  - the name in `theme.text.default`, or `theme.text.muted` when `is_ignored`;
  - the selected row in the palette's selected-row fill, using the same token as the palette's selected row, only when the pane is focused; unfocused selection uses a muted fill.
- **Sizing.** Row height reuses the palette or menu row-height token. Scroll keeps the selection visible through `scroll.scroll_to_item`.

- [ ] Write a test that the renderer's row label and colour come from a pure `row_style(row, is_selected, is_focused, &Theme) -> (SharedString, Hsla, Option<Hsla>)`. Implement and verify, then do a manual check with `cargo run -p app` and Cmd-E.
- [ ] Commit `feat(file-tree): load and render the tree in the background`.

### Task 6: Tree keys, mouse, opening files, placement

**Files:** `shell/file_tree_input.rs`, `shell.rs` (the `consume_key` document branch), `command.rs`, `keymap.rs`, `menus.rs`.

**Behaviour:**
- **Keys.** The document key branch dispatches to the tree when the focused document is a tree:
  - j / Down and k / Up move;
  - Enter or l runs `toggle_selected` (Load spawns a listing; Open runs the open rules);
  - h or Left runs `collapse_or_parent`;
  - Right expands;
  - `y` copies the selected path to the clipboard.
  - `a`, `A`, `r` and `d` belong to Task 7.
  - Other keys aren't consumed, so global bindings still work.
- **Mouse.** A click selects and focuses the pane. A double-click acts like Enter.
- **Open rules**, a pure helper `choose_editor_target(tab_leaves, documents, last_focused_editor, path) -> OpenTarget { Focus(PanelId), Replace(PanelId), SplitRightOf(PanelId) }`:
  1. If an editor in the active tab already shows `path`, focus it.
  2. Otherwise, if the most recently focused editor in the tab is clean, replace its file. Reload that pane's state with the new path and update its record's `path` and title.
  3. Otherwise, split a new editor to the right of the tree.

  Track "most recently focused editor" with a `Shell.last_editor: Option<PanelId>` updated in `focus_pane`.
- **Placement.**
  - "Open Folder…" (Cmd-Shift-O) puts the tree to the left of the focused pane, using `layout.insert_beside(focused, tree, DropZone::Left)` through the tab API. Add a `Tab::insert_beside_focused(panel, zone)` if none exists.
  - Cmd-E keeps its current behaviour, but places the tree on the left too.
- **New command `ToggleFileTree` (Cmd-Shift-B).** If the active tab has a tree pane, close it. Otherwise open one, on the left, for the workspace root or else the focused pane's cwd.

- [ ] **Tests first:**
  - `choose_editor_target` cases: already open, then clean reuse, then dirty forces a split, then no editor at all forces a split.
  - `cmd_shift_b_toggles_file_tree` keymap test.
  - A tab test that `insert_beside_focused(…, Left)` puts the new leaf first.
- [ ] RED, implement, GREEN, verify, plus a manual check.
- [ ] Commit `feat(file-tree): navigate, open files into editors and toggle with cmd-shift-b`.

### Task 7: Tree file operations, watching, external editor changes

**Files:**
- `shell/file_tree_input.rs`
- `shell/text_input.rs`: new `FieldTarget::TreeName`, with `active_field` returning it while a tree field is open.
- `confirm.rs`: `Confirmation::TrashPath(PanelId, PathBuf)` doesn't fit, because `Confirmation` is `Copy`. Store the pending path in `Shell.pending_trash: Option<PathBuf>` and use `Confirmation::TrashPath(PanelId)`.
- `document.rs`: `EditorPane.changed_on_disk: bool`.
- `shell/pane_header.rs`: the badge.
- `command.rs`, `pane_menu.rs`: `ReloadFromDisk`.
- `shell.rs`: watcher ownership and the event pump.

**Behaviour:**
- **Inline field.** `a` opens an inline field below the selection for a new file, `A` for a new folder, and `r` for rename (prefilled). The field is a `TextField` rendered in place of, or under, the row. Enter commits through `ops::*` on the background executor, then refreshes the parent directory and selects the new path. Esc cancels. Errors (already exists, invalid name, I/O) appear in `self.notice` as `"Couldn't {verb} {name}: {err}"`.
- **Trash.**
  - `d` sets `pending = Some(Confirmation::TrashPath(panel))` with `pending_trash`.
  - The copy is "Move "{name}" to the Trash?", action "Move to Trash", destructive.
  - Accept runs `ops::trash` in the background, then refreshes.
  - If an open editor shows the trashed file, that editor stays open, gets `changed_on_disk = true`, and keeps its buffer.
- **Watching.**
  - `Shell` owns one `fs_service::watch::Watcher` and a foreground task that receives batches from a bridge: a background thread forwarding the `std::sync::mpsc::Receiver` into a channel GPUI can await, or a polling task on a timer of `WATCH_POLL_INTERVAL` 100 ms using `try_recv`. The simplest correct option is fine; record which you chose.
  - Tree roots are watched recursively. Each open editor's file parent is watched non-recursively unless it's already covered by a tree root.
  - On a batch: for each tree, refresh `dirs_needing_refresh`. For each editor whose path is in the batch, apply `external_change_decision(is_dirty, file_still_exists) -> ExternalChange { Reload, MarkChanged, Ignore }`:
    - clean and still existing means Reload (background reload, keep cursors clamped);
    - dirty, or the file is gone, means MarkChanged.
  - A save made by our own `save_focused_editor` must not trigger a reload loop. Record `(path, saved_at: Instant)` and ignore events for that path within `SELF_WRITE_GRACE` = 500 ms.
- **Badge and reload.** Header shows a "CHANGED" badge, same style as COPY, when `changed_on_disk`. `ReloadFromDisk` in the palette and in the editor pane's menu reloads in the background; a dirty buffer asks first with `Confirmation::DiscardPane` and the existing copy. A successful reload or save clears the flag.

- [ ] **Tests first:**
  - `external_change_decision`: clean and existing gives Reload; dirty gives MarkChanged; missing gives MarkChanged.
  - `self_write_is_ignored_within_grace` (pure helper over `(path, Instant)`).
  - `trash_copy_names_the_item`.
  - `tree_name_field_validates_before_commit`: uses `fs_service::ops::validate_name` and covers empty, `..` and `/`.
  - The pane menu shows "Reload From Disk" only for editor panes.
- [ ] RED, implement, GREEN, verify, plus a manual check.
- [ ] Commit in at least two commits: `feat(file-tree): create, rename and trash from the tree` and `feat(editor): watch files and reload or flag external changes`.

### Task 8: Manual verification (user)

- [ ] Run `cargo run --release -p app`, `cd` into a big repo, and press Cmd-E.
  - Check: expand `node_modules` or `target`; ignored entries are dimmed and expanding stays smooth.
  - Check: j, k, l and h work; Enter on a file opens it to the right; opening another file reuses that editor while it's clean.
- [ ] Press `a`, type `x.txt`, Enter. Then `r` to rename it, then `d` and confirm. The file must end up in the Trash.
- [ ] Run `git checkout` on another branch in a terminal. The tree refreshes once, and an open clean file reloads. An open dirty file shows CHANGED and keeps your edits.
- [ ] Press Cmd-Shift-B: it hides the tree, and pressing it again shows it on the left.
