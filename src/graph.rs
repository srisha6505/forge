//! Graph view: visualizes wikilinks between notes as a node-edge graph.
//!
//! Layout: Fruchterman-Reingold force-directed, computed once on set_data.
//! Pan: drag background. Zoom: scroll wheel. Click node -> open note.

use std::path::PathBuf;

use gpui::*;
use gpui_component::ActiveTheme;

use crate::links::LinkIndex;
use crate::theme as t;

/// Emitted when user clicks a node.
#[derive(Clone, Debug)]
pub enum GraphEvent {
    OpenNote(PathBuf),
}

#[derive(Clone, Debug)]
pub struct GraphNode {
    pub path: PathBuf,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

pub struct GraphView {
    focus_handle: FocusHandle,
    nodes: Vec<GraphNode>,
    edges: Vec<(usize, usize)>,
    pan_x: f32,
    pan_y: f32,
    zoom: f32,
    hover_node: Option<usize>,
    dragging_bg: bool,
    /// (node_idx, anchor_graph_x, anchor_graph_y) where anchor is the graph-space
    /// point under the cursor at drag start. Updated on move.
    dragging_node: Option<usize>,
    last_drag_px: Option<(f32, f32)>,
    /// Pixel position of mouse_down, used to distinguish click from drag.
    down_px: Option<(f32, f32)>,
    /// Tracks how far cursor has moved since mouse_down.
    drag_distance: f32,
    last_bounds: Option<Bounds<Pixels>>,
    needs_refit: bool,
}

impl EventEmitter<GraphEvent> for GraphView {}

impl Focusable for GraphView {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl GraphView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            nodes: Vec::new(),
            edges: Vec::new(),
            pan_x: 0.0, pan_y: 0.0, zoom: 1.0,
            hover_node: None,
            dragging_bg: false,
            dragging_node: None,
            last_drag_px: None,
            down_px: None,
            drag_distance: 0.0,
            last_bounds: None,
            needs_refit: false,
        }
    }

    /// Rebuild nodes + edges from a LinkIndex. Call on vault load/refresh.
    pub fn set_data(&mut self, link_index: &LinkIndex) {
        let paths: Vec<PathBuf> = link_index.all_paths().into_iter().map(|p| p.to_path_buf()).collect();

        // Build edges: for each path, look at its backlinks (incoming) to derive connectivity.
        // We'll collect unique edges (u, v) where u < v.
        let mut edges: Vec<(usize, usize)> = Vec::new();
        let idx_of: std::collections::HashMap<PathBuf, usize> = paths
            .iter().enumerate().map(|(i, p)| (p.clone(), i)).collect();

        for (i, path) in paths.iter().enumerate() {
            for lref in link_index.backlinks_for_path(path) {
                // lref.source -> path (source links to path)
                if let Some(&j) = idx_of.get(&lref.source) {
                    let (a, b) = if i < j { (i, j) } else { (j, i) };
                    if a != b && !edges.contains(&(a, b)) {
                        edges.push((a, b));
                    }
                }
            }
        }

        // Degree per node (for node radius scaling)
        let n = paths.len();
        let mut deg = vec![0u32; n];
        for &(a, b) in &edges {
            deg[a] += 1;
            deg[b] += 1;
        }

        // Fruchterman-Reingold force-directed layout.
        let positions = fruchterman_reingold(n, &edges);

        self.nodes = paths.iter().enumerate().map(|(i, p)| {
            let (x, y) = positions[i];
            let d = deg[i] as f32;
            let radius = (4.0 + (d + 1.0).ln() * 2.5).clamp(4.0, 18.0);
            let label = p.file_stem().and_then(|s| s.to_str()).unwrap_or("?").to_string();
            GraphNode { path: p.clone(), label, x, y, radius }
        }).collect();

        self.edges = edges;
        // Reset view so the graph is centered + fits.
        self.pan_x = 0.0; self.pan_y = 0.0; self.zoom = 1.0;
        self.fit_to_viewport();
        self.needs_refit = self.last_bounds.is_none();
    }

    /// Adjust zoom so the whole graph fits. Assumes last_bounds is known; if not,
    /// picks a sensible default zoom based on layout extent.
    fn fit_to_viewport(&mut self) {
        if self.nodes.is_empty() { return; }
        let (min_x, max_x, min_y, max_y) = self.nodes.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY),
            |(nx, xx, ny, xy), n| (nx.min(n.x), xx.max(n.x), ny.min(n.y), xy.max(n.y))
        );
        let extent_w = (max_x - min_x).max(1.0);
        let extent_h = (max_y - min_y).max(1.0);
        let extent = extent_w.max(extent_h);
        // Assume ~1000x700 viewport if unknown; fit with 15% margin.
        let viewport = self.last_bounds
            .map(|b| {
                let w: f32 = b.size.width.into();
                let h: f32 = b.size.height.into();
                w.min(h)
            })
            .unwrap_or(700.0);
        self.zoom = (viewport * 0.75 / extent).clamp(0.2, 2.5);
        // Center layout -- translate so midpoint is at (0, 0).
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        for n in &mut self.nodes { n.x -= cx; n.y -= cy; }
    }

    fn hit_test_node(&self, local_x: f32, local_y: f32, center: Point<f32>) -> Option<usize> {
        // Convert screen-local to graph coords.
        let gx = (local_x - center.x - self.pan_x) / self.zoom;
        let gy = (local_y - center.y - self.pan_y) / self.zoom;
        self.nodes.iter().enumerate()
            .find(|(_, n)| {
                let dx = gx - n.x;
                let dy = gy - n.y;
                (dx * dx + dy * dy).sqrt() <= n.radius + 2.0
            })
            .map(|(i, _)| i)
    }

    fn on_wheel(&mut self, event: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let delta_y: f32 = match event.delta {
            ScrollDelta::Pixels(p) => p.y.into(),
            ScrollDelta::Lines(l) => l.y * 30.0,
        };
        let factor = if delta_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
        // Zoom around the cursor position so content under cursor stays anchored.
        if let Some(bounds) = self.last_bounds {
            let mx: f32 = event.position.x.into();
            let my: f32 = event.position.y.into();
            let bx: f32 = bounds.origin.x.into();
            let by: f32 = bounds.origin.y.into();
            let bw: f32 = bounds.size.width.into();
            let bh: f32 = bounds.size.height.into();
            let center_x = bx + bw * 0.5;
            let center_y = by + bh * 0.5;
            // Graph coord under cursor pre-zoom.
            let gx = (mx - center_x - self.pan_x) / self.zoom;
            let gy = (my - center_y - self.pan_y) / self.zoom;
            let new_zoom = (self.zoom * factor).clamp(0.1, 5.0);
            // Keep (gx, gy) under cursor after zoom change.
            self.pan_x = mx - center_x - gx * new_zoom;
            self.pan_y = my - center_y - gy * new_zoom;
            self.zoom = new_zoom;
        } else {
            self.zoom = (self.zoom * factor).clamp(0.1, 5.0);
        }
        cx.notify();
    }
}

