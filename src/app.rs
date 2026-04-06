//! Forge application shell: window, sidebar, tabs, file management.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use gpui::*;
use gpui_component::Root;
use gpui_component::input::{Input, InputState, InputEvent};
use gpui_component::menu::ContextMenuExt;
use gpui_component::theme::{Theme as GTheme, ThemeMode as GThemeMode};
use gpui_component::ActiveTheme;

use crate::editor::{self, Editor, EditorEvent, VaultFile};
use crate::graph::{GraphEvent, GraphView};
use crate::icons;
use crate::links::LinkIndex;
use crate::search::VaultSearch;
use crate::settings::Settings;
use crate::theme as t;

actions!(forge, [OpenFolder, Save, Quit, ToggleTheme, ToggleSidebar, NewFile, DeleteFile, CloseTab, NextTab, PrevTab, RefreshVault, ToggleReadableWidth, ToggleBacklinks, ShowFiles, ShowGraph, ShowSettings, ShowSearch, NavBack, NavForward, CtxOpen, CtxOpenNewTab, CtxRename, CtxDelete, CtxCopyPath, CtxReveal, CtxDuplicate, CtxNewFileHere, CtxNewFolderHere, CtxFolderRename, CtxFolderDelete]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanel {
    Files,
    Graph,
    Settings,
}

// ── File tree ──

#[derive(Debug)]
enum FileTreeEntry {
    File { name: String, path: PathBuf },
    Folder { name: String, path: String, children: Vec<FileTreeEntry> },
}

/// Walk directory and produce a tree of folders + .md files (including empty folders).
/// Also returns a flat list of all .md file paths for the files list.
fn walk_vault(dir: &Path) -> (Vec<FileTreeEntry>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let entries = walk_dir(dir, &mut files);
    (entries, files)
}

fn walk_dir(dir: &Path, all_files: &mut Vec<PathBuf>) -> Vec<FileTreeEntry> {
    let mut folders: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            // Skip hidden
            if name.starts_with('.') { continue; }
            if p.is_dir() {
                folders.push((name, p));
            } else {
                // Show .md files with display name (stripped extension)
                // Other file types: show full filename
                let is_md = p.extension().map_or(false, |e| e == "md");
                if is_md {
                    let display = if name.ends_with(".md") { name[..name.len()-3].to_string() } else { name.clone() };
                    files.push((display, p.clone()));
                    all_files.push(p);
                } else {
                    files.push((name, p));
                }
            }
        }
    }

    folders.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let mut entries = Vec::new();
    for (name, path) in folders {
        let children = walk_dir(&path, all_files);
        entries.push(FileTreeEntry::Folder {
            name,
            path: path.to_string_lossy().to_string(),
            children,
        });
    }
    for (name, path) in files {
        entries.push(FileTreeEntry::File { name, path });
    }
    entries
}

/// Reveal a file in the platform's file manager (non-blocking).
fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "linux")]
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("/"));
        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display())).spawn();
    }
}

fn display_name(path: &Path) -> String {
    let n = path.file_name().unwrap_or_default().to_string_lossy();
    if n.ends_with(".md") { n[..n.len()-3].to_string() } else { n.to_string() }
}

// ── App ──

/// A single open tab.
#[derive(Clone)]
struct Tab {
    path: PathBuf,
    name: String,
}

pub struct ForgeApp {
    focus_handle: FocusHandle,
    vault_path: Option<PathBuf>,
    vault_name: Option<String>,
    files: Vec<PathBuf>,
    file_tree: Vec<FileTreeEntry>,
    tabs: Vec<Tab>,
    active_tab: Option<usize>,
    editor: Entity<Editor>,
    sidebar_visible: bool,
    collapsed_folders: HashSet<String>,
    status_message: Option<String>,
    settings: Settings,
    _watcher: Option<notify::RecommendedWatcher>,
    title_input: Entity<InputState>,
    renaming_title: bool,
    readable_width: bool,
    link_index: LinkIndex,
    backlinks_visible: bool,
    scrollbar_drag: Option<ScrollbarDrag>,
    sidebar_drag: Option<f32>, // drag start x for sidebar resize
    side_panel: SidePanel,
    graph_view: Entity<GraphView>,
    /// Navigation history of opened file paths (for back/forward).
    nav_history: Vec<PathBuf>,
    nav_pos: usize,
    nav_in_flight: bool,
    /// Path targeted by the sidebar right-click menu.
    ctx_path: Option<PathBuf>,
    /// True if `ctx_path` is a directory (vs a file). Used to pick which menu
    /// + action set to show.
    ctx_is_folder: bool,
    /// Which font dropdown is currently expanded in settings: "body", "interface", "mono", or None.
    font_dropdown: Option<&'static str>,
    /// Whether settings have been modified since last save.
    settings_dirty: bool,
    vault_search: Option<VaultSearch>,
    search_query: String,
    search_results: Vec<crate::search::SearchResult>,
    search_input: Entity<InputState>,
    search_visible: bool,
}

#[derive(Clone, Debug)]
struct ScrollbarDrag {
    grab_offset: f32,    // offset inside the thumb when grab started
    track_height: f32,   // cached for drag math
    total_height: f32,
}

