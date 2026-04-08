//! Inline icon shapes built from GPUI divs.
//! All icons are ~13x13 visual size and take a color parameter.

use gpui::*;
use std::path::Path;

/// Width and height for file-type icons.
pub const ICON_W: f32 = 11.0;
pub const ICON_H: f32 = 13.0;

/// Notion-style page icon: rectangle with a folded top-right corner.
pub fn file_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().relative()
        .w(px(12.)).h(px(14.))
        .child(
            // Main page body
            div().absolute().top(px(0.)).left(px(0.))
                .w(px(12.)).h(px(14.))
                .border_1().border_color(color)
                .rounded(px(1.5))
        )
        .child(
            // Folded corner
            div().absolute().top(px(0.)).right(px(0.))
                .w(px(4.)).h(px(4.))
                .bg(color.opacity(0.9))
                .rounded_bl(px(1.))
        )
        .child(
            div().absolute().top(px(6.)).left(px(2.))
                .w(px(7.)).h_px().bg(color.opacity(0.7))
        )
        .child(
            div().absolute().top(px(9.)).left(px(2.))
                .w(px(7.)).h_px().bg(color.opacity(0.7))
        )
}

/// Folder icon: outlined rectangle with top-left tab notch.
pub fn folder_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().relative()
        .w(px(14.)).h(px(12.))
        .child(
            // Body (outlined)
            div().absolute().top(px(3.)).left(px(0.))
                .w(px(14.)).h(px(9.))
                .border_1().border_color(color)
                .rounded(px(1.5))
        )
        .child(
            // Tab notch on top-left (filled, overlaps top border)
            div().absolute().top(px(0.)).left(px(1.))
                .w(px(5.)).h(px(4.))
                .bg(color)
                .rounded_t(px(1.5))
        )
}

/// Image icon: rectangle with a small triangle (mountain) and dot (sun) inside.
pub fn image_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().relative()
        .w(px(ICON_W)).h(px(ICON_H))
        .border_1().border_color(color)
        .rounded(px(1.5))
        .child(
            // Sun dot
            div().absolute().top(px(2.)).left(px(2.))
                .w(px(2.)).h(px(2.))
                .bg(color)
                .rounded_full()
        )
        .child(
            // Mountain base
            div().absolute().bottom(px(2.)).left(px(1.5))
                .w(px(6.)).h(px(2.))
                .bg(color.opacity(0.6))
                .rounded_sm()
        )
}

/// PDF icon: document with "PDF" label area at bottom.
pub fn pdf_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .flex().flex_col()
        .w(px(ICON_W)).h(px(ICON_H))
        .border_1().border_color(color)
        .rounded(px(1.5))
        .child(
            // Upper area (document body)
            div().flex_1().flex().flex_col().items_center().justify_center().gap(px(1.))
                .child(div().w(px(5.)).h_px().bg(color.opacity(0.5)))
                .child(div().w(px(5.)).h_px().bg(color.opacity(0.5)))
        )
        .child(
            // PDF label bar at bottom
            div().w_full().h(px(4.))
                .bg(color.opacity(0.7))
                .rounded_b(px(1.))
        )
}

/// Code file icon: angle brackets inside rectangle.
pub fn code_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .flex().items_center().justify_center()
        .w(px(ICON_W)).h(px(ICON_H))
        .border_1().border_color(color)
        .rounded(px(1.5))
        .text_size(px(7.)).text_color(color)
        .child("{}")
}

/// Pick icon based on file extension.
pub fn icon_for_path(path: &Path, color: Hsla) -> Div {
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase());
    match ext.as_deref() {
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("svg") | Some("bmp") => image_icon(color),
        Some("pdf") => pdf_icon(color),
        Some("rs") | Some("py") | Some("js") | Some("ts") | Some("tsx") | Some("jsx") | Some("go") | Some("java") | Some("cpp") | Some("c") | Some("h") | Some("hpp") | Some("json") | Some("toml") | Some("yaml") | Some("yml") | Some("sh") | Some("css") | Some("html") => code_icon(color),
        _ => file_icon(color),
    }
}