impl Render for GraphView {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let nc = self.nodes.len();
        let ec = self.edges.len();
        div()
            .id("graph-root")
            .size_full().relative()
            .track_focus(&self.focus_handle)
            .key_context("GraphView")
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, w, cx| this.on_mouse_down(e, w, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, w, cx| this.on_mouse_move(e, w, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, w, cx| this.on_mouse_up(e, w, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, w, cx| this.on_mouse_up(e, w, cx)))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, w, cx| this.on_wheel(e, w, cx)))
            .child(GraphElement { view: cx.entity().clone() })
            .child(
                div().absolute().top(px(12.)).right(px(12.))
                    .text_size(px(t::FONT_TINY)).text_color(muted.opacity(0.7))
                    .child(format!("{} nodes · {} edges", nc, ec))
            )
    }
}

struct GraphElement {
    view: Entity<GraphView>,
}

impl IntoElement for GraphElement {
    type Element = Self;
    fn into_element(self) -> Self::Element { self }
}

impl Element for GraphElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> { None }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size = size(relative(1.0).into(), relative(1.0).into());
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut Self::RequestLayoutState, _window: &mut Window, cx: &mut App) -> Self::PrepaintState {
        self.view.update(cx, |view, _| {
            view.last_bounds = Some(bounds);
            if view.needs_refit {
                view.fit_to_viewport();
                view.needs_refit = false;
            }
        });
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut Self::RequestLayoutState, _: &mut Self::PrepaintState, window: &mut Window, cx: &mut App) {
        let (nodes, edges, pan_x, pan_y, zoom, hover, is_dark) = self.view.update(cx, |v, cx| {
            (v.nodes.clone(), v.edges.clone(), v.pan_x, v.pan_y, v.zoom, v.hover_node, cx.theme().mode.is_dark())
        });

        let bg = if is_dark { hsla(0.0, 0.0, 0.10, 1.0) } else { hsla(0.0, 0.0, 0.98, 1.0) };
        let edge_color = if is_dark { hsla(0.0, 0.0, 0.45, 0.55) } else { hsla(0.0, 0.0, 0.60, 0.55) };
        let node_color = if is_dark { hsla(0.58, 0.55, 0.62, 1.0) } else { hsla(0.58, 0.65, 0.52, 1.0) };
        let node_hover = hsla(0.08, 0.80, 0.58, 1.0);
        let label_color = cx.theme().foreground;

        // Background
        window.paint_quad(fill(bounds, bg));

        let center = point(
            bounds.origin.x + bounds.size.width * 0.5,
            bounds.origin.y + bounds.size.height * 0.5,
        );
        let cx_f: f32 = center.x.into();
        let cy_f: f32 = center.y.into();

        let transform = |gx: f32, gy: f32| -> Point<Pixels> {
            point(px(cx_f + (gx * zoom) + pan_x), px(cy_f + (gy * zoom) + pan_y))
        };

        // Edges
        if !edges.is_empty() {
            let mut builder = PathBuilder::stroke(px(1.2));
            for &(a, b) in &edges {
                let (Some(na), Some(nb)) = (nodes.get(a), nodes.get(b)) else { continue };
                builder.move_to(transform(na.x, na.y));
                builder.line_to(transform(nb.x, nb.y));
            }
            if let Ok(path) = builder.build() {
                window.paint_path(path, edge_color);
            }
        }

        // Nodes (as rounded quads -- corner radius = radius makes them circles)
        for (i, n) in nodes.iter().enumerate() {
            let p = transform(n.x, n.y);
            let r = (n.radius * zoom).max(1.5);
            let col = if hover == Some(i) { node_hover } else { node_color };
            let node_bounds = Bounds {
                origin: point(p.x - px(r), p.y - px(r)),
                size: size(px(r * 2.0), px(r * 2.0)),
            };
            window.paint_quad(PaintQuad {
                bounds: node_bounds,
                corner_radii: Corners::all(px(r)),
                background: col.into(),
                border_widths: Edges::default(),
                border_color: transparent_black(),
                border_style: BorderStyle::default(),
            });
        }

        // Labels (only draw for hovered + top-degree nodes to avoid label spam)
        // Show label for all nodes if total < 60; else only hovered.
        let show_all_labels = nodes.len() < 60;
        let label_font = Font {
            family: "DejaVu Sans".into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        };
        for (i, n) in nodes.iter().enumerate() {
            if !show_all_labels && hover != Some(i) { continue; }
            let p = transform(n.x, n.y);
            let r = (n.radius * zoom).max(1.5);
            let font_size = if hover == Some(i) { 12.0 } else { 10.0 };
            let run = TextRun {
                len: n.label.len(),
                font: label_font.clone(),
                color: label_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                n.label.clone().into(),
                px(font_size),
                &[run],
                None,
            );
            let text_width: f32 = shaped.width.into();
            let label_origin = point(p.x - px(text_width * 0.5), p.y + px(r + 3.0));
            let _ = shaped.paint(label_origin, px(font_size + 4.0), window, cx);
        }
    }
}

