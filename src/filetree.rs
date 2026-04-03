use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
    Directory,
    File,
}

#[derive(Debug)]
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub kind: NodeKind,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
}

#[derive(Debug)]
pub struct FlatEntry {
    pub depth: usize,
    pub name: String,
    pub path: PathBuf,
    pub kind: NodeKind,
    pub expanded: bool,
}

pub struct FileTree {
    pub root: TreeNode,
    pub flat_view: Vec<FlatEntry>,
    pub selected_index: usize,
}

impl FileTree {
    pub fn scan(root: &Path) -> Self {
        let mut dirs_with_children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut md_files: Vec<PathBuf> = Vec::new();

        let walker = WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path().to_path_buf();
            if path == root {
                continue;
            }
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "md" {
                        md_files.push(path);
                    }
                }
            }
        }

        // Build set of directories that contain .md files (transitively)
        for md_file in &md_files {
            let mut ancestor = md_file.parent();
            while let Some(dir) = ancestor {
                if dir == root {
                    break;
                }
                dirs_with_children.entry(dir.to_path_buf()).or_default();
                ancestor = dir.parent();
            }
        }

        // Build tree recursively
        let root_node = Self::build_node(root, &md_files, &dirs_with_children, 0);

        let mut tree = FileTree {
            root: root_node,
            flat_view: Vec::new(),
            selected_index: 0,
        };
        tree.rebuild_flat_view();
        tree
    }

    fn build_node(
        dir: &Path,
        md_files: &[PathBuf],
        dirs_with_children: &HashMap<PathBuf, Vec<PathBuf>>,
        depth: usize,
    ) -> TreeNode {
        let mut children = Vec::new();

        // Collect direct child directories that are in dirs_with_children or contain md files
        let mut child_dirs: Vec<PathBuf> = Vec::new();
        let mut child_files: Vec<PathBuf> = Vec::new();

        for md_file in md_files {
            if md_file.parent() == Some(dir) {
                child_files.push(md_file.clone());
            }
        }

        for dir_path in dirs_with_children.keys() {
            if dir_path.parent() == Some(dir) {
                child_dirs.push(dir_path.clone());
            }
        }

        // Sort: dirs first (alphabetical), then files (alphabetical)
        child_dirs.sort();
        child_files.sort();

        for child_dir in child_dirs {
            let node = Self::build_node(&child_dir, md_files, dirs_with_children, depth + 1);
            if !node.children.is_empty() {
                children.push(node);
            }
        }

        for child_file in child_files {
            let name = child_file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            children.push(TreeNode {
                name,
                path: child_file,
                kind: NodeKind::File,
                children: Vec::new(),
                expanded: false,
            });
        }

        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        TreeNode {
            name,
            path: dir.to_path_buf(),
            kind: NodeKind::Directory,
            children,
            expanded: depth == 0, // root is expanded by default
        }
    }

    pub fn rebuild_flat_view(&mut self) {
        self.flat_view.clear();
        Self::flatten_node(&self.root, 0, true, &mut self.flat_view);
        if self.selected_index >= self.flat_view.len() && !self.flat_view.is_empty() {
            self.selected_index = self.flat_view.len() - 1;
        }
    }

    fn flatten_node(node: &TreeNode, depth: usize, skip_self: bool, out: &mut Vec<FlatEntry>) {
        if !skip_self {
            out.push(FlatEntry {
                depth,
                name: node.name.clone(),
                path: node.path.clone(),
                kind: node.kind,
                expanded: node.expanded,
            });
        }

        let show_children = skip_self || (node.kind == NodeKind::Directory && node.expanded);
        if show_children {
            let child_depth = if skip_self { depth } else { depth + 1 };
            for child in &node.children {
                Self::flatten_node(child, child_depth, false, out);
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.flat_view.is_empty() && self.selected_index < self.flat_view.len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn toggle_expand(&mut self) {
        if self.flat_view.is_empty() {
            return;
        }
        let entry = &self.flat_view[self.selected_index];
        if entry.kind != NodeKind::Directory {
            return;
        }
        let path = entry.path.clone();
        Self::toggle_node(&mut self.root, &path);
        self.rebuild_flat_view();
    }

    fn toggle_node(node: &mut TreeNode, path: &Path) -> bool {
        if node.path == path {
            node.expanded = !node.expanded;
            return true;
        }
        for child in &mut node.children {
            if Self::toggle_node(child, path) {
                return true;
            }
        }
        false
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.flat_view.get(self.selected_index).map(|e| e.path.as_path())
    }

    pub fn selected_kind(&self) -> Option<NodeKind> {
        self.flat_view.get(self.selected_index).map(|e| e.kind)
    }
}