impl ForgeApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| Editor::new(cx));
        window.focus(&editor.focus_handle(cx));

        let settings = Settings::load();
        let theme_mode = if settings.theme == "light" { GThemeMode::Light } else { GThemeMode::Dark };
        GTheme::change(theme_mode, Some(window), cx);

        let title_input = cx.new(|cx| InputState::new(window, cx));

        // Subscribe to title input events
        cx.subscribe(&title_input, |this: &mut Self, _, event: &InputEvent, cx| {
            match event {
                InputEvent::PressEnter { .. } => this.commit_rename(cx),
                InputEvent::Blur => {
                    if this.renaming_title { this.commit_rename(cx); }
                }
                _ => {}
            }
        }).detach();

        // Subscribe to editor events (wikilink opens)
        cx.subscribe(&editor, |this: &mut Self, _, event: &EditorEvent, cx| {
            match event {
                EditorEvent::OpenWikilink { target, heading: _ } => {
                    this.open_note_by_name(target, cx);
                }
            }
        }).detach();

        // Graph view entity
        let graph_view = cx.new(|cx| GraphView::new(cx));
        // Search input
        let search_input = cx.new(|cx| InputState::new(window, cx));
        cx.subscribe(&search_input, |this: &mut Self, _, event: &InputEvent, cx| {
            match event {
                InputEvent::Change => {
                    this.search_query = this.search_input.read(cx).value().to_string();
                    // Execute search
                    if let Some(vs) = &this.vault_search {
                        if !this.search_query.trim().is_empty() {
                            this.search_results = vs.search(&this.search_query, 20).unwrap_or_default();
                        } else {
                            this.search_results.clear();
                        }
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    // Open the first result if available
                    if let Some(first) = this.search_results.first() {
                        let path = first.chunk.file_path.clone();
                        this.search_visible = false;
                        this.side_panel = SidePanel::Files;
                        this.open_path_as_tab(path, cx, false);
                    }
                }
                _ => {}
            }
        }).detach();

        cx.subscribe(&graph_view, |this: &mut Self, _, event: &GraphEvent, cx| {
            match event {
                GraphEvent::OpenNote(path) => {
                    this.side_panel = SidePanel::Files;
                    this.open_path_as_tab(path.clone(), cx, false);
                }
            }
        }).detach();

        let mut app = Self {
            focus_handle: cx.focus_handle(),
            vault_path: None, vault_name: None,
            files: Vec::new(), file_tree: Vec::new(),
            tabs: Vec::new(), active_tab: None,
            editor, sidebar_visible: true, collapsed_folders: HashSet::new(),
            status_message: None, settings,
            _watcher: None,
            title_input, renaming_title: false,
            readable_width: true,
            link_index: LinkIndex::new(),
            backlinks_visible: false,
            scrollbar_drag: None,
            sidebar_drag: None,
            side_panel: SidePanel::Files,
            graph_view,
            nav_history: Vec::new(),
            nav_pos: 0,
            nav_in_flight: false,
            ctx_path: None,
            ctx_is_folder: false,
            font_dropdown: None,
            settings_dirty: false,
            vault_search: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_input,
            search_visible: false,
        };

        // Auto-load last vault (or default test vault)
        if let Some(vp) = app.settings.resolved_vault_path() {
            app.load_vault_sync(vp);
            app.start_watcher(cx);
            app.push_vault_state_to_editor(cx);
            // Restore open tabs
            let tabs_to_open: Vec<PathBuf> = app.settings.open_tabs.iter()
                .filter(|p| p.exists())
                .cloned()
                .collect();
            let active = app.settings.active_tab;
            for path in &tabs_to_open {
                app.open_path_as_tab(path.clone(), cx, false);
            }
            if let Some(idx) = active.filter(|&i| i < app.tabs.len()) {
                app.switch_to_tab(idx, cx);
            }
        }
        app
    }

    // ── Vault ──

    fn open_folder(&mut self, _: &OpenFolder, _: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Select vault".into()) });
        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.into_iter().next() {
                    this.update(cx, |a, cx| {
                        a.load_vault_sync(p.clone());
                        a.settings.set_vault(&p);
                        a.tabs.clear();
                        a.active_tab = None;
                        a.editor.update(cx, |ed, _| ed.set_content(String::new()));
                        a.start_watcher(cx);
                        a.push_vault_state_to_editor(cx);
                        cx.notify();
                    }).ok();
                }
            }
        }).detach();
    }

    fn load_vault_sync(&mut self, path: PathBuf) {
        self.vault_name = path.file_name().map(|n| n.to_string_lossy().to_string());
        self.vault_path = Some(path.clone());
        self.collapsed_folders.clear();
        self.refresh_file_tree();
        // Build the wikilink index (scans content of every .md file).
        self.link_index = LinkIndex::scan_vault(&path);
        // Build search index
        let db_path = dirs::config_dir().unwrap_or_default().join("forge").join("forge.db");
        let idx_path = dirs::config_dir().unwrap_or_default().join("forge").join("vault.usearch");
        if let Ok(mut vs) = VaultSearch::new(&db_path, &idx_path) {
            let _ = vs.build_vault(&path);
            self.vault_search = Some(vs);
        }
    }

    fn refresh_file_tree(&mut self) {
        let Some(path) = self.vault_path.clone() else { return };
        let (tree, files) = walk_vault(&path);
        self.files = files;
        self.file_tree = tree;
    }

    /// Build the list of vault files for the editor's autocomplete.
    fn build_vault_files(&self) -> Vec<VaultFile> {
        let Some(root) = self.vault_path.as_ref() else { return Vec::new(); };
        let mut out: Vec<VaultFile> = self.files.iter().filter_map(|abs| {
            let basename = abs.file_stem()?.to_str()?.to_string();
            let rel = abs.strip_prefix(root).unwrap_or(abs);
            let rel_str = rel.to_string_lossy();
            let rel_display = if let Some(s) = rel_str.strip_suffix(".md") { s.to_string() } else { rel_str.to_string() };
            Some(VaultFile { basename, rel_path: rel_display, abs_path: abs.clone() })
        }).collect();
        out.sort_by(|a, b| a.basename.to_lowercase().cmp(&b.basename.to_lowercase()));
        out
    }

    /// Push the current vault state (known note names + file list) to the editor.
    fn push_vault_state_to_editor(&self, cx: &mut Context<Self>) {
        let known: HashSet<String> = self.files.iter()
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()))
            .collect();
        let vault_files = self.build_vault_files();
        let vault_root = self.vault_path.clone();
        let (bf, mf, fs) = (self.settings.body_font.clone(), self.settings.mono_font.clone(), self.settings.font_size);
        self.editor.update(cx, |ed, cx| {
            ed.set_known_notes(known);
            ed.set_vault_files(vault_files);
            ed.set_vault_root(vault_root);
            ed.set_fonts(&bf, &mf, fs);
            cx.notify();
        });
        // Rebuild graph data from the link index.
        let link_index_ref = &self.link_index;
        self.graph_view.update(cx, |g, cx| {
            g.set_data(link_index_ref);
            cx.notify();
        });
    }

    /// Resolve a wikilink target and open the matching note in a tab.
    fn open_note_by_name(&mut self, target: &str, cx: &mut Context<Self>) {
        let Some(path) = self.link_index.resolve(target).map(|p| p.to_path_buf()) else {
            self.status_message = Some(format!("No note matching [[{}]]", target));
            cx.notify();
            return;
        };
        self.open_path_as_tab(path, cx, false);
    }

    fn refresh_vault(&mut self, _: &RefreshVault, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh_file_tree();
        if let Some(vp) = self.vault_path.clone() {
            self.link_index = LinkIndex::scan_vault(&vp);
        }
        self.push_vault_state_to_editor(cx);
        self.status_message = Some("Refreshed".into());
        cx.notify();
    }

    fn start_watcher(&mut self, cx: &mut Context<Self>) {
        use notify::{Watcher, RecursiveMode, RecommendedWatcher, Config};
        let Some(path) = self.vault_path.clone() else { return };

        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, _>| {
                if res.is_ok() { let _ = tx.send(()); }
            },
            Config::default(),
        );
        if let Ok(mut w) = watcher {
            if w.watch(&path, RecursiveMode::Recursive).is_ok() {
                self._watcher = Some(w);
                // Poll for changes
                cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    loop {
                        cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                        let mut changed = false;
                        while rx.try_recv().is_ok() { changed = true; }
                        if changed {
                            let ok = this.update(cx, |a, cx| {
                                // Only refresh file tree (cheap walk). Skip the expensive
                                // full-vault link index rescan -- it re-reads every .md
                                // file's content. Instead, incremental updates happen on save.
                                // Full rescan only on explicit Refresh (F5/Ctrl+R).
                                a.refresh_file_tree();
                                a.push_vault_state_to_editor(cx);
                                cx.notify();
                            }).is_ok();
                            if !ok { break; }
                        }
                    }
                }).detach();
            }
        }
    }

    // ── Tabs ──

    fn open_path_as_tab(&mut self, path: PathBuf, cx: &mut Context<Self>, new_tab: bool) {
        // Check if already open
        if let Some(existing) = self.tabs.iter().position(|t| t.path == path) {
            self.switch_to_tab(existing, cx);
            return;
        }

        let name = display_name(&path);
        let tab = Tab { path: path.clone(), name };

        if new_tab || self.active_tab.is_none() {
            self.tabs.push(tab);
            self.active_tab = Some(self.tabs.len() - 1);
        } else {
            // Replace current tab
            if let Some(idx) = self.active_tab {
                self.tabs[idx] = tab;
            }
        }

        self.load_active_tab(cx);
        self.persist_tabs();
    }

    fn switch_to_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() { return; }
        self.active_tab = Some(index);
        self.load_active_tab(cx);
        self.persist_tabs();
    }

    fn load_active_tab(&mut self, cx: &mut Context<Self>) {
        let Some(idx) = self.active_tab else { return };
        let Some(tab) = self.tabs.get(idx) else { return };
        let path = tab.path.clone();
        let content = fs::read_to_string(&path).unwrap_or_default();
        self.editor.update(cx, |ed, _| ed.set_content(content));
        self.status_message = None;
        // Push to navigation history unless we're replaying from history.
        if !self.nav_in_flight {
            // Truncate forward history (anything after current position).
            if self.nav_pos + 1 < self.nav_history.len() {
                self.nav_history.truncate(self.nav_pos + 1);
            }
            // Don't duplicate if identical to current head.
            let already_head = self.nav_history.last().map(|p| p == &path).unwrap_or(false);
            if !already_head {
                self.nav_history.push(path);
                self.nav_pos = self.nav_history.len().saturating_sub(1);
            }
        }
        cx.notify();
    }

    fn close_tab_at(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() { return; }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active_tab = None;
            self.editor.update(cx, |ed, _| ed.set_content(String::new()));
        } else {
            let new_active = if let Some(active) = self.active_tab {
                if active >= self.tabs.len() { self.tabs.len() - 1 }
                else if index < active { active - 1 }
                else { active.min(self.tabs.len() - 1) }
            } else { 0 };
            self.active_tab = Some(new_active);
            self.load_active_tab(cx);
        }
        self.persist_tabs();
        cx.notify();
    }

    fn close_current_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.active_tab { self.close_tab_at(idx, cx); }
    }

    fn next_tab(&mut self, _: &NextTab, _: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() { return; }
        let next = self.active_tab.map(|i| (i + 1) % self.tabs.len()).unwrap_or(0);
        self.switch_to_tab(next, cx);
    }

    fn prev_tab(&mut self, _: &PrevTab, _: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() { return; }
        let prev = self.active_tab.map(|i| if i == 0 { self.tabs.len() - 1 } else { i - 1 }).unwrap_or(0);
        self.switch_to_tab(prev, cx);
    }

    fn persist_tabs(&mut self) {
        self.settings.open_tabs = self.tabs.iter().map(|t| t.path.clone()).collect();
        self.settings.active_tab = self.active_tab;
        self.settings.save();
    }

    // ── File operations ──

    fn save(&mut self, _: &Save, _: &mut Window, cx: &mut Context<Self>) {
        let Some(idx) = self.active_tab else { return };
        let Some(tab) = self.tabs.get(idx) else { return };
        let path = tab.path.clone();
        let content = self.editor.read(cx).buffer.text();
        match fs::write(&path, &content) {
            Ok(()) => {
                self.editor.update(cx, |ed, _| ed.buffer.mark_clean());
                self.link_index.update_file(&path, &content);
                self.push_vault_state_to_editor(cx);
                self.status_message = Some("Saved".into());
            }
            Err(e) => self.status_message = Some(format!("Error: {}", e)),
        }
        cx.notify();
    }

    fn new_file(&mut self, _: &NewFile, _: &mut Window, cx: &mut Context<Self>) {
        let Some(vault_path) = self.vault_path.clone() else { return };
        let mut name = "Untitled".to_string();
        let mut counter = 1u32;
        while vault_path.join(format!("{}.md", name)).exists() {
            counter += 1; name = format!("Untitled {}", counter);
        }
        let file_path = vault_path.join(format!("{}.md", name));
        if fs::write(&file_path, "").is_ok() {
            self.load_vault_sync(vault_path);
            self.push_vault_state_to_editor(cx);
            self.open_path_as_tab(file_path, cx, true);
            self.status_message = Some(format!("Created {}.md", name));
            cx.notify();
        }
    }

    fn delete_file(&mut self, _: &DeleteFile, _: &mut Window, cx: &mut Context<Self>) {
        let Some(idx) = self.active_tab else { return };
        let Some(tab) = self.tabs.get(idx).cloned() else { return };
        if fs::remove_file(&tab.path).is_ok() {
            self.close_tab_at(idx, cx);
            self.link_index.remove_file(&tab.path);
            if let Some(vp) = self.vault_path.clone() { self.load_vault_sync(vp); }
            self.push_vault_state_to_editor(cx);
            self.status_message = Some("Deleted".into());
            cx.notify();
        }
    }

    // ── Theme/sidebar ──

    fn toggle_theme(&mut self, _: &ToggleTheme, w: &mut Window, cx: &mut Context<Self>) {
        let new = if cx.theme().mode.is_dark() { GThemeMode::Light } else { GThemeMode::Dark };
        GTheme::change(new, Some(w), cx);
        self.settings.theme = if new.is_dark() { "dark" } else { "light" }.into();
        self.settings.save();
        w.refresh(); cx.notify();
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, w: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible; w.refresh(); cx.notify();
    }

    fn toggle_readable_width(&mut self, _: &ToggleReadableWidth, w: &mut Window, cx: &mut Context<Self>) {
        self.readable_width = !self.readable_width; w.refresh(); cx.notify();
    }

    fn toggle_backlinks(&mut self, _: &ToggleBacklinks, w: &mut Window, cx: &mut Context<Self>) {
        self.backlinks_visible = !self.backlinks_visible; w.refresh(); cx.notify();
    }

    fn nav_back(&mut self, _: &NavBack, _: &mut Window, cx: &mut Context<Self>) {
        if self.nav_pos == 0 || self.nav_history.is_empty() { return; }
        self.nav_pos -= 1;
        let target = self.nav_history[self.nav_pos].clone();
        self.nav_in_flight = true;
        self.open_path_as_tab(target, cx, false);
        self.nav_in_flight = false;
    }

    fn nav_forward(&mut self, _: &NavForward, _: &mut Window, cx: &mut Context<Self>) {
        if self.nav_pos + 1 >= self.nav_history.len() { return; }
        self.nav_pos += 1;
        let target = self.nav_history[self.nav_pos].clone();
        self.nav_in_flight = true;
        self.open_path_as_tab(target, cx, false);
        self.nav_in_flight = false;
    }

    // ── Sidebar right-click context menu ──

    fn ctx_open(&mut self, _: &CtxOpen, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.ctx_path.clone() { self.open_path_as_tab(p, cx, false); }
    }
    fn ctx_open_new_tab(&mut self, _: &CtxOpenNewTab, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.ctx_path.clone() { self.open_path_as_tab(p, cx, true); }
    }
    fn ctx_rename(&mut self, _: &CtxRename, w: &mut Window, cx: &mut Context<Self>) {
        // Open the clicked file as active tab first, then start rename on title.
        if let Some(p) = self.ctx_path.clone() {
            self.open_path_as_tab(p, cx, false);
            self.start_rename(w, cx);
        }
    }
    fn ctx_delete(&mut self, _: &CtxDelete, _: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.ctx_path.clone() else { return; };
        // Close any tab pointing to this path.
        let tab_idx = self.tabs.iter().position(|t| t.path == p);
        if fs::remove_file(&p).is_ok() {
            if let Some(idx) = tab_idx { self.close_tab_at(idx, cx); }
            self.link_index.remove_file(&p);
            if let Some(vp) = self.vault_path.clone() { self.load_vault_sync(vp); }
            self.push_vault_state_to_editor(cx);
            self.status_message = Some("Deleted".into());
            cx.notify();
        }
    }
    fn ctx_copy_path(&mut self, _: &CtxCopyPath, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.ctx_path.clone() {
            let s = p.to_string_lossy().to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(s));
            self.status_message = Some("Path copied".into());
            cx.notify();
        }
    }
    fn ctx_reveal(&mut self, _: &CtxReveal, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.ctx_path.clone() {
            reveal_in_file_manager(&p);
            self.status_message = Some("Revealing…".into());
            cx.notify();
        }
    }
    fn ctx_new_file_here(&mut self, _: &CtxNewFileHere, _: &mut Window, cx: &mut Context<Self>) {
        // Target directory: the ctx_path if it's a folder, else the parent of the file.
        let Some(base) = self.ctx_path.clone() else { return; };
        let dir = if base.is_dir() { base } else { base.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| self.vault_path.clone().unwrap_or_default()) };
        let mut name = "Untitled".to_string();
        let mut n = 1u32;
        while dir.join(format!("{}.md", name)).exists() {
            n += 1; name = format!("Untitled {}", n);
        }
        let file_path = dir.join(format!("{}.md", name));
        if fs::write(&file_path, "").is_ok() {
            if let Some(vp) = self.vault_path.clone() { self.load_vault_sync(vp); }
            self.push_vault_state_to_editor(cx);
            self.open_path_as_tab(file_path, cx, true);
            self.status_message = Some(format!("Created {}.md", name));
            cx.notify();
        }
    }
    fn ctx_new_folder_here(&mut self, _: &CtxNewFolderHere, _: &mut Window, cx: &mut Context<Self>) {
        let Some(base) = self.ctx_path.clone() else { return; };
        let parent = if base.is_dir() { base } else { base.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| self.vault_path.clone().unwrap_or_default()) };
        let mut name = "New folder".to_string();
        let mut n = 1u32;
        while parent.join(&name).exists() {
            n += 1; name = format!("New folder {}", n);
        }
        let folder_path = parent.join(&name);
        if fs::create_dir(&folder_path).is_ok() {
            if let Some(vp) = self.vault_path.clone() { self.load_vault_sync(vp); }
            self.push_vault_state_to_editor(cx);
            self.status_message = Some(format!("Created folder {}", name));
            cx.notify();
        }
    }
    fn ctx_folder_rename(&mut self, _: &CtxFolderRename, _w: &mut Window, cx: &mut Context<Self>) {
        // Folder rename isn't hooked into the title input UI yet. For now,
        // just surface a hint so the user can handle via file manager.
        self.status_message = Some("Folder rename: use Reveal in file manager for now".into());
        cx.notify();
    }
    fn ctx_folder_delete(&mut self, _: &CtxFolderDelete, _: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.ctx_path.clone() else { return; };
        if !p.is_dir() { return; }
        // Only delete if empty — require user to clear first.
        let is_empty = fs::read_dir(&p).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !is_empty {
            self.status_message = Some("Folder not empty — delete files first".into());
            cx.notify();
            return;
        }
        if fs::remove_dir(&p).is_ok() {
            if let Some(vp) = self.vault_path.clone() { self.load_vault_sync(vp); }
            self.push_vault_state_to_editor(cx);
            self.status_message = Some("Folder deleted".into());
            cx.notify();
        }
    }
    fn ctx_duplicate(&mut self, _: &CtxDuplicate, _: &mut Window, cx: &mut Context<Self>) {
        let Some(src) = self.ctx_path.clone() else { return; };
        let Some(parent) = src.parent() else { return; };
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("md").to_string();
        // Find unique name: "{stem} copy.ext", "{stem} copy 2.ext", ...
        let mut candidate = parent.join(format!("{} copy.{}", stem, ext));
        let mut n = 2u32;
        while candidate.exists() {
            candidate = parent.join(format!("{} copy {}.{}", stem, n, ext));
            n += 1;
        }
        if fs::copy(&src, &candidate).is_ok() {
            if let Some(vp) = self.vault_path.clone() { self.load_vault_sync(vp); }
            self.push_vault_state_to_editor(cx);
            self.status_message = Some(format!("Duplicated to {}", candidate.file_name().and_then(|n| n.to_str()).unwrap_or("")));
            cx.notify();
        }
    }

    fn show_files(&mut self, _: &ShowFiles, _w: &mut Window, cx: &mut Context<Self>) {
        self.side_panel = SidePanel::Files; cx.notify();
    }
    fn show_graph(&mut self, _: &ShowGraph, _w: &mut Window, cx: &mut Context<Self>) {
        self.side_panel = SidePanel::Graph; cx.notify();
    }
    fn show_settings(&mut self, _: &ShowSettings, _w: &mut Window, cx: &mut Context<Self>) {
        self.side_panel = SidePanel::Settings; cx.notify();
    }
    fn show_search(&mut self, _: &ShowSearch, w: &mut Window, cx: &mut Context<Self>) {
        self.search_visible = !self.search_visible;
        if self.search_visible {
            w.focus(&self.search_input.read(cx).focus_handle(cx));
        }
        cx.notify();
    }

    fn _consume_backspace(&mut self, _: &editor::Backspace, _: &mut Window, _: &mut Context<Self>) {}
    fn _consume_delete(&mut self, _: &editor::Delete, _: &mut Window, _: &mut Context<Self>) {}

    fn toggle_read_mode(&mut self, _: &editor::ToggleReadMode, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.read_mode = !ed.read_mode;
            cx.notify();
        });
        cx.notify();
    }

    // ── Context menu action forwarders ──
    // These let the context menu dispatch actions that reach the editor.

    fn fwd_cut(&mut self, _: &editor::Cut, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_cut(&editor::Cut, w, cx));
    }
    fn fwd_copy(&mut self, _: &editor::Copy, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_copy(&editor::Copy, w, cx));
    }
    fn fwd_paste(&mut self, _: &editor::Paste, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_paste(&editor::Paste, w, cx));
    }
    fn fwd_select_all(&mut self, _: &editor::SelectAll, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_select_all(&editor::SelectAll, w, cx));
    }
    fn fwd_select_line(&mut self, _: &editor::SelectLine, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_select_line(&editor::SelectLine, w, cx));
    }
    fn fwd_undo(&mut self, _: &editor::Undo, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_undo(&editor::Undo, w, cx));
    }
    fn fwd_bold(&mut self, _: &editor::ToggleBold, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_toggle_bold(&editor::ToggleBold, w, cx));
    }
    fn fwd_italic(&mut self, _: &editor::ToggleItalic, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_toggle_italic(&editor::ToggleItalic, w, cx));
    }
    fn fwd_code(&mut self, _: &editor::ToggleCode, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_toggle_code(&editor::ToggleCode, w, cx));
    }
    fn fwd_strike(&mut self, _: &editor::ToggleStrikethrough, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_toggle_strikethrough(&editor::ToggleStrikethrough, w, cx));
    }
    fn fwd_h1(&mut self, _: &editor::InsertHeading1, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_h1(&editor::InsertHeading1, w, cx));
    }
    fn fwd_h2(&mut self, _: &editor::InsertHeading2, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_h2(&editor::InsertHeading2, w, cx));
    }
    fn fwd_h3(&mut self, _: &editor::InsertHeading3, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_h3(&editor::InsertHeading3, w, cx));
    }
    fn fwd_bullet(&mut self, _: &editor::InsertBulletList, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_bullet(&editor::InsertBulletList, w, cx));
    }
    fn fwd_numbered(&mut self, _: &editor::InsertNumberedList, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_numbered(&editor::InsertNumberedList, w, cx));
    }
    fn fwd_table(&mut self, _: &editor::InsertTable, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_table(&editor::InsertTable, w, cx));
    }
    fn fwd_code_block(&mut self, _: &editor::InsertCodeBlock, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_code_block(&editor::InsertCodeBlock, w, cx));
    }
    fn fwd_hr(&mut self, _: &editor::InsertHorizontalRule, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_insert_hr(&editor::InsertHorizontalRule, w, cx));
    }
    fn fwd_zoom_in(&mut self, _: &editor::ZoomIn, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_zoom_in(&editor::ZoomIn, w, cx));
    }
    fn fwd_zoom_out(&mut self, _: &editor::ZoomOut, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_zoom_out(&editor::ZoomOut, w, cx));
    }
    fn fwd_zoom_reset(&mut self, _: &editor::ZoomReset, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| ed.on_zoom_reset(&editor::ZoomReset, w, cx));
    }

    fn start_rename(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(idx) = self.active_tab else { return };
        let Some(tab) = self.tabs.get(idx) else { return };
        let current_name = tab.name.clone();
        self.title_input.update(cx, |state, cx| {
            state.set_value(current_name, w, cx);
        });
        self.renaming_title = true;
        w.focus(&self.title_input.read(cx).focus_handle(cx));
        cx.notify();
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        if !self.renaming_title { return; }
        self.renaming_title = false;

        let Some(idx) = self.active_tab else { cx.notify(); return };
        let Some(tab) = self.tabs.get(idx).cloned() else { cx.notify(); return };

        let new_name = self.title_input.read(cx).value().to_string();
        let new_name = new_name.trim();

        // Sanitize
        let sanitized: String = new_name.chars()
            .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            .collect();

        if sanitized.is_empty() || sanitized == tab.name {
            cx.notify();
            return;
        }

        let parent = tab.path.parent().unwrap_or(std::path::Path::new(""));
        let new_path = parent.join(format!("{}.md", sanitized));

        if new_path.exists() && new_path != tab.path {
            self.status_message = Some("A file with that name already exists".into());
            cx.notify();
            return;
        }

        if fs::rename(&tab.path, &new_path).is_ok() {
            // Update tab
            self.tabs[idx].path = new_path.clone();
            self.tabs[idx].name = sanitized.clone();
            // Update link index: drop old path, register new; reindex content so outgoing
            // links are re-keyed to the renamed source.
            self.link_index.remove_file(&tab.path);
            let content = fs::read_to_string(&new_path).unwrap_or_default();
            self.link_index.update_file(&new_path, &content);
            // Refresh file tree
            self.refresh_file_tree();
            self.push_vault_state_to_editor(cx);
            self.persist_tabs();
            self.status_message = Some(format!("Renamed to {}.md", sanitized));
        } else {
            self.status_message = Some("Rename failed".into());
        }
        cx.notify();
    }

    // ── Rendering helpers ──

    fn is_path_active(&self, path: &Path) -> bool {
        self.active_tab
            .and_then(|i| self.tabs.get(i))
            .map(|t| t.path == path)
            .unwrap_or(false)
    }

    fn render_backlinks_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.backlinks_visible { return None; }
        let Some(idx) = self.active_tab else { return None; };
        let Some(tab) = self.tabs.get(idx) else { return None; };
        let current_path = tab.path.clone();

        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let border = cx.theme().border;
        let topbar_bg = cx.theme().tab_bar;

        let backlinks = self.link_index.backlinks_for_path(&current_path);
        let count = backlinks.len();

        let mut items: Vec<AnyElement> = Vec::new();
        if count == 0 {
            items.push(div().px(px(14.)).py(px(12.)).text_size(px(t::FONT_SM)).text_color(muted.opacity(0.6))
                .child("No backlinks").into_any_element());
        } else {
            for (i, lref) in backlinks.iter().enumerate() {
                let source = lref.source.clone();
                let line_num = lref.line;
                let source_name = source.file_stem().and_then(|s| s.to_str()).unwrap_or("?").to_string();
                let preview = lref.context.clone();
                items.push(
                    div()
                        .id(ElementId::NamedInteger("bl".into(), i as u64))
                        .flex().flex_col().gap(px(2.))
                        .px(px(14.)).py(px(6.))
                        .min_h(px(t::BACKLINKS_ITEM_HEIGHT))
                        .border_b_1().border_color(border.opacity(0.5))
                        .cursor_pointer()
                        .hover(move |s| s.bg(accent.opacity(0.06)))
                        .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            this.open_path_as_tab(source.clone(), cx, false);
                            // Best-effort: scroll to the referencing line.
                            let _ = line_num;
                        }))
                        .child(div().text_size(px(t::FONT_UI)).font_weight(FontWeight::MEDIUM).text_color(fg).child(source_name))
                        .child(div().text_size(px(t::FONT_SM)).text_color(muted).overflow_hidden().whitespace_nowrap().text_ellipsis().child(preview))
                        .into_any_element()
                );
            }
        }

        let mut list = div().id("backlinks-list").flex().flex_col().flex_1().overflow_y_scroll();
        for item in items { list = list.child(item); }

        Some(
            div().flex().flex_col().h(px(t::BACKLINKS_PANEL_HEIGHT)).flex_shrink_0()
                .bg(topbar_bg).border_t_1().border_color(border)
                .child(
                    div().flex().items_center().justify_between().h(px(28.)).px(px(14.)).flex_shrink_0()
                        .border_b_1().border_color(border)
                        .child(div().text_size(px(t::FONT_TINY)).font_weight(FontWeight::SEMIBOLD).text_color(muted)
                            .child(format!("BACKLINKS ({})", count)))
                        .child(
                            div().id("bl-close").w(px(18.)).h(px(18.)).flex().items_center().justify_center()
                                .rounded(px(3.)).text_size(px(12.)).text_color(muted.opacity(0.6))
                                .cursor_pointer()
                                .hover(move |s| s.bg(accent.opacity(0.15)).text_color(fg))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.toggle_backlinks(&ToggleBacklinks, w, cx)))
                                .child("\u{2715}")
                        )
                )
                .child(list)
                .into_any_element()
        )
    }

    fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let sidebar_bg = cx.theme().sidebar;
        let border = cx.theme().border;

        let sel = self.side_panel;

        let files_active = sel == SidePanel::Files;
        let graph_active = sel == SidePanel::Graph;
        let settings_active = sel == SidePanel::Settings;

        let files_btn = div().id("rail-files")
            .flex().items_center().justify_center()
            .w(px(32.)).h(px(32.)).rounded(px(t::RADIUS_MD))
            .bg(if files_active { accent.opacity(0.18) } else { transparent_black() })
            .cursor_pointer().hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.10)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.show_files(&ShowFiles, w, cx)))
            .child(icons::rail_files_icon(if files_active { fg } else { muted }));

        let search_btn = div().id("rail-search")
            .flex().items_center().justify_center()
            .w(px(32.)).h(px(32.)).rounded(px(t::RADIUS_MD))
            .bg(transparent_black())
            .cursor_pointer().hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.10)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.show_search(&ShowSearch, w, cx)))
            .child(icons::rail_search_icon(muted));

        let graph_btn = div().id("rail-graph")
            .flex().items_center().justify_center()
            .w(px(32.)).h(px(32.)).rounded(px(t::RADIUS_MD))
            .bg(if graph_active { accent.opacity(0.18) } else { transparent_black() })
            .cursor_pointer().hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.10)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.show_graph(&ShowGraph, w, cx)))
            .child(icons::rail_graph_icon(if graph_active { fg } else { muted }));

        let settings_btn = div().id("rail-settings")
            .flex().items_center().justify_center()
            .w(px(32.)).h(px(32.)).rounded(px(t::RADIUS_MD))
            .bg(if settings_active { accent.opacity(0.18) } else { transparent_black() })
            .cursor_pointer().hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.10)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.show_settings(&ShowSettings, w, cx)))
            .child(icons::rail_settings_icon(if settings_active { fg } else { muted }));

        div().flex().flex_col().items_center().gap(px(4.))
            .w(px(44.)).h_full().flex_shrink_0()
            .bg(sidebar_bg).border_r_1().border_color(border)
            .pt(px(10.))
            .child(files_btn)
            .child(search_btn)
            .child(graph_btn)
            .child(settings_btn)
    }

    fn render_settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let accent = cx.theme().accent;
        let card_bg = cx.theme().tab_bar;

        let vault_path_str = self.vault_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(no vault)".to_string());
        let theme_name = self.settings.theme.clone();
        let zoom = self.editor.read(cx).zoom;
        let body_font = self.settings.body_font.clone();
        let interface_font = self.settings.interface_font.clone();
        let mono_font = self.settings.mono_font.clone();
        let font_size = self.settings.font_size;
        let is_dirty = self.settings_dirty;

        use crate::settings::{BODY_FONTS, MONO_FONTS};

        // ---- helpers ----
        let section = |title: &str| -> Div {
            div().text_size(px(11.)).text_color(muted.opacity(0.6))
                .font_weight(FontWeight::BOLD).pt(px(24.)).pb(px(8.))
                .child(title.to_string())
        };
        let card_start = || -> Div {
            div().flex().flex_col()
                .rounded(px(t::RADIUS_LG)).border_1().border_color(border)
                .bg(card_bg).overflow_hidden()
        };
        let row_el = |label: &str, value: AnyElement, last: bool| -> AnyElement {
            let mut r = div().flex().flex_row().items_center().justify_between()
                .py(px(10.)).px(px(14.))
                .child(div().text_size(px(t::FONT_SM)).text_color(muted).child(label.to_string()))
                .child(value);
            if !last { r = r.border_b_1().border_color(border.opacity(0.4)); }
            r.into_any_element()
        };
        // Font dropdown button + expandable list
        let font_dropdown_el = |id: &'static str, current: &str, list: &'static [&'static str],
                                 open: bool, setter: fn(&mut Settings, String)| -> AnyElement {
            let cur = current.to_string();
            let mut col = div().flex().flex_col().relative();
            // Button row
            col = col.child(
                div().id(ElementId::Name(id.into()))
                    .flex().flex_row().items_center().gap(px(4.))
                    .px(px(10.)).py(px(4.)).rounded(px(t::RADIUS_SM))
                    .border_1().border_color(border.opacity(0.5))
                    .bg(card_bg).text_size(px(t::FONT_SM)).text_color(fg)
                    .cursor_pointer()
                    .hover(move |s: gpui::StyleRefinement| s.border_color(accent.opacity(0.6)))
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        this.font_dropdown = if this.font_dropdown == Some(id) { None } else { Some(id) };
                        cx.notify();
                    }))
                    .child(cur.clone())
                    .child(div().text_size(px(9.)).text_color(muted.opacity(0.6)).child(if open { "\u{25B4}" } else { "\u{25BE}" }))
            );
            // Expanded list
            if open {
                let mut items = div().flex().flex_col().pt(px(4.)).pb(px(2.));
                for &font in list {
                    let is_sel = font == cur.as_str();
                    let font_str = font.to_string();
                    items = items.child(
                        div().id(ElementId::Name(format!("{}-{}", id, font).into()))
                            .py(px(4.)).px(px(10.)).rounded(px(t::RADIUS_SM))
                            .text_size(px(t::FONT_SM))
                            .text_color(if is_sel { fg } else { muted })
                            .font_weight(if is_sel { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                            .cursor_pointer()
                            .hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.10)))
                            .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                setter(&mut this.settings, font_str.clone());
                                this.font_dropdown = None;
                                this.settings_dirty = true;
                                cx.notify();
                            }))
                            .child(font.to_string())
                    );
                }
                col = col.child(
                    div().id(ElementId::Name(format!("{}-list", id).into()))
                        .absolute().top(px(34.)).left(px(0.)).w(px(240.))
                        .max_h(px(200.)).overflow_y_scroll()
                        .border_1().border_color(border).rounded(px(t::RADIUS_SM))
                        .bg(card_bg).shadow_lg()
                        .child(items)
                );
            }
            col.into_any_element()
        };
        // Font size row
        let size_row = div().flex().flex_row().items_center().gap(px(8.))
            .child(
                div().id("fs-dn").w(px(28.)).h(px(28.)).flex().items_center().justify_center()
                    .rounded(px(t::RADIUS_SM)).border_1().border_color(border.opacity(0.5))
                    .bg(card_bg).text_size(px(14.)).text_color(fg).cursor_pointer()
                    .hover(move |s: gpui::StyleRefinement| s.border_color(accent))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.settings.font_size = (this.settings.font_size - 1.0).clamp(10.0, 28.0);
                        this.settings_dirty = true; cx.notify();
                    }))
                    .child("\u{2212}") // minus sign
            )
            .child(div().text_size(px(t::FONT_SM)).text_color(fg).min_w(px(40.)).flex().items_center().justify_center()
                .child(format!("{:.0} px", font_size)))
            .child(
                div().id("fs-up").w(px(28.)).h(px(28.)).flex().items_center().justify_center()
                    .rounded(px(t::RADIUS_SM)).border_1().border_color(border.opacity(0.5))
                    .bg(card_bg).text_size(px(14.)).text_color(fg).cursor_pointer()
                    .hover(move |s: gpui::StyleRefinement| s.border_color(accent))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.settings.font_size = (this.settings.font_size + 1.0).clamp(10.0, 28.0);
                        this.settings_dirty = true; cx.notify();
                    }))
                    .child("+")
            );
        let body_open = self.font_dropdown == Some("font-body");
        let iface_open = self.font_dropdown == Some("font-iface");
        let mono_open = self.font_dropdown == Some("font-mono");

        // ---- compose ----
        div().id("settings-scroll").size_full().bg(cx.theme().background).overflow_y_scroll()
            .child(
                div().max_w(px(680.)).mx_auto().px(px(40.)).py(px(36.))
                    .child(div().text_size(px(24.)).font_weight(FontWeight::BOLD).text_color(fg).pb(px(4.)).child("Settings"))
                    .child(div().text_size(px(t::FONT_SM)).text_color(muted.opacity(0.6)).pb(px(8.))
                        .child("Preferences are saved to ~/.config/forge/settings.json"))
                    // General
                    .child(section("GENERAL"))
                    .child(card_start()
                        .child(row_el("Vault", div().text_size(px(t::FONT_SM)).text_color(fg).max_w(px(400.)).overflow_hidden().text_ellipsis().whitespace_nowrap().child(vault_path_str).into_any_element(), false))
                        .child(row_el("Theme", div().text_size(px(t::FONT_SM)).text_color(fg).child(theme_name).into_any_element(), false))
                        .child(row_el("Zoom", div().text_size(px(t::FONT_SM)).text_color(fg).child(format!("{:.0}%", zoom * 100.0)).into_any_element(), false))
                        .child(row_el("Notes", div().text_size(px(t::FONT_SM)).text_color(fg).child(format!("{}", self.files.len())).into_any_element(), true))
                    )
                    // Fonts
                    .child(section("FONTS"))
                    .child(card_start()
                        .child(row_el("Text font",
                            font_dropdown_el("font-body", &body_font, BODY_FONTS, body_open, |s, v| s.body_font = v), false))
                        .child(row_el("Interface font",
                            font_dropdown_el("font-iface", &interface_font, BODY_FONTS, iface_open, |s, v| s.interface_font = v), false))
                        .child(row_el("Monospace font",
                            font_dropdown_el("font-mono", &mono_font, MONO_FONTS, mono_open, |s, v| s.mono_font = v), false))
                        .child(row_el("Font size", size_row.into_any_element(), true))
                    )
                    // Shortcuts
                    .child(section("KEYBOARD SHORTCUTS"))
                    .child(card_start()
                        .child(row_el("Toggle theme", div().text_size(px(t::FONT_SM)).text_color(fg).child("Ctrl+Shift+T").into_any_element(), false))
                        .child(row_el("Open vault", div().text_size(px(t::FONT_SM)).text_color(fg).child("Ctrl+O").into_any_element(), false))
                        .child(row_el("Zoom in / out / reset", div().text_size(px(t::FONT_SM)).text_color(fg).child("Ctrl+= / - / 0").into_any_element(), false))
                        .child(row_el("Toggle backlinks", div().text_size(px(t::FONT_SM)).text_color(fg).child("Ctrl+Shift+B").into_any_element(), false))
                        .child(row_el("Back / Forward", div().text_size(px(t::FONT_SM)).text_color(fg).child("Alt+Left / Right").into_any_element(), false))
                        .child(row_el("Toggle sidebar", div().text_size(px(t::FONT_SM)).text_color(fg).child("Ctrl+B").into_any_element(), true))
                    )
                    // Action buttons
                    .child(
                        div().pt(px(24.)).flex().flex_row().gap(px(10.))
                            .child(
                                div().id("settings-apply")
                                    .flex().items_center().justify_center()
                                    .px(px(18.)).py(px(8.))
                                    .rounded(px(t::RADIUS_MD))
                                    .bg(if is_dirty { accent } else { accent.opacity(0.3) })
                                    .text_color(if is_dirty { gpui::white() } else { muted })
                                    .text_size(px(t::FONT_SM)).font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.85)))
                                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.settings.save();
                                        this.settings_dirty = false;
                                        this.status_message = Some("Settings saved".into());
                                        cx.notify();
                                    }))
                                    .child("Apply")
                            )
                            .child(
                                div().id("settings-reset")
                                    .flex().items_center().justify_center()
                                    .px(px(18.)).py(px(8.))
                                    .rounded(px(t::RADIUS_MD))
                                    .border_1().border_color(border)
                                    .text_color(muted).text_size(px(t::FONT_SM))
                                    .cursor_pointer()
                                    .hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.08)))
                                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.settings.body_font = crate::settings::default_body_font();
                                        this.settings.interface_font = crate::settings::default_interface_font();
                                        this.settings.mono_font = crate::settings::default_mono_font();
                                        this.settings.font_size = crate::settings::default_font_size();
                                        this.settings.save();
                                        this.settings_dirty = false;
                                        this.status_message = Some("Reset to defaults".into());
                                        cx.notify();
                                    }))
                                    .child("Reset defaults")
                            )
                            .child(
                                div().id("settings-open-vault")
                                    .flex().items_center().justify_center()
                                    .px(px(18.)).py(px(8.))
                                    .rounded(px(t::RADIUS_MD))
                                    .border_1().border_color(border)
                                    .text_color(muted).text_size(px(t::FONT_SM))
                                    .cursor_pointer()
                                    .hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.08)))
                                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.open_folder(&OpenFolder, w, cx)))
                                    .child("Open vault...")
                            )
                    )
            )
            .into_any_element()
    }

    fn render_search_modal(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search_visible { return None; }

        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let accent = cx.theme().accent;
        let is_dark = cx.theme().mode.is_dark();
        let overlay_bg = if is_dark { hsla(0.0, 0.0, 0.0, 0.5) } else { hsla(0.0, 0.0, 0.0, 0.3) };
        let card_bg = if is_dark { hsla(0.0, 0.0, 0.15, 1.0) } else { hsla(0.0, 0.0, 1.0, 1.0) };

        let mut results_div = div().id("search-results-list").flex().flex_col()
            .max_h(px(400.)).overflow_y_scroll();

        if !self.search_query.is_empty() && self.search_results.is_empty() {
            results_div = results_div.child(
                div().px(px(20.)).py(px(16.)).text_size(px(13.)).text_color(muted.opacity(0.6))
                    .child(format!("No results for \"{}\"", self.search_query))
            );
        }

        for (i, result) in self.search_results.iter().enumerate() {
            let file_name = result.chunk.file_path.file_stem()
                .and_then(|s| s.to_str()).unwrap_or("?").to_string();
            let heading = result.chunk.heading.clone();
            // Build preview: show content with query context
            let preview: String = result.chunk.content.chars().take(150).collect();
            let rel_path = result.chunk.file_path.file_name()
                .and_then(|s| s.to_str()).unwrap_or("").to_string();
            let path = result.chunk.file_path.clone();
            let score = result.score;

            results_div = results_div.child(
                div().id(ElementId::NamedInteger("sresult".into(), i as u64))
                    .flex().flex_col().gap(px(2.))
                    .px(px(16.)).py(px(8.))
                    .border_b_1().border_color(border.opacity(0.2))
                    .cursor_pointer()
                    .hover(move |s: gpui::StyleRefinement| s.bg(accent.opacity(0.08)))
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        this.search_visible = false;
                        this.side_panel = SidePanel::Files;
                        this.open_path_as_tab(path.clone(), cx, false);
                    }))
                    // Row 1: file name + score
                    .child(
                        div().flex().flex_row().items_center().justify_between()
                            .child(div().flex().flex_row().items_center().gap(px(8.))
                                .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(fg).child(file_name))
                                .child(div().text_size(px(11.)).text_color(accent.opacity(0.7)).child(heading))
                            )
                            .child(div().text_size(px(10.)).text_color(muted.opacity(0.4))
                                .child(format!("{:.0}%", score * 100.0)))
                    )
                    // Row 2: content preview
                    .child(div().text_size(px(12.)).text_color(muted.opacity(0.8))
                        .overflow_hidden().whitespace_nowrap().text_ellipsis()
                        .child(preview))
                    // Row 3: file path
                    .child(div().text_size(px(10.)).text_color(muted.opacity(0.4)).child(rel_path))
            );
        }

        let has_vectors = self.vault_search.as_ref().map_or(false, |s| s.vectors_available());
        let chunk_count = self.vault_search.as_ref().map_or(0, |s| s.chunk_count());

        Some(
            // Full-screen overlay backdrop
            div().id("search-overlay").absolute().top(px(0.)).left(px(0.)).size_full()
                .bg(overlay_bg)
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    // Click backdrop to close
                    this.search_visible = false;
                    cx.notify();
                }))
                .flex().justify_center().pt(px(80.))
                .child(
                    // Modal card (stop click propagation by having its own mouse handler)
                    div().id("search-modal-card")
                        .w(px(650.)).max_h(px(520.))
                        .bg(card_bg)
                        .rounded(px(12.))
                        .border_1().border_color(border.opacity(0.4))
                        .shadow_lg()
                        .flex().flex_col()
                        .on_mouse_down(MouseButton::Left, |_, _, _| { /* stop propagation */ })
                        // Input row
                        .child(
                            div().flex().flex_row().items_center()
                                .px(px(16.)).py(px(12.))
                                .border_b_1().border_color(border.opacity(0.3))
                                .child(
                                    div().text_size(px(14.)).text_color(muted.opacity(0.5)).pr(px(8.)).child("\u{1F50D}")
                                )
                                .child(
                                    div().flex_1().child(
                                        Input::new(&self.search_input).appearance(false).bordered(false)
                                    )
                                )
                        )
                        // Results
                        .child(results_div)
                        // Footer
                        .child(
                            div().flex().items_center().justify_between()
                                .px(px(16.)).py(px(6.))
                                .border_t_1().border_color(border.opacity(0.2))
                                .child(div().text_size(px(10.)).text_color(muted.opacity(0.4))
                                    .child(format!("{} chunks indexed{}", chunk_count, if has_vectors { " · semantic" } else { "" })))
                                .child(div().text_size(px(10.)).text_color(muted.opacity(0.4))
                                    .child("esc to close"))
                        )
                )
                .into_any_element()
        )
    }

    fn render_file_tree(&self, entries: &[FileTreeEntry], depth: usize, out: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        for entry in entries {
            match entry {
                FileTreeEntry::Folder { name, path, children } => {
                    let collapsed = self.collapsed_folders.contains(path);
                    let chevron = if collapsed { icons::chevron_right_char() } else { icons::chevron_down_char() };
                    let pad = px(t::SIDEBAR_PADDING_LEFT + depth as f32 * t::SIDEBAR_INDENT_PER_LEVEL);
                    let fid = path.clone();

                    let is_dark_f = cx.theme().mode.is_dark();
                    let folder_hover_bg = if is_dark_f { hsla(0.0, 0.0, 1.0, 0.05) } else { hsla(0.0, 0.0, 0.0, 0.04) };
                    let dir_pb = PathBuf::from(path.clone());
                    let dir_pb_ctx = dir_pb.clone();
                    let dir_is_ctx = self.ctx_path.as_ref().map(|p| p == &dir_pb).unwrap_or(false);
                    let folder_bg = if dir_is_ctx { folder_hover_bg } else { transparent_black() };
                    let folder_menu_label: SharedString = name.clone().into();
                    out.push(div()
                        .id(ElementId::Name(format!("d-{}", path).into()))
                        .flex().items_center().gap(px(6.))
                        .min_h(px(t::SIDEBAR_ITEM_HEIGHT)).py(px(1.)).pl(pad).pr(px(8.))
                        .mx(px(6.)).rounded(px(t::RADIUS_SM))
                        .text_size(px(t::FONT_UI)).text_color(if dir_is_ctx { fg } else { muted }).font_weight(FontWeight::MEDIUM)
                        .bg(folder_bg)
                        .cursor_pointer().hover(move |s: gpui::StyleRefinement| s.bg(folder_hover_bg).text_color(fg))
                        .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            if this.collapsed_folders.contains(&fid) { this.collapsed_folders.remove(&fid); }
                            else { this.collapsed_folders.insert(fid.clone()); }
                            cx.notify();
                        }))
                        .on_mouse_down(MouseButton::Right, cx.listener(move |this, _, _, cx| {
                            this.ctx_path = Some(dir_pb_ctx.clone());
                            this.ctx_is_folder = true;
                            cx.notify();
                        }))
                        .context_menu({
                            let menu_label = folder_menu_label.clone();
                            move |menu: gpui_component::menu::PopupMenu, _w, _cx| {
                                menu.min_w(px(220.))
                                    .label(menu_label.clone())
                                    .separator()
                                    .menu("New file here", Box::new(CtxNewFileHere))
                                    .menu("New folder", Box::new(CtxNewFolderHere))
                                    .separator()
                                    .menu("Rename", Box::new(CtxFolderRename))
                                    .menu("Copy path", Box::new(CtxCopyPath))
                                    .menu("Reveal in file manager", Box::new(CtxReveal))
                                    .separator()
                                    .menu("Delete (empty only)", Box::new(CtxFolderDelete))
                            }
                        })
                        .child(div().w(px(10.)).text_size(px(9.)).text_color(muted.opacity(0.5)).child(chevron))
                        .child(icons::folder_icon(muted.opacity(0.7)))
                        .child(name.clone())
                        .into_any_element());

                    if !collapsed {
                        self.render_file_tree(children, depth + 1, out, cx);
                    }
                }
                FileTreeEntry::File { name, path } => {
                    let sel = self.is_path_active(path);
                    let is_ctx = self.ctx_path.as_ref().map(|p| p == path).unwrap_or(false);
                    let pad = px(t::SIDEBAR_PADDING_LEFT + depth as f32 * t::SIDEBAR_INDENT_PER_LEVEL + 16.0);
                    let path_click = path.clone();
                    let path_ctrl = path.clone();
                    let path_ctx = path.clone();
                    let id_str = path.to_string_lossy().to_string();
                    let menu_label: SharedString = name.clone().into();

                    // Obsidian-style: subtle grey bg for active file, same for ctx target.
                    // Hover: same grey. No borders.
                    let is_dark_row = cx.theme().mode.is_dark();
                    let row_bg_active = if is_dark_row { hsla(0.0, 0.0, 1.0, 0.08) } else { hsla(0.0, 0.0, 0.0, 0.06) };
                    let row_bg_hover  = if is_dark_row { hsla(0.0, 0.0, 1.0, 0.05) } else { hsla(0.0, 0.0, 0.0, 0.04) };
                    let bg_color = if sel { row_bg_active } else if is_ctx { row_bg_active } else { transparent_black() };
                    let text_color = if sel { fg } else if is_ctx { fg } else { muted };

                    out.push(div()
                        .id(ElementId::Name(format!("f-{}", id_str).into()))
                        .flex().items_center().gap(px(6.))
                        .min_h(px(t::SIDEBAR_ITEM_HEIGHT)).py(px(1.)).pl(pad).pr(px(8.))
                        .rounded(px(t::RADIUS_SM))
                        .mx(px(6.))
                        .text_size(px(t::FONT_UI))
                        .text_color(text_color)
                        .bg(bg_color)
                        .cursor_pointer()
                        .hover(move |s: gpui::StyleRefinement| s.bg(row_bg_hover).text_color(fg))
                        .on_mouse_up(MouseButton::Left, cx.listener(move |this, event: &MouseUpEvent, _, cx| {
                            let p = if event.modifiers.control || event.modifiers.platform { path_ctrl.clone() } else { path_click.clone() };
                            let is_md = p.extension().map_or(false, |e| e == "md");
                            if !is_md {
                                this.status_message = Some(format!("Can't open {} files yet", p.extension().and_then(|e| e.to_str()).unwrap_or("unknown")));
                                cx.notify();
                                return;
                            }
                            let new_tab = event.modifiers.control || event.modifiers.platform;
                            this.open_path_as_tab(p, cx, new_tab);
                        }))
                        .on_mouse_down(MouseButton::Right, cx.listener(move |this, _, _, cx| {
                            this.ctx_path = Some(path_ctx.clone());
                            this.ctx_is_folder = false;
                            cx.notify();
                        }))
                        .context_menu({
                            let menu_label = menu_label.clone();
                            move |menu: gpui_component::menu::PopupMenu, _w, _cx| {
                                menu.min_w(px(220.))
                                    .label(menu_label.clone())
                                    .separator()
                                    .menu("Open", Box::new(CtxOpen))
                                    .menu("Open in new tab", Box::new(CtxOpenNewTab))
                                    .separator()
                                    .menu("New file here", Box::new(CtxNewFileHere))
                                    .menu("Make a copy", Box::new(CtxDuplicate))
                                    .menu("Rename", Box::new(CtxRename))
                                    .menu("Copy path", Box::new(CtxCopyPath))
                                    .menu("Reveal in file manager", Box::new(CtxReveal))
                                    .separator()
                                    .menu("Delete", Box::new(CtxDelete))
                            }
                        })
                        .child(icons::icon_for_path(path, muted.opacity(0.6)))
                        .child(name.clone())
                        .into_any_element());
                }
            }
        }
    }
}