impl GraphView {
    pub fn on_mouse_down(&mut self, event: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.last_bounds else { return; };
        let mx: f32 = event.position.x.into();
        let my: f32 = event.position.y.into();
        let lx = mx - f32::from(bounds.origin.x);
        let ly = my - f32::from(bounds.origin.y);
        let bw: f32 = bounds.size.width.into();
        let bh: f32 = bounds.size.height.into();
        let center = Point { x: bw * 0.5, y: bh * 0.5 };
        self.down_px = Some((mx, my));
        self.last_drag_px = Some((mx, my));
        self.drag_distance = 0.0;
        if let Some(idx) = self.hit_test_node(lx, ly, center) {
            self.dragging_node = Some(idx);
        } else {
            self.dragging_bg = true;
        }
        cx.notify();
    }

    pub fn on_mouse_move(&mut self, event: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.last_bounds else { return; };
        let mx: f32 = event.position.x.into();
        let my: f32 = event.position.y.into();
        if let Some((lx, ly)) = self.last_drag_px {
            let dx = mx - lx;
            let dy = my - ly;
            self.drag_distance += (dx * dx + dy * dy).sqrt();
            if let Some(idx) = self.dragging_node {
                // Convert pixel delta to graph-space delta by dividing by zoom.
                if let Some(n) = self.nodes.get_mut(idx) {
                    n.x += dx / self.zoom;
                    n.y += dy / self.zoom;
                    self.last_drag_px = Some((mx, my));
                    cx.notify();
                    return;
                }
            }
            if self.dragging_bg {
                self.pan_x += dx;
                self.pan_y += dy;
                self.last_drag_px = Some((mx, my));
                cx.notify();
                return;
            }
        }
        // Hover detection (only when not dragging).
        let lx = mx - f32::from(bounds.origin.x);
        let ly = my - f32::from(bounds.origin.y);
        let bw: f32 = bounds.size.width.into();
        let bh: f32 = bounds.size.height.into();
        let center = Point { x: bw * 0.5, y: bh * 0.5 };
        let new_hover = self.hit_test_node(lx, ly, center);
        if new_hover != self.hover_node {
            self.hover_node = new_hover;
            cx.notify();
        }
    }

