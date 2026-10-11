use fs_service::Entry;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub is_ignored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeAction {
    None,
    Load(PathBuf),
    Open(PathBuf),
}

pub struct FileTree {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<Entry>>,
    rows: Vec<Row>,
    selected: usize,
}

impl FileTree {
    pub fn new(root: PathBuf) -> Self {
        let expanded = HashSet::from([root.clone()]);
        Self { root, expanded, children: HashMap::new(), rows: Vec::new(), selected: 0 }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn expanded_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.expanded.iter().cloned().collect();
        dirs.sort();
        dirs
    }

    pub fn set_children(&mut self, dir: &Path, entries: Vec<Entry>) {
        if !self.is_known_dir(dir) {
            return;
        }
        let removed_dirs = self.removed_subdirectories(dir, &entries);
        for removed in &removed_dirs {
            self.forget_subtree(removed);
        }
        self.children.insert(dir.to_path_buf(), entries);
        self.rebuild_rows();
    }

    pub fn move_selection(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub fn select_path(&mut self, path: &Path) -> bool {
        let Some(index) = self.rows.iter().position(|row| row.path == path) else {
            return false;
        };
        self.selected = index;
        true
    }

    pub fn toggle_selected(&mut self) -> TreeAction {
        let Some(row) = self.selected_row().cloned() else {
            return TreeAction::None;
        };
        if !row.is_dir {
            return TreeAction::Open(row.path);
        }
        if row.is_expanded {
            self.expanded.remove(&row.path);
            self.rebuild_rows();
            return TreeAction::None;
        }
        self.expanded.insert(row.path.clone());
        self.rebuild_rows();
        if self.children.contains_key(&row.path) {
            return TreeAction::None;
        }
        TreeAction::Load(row.path)
    }

    pub fn expand_selected(&mut self) -> TreeAction {
        let is_collapsed_dir = self.selected_row().is_some_and(|row| row.is_dir && !row.is_expanded);
        if !is_collapsed_dir {
            return TreeAction::None;
        }
        self.toggle_selected()
    }

    pub fn select_index(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }

    pub fn collapse_or_parent(&mut self) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        if row.is_dir && row.is_expanded {
            self.expanded.remove(&row.path);
            self.rebuild_rows();
            return;
        }
        if let Some(parent) = row.path.parent() {
            self.select_path(parent);
        }
    }

    pub fn dirs_needing_refresh(&self, changed: &[PathBuf]) -> Vec<PathBuf> {
        let known_dirs: BTreeSet<&PathBuf> = self.children.keys().chain(self.expanded.iter()).collect();
        known_dirs
            .into_iter()
            .filter(|dir| changed.iter().any(|path| path.parent() == Some(dir.as_path())))
            .cloned()
            .collect()
    }

    fn is_known_dir(&self, dir: &Path) -> bool {
        dir == self.root || self.children.contains_key(dir) || self.expanded.contains(dir)
    }

    fn removed_subdirectories(&self, dir: &Path, entries: &[Entry]) -> Vec<PathBuf> {
        let Some(previous) = self.children.get(dir) else {
            return Vec::new();
        };
        let kept_dirs: HashSet<&PathBuf> = entries.iter().filter(|entry| entry.is_dir).map(|entry| &entry.path).collect();
        previous
            .iter()
            .filter(|entry| entry.is_dir && !kept_dirs.contains(&entry.path))
            .map(|entry| entry.path.clone())
            .collect()
    }

    fn forget_subtree(&mut self, removed: &Path) {
        self.children.retain(|path, _| !path.starts_with(removed));
        self.expanded.retain(|path| !path.starts_with(removed));
    }

    fn rebuild_rows(&mut self) {
        let previously_selected = self.selected_row().map(|row| row.path.clone());
        let mut rows = Vec::new();
        self.append_rows(&self.root, 0, &mut rows);
        self.rows = rows;
        let last = self.rows.len().saturating_sub(1);
        self.selected = previously_selected
            .and_then(|path| self.rows.iter().position(|row| row.path == path))
            .unwrap_or_else(|| self.selected.min(last));
    }