impl Focusable for ForgeApp {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Render for ForgeApp {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let fg = cx.theme().foreground;
        let sidebar_bg = cx.theme().sidebar;
        let topbar_bg = cx.theme().tab_bar;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let accent = cx.theme().accent;
        let is_dark = cx.theme().mode.is_dark();

        // Sync font settings into editor (cheap -- only notifies if changed).
        {
            let ed = self.editor.read(cx);
            let need_update = ed.body_font_family.as_ref() != self.settings.body_font.as_str()
                || ed.mono_font_family.as_ref() != self.settings.mono_font.as_str()
                || (ed.base_font_size - self.settings.font_size).abs() > 0.01;
            if need_update {
                let (bf, mf, fs) = (self.settings.body_font.clone(), self.settings.mono_font.clone(), self.settings.font_size);
                self.editor.update(cx, |ed, cx| { ed.set_fonts(&bf, &mf, fs); cx.notify(); });
            }
        }

        // ── Sidebar ──
        let vault_name = self.vault_name.clone().unwrap_or_else(|| "Forge".into());
        let first_char = vault_name.chars().next().unwrap_or('F').to_uppercase().to_string();

        let mut file_items: Vec<AnyElement> = Vec::new();
        if self.file_tree.is_empty() {
            file_items.push(div().px(px(16.)).py(px(16.)).text_size(px(13.)).text_color(muted.opacity(0.6))
                .child("Open a vault (Ctrl+O)").into_any_element());
        } else {
            self.render_file_tree(&self.file_tree, 0, &mut file_items, cx);
        }
        let mut file_tree = div().id("file-tree").flex_1().overflow_y_scroll().pb(px(8.));
        for item in file_items { file_tree = file_tree.child(item); }
        // Empty-area target at bottom: right-click here for vault-level actions.
        // Separate from file/folder items so their menus don't double-fire.
        file_tree = file_tree.child(
            div().id("file-tree-empty")
                .flex_1().min_h(px(60.))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, _, _, cx| {
                    this.ctx_path = this.vault_path.clone();
                    this.ctx_is_folder = true;
                    cx.notify();
                }))
                .context_menu(|menu: gpui_component::menu::PopupMenu, _w, _cx| {
                    menu.min_w(px(180.))
                        .menu("New file", Box::new(CtxNewFileHere))
                        .menu("New folder", Box::new(CtxNewFolderHere))
                        .separator()
                        .menu("Open vault", Box::new(OpenFolder))
                        .menu("Refresh", Box::new(RefreshVault))
                })
        );

        let sw = self.settings.sidebar_width;
        let sidebar = div().id("sidebar-main").flex().flex_col().w(px(sw)).h_full().bg(sidebar_bg).border_r_1().border_color(border)
            .text_size(px(t::FONT_UI))
            .child(div().flex().items_center().gap(px(10.)).px(px(14.)).h(px(t::SIDEBAR_HEADER_HEIGHT)).flex_shrink_0()
                .child(div().w(px(20.)).h(px(20.)).rounded(px(t::RADIUS_SM)).bg(accent.opacity(0.2)).flex().items_center().justify_center()
                    .text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(fg.opacity(0.7)).child(first_char))
                .child(div().text_size(px(t::FONT_UI)).font_weight(FontWeight::SEMIBOLD).text_color(fg).child(vault_name)))
            .child(div().flex().items_center().px(px(14.)).pt(px(8.)).pb(px(4.)).flex_shrink_0()
                .text_size(px(10.)).font_weight(FontWeight::MEDIUM).text_color(muted.opacity(0.5)).child("NOTES"))
            .child(file_tree)
            .child(div().flex_shrink_0().border_t_1().border_color(border).px(px(6.)).py(px(6.))
                .child(div().id("new-btn").flex().items_center().gap(px(8.)).h(px(t::SIDEBAR_ITEM_HEIGHT)).px(px(8.)).rounded(px(t::RADIUS_MD))
                    .text_size(px(t::FONT_UI)).text_color(muted).cursor_pointer().hover(move |s| s.bg(accent.opacity(0.06)))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.new_file(&NewFile, w, cx)))
                    .child("+ New page"))
                .child(div().id("del-btn").flex().items_center().gap(px(8.)).h(px(t::SIDEBAR_ITEM_HEIGHT)).px(px(8.)).rounded(px(t::RADIUS_MD))
                    .text_size(px(t::FONT_UI)).text_color(muted).cursor_pointer().hover(move |s| s.bg(accent.opacity(0.06)))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.delete_file(&DeleteFile, w, cx)))
                    .child("Trash")));

        // ── Tab bar ──
        let active_tab = self.active_tab;
        let mut tab_bar = div().flex().flex_row().items_center().h(px(t::TAB_BAR_HEIGHT))
            .bg(topbar_bg).border_b_1().border_color(border).flex_shrink_0()
            .overflow_hidden();

        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_active = active_tab == Some(idx);
            let tab_name = tab.name.clone();
            let close_id = format!("close-tab-{}", idx);

            let tab_el = div()
                .id(ElementId::NamedInteger("tab".into(), idx as u64))
                .flex().flex_row().items_center().gap(px(8.))
                .h_full().pl(px(12.)).pr(px(6.))
                .border_r_1().border_color(border)
                .text_size(px(12.))
                .text_color(if is_active { fg } else { muted })
                .bg(if is_active { bg } else { transparent_black() })
                .cursor_pointer()
                .hover(move |s| s.bg(accent.opacity(0.05)))
                .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| this.switch_to_tab(idx, cx)))
                .child(div().max_w(px(t::TAB_MAX_WIDTH)).min_w(px(0.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(tab_name))
                .child(
                    div().id(ElementId::Name(close_id.into()))
                        .flex().items_center().justify_center()
                        .w(px(16.)).h(px(16.)).rounded(px(3.))
                        .text_size(px(12.)).text_color(muted.opacity(0.6))
                        .hover(move |s| s.bg(accent.opacity(0.2)).text_color(fg))
                        .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            this.close_tab_at(idx, cx);
                        }))
                        .child("\u{2715}")
                );
            tab_bar = tab_bar.child(tab_el);
        }

        // Spacer + theme button
        tab_bar = tab_bar
            .child(div().flex_1())
            .child(
                div().id("theme-btn").px(px(10.)).h_full().flex().items_center().text_size(px(12.)).text_color(muted)
                    .cursor_pointer().hover(move |s| s.bg(accent.opacity(0.10)))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.toggle_theme(&ToggleTheme, w, cx)))
                    .child(if is_dark { "\u{263C}" } else { "\u{263E}" })
            );

        // ── Content ──
        let big_title = self.active_tab
            .and_then(|i| self.tabs.get(i))
            .map(|t| t.name.clone())
            .unwrap_or_default();

        let content = if self.active_tab.is_some() {
            div().id("editor-wrap").size_full()
                .context_menu(|menu, _, _| {
                    menu.min_w(px(200.))
                        .menu("Cut", Box::new(editor::Cut))
                        .menu("Copy", Box::new(editor::Copy))
                        .menu("Paste", Box::new(editor::Paste))
                        .separator()
                        .menu("Select All", Box::new(editor::SelectAll))
                        .menu("Select Line", Box::new(editor::SelectLine))
                        .menu("Undo", Box::new(editor::Undo))
                        .separator()
                        .menu("Bold", Box::new(editor::ToggleBold))
                        .menu("Italic", Box::new(editor::ToggleItalic))
                        .menu("Code", Box::new(editor::ToggleCode))
                        .menu("Strikethrough", Box::new(editor::ToggleStrikethrough))
                        .separator()
                        .menu("Heading 1", Box::new(editor::InsertHeading1))
                        .menu("Heading 2", Box::new(editor::InsertHeading2))
                        .menu("Heading 3", Box::new(editor::InsertHeading3))
                        .separator()
                        .menu("Bullet List", Box::new(editor::InsertBulletList))
                        .menu("Numbered List", Box::new(editor::InsertNumberedList))
                        .menu("Table", Box::new(editor::InsertTable))
                        .menu("Code Block", Box::new(editor::InsertCodeBlock))
                        .menu("Horizontal Rule", Box::new(editor::InsertHorizontalRule))
                        .separator()
                        .menu("Save", Box::new(Save))
                })
                .child({
                    let title_el: AnyElement = if self.renaming_title {
                        div().pt(px(t::CONTENT_PADDING_TOP)).pb(px(t::TITLE_PADDING_BOTTOM)).flex_shrink_0()
                            .child(
                                Input::new(&self.title_input)
                                    .appearance(false)
                                    .bordered(false)
                            )
                            .text_size(px(t::FONT_TITLE)).font_weight(FontWeight::BOLD).text_color(fg)
                            .into_any_element()
                    } else {
                        div()
                            .id("title-click")
                            .text_size(px(t::FONT_TITLE)).font_weight(FontWeight::BOLD)
                            .text_color(fg).pt(px(t::CONTENT_PADDING_TOP)).pb(px(t::TITLE_PADDING_BOTTOM)).flex_shrink_0()
                            .cursor_pointer()
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.start_rename(w, cx)))
                            .child(big_title)
                            .into_any_element()
                    };
                    if self.readable_width {
                        div().max_w(px(t::CONTENT_MAX_WIDTH)).mx_auto().h_full().px(px(t::CONTENT_PADDING_X)).text_color(fg)
                            .flex().flex_col()
                            .child(title_el)
                            .child(div().flex_1().min_h(px(0.)).child(self.editor.clone()))
                    } else {
                        div().w_full().h_full().px(px(t::CONTENT_PADDING_X_WIDE)).text_color(fg)
                            .flex().flex_col()
                            .child(title_el)
                            .child(div().flex_1().min_h(px(0.)).child(self.editor.clone()))
                    }
                })
                .into_any_element()
        } else {
            div().flex_1().flex().items_center().justify_center()
                .child(div().flex().flex_col().items_center().gap(px(16.))
                    .child(div().text_size(px(36.)).font_weight(FontWeight::BOLD).text_color(muted.opacity(0.3)).child("Forge"))
                    .child(div().text_size(px(14.)).text_color(muted.opacity(0.6))
                        .child(if self.vault_path.is_some() { "Select a note" } else { "Open a vault with Ctrl+O" })))
                .into_any_element()
        };

        let backlinks_panel = self.render_backlinks_panel(cx);
        let mut main_content = div().flex().flex_col().flex_1().min_w(px(0.)).min_h(px(0.)).bg(bg)
            .child(tab_bar).child(content);
        if let Some(panel) = backlinks_panel { main_content = main_content.child(panel); }

        // ── Status bar ──
        let mut sbar = div().flex().items_center().justify_between().h(px(t::STATUSBAR_HEIGHT)).px(px(12.))
            .bg(topbar_bg).border_t_1().border_color(border).flex_shrink_0()
            .text_size(px(t::FONT_TINY)).text_color(muted);
        sbar = sbar.child(div().child(self.vault_name.clone().unwrap_or_default()));
        if self.active_tab.is_some() {
            // buffer.len_bytes() is O(1) on the rope; buffer.text() was O(n) and
            // allocated the full file on EVERY render (triggered by cursor blinks,
            // scroll-animation ticks, and other notifies from the editor entity).
            let editor_ref = self.editor.read(cx);
            let chars = editor_ref.buffer.len_bytes();
            let dirty = if editor_ref.buffer.is_dirty() { " [modified]" } else { "" };
            sbar = sbar.child(div().flex().items_center().gap(px(12.))
                .child(format!("{} chars", chars))
                .child(dirty));
        }

        // ── Scrollbar column (pinned to window right edge) ──
        let scrollbar_col = {
            let editor = self.editor.read(cx);
            let total = editor.total_height();
            let viewport_h = editor.viewport_height();
            if self.active_tab.is_some() && total > viewport_h + 1.0 && viewport_h > 0.0 {
                let track_color = if is_dark { hsla(0., 0., 1., 0.05) } else { hsla(0., 0., 0., 0.05) };
                let thumb_color = if is_dark { hsla(0., 0., 1., 0.35) } else { hsla(0., 0., 0., 0.35) };
                let thumb_hover_color = if is_dark { hsla(0., 0., 1., 0.55) } else { hsla(0., 0., 0., 0.55) };
                let thumb_h_ratio = (viewport_h / total).clamp(0.04, 1.0);
                let thumb_h = (viewport_h * thumb_h_ratio).round();
                let scroll_max = (total - viewport_h).max(1.0);
                let thumb_y_ratio = (editor.scroll_offset() / scroll_max).clamp(0.0, 1.0);
                let thumb_y = ((viewport_h - thumb_h) * thumb_y_ratio).round();
                let editor_entity = self.editor.clone();
                let viewport_h_captured = viewport_h;
                let total_captured = total;
                let _ = thumb_h; // captured by thumb down handler
                Some(
                    div().w(px(12.)).flex_shrink_0().h_full().bg(track_color).relative()
                        .id("scrollbar-track")
                        .cursor_pointer()
                        // Click on empty track -> jump the thumb there.
                        .on_mouse_down(MouseButton::Left, cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                            let track_top: f32 = event.position.y.into();
                            // We don't easily have the track's top-y here in listener coords;
                            // use the editor's known viewport geometry + click position.
                            // For v1, just scroll_target to click_y * ratio_of_track.
                            let _ = track_top;
                            editor_entity.update(cx, |ed, cx| {
                                // Treat the click y (relative to track top, approximated by
                                // taking position.y as a fraction of viewport_h_captured).
                                let frac = (track_top / viewport_h_captured).clamp(0.0, 1.0);
                                let max_scroll = (total_captured - viewport_h_captured).max(1.0);
                                ed.set_scroll_target(frac * max_scroll);
                                cx.notify();
                            });
                        }))
                        .child(
                            div()
                                .id("scrollbar-thumb")
                                .absolute().left(px(2.)).top(px(thumb_y)).w(px(8.)).h(px(thumb_h))
                                .rounded(px(4.)).bg(thumb_color)
                                .hover(move |s| s.bg(thumb_hover_color))
                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, _| {
                                    // Begin drag: remember grab offset within the thumb.
                                    let click_y: f32 = event.position.y.into();
                                    this.scrollbar_drag = Some(ScrollbarDrag {
                                        grab_offset: click_y - thumb_y,
                                        track_height: viewport_h_captured,
                                        total_height: total_captured,
                                    });
                                }))
                        )
                )
            } else { None }
        };

        // ── Assemble ──
        let rail = self.render_rail(cx);
        let mut main_row = div().flex().flex_row().flex_1().min_h(px(0.));
        main_row = main_row.child(rail);
        match self.side_panel {
            SidePanel::Files => {
                if self.sidebar_visible {
                    main_row = main_row.child(sidebar);
                    // Resize handle (4px wide drag strip between sidebar and content)
                    main_row = main_row.child(
                        div().id("sidebar-resize").w(px(4.)).h_full().flex_shrink_0()
                            .cursor_col_resize()
                            .hover(move |s: gpui::StyleRefinement| s.bg(border.opacity(0.6)))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, _| {
                                let x: f32 = event.position.x.into();
                                this.sidebar_drag = Some(x);
                            }))
                    );
                }
                main_row = main_row.child(main_content);
                if let Some(sb) = scrollbar_col { main_row = main_row.child(sb); }
            }
            SidePanel::Graph => {
                main_row = main_row.child(
                    div().flex().flex_col().flex_1().min_w(px(0.)).min_h(px(0.)).bg(bg)
                        .child(self.graph_view.clone())
                );
            }
            SidePanel::Settings => {
                main_row = main_row.child(
                    div().flex().flex_col().flex_1().min_w(px(0.)).min_h(px(0.))
                        .child(self.render_settings_panel(cx))
                );
            }
        }

        div().flex().flex_col().size_full().bg(bg).text_color(fg).relative()
            .key_context("ForgeApp").track_focus(&self.focus_handle(cx))
            // Global scrollbar drag handlers -- keep tracking even if the mouse
            // drifts outside the 12px-wide scrollbar column during a drag.
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                // Sidebar resize drag
                if this.sidebar_drag.is_some() {
                    let mx: f32 = event.position.x.into();
                    // Subtract the 44px rail from the calculation.
                    let new_w = (mx - 44.0).clamp(160.0, 600.0);
                    this.settings.sidebar_width = new_w;
                    cx.notify();
                    return;
                }
                // Scrollbar drag
                let Some(drag) = this.scrollbar_drag.as_ref() else { return; };
                let click_y: f32 = event.position.y.into();
                let thumb_h = (drag.track_height * (drag.track_height / drag.total_height).clamp(0.04, 1.0)).round();
                let thumb_top_y = click_y - drag.grab_offset;
                let frac = (thumb_top_y / (drag.track_height - thumb_h).max(1.0)).clamp(0.0, 1.0);
                let max_scroll = (drag.total_height - drag.track_height).max(1.0);
                let editor = this.editor.clone();
                editor.update(cx, |ed, cx| {
                    ed.set_scroll_target(frac * max_scroll);
                    cx.notify();
                });
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| {
                if this.sidebar_drag.is_some() {
                    this.sidebar_drag = None;
                    this.settings.save(); // persist new width
                }
                this.scrollbar_drag = None;
            }))
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::delete_file))
            .on_action(cx.listener(Self::close_current_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::prev_tab))
            .on_action(cx.listener(Self::refresh_vault))
            .on_action(cx.listener(Self::toggle_readable_width))
            .on_action(cx.listener(Self::toggle_backlinks))
            .on_action(cx.listener(Self::show_files))
            .on_action(cx.listener(Self::show_graph))
            .on_action(cx.listener(Self::show_settings))
            .on_action(cx.listener(Self::show_search))
            .on_action(cx.listener(Self::nav_back))
            .on_action(cx.listener(Self::nav_forward))
            .on_action(cx.listener(Self::ctx_open))
            .on_action(cx.listener(Self::ctx_open_new_tab))
            .on_action(cx.listener(Self::ctx_rename))
            .on_action(cx.listener(Self::ctx_delete))
            .on_action(cx.listener(Self::ctx_copy_path))
            .on_action(cx.listener(Self::ctx_reveal))
            .on_action(cx.listener(Self::ctx_duplicate))
            .on_action(cx.listener(Self::ctx_new_file_here))
            .on_action(cx.listener(Self::ctx_new_folder_here))
            .on_action(cx.listener(Self::ctx_folder_rename))
            .on_action(cx.listener(Self::ctx_folder_delete))
            .on_action(cx.listener(Self::_consume_backspace))
            .on_action(cx.listener(Self::_consume_delete))
            .on_action(cx.listener(Self::toggle_read_mode))
            .on_action(cx.listener(Self::fwd_cut))
            .on_action(cx.listener(Self::fwd_copy))
            .on_action(cx.listener(Self::fwd_paste))
            .on_action(cx.listener(Self::fwd_select_all))
            .on_action(cx.listener(Self::fwd_select_line))
            .on_action(cx.listener(Self::fwd_undo))
            .on_action(cx.listener(Self::fwd_bold))
            .on_action(cx.listener(Self::fwd_italic))
            .on_action(cx.listener(Self::fwd_code))
            .on_action(cx.listener(Self::fwd_strike))
            .on_action(cx.listener(Self::fwd_h1))
            .on_action(cx.listener(Self::fwd_h2))
            .on_action(cx.listener(Self::fwd_h3))
            .on_action(cx.listener(Self::fwd_bullet))
            .on_action(cx.listener(Self::fwd_numbered))
            .on_action(cx.listener(Self::fwd_table))
            .on_action(cx.listener(Self::fwd_code_block))
            .on_action(cx.listener(Self::fwd_hr))
            .on_action(cx.listener(Self::fwd_zoom_in))
            .on_action(cx.listener(Self::fwd_zoom_out))
            .on_action(cx.listener(Self::fwd_zoom_reset))
            .child(main_row).child(sbar)
            .children(self.render_search_modal(cx))
    }
}