    pub fn on_mouse_up(&mut self, _event: &MouseUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        // If a node was under mouse_down and distance is tiny -> treat as click.
        let was_click = self.drag_distance < 4.0;
        let clicked_node = self.dragging_node;
        self.dragging_bg = false;
        self.dragging_node = None;
        self.last_drag_px = None;
        self.down_px = None;
        self.drag_distance = 0.0;
        if was_click {
            if let Some(idx) = clicked_node {
                if let Some(n) = self.nodes.get(idx) {
                    cx.emit(GraphEvent::OpenNote(n.path.clone()));
                }
            }
        }
        cx.notify();
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn edge_count(&self) -> usize { self.edges.len() }
}

/// Fruchterman-Reingold force-directed layout.
/// Returns positions in an arbitrary coordinate space -- caller normalizes.
fn fruchterman_reingold(n: usize, edges: &[(usize, usize)]) -> Vec<(f32, f32)> {
    if n == 0 { return Vec::new(); }
    if n == 1 { return vec![(0.0, 0.0)]; }

    // Arena size scales with node count; keeps target density constant.
    let area = 1_000_000.0_f32 * (n as f32).max(1.0) / 50.0;
    let w = area.sqrt();
    let h = w;
    let k = (area / n as f32).sqrt();

    // Deterministic initial layout: place on a golden-angle spiral so same vault
    // always produces same graph. Avoids PRNG dep.
    let golden = std::f32::consts::PI * (3.0 - (5.0_f32).sqrt());
    let mut pos: Vec<(f32, f32)> = (0..n).map(|i| {
        let r = (i as f32 + 0.5).sqrt() * (w * 0.3 / (n as f32).sqrt());
        let theta = golden * i as f32;
        (r * theta.cos(), r * theta.sin())
    }).collect();

    // Build adjacency list (symmetric) for attraction.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        adj[a].push(b);
        adj[b].push(a);
    }

    // Iterate. More iterations for larger graphs.
    let iterations: usize = (120 + n).min(400);
    let mut t = w * 0.10; // initial temperature (max displacement per step)

    for _ in 0..iterations {
        let mut disp: Vec<(f32, f32)> = vec![(0.0, 0.0); n];

        // Repulsion: all pairs push apart. O(n²) is fine up to ~1000 nodes.
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i].0 - pos[j].0;
                let dy = pos[i].1 - pos[j].1;
                let dist_sq = (dx * dx + dy * dy).max(0.01);
                let dist = dist_sq.sqrt();
                // Repulsive force magnitude: k² / d
                let f = (k * k) / dist;
                let ux = dx / dist;
                let uy = dy / dist;
                disp[i].0 += ux * f;
                disp[i].1 += uy * f;
                disp[j].0 -= ux * f;
                disp[j].1 -= uy * f;
            }
        }

        // Attraction: along edges, pull nodes together. f = d² / k
        for &(a, b) in edges {
            let dx = pos[a].0 - pos[b].0;
            let dy = pos[a].1 - pos[b].1;
            let dist = (dx * dx + dy * dy).sqrt().max(0.01);
            let f = (dist * dist) / k;
            let ux = dx / dist;
            let uy = dy / dist;
            disp[a].0 -= ux * f;
            disp[a].1 -= uy * f;
            disp[b].0 += ux * f;
            disp[b].1 += uy * f;
        }

        // Move, bounded by temperature; clamp to arena.
        for i in 0..n {
            let (dx, dy) = disp[i];
            let mag = (dx * dx + dy * dy).sqrt().max(0.0001);
            let step = mag.min(t);
            pos[i].0 += (dx / mag) * step;
            pos[i].1 += (dy / mag) * step;
            // Weak containment: clamp into arena so isolated nodes don't fly off.
            pos[i].0 = pos[i].0.clamp(-w * 0.5, w * 0.5);
            pos[i].1 = pos[i].1.clamp(-h * 0.5, h * 0.5);
        }

        // Cooling: temperature decays toward a small constant.
        t *= 0.97;
        if t < 0.5 { t = 0.5; }
    }

    pos
}