    fn append_rows(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        let Some(entries) = self.children.get(dir) else {
            return;
        };
        for entry in entries {
            let is_expanded = entry.is_dir && self.expanded.contains(&entry.path);
            rows.push(Row {
                path: entry.path.clone(),
                name: entry.name.clone(),
                depth,
                is_dir: entry.is_dir,
                is_expanded,
                is_ignored: entry.is_ignored,
            });
            if is_expanded {
                self.append_rows(&entry.path, depth + 1, rows);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/r")
    }

    fn entry(parent: &str, name: &str, is_dir: bool) -> Entry {
        Entry { path: PathBuf::from(parent).join(name), name: name.to_string(), is_dir, is_ignored: false }
    }

    fn sample_tree() -> FileTree {
        let mut tree = FileTree::new(root());
        tree.set_children(
            &root(),
            vec![entry("/r", "src", true), entry("/r", "docs", true), entry("/r", "a.txt", false)],
        );
        tree
    }

    fn load_collapsed_src(tree: &mut FileTree) {
        tree.select_path(&PathBuf::from("/r/src"));
        tree.toggle_selected();
        tree.set_children(&PathBuf::from("/r/src"), vec![entry("/r/src", "main.rs", false)]);
        tree.toggle_selected();
    }

    fn row_names(tree: &FileTree) -> Vec<&str> {
        tree.rows().iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn rows_flatten_only_expanded_directories() {
        let mut tree = sample_tree();
        load_collapsed_src(&mut tree);

        assert_eq!(row_names(&tree), ["src", "docs", "a.txt"]);

        tree.select_path(&PathBuf::from("/r/src"));
        tree.toggle_selected();

        assert_eq!(row_names(&tree), ["src", "main.rs", "docs", "a.txt"]);
    }

    #[test]
    fn toggle_loads_then_expands_then_collapses() {
        let mut tree = sample_tree();
        tree.select_path(&PathBuf::from("/r/src"));

        assert_eq!(tree.toggle_selected(), TreeAction::Load(PathBuf::from("/r/src")));
        assert!(tree.selected_row().unwrap().is_expanded);

        tree.set_children(&PathBuf::from("/r/src"), vec![entry("/r/src", "main.rs", false)]);
        assert_eq!(tree.toggle_selected(), TreeAction::None);
        assert!(!tree.selected_row().unwrap().is_expanded);

        assert_eq!(tree.toggle_selected(), TreeAction::None);
        assert!(tree.selected_row().unwrap().is_expanded);

        tree.select_path(&PathBuf::from("/r/src/main.rs"));
        assert_eq!(tree.toggle_selected(), TreeAction::Open(PathBuf::from("/r/src/main.rs")));
    }

    #[test]
    fn expand_only_opens_collapsed_directories() {
        let mut tree = sample_tree();
        tree.select_path(&PathBuf::from("/r/a.txt"));
        assert_eq!(tree.expand_selected(), TreeAction::None);

        tree.select_path(&PathBuf::from("/r/src"));
        assert_eq!(tree.expand_selected(), TreeAction::Load(PathBuf::from("/r/src")));
        tree.set_children(&PathBuf::from("/r/src"), vec![entry("/r/src", "main.rs", false)]);

        assert_eq!(tree.expand_selected(), TreeAction::None);
        assert!(tree.selected_row().unwrap().is_expanded);
    }

    #[test]
    fn select_index_ignores_rows_past_the_end() {
        let mut tree = sample_tree();
        tree.select_index(2);
        assert_eq!(tree.selected_row().unwrap().path, PathBuf::from("/r/a.txt"));

        tree.select_index(3);
        assert_eq!(tree.selected_index(), 2);
    }

    #[test]
    fn collapse_or_parent_moves_to_the_parent_row() {
        let mut tree = sample_tree();
        tree.select_path(&PathBuf::from("/r/src"));
        tree.toggle_selected();
        tree.set_children(&PathBuf::from("/r/src"), vec![entry("/r/src", "main.rs", false)]);
        tree.select_path(&PathBuf::from("/r/src/main.rs"));

        tree.collapse_or_parent();
        assert_eq!(tree.selected_row().unwrap().path, PathBuf::from("/r/src"));

        tree.collapse_or_parent();
        assert_eq!(row_names(&tree), ["src", "docs", "a.txt"]);
        assert_eq!(tree.selected_row().unwrap().path, PathBuf::from("/r/src"));
    }

    #[test]
    fn selection_survives_a_refresh_and_clamps_when_its_row_disappears() {
        let mut tree = sample_tree();
        tree.select_path(&PathBuf::from("/r/a.txt"));

        tree.set_children(
            &root(),
            vec![entry("/r", "new", true), entry("/r", "src", true), entry("/r", "a.txt", false)],
        );
        assert_eq!(tree.selected_row().unwrap().path, PathBuf::from("/r/a.txt"));

        tree.set_children(&root(), vec![entry("/r", "src", true)]);
        assert_eq!(tree.selected_index(), 0);
        assert_eq!(tree.selected_row().unwrap().path, PathBuf::from("/r/src"));
    }

    #[test]
    fn changed_paths_map_to_their_loaded_parent_directories() {
        let mut tree = sample_tree();
        load_collapsed_src(&mut tree);

        let changed = vec![PathBuf::from("/r/src/new.rs"), PathBuf::from("/r/b.txt"), PathBuf::from("/elsewhere/x")];

        assert_eq!(tree.dirs_needing_refresh(&changed), vec![PathBuf::from("/r"), PathBuf::from("/r/src")]);
        assert!(tree.dirs_needing_refresh(&[PathBuf::from("/r/docs/x")]).is_empty());
    }

    #[test]
    fn refresh_covers_expanded_unloaded_and_loaded_collapsed_directories() {
        let mut tree = sample_tree();
        load_collapsed_src(&mut tree);
        tree.select_path(&PathBuf::from("/r/docs"));
        assert_eq!(tree.toggle_selected(), TreeAction::Load(PathBuf::from("/r/docs")));

        assert_eq!(tree.dirs_needing_refresh(&[PathBuf::from("/r/docs/x")]), vec![PathBuf::from("/r/docs")]);
        assert_eq!(tree.dirs_needing_refresh(&[PathBuf::from("/r/src/y")]), vec![PathBuf::from("/r/src")]);
    }

    #[test]
    fn empty_tree_ignores_navigation() {
        let mut tree = FileTree::new(root());

        assert_eq!(tree.toggle_selected(), TreeAction::None);
        tree.collapse_or_parent();
        tree.move_selection(1);
        tree.move_selection(-1);

        assert!(tree.selected_row().is_none());
        assert!(tree.rows().is_empty());
    }

    #[test]
    fn dropping_a_directory_prunes_its_loaded_and_expanded_descendants() {
        let mut tree = sample_tree();
        tree.select_path(&PathBuf::from("/r/src"));
        tree.toggle_selected();
        tree.set_children(&PathBuf::from("/r/src"), vec![entry("/r/src", "lib", true)]);
        tree.select_path(&PathBuf::from("/r/src/lib"));
        tree.toggle_selected();
        tree.set_children(&PathBuf::from("/r/src/lib"), vec![entry("/r/src/lib", "mod.rs", false)]);

        tree.set_children(&root(), vec![entry("/r", "docs", true), entry("/r", "a.txt", false)]);

        assert_eq!(tree.expanded_dirs(), vec![root()]);
        assert!(tree.dirs_needing_refresh(&[PathBuf::from("/r/src/x"), PathBuf::from("/r/src/lib/y")]).is_empty());

        tree.set_children(&root(), vec![entry("/r", "src", true)]);
        assert_eq!(row_names(&tree), ["src"]);
        assert!(!tree.rows()[0].is_expanded);
    }

    #[test]
    fn listings_for_unknown_directories_are_dropped() {
        let mut tree = sample_tree();
        tree.set_children(&PathBuf::from("/r/gone"), vec![entry("/r/gone", "x.rs", false)]);

        assert!(tree.dirs_needing_refresh(&[PathBuf::from("/r/gone/y")]).is_empty());
        assert_eq!(tree.expanded_dirs(), vec![root()]);
    }

    #[test]
    fn depth_increases_with_nesting() {
        let mut tree = sample_tree();
        tree.select_path(&PathBuf::from("/r/src"));
        tree.toggle_selected();
        tree.set_children(&PathBuf::from("/r/src"), vec![entry("/r/src", "main.rs", false)]);

        let depths: Vec<usize> = tree.rows().iter().map(|row| row.depth).collect();
        assert_eq!(depths, [0, 1, 0, 0]);
    }
}