pub fn run_app() {
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            // App-level
            KeyBinding::new("ctrl-o", OpenFolder, Some("ForgeApp")),
            KeyBinding::new("ctrl-s", Save, Some("ForgeApp")),
            KeyBinding::new("ctrl-s", Save, Some("Editor")),
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("ctrl-shift-t", ToggleTheme, Some("ForgeApp")),
            KeyBinding::new("ctrl-shift-t", ToggleTheme, Some("Editor")),
            KeyBinding::new("ctrl-n", NewFile, Some("ForgeApp")),
            KeyBinding::new("ctrl-n", NewFile, Some("Editor")),
            // Tab management
            KeyBinding::new("ctrl-w", CloseTab, Some("ForgeApp")),
            KeyBinding::new("ctrl-w", CloseTab, Some("Editor")),
            KeyBinding::new("ctrl-tab", NextTab, Some("ForgeApp")),
            KeyBinding::new("ctrl-tab", NextTab, Some("Editor")),
            KeyBinding::new("ctrl-shift-tab", PrevTab, Some("ForgeApp")),
            KeyBinding::new("ctrl-shift-tab", PrevTab, Some("Editor")),
            KeyBinding::new("f5", RefreshVault, Some("ForgeApp")),
            KeyBinding::new("f5", RefreshVault, Some("Editor")),
            KeyBinding::new("ctrl-r", RefreshVault, Some("ForgeApp")),
            KeyBinding::new("ctrl-r", RefreshVault, Some("Editor")),
            KeyBinding::new("ctrl-shift-r", ToggleReadableWidth, Some("ForgeApp")),
            KeyBinding::new("ctrl-shift-r", ToggleReadableWidth, Some("Editor")),
            KeyBinding::new("ctrl-shift-b", ToggleBacklinks, Some("ForgeApp")),
            KeyBinding::new("ctrl-shift-b", ToggleBacklinks, Some("Editor")),
            KeyBinding::new("ctrl-shift-f", ShowSearch, Some("ForgeApp")),
            KeyBinding::new("ctrl-shift-f", ShowSearch, Some("Editor")),
            // Navigation (back / forward across file history)
            KeyBinding::new("alt-left", NavBack, Some("ForgeApp")),
            KeyBinding::new("alt-left", NavBack, Some("Editor")),
            KeyBinding::new("alt-right", NavForward, Some("ForgeApp")),
            KeyBinding::new("alt-right", NavForward, Some("Editor")),
            KeyBinding::new("ctrl-alt-left", NavBack, Some("ForgeApp")),
            KeyBinding::new("ctrl-alt-right", NavForward, Some("ForgeApp")),
            // Movement
            KeyBinding::new("left", editor::MoveLeft, Some("Editor")),
            KeyBinding::new("right", editor::MoveRight, Some("Editor")),
            KeyBinding::new("up", editor::MoveUp, Some("Editor")),
            KeyBinding::new("down", editor::MoveDown, Some("Editor")),
            KeyBinding::new("ctrl-left", editor::MoveWordLeft, Some("Editor")),
            KeyBinding::new("ctrl-right", editor::MoveWordRight, Some("Editor")),
            KeyBinding::new("home", editor::MoveHome, Some("Editor")),
            KeyBinding::new("end", editor::MoveEnd, Some("Editor")),
            KeyBinding::new("pageup", editor::PageUp, Some("Editor")),
            KeyBinding::new("pagedown", editor::PageDown, Some("Editor")),
            KeyBinding::new("ctrl-home", editor::MoveDocStart, Some("Editor")),
            KeyBinding::new("ctrl-end", editor::MoveDocEnd, Some("Editor")),
            // Selection
            KeyBinding::new("shift-left", editor::SelectLeft, Some("Editor")),
            KeyBinding::new("shift-right", editor::SelectRight, Some("Editor")),
            KeyBinding::new("shift-up", editor::SelectUp, Some("Editor")),
            KeyBinding::new("shift-down", editor::SelectDown, Some("Editor")),
            KeyBinding::new("ctrl-shift-left", editor::SelectWordLeft, Some("Editor")),
            KeyBinding::new("ctrl-shift-right", editor::SelectWordRight, Some("Editor")),
            KeyBinding::new("shift-home", editor::SelectHome, Some("Editor")),
            KeyBinding::new("shift-end", editor::SelectEnd, Some("Editor")),
            KeyBinding::new("ctrl-a", editor::SelectAll, Some("Editor")),
            KeyBinding::new("ctrl-l", editor::SelectLine, Some("Editor")),
            // Editing
            KeyBinding::new("backspace", editor::Backspace, Some("Editor")),
            KeyBinding::new("backspace", editor::Backspace, Some("ForgeApp")),
            KeyBinding::new("delete", editor::Delete, Some("Editor")),
            KeyBinding::new("delete", editor::Delete, Some("ForgeApp")),
            KeyBinding::new("ctrl-backspace", editor::BackspaceWord, Some("Editor")),
            KeyBinding::new("ctrl-delete", editor::DeleteWord, Some("Editor")),
            KeyBinding::new("enter", editor::Enter, Some("Editor")),
            KeyBinding::new("tab", editor::Indent, Some("Editor")),
            KeyBinding::new("shift-tab", editor::Dedent, Some("Editor")),
            KeyBinding::new("ctrl-c", editor::Copy, Some("Editor")),
            KeyBinding::new("ctrl-x", editor::Cut, Some("Editor")),
            KeyBinding::new("ctrl-v", editor::Paste, Some("Editor")),
            KeyBinding::new("ctrl-z", editor::Undo, Some("Editor")),
            KeyBinding::new("ctrl-shift-z", editor::Redo, Some("Editor")),
            KeyBinding::new("ctrl-y", editor::Redo, Some("Editor")),
            KeyBinding::new("ctrl-shift-d", editor::DuplicateLine, Some("Editor")),
            // Formatting
            KeyBinding::new("ctrl-b", editor::ToggleBold, Some("Editor")),
            KeyBinding::new("ctrl-i", editor::ToggleItalic, Some("Editor")),
            KeyBinding::new("ctrl-shift-c", editor::ToggleCode, Some("Editor")),
            KeyBinding::new("ctrl-shift-s", editor::ToggleStrikethrough, Some("Editor")),
            // Read mode toggle
            KeyBinding::new("ctrl-e", editor::ToggleReadMode, Some("Editor")),
            KeyBinding::new("ctrl-e", editor::ToggleReadMode, Some("ForgeApp")),
            // Wikilink autocomplete
            KeyBinding::new("escape", editor::AutocompleteCancel, Some("Editor")),
            // Zoom
            KeyBinding::new("ctrl-=", editor::ZoomIn, Some("Editor")),
            KeyBinding::new("ctrl-=", editor::ZoomIn, Some("ForgeApp")),
            KeyBinding::new("ctrl-plus", editor::ZoomIn, Some("Editor")),
            KeyBinding::new("ctrl-plus", editor::ZoomIn, Some("ForgeApp")),
            KeyBinding::new("ctrl--", editor::ZoomOut, Some("Editor")),
            KeyBinding::new("ctrl--", editor::ZoomOut, Some("ForgeApp")),
            KeyBinding::new("ctrl-0", editor::ZoomReset, Some("Editor")),
            KeyBinding::new("ctrl-0", editor::ZoomReset, Some("ForgeApp")),
        ]);

        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            |window, cx| {
                let view = cx.new(|cx| ForgeApp::new(window, cx));
                let any_view: AnyView = view.into();
                cx.new(|cx| Root::new(any_view, window, cx))
            },
        ).unwrap();
    });
}