/// Close button X character (for tabs).
pub fn close_char() -> &'static str { "\u{2715}" }

/// Chevron right (collapsed folder).
pub fn chevron_right_char() -> &'static str { "\u{25B8}" }

/// Chevron down (expanded folder).
pub fn chevron_down_char() -> &'static str { "\u{25BE}" }

// ── Rail icons (18px square, for the narrow left rail) ──

pub const RAIL_ICON_SIZE: f32 = 18.0;

/// Files rail icon: two stacked horizontal bars representing a list.
pub fn rail_files_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().flex().flex_col().justify_center().gap(px(3.))
        .w(px(RAIL_ICON_SIZE)).h(px(RAIL_ICON_SIZE))
        .child(div().w(px(14.)).h(px(2.)).bg(color).rounded(px(1.)))
        .child(div().w(px(14.)).h(px(2.)).bg(color).rounded(px(1.)))
        .child(div().w(px(10.)).h(px(2.)).bg(color).rounded(px(1.)))
}

/// Graph rail icon: three circles connected by lines.
pub fn rail_graph_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().relative()
        .w(px(RAIL_ICON_SIZE)).h(px(RAIL_ICON_SIZE))
        // Top-left node
        .child(div().absolute().top(px(1.)).left(px(2.))
            .w(px(5.)).h(px(5.)).rounded_full().bg(color))
        // Top-right node
        .child(div().absolute().top(px(2.)).right(px(1.))
            .w(px(4.)).h(px(4.)).rounded_full().bg(color))
        // Bottom-center node
        .child(div().absolute().bottom(px(1.)).left(px(6.))
            .w(px(5.)).h(px(5.)).rounded_full().bg(color))
        // Edge: top-left -> bottom-center (diagonal)
        .child(div().absolute().top(px(6.)).left(px(5.))
            .w(px(6.)).h_px().bg(color.opacity(0.6)))
        // Edge: top-right -> bottom-center (diagonal)
        .child(div().absolute().top(px(5.)).right(px(4.))
            .w(px(5.)).h_px().bg(color.opacity(0.6)))
}

/// Settings rail icon: gear-like shape (concentric circles + notches).
pub fn rail_settings_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().relative()
        .w(px(RAIL_ICON_SIZE)).h(px(RAIL_ICON_SIZE))
        // Outer ring
        .child(div().absolute().top(px(2.)).left(px(2.))
            .w(px(14.)).h(px(14.)).rounded_full()
            .border_2().border_color(color))
        // Center dot
        .child(div().absolute().top(px(7.)).left(px(7.))
            .w(px(4.)).h(px(4.)).rounded_full().bg(color))
}

/// Search rail icon: magnifying glass (circle + diagonal line).
pub fn rail_search_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().relative()
        .w(px(RAIL_ICON_SIZE)).h(px(RAIL_ICON_SIZE))
        // Circle
        .child(div().absolute().top(px(2.)).left(px(2.))
            .w(px(10.)).h(px(10.)).rounded_full()
            .border_2().border_color(color))
        // Handle
        .child(div().absolute().bottom(px(2.)).right(px(2.))
            .w(px(6.)).h(px(2.)).bg(color).rounded(px(1.)))
}

/// Speech bubble icon for the chat rail button.
pub fn rail_chat_icon(color: Hsla) -> Div {
    div()
        .flex_shrink_0().relative()
        .w(px(RAIL_ICON_SIZE)).h(px(RAIL_ICON_SIZE))
        // Bubble body
        .child(div().absolute().top(px(1.)).left(px(1.))
            .w(px(14.)).h(px(10.))
            .rounded(px(3.))
            .border_2().border_color(color))
        // Tail
        .child(div().absolute().bottom(px(1.)).left(px(4.))
            .w(px(4.)).h(px(4.))
            .bg(color))
}
