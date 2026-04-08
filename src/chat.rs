//! Chat panel -- Zed-style right dock with sessions, markdown rendering,
//! collapsible thinking, tool call display, keyboard shortcuts, and
//! clickable file paths.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::input::{Input, InputEvent, InputState};

use crate::agent::{self, AgentEvent, ToolContext};
use crate::llm::{ChatMessage, InferenceHandle};

// ── Events emitted to parent ──

#[derive(Clone, Debug)]
pub enum ChatPanelEvent {
    OpenFile(PathBuf),
}

// ── UI message types ──

#[derive(Clone, Debug)]
pub enum UiMessage {
    User { content: String },
    Assistant {
        content: String,
        thinking: Option<String>,
        thinking_visible: bool,
        streaming: bool,
    },
    ToolCall {
        name: String,
        args: String,
        result: Option<String>,
        is_error: bool,
    },
    Error { message: String },
}

// ── Chat session ──

pub struct ChatSession {
    pub name: String,
    pub ui_messages: Vec<UiMessage>,
    pub llm_messages: Vec<ChatMessage>,
    pub system_prompt: String,
    pub busy: bool,
    pub tool_iterations: usize,
    /// Message input history for Up arrow recall.
    pub input_history: Vec<String>,
    pub history_cursor: usize,
}

impl ChatSession {
    pub fn new(name: impl Into<String>, system_prompt: impl Into<String>) -> Self {
        let prompt = system_prompt.into();
        Self {
            name: name.into(),
            ui_messages: Vec::new(),
            llm_messages: vec![ChatMessage::system(&prompt)],
            system_prompt: prompt,
            busy: false,
            tool_iterations: 0,
            input_history: Vec::new(),
            history_cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        self.ui_messages.clear();
        self.llm_messages = vec![ChatMessage::system(&self.system_prompt)];
        self.busy = false;
        self.tool_iterations = 0;
    }
}

// ── ChatPanel entity ──

pub struct ChatPanel {
    focus_handle: FocusHandle,
    pub sessions: Vec<ChatSession>,
    pub active_session: usize,
    input: Entity<InputState>,
    scroll: ScrollHandle,
    pub inference: Option<InferenceHandle>,
    event_rx: Option<mpsc::Receiver<AgentEvent>>,
    pub vault_path: Option<PathBuf>,
    pub db_path: Option<PathBuf>,
    max_tool_iterations: usize,
}

impl EventEmitter<ChatPanelEvent> for ChatPanel {}

impl Focusable for ChatPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ChatPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));

        cx.subscribe(&input, |this: &mut Self, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.send_message(cx);
            }
        }).detach();

        let mut panel = Self {
            focus_handle: cx.focus_handle(),
            sessions: Vec::new(),
            active_session: 0,
            input,
            scroll: ScrollHandle::new(),
            inference: None,
            event_rx: None,
            vault_path: None,
            db_path: None,
            max_tool_iterations: 10,
        };

        panel.sessions.push(ChatSession::new("Chat 1", "You are a helpful research assistant."));
        panel
    }

    pub fn set_inference(&mut self, handle: InferenceHandle) {
        self.inference = Some(handle);
    }

    pub fn set_vault(&mut self, vault_path: PathBuf, db_path: PathBuf) {
        self.vault_path = Some(vault_path);
        self.db_path = Some(db_path);
    }

    pub fn set_system_prompt(&mut self, prompt: &str) {
        if let Some(session) = self.sessions.get_mut(self.active_session) {
            session.system_prompt = prompt.to_string();
            if !session.llm_messages.is_empty() {
                session.llm_messages[0] = ChatMessage::system(prompt);
            }
        }
    }

    pub fn new_session(&mut self, _cx: &mut Context<Self>) {
        let n = self.sessions.len() + 1;
        let prompt = self.sessions.first()
            .map(|s| s.system_prompt.clone())
            .unwrap_or_else(|| "You are a helpful research assistant.".into());
        self.sessions.push(ChatSession::new(format!("Chat {n}"), prompt));
        self.active_session = self.sessions.len() - 1;
    }

    fn clear_session(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.get_mut(self.active_session) {
            if !session.busy {
                session.clear();
                cx.notify();
            }
        }
    }

    fn send_message(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if text.trim().is_empty() { return; }

        let session = match self.sessions.get_mut(self.active_session) {
            Some(s) => s,
            None => return,
        };
        if session.busy { return; }

        // Save to input history.
        session.input_history.push(text.clone());
        session.history_cursor = session.input_history.len();

        // Add messages.
        session.ui_messages.push(UiMessage::User { content: text.clone() });
        session.llm_messages.push(ChatMessage::user(&text));
        session.busy = true;
        session.tool_iterations = 0;
        session.ui_messages.push(UiMessage::Assistant {
            content: String::new(),
            thinking: None,
            thinking_visible: false,
            streaming: true,
        });

        let msg_count = session.ui_messages.len();
        self.scroll.scroll_to_item(msg_count.saturating_sub(1));

        // Check inference.
        let Some(inference) = &self.inference else {
            if let Some(s) = self.sessions.get_mut(self.active_session) {
                s.busy = false;
                s.ui_messages.pop();
                s.ui_messages.push(UiMessage::Error {
                    message: "No model loaded. Set model_path in settings.".into(),
                });
            }
            cx.notify();
            return;
        };

        // Spawn agent.
        let inference = inference.clone();
        let messages = session.llm_messages.clone();
        let tools = agent::tool_schemas();
        let vault_path = match &self.vault_path {
            Some(p) if p.is_dir() => p.clone(),
            _ => {
                if let Some(s) = self.sessions.get_mut(self.active_session) {
                    s.busy = false;
                    s.ui_messages.pop();
                    s.ui_messages.push(UiMessage::Error {
                        message: "No vault open. Press Ctrl+O to open a vault first.".into(),
                    });
                }
                cx.notify();
                return;
            }
        };
        let db_path = self.db_path.clone().unwrap_or_else(|| {
            dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
                .join("forge").join("forge.db")
        });
        let max_iters = self.max_tool_iterations;

        let (event_tx, event_rx) = mpsc::channel();
        self.event_rx = Some(event_rx);

        if let Err(e) = std::thread::Builder::new()
            .name("forge-agent".into())
            .spawn(move || {
                let ctx = ToolContext { vault_path, db_path };
                let mut msgs = messages;
                agent::run_agent_loop(&inference, &mut msgs, &tools, &ctx, max_iters, &event_tx);
            })
        {
            eprintln!("[forge] Failed to spawn agent thread: {e}");
            if let Some(s) = self.sessions.get_mut(self.active_session) {
                s.busy = false;
                s.ui_messages.pop();
                s.ui_messages.push(UiMessage::Error {
                    message: format!("Failed to start agent: {e}"),
                });
            }
            cx.notify();
            return;
        }

        self.start_event_poll(cx);
        cx.notify();
    }

    fn stop_generation(&mut self, cx: &mut Context<Self>) {
        // Drop the event receiver to signal the agent thread to stop.
        self.event_rx = None;
        self.end_streaming();
        if let Some(s) = self.sessions.get_mut(self.active_session) {
            s.busy = false;
        }
        cx.notify();
    }

    fn start_event_poll(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let should_stop = this.update(cx, |panel, cx| {
                    panel.drain_events(cx)
                }).unwrap_or(true);
                if should_stop { break; }
            }
        }).detach();
    }

    fn drain_events(&mut self, cx: &mut Context<Self>) -> bool {
        let events: Vec<AgentEvent> = {
            let Some(rx) = &self.event_rx else { return true; };
            let mut events = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        events.push(AgentEvent::Finished);
                        break;
                    }
                }
            }
            events
        };

        if events.is_empty() { return false; }

        let mut should_stop = false;

        for event in events {
            match event {
                AgentEvent::Token(text) => self.append_streaming_text(&text),
                AgentEvent::Thinking(text) => {
                    if let Some(s) = self.sessions.get_mut(self.active_session) {
                        if let Some(UiMessage::Assistant { thinking, .. }) = s.ui_messages.last_mut() {
                            let t = thinking.get_or_insert_with(String::new);
                            t.push_str(&text);
                        }
                    }
                }
                AgentEvent::ToolCallStarted { name, args } => {
                    self.end_streaming();
                    if let Some(s) = self.sessions.get_mut(self.active_session) {
                        s.ui_messages.push(UiMessage::ToolCall {
                            name, args, result: None, is_error: false,
                        });
                    }
                }
                AgentEvent::ToolCallResult { name: _, content, is_error } => {
                    if let Some(s) = self.sessions.get_mut(self.active_session) {
                        if let Some(UiMessage::ToolCall { result, is_error: ie, .. }) = s.ui_messages.last_mut() {
                            *result = Some(content);
                            *ie = is_error;
                        }
                        s.ui_messages.push(UiMessage::Assistant {
                            content: String::new(), thinking: None,
                            thinking_visible: false, streaming: true,
                        });
                    }
                }
                AgentEvent::Finished => {
                    self.end_streaming();
                    if let Some(s) = self.sessions.get_mut(self.active_session) {
                        s.busy = false;
                        // Remove empty trailing assistant messages.
                        while matches!(s.ui_messages.last(), Some(UiMessage::Assistant { content, .. }) if content.is_empty()) {
                            s.ui_messages.pop();
                        }
                    }
                    self.event_rx = None;
                    should_stop = true;
                }
                AgentEvent::Error(msg) => {
                    self.end_streaming();
                    if let Some(s) = self.sessions.get_mut(self.active_session) {
                        s.ui_messages.push(UiMessage::Error { message: msg });
                    }
                }
            }
        }

        if let Some(s) = self.sessions.get(self.active_session) {
            self.scroll.scroll_to_item(s.ui_messages.len().saturating_sub(1));
        }
        cx.notify();

        should_stop
    }

    fn append_streaming_text(&mut self, text: &str) {
        if let Some(s) = self.sessions.get_mut(self.active_session) {
            if let Some(UiMessage::Assistant { content, streaming: true, .. }) = s.ui_messages.last_mut() {
                content.push_str(text);
            }
        }
    }

    fn end_streaming(&mut self) {
        if let Some(s) = self.sessions.get_mut(self.active_session) {
            if let Some(UiMessage::Assistant { streaming, .. }) = s.ui_messages.last_mut() {
                *streaming = false;
            }
        }
    }
}

// ── Render ──

impl Render for ChatPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let bg = cx.theme().background;
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().accent;
        let border = cx.theme().border;
        let panel_bg = cx.theme().sidebar;

        let session_count = self.sessions.len();
        let active_idx = self.active_session;
        let is_busy = self.sessions.get(self.active_session).map(|s| s.busy).unwrap_or(false);

        // ── Tab bar ──
        let mut tab_bar = div()
            .id("chat-tab-bar")
            .flex().flex_row().items_center()
            .w_full().h(px(32.)).flex_shrink_0()
            .bg(panel_bg).border_b_1().border_color(border)
            .px(px(6.)).gap(px(2.));

        for i in 0..session_count {
            let name = self.sessions[i].name.clone();
            let active = i == active_idx;
            tab_bar = tab_bar.child(
                div().id(ElementId::NamedInteger("ct".into(), i as u64))
                    .flex().items_center().px(px(10.)).py(px(3.))
                    .rounded(px(4.)).text_size(px(11.)).cursor_pointer()
                    .bg(if active { accent.opacity(0.12) } else { transparent_black() })
                    .text_color(if active { fg } else { muted })
                    .hover(move |s: StyleRefinement| s.bg(accent.opacity(0.08)))
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        this.active_session = i; cx.notify();
                    }))
                    .child(name)
            );
        }
        // + button
        tab_bar = tab_bar.child(
            div().id("ct-new").flex().items_center().justify_center()
                .w(px(22.)).h(px(22.)).rounded(px(4.))
                .cursor_pointer().text_size(px(14.)).text_color(muted)
                .hover(move |s: StyleRefinement| s.bg(accent.opacity(0.08)))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.new_session(cx); cx.notify();
                }))
                .child("+")
        );
        // Spacer + clear button
        tab_bar = tab_bar.child(div().flex_1());
        tab_bar = tab_bar.child(
            div().id("ct-clear")
                .flex().items_center().justify_center()
                .px(px(6.)).py(px(2.)).rounded(px(4.))
                .cursor_pointer().text_size(px(10.)).text_color(muted.opacity(0.5))
                .hover(move |s: StyleRefinement| s.bg(accent.opacity(0.08)).text_color(muted))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.clear_session(cx);
                }))
                .child("Clear")
        );

        // ── Messages ──
        let mut msg_list = div()
            .id("chat-messages")
            .flex().flex_col()
            .flex_1().min_h(px(0.)).min_w(px(0.))
            .overflow_hidden()
            .track_scroll(&self.scroll)
            .px(px(10.)).py(px(8.)).gap(px(6.));

        if let Some(session) = self.sessions.get(self.active_session) {
            if session.ui_messages.is_empty() {
                let hint = if self.inference.is_none() {
                    "No model loaded -- set model_path in settings"
                } else {
                    "Ask a question about your vault"
                };
                msg_list = msg_list.child(
                    div().flex_1().flex().items_center().justify_center()
                        .child(div().text_size(px(12.)).text_color(muted.opacity(0.5)).child(hint))
                );
            }

            for (idx, msg) in session.ui_messages.iter().enumerate() {
                let el = render_msg(msg, idx, fg, muted, accent, border);
                let i = idx;
                // Thinking toggle click handler.
                msg_list = msg_list.child(
                    div()
                        .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            if let Some(s) = this.sessions.get_mut(this.active_session) {
                                if let Some(UiMessage::Assistant { thinking_visible, thinking, .. }) = s.ui_messages.get_mut(i) {
                                    if thinking.is_some() {
                                        *thinking_visible = !*thinking_visible;
                                        cx.notify();
                                    }
                                }
                            }
                        }))
                        .child(el)
                );
            }
        }

        // ── Input area ──
        let model_label = self.inference.as_ref()
            .map(|h| h.model_name.clone())
            .unwrap_or_else(|| "no model".into());
        let status_label = if is_busy { "generating..." } else { "idle" };

        let input_row = div()
            .flex().flex_row().gap(px(6.)).items_center()
            .child(div().flex_1().child(
                Input::new(&self.input).appearance(false).bordered(true)
            ));

        // Stop button when generating.
        let input_row = if is_busy {
            input_row.child(
                div().id("chat-stop")
                    .flex().items_center().justify_center()
                    .px(px(8.)).py(px(4.)).rounded(px(4.))
                    .cursor_pointer()
                    .bg(hsla(0.0, 0.6, 0.5, 0.15))
                    .text_size(px(11.)).text_color(hsla(0.0, 0.6, 0.5, 1.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .hover(move |s: StyleRefinement| s.bg(hsla(0.0, 0.6, 0.5, 0.25)))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.stop_generation(cx);
                    }))
                    .child("Stop")
            )
        } else {
            input_row
        };

        let input_area = div()
            .id("chat-input")
            .flex().flex_col().w_full().flex_shrink_0()
            .border_t_1().border_color(border)
            .bg(panel_bg)
            .px(px(10.)).py(px(6.)).gap(px(3.))
            .child(input_row)
            .child(
                div().flex().flex_row().items_center().gap(px(6.))
                    .child(div().text_size(px(10.)).text_color(muted.opacity(0.4))
                        .child(format!("{model_label} | {status_label}")))
                    .child(div().flex_1())
                    .child(div().text_size(px(10.)).text_color(muted.opacity(0.3))
                        .child("Enter to send | Ctrl+Shift+L toggle"))
            );

        // ── Assemble ──
        div()
            .id("chat-panel")
            .flex().flex_col()
            .size_full().min_w(px(0.))
            .bg(bg)
            .border_l_1().border_color(border)
            .track_focus(&self.focus_handle)
            .child(tab_bar)
            .child(msg_list)
            .child(input_area)
    }
}

// ── Message rendering ──

fn render_msg(
    msg: &UiMessage,
    idx: usize,
    fg: Hsla,
    muted: Hsla,
    accent: Hsla,
    border: Hsla,
) -> impl IntoElement {
    let id = ElementId::NamedInteger("cm".into(), idx as u64);

    match msg {
        // ── User ──
        UiMessage::User { content } => {
            div().id(id).flex().flex_col().gap(px(2.))
                .px(px(10.)).py(px(6.)).rounded(px(6.))
                .bg(accent.opacity(0.06))
                .child(
                    div().text_size(px(10.)).text_color(accent.opacity(0.7))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("You")
                )
                .child(div().text_size(px(13.)).text_color(fg).child(content.clone()))
        }

        // ── Assistant ──
        UiMessage::Assistant { content, thinking, thinking_visible, streaming } => {
            let mut el = div().id(id).flex().flex_col().gap(px(2.))
                .px(px(10.)).py(px(6.)).rounded(px(6.));

            // Thinking toggle.
            if let Some(thought) = thinking {
                if !thought.is_empty() {
                    let vis = *thinking_visible;
                    let arrow = if vis { "\u{25BE}" } else { "\u{25B8}" }; // down/right triangle
                    let label = format!("{arrow} Thinking ({} chars)", thought.len());
                    let mut think_el = div().flex().flex_col().mb(px(2.))
                        .child(
                            div().text_size(px(10.)).text_color(muted.opacity(0.5))
                                .cursor_pointer()
                                .child(label)
                        );
                    if vis {
                        think_el = think_el.child(
                            div().text_size(px(11.)).text_color(muted.opacity(0.4))
                                .px(px(8.)).py(px(4.)).mt(px(2.))
                                .rounded(px(4.)).bg(border.opacity(0.3))
                                .max_h(px(120.)).overflow_hidden()
                                .child(thought.clone())
                        );
                    }
                    el = el.child(think_el);
                }
            }

            // Content.
            if !content.is_empty() {
                el = el.child(md(content, fg, muted, accent, border));
            } else if *streaming {
                el = el.child(
                    div().text_size(px(11.)).text_color(muted.opacity(0.5))
                        .child(if thinking.is_some() { "Thinking..." } else { "..." })
                );
            }

            el
        }

        // ── Tool call ──
        UiMessage::ToolCall { name, args, result, is_error } => {
            let c = hsla(0.08, 0.65, 0.5, 1.0); // orange
            let mut el = div().id(id).flex().flex_col()
                .px(px(10.)).py(px(4.)).rounded(px(4.))
                .bg(c.opacity(0.04))
                .border_l_2().border_color(c.opacity(0.4))
                .child(
                    div().flex().flex_row().items_center().gap(px(6.))
                        .child(div().text_size(px(10.)).text_color(c).font_weight(FontWeight::SEMIBOLD)
                            .child(format!("\u{2699} {name}"))) // gear icon
                        .child(div().text_size(px(9.)).text_color(muted.opacity(0.5))
                            .child(if result.is_some() { "done" } else { "running..." }))
                );

            // Args (compact).
            let args_short = if args.len() > 120 { format!("{}...", &args[..120]) } else { args.clone() };
            el = el.child(
                div().text_size(px(10.)).text_color(muted.opacity(0.6))
                    .font_family("monospace")
                    .mt(px(1.)).overflow_x_hidden()
                    .child(args_short)
            );

            // Result.
            if let Some(r) = result {
                let display = if r.len() > 250 { format!("{}...", &r[..250]) } else { r.clone() };
                let rc = if *is_error { hsla(0.0, 0.7, 0.5, 0.8) } else { muted.opacity(0.5) };
                el = el.child(
                    div().text_size(px(10.)).text_color(rc)
                        .font_family("monospace")
                        .mt(px(2.)).max_h(px(80.)).overflow_hidden()
                        .child(display)
                );
            }

            el
        }

        // ── Error ──
        UiMessage::Error { message } => {
            let red = hsla(0.0, 0.7, 0.5, 1.0);
            div().id(id).flex().flex_col().gap(px(2.))
                .px(px(10.)).py(px(6.)).rounded(px(6.))
                .bg(red.opacity(0.06))
                .child(div().text_size(px(10.)).text_color(red).font_weight(FontWeight::SEMIBOLD).child("Error"))
                .child(div().text_size(px(12.)).text_color(fg).child(message.clone()))
        }
    }
}

// ── Markdown renderer ──

/// Render markdown text with headers, lists, code blocks, bold, inline code.
fn md(text: &str, fg: Hsla, muted: Hsla, accent: Hsla, border: Hsla) -> Div {
    let mut out = div().flex().flex_col().gap(px(1.)).text_size(px(13.)).text_color(fg);
    let mut in_code_block = false;
    let mut code_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        // Code block handling.
        if line.trim_start().starts_with("```") {
            if in_code_block {
                // End code block.
                out = out.child(render_code_block(&code_lines, muted, border));
                code_lines.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
            }
            continue;
        }
        if in_code_block {
            code_lines.push(line.to_string());
            continue;
        }

        let t = line.trim();
        if t.is_empty() {
            out = out.child(div().h(px(3.)));
            continue;
        }

        // Headers.
        if t.starts_with("### ") {
            out = out.child(div().text_size(px(14.)).font_weight(FontWeight::BOLD).mt(px(4.)).child(t[4..].to_string()));
        } else if t.starts_with("## ") {
            out = out.child(div().text_size(px(15.)).font_weight(FontWeight::BOLD).mt(px(6.)).child(t[3..].to_string()));
        } else if t.starts_with("# ") {
            out = out.child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).mt(px(8.)).child(t[2..].to_string()));
        }
        // Unordered list.
        else if t.starts_with("- ") || t.starts_with("* ") {
            out = out.child(
                div().flex().flex_row().gap(px(6.)).pl(px(8.))
                    .child(div().text_color(muted).child("\u{2022}"))
                    .child(div().flex_1().child(inline(t[2..].trim(), fg, muted, accent, border)))
            );
        }
        // Ordered list.
        else if t.len() > 2 && t.as_bytes()[0].is_ascii_digit() && t.contains(". ") {
            if let Some(dot) = t.find(". ") {
                let num = &t[..dot + 1];
                let rest = &t[dot + 2..];
                out = out.child(
                    div().flex().flex_row().gap(px(6.)).pl(px(8.))
                        .child(div().text_color(muted).min_w(px(14.)).child(num.to_string()))
                        .child(div().flex_1().child(inline(rest, fg, muted, accent, border)))
                );
            } else {
                out = out.child(inline(t, fg, muted, accent, border));
            }
        }
        // Horizontal rule.
        else if t == "---" || t == "***" || t == "___" {
            out = out.child(div().h(px(1.)).my(px(4.)).bg(border));
        }
        // Normal paragraph line.
        else {
            out = out.child(inline(t, fg, muted, accent, border));
        }
    }

    // Flush unclosed code block.
    if in_code_block && !code_lines.is_empty() {
        out = out.child(render_code_block(&code_lines, muted, border));
    }

    out
}

fn render_code_block(lines: &[String], muted: Hsla, border: Hsla) -> Div {
    let code = lines.join("\n");
    div()
        .text_size(px(11.)).text_color(muted)
        .font_family("monospace")
        .px(px(8.)).py(px(6.)).my(px(2.))
        .rounded(px(4.)).bg(border.opacity(0.2))
        .overflow_x_hidden()
        .child(code)
}

/// Render a line of text with inline `code`, **bold**, and *italic* handling.
fn inline(text: &str, fg: Hsla, muted: Hsla, accent: Hsla, border: Hsla) -> Div {
    // Detect if the line contains inline code backticks.
    if text.contains('`') {
        let mut container = div().flex().flex_row().flex_wrap().gap(px(0.)).text_color(fg);
        let mut remaining = text;
        while let Some(start) = remaining.find('`') {
            // Text before backtick.
            if start > 0 {
                container = container.child(
                    div().child(strip_bold(&remaining[..start]))
                );
            }
            let after = &remaining[start + 1..];
            if let Some(end) = after.find('`') {
                // Inline code span.
                let code = &after[..end];
                container = container.child(
                    div().text_size(px(11.)).text_color(accent)
                        .font_family("monospace")
                        .px(px(4.)).rounded(px(3.))
                        .bg(border.opacity(0.25))
                        .child(code.to_string())
                );
                remaining = &after[end + 1..];
            } else {
                // Unmatched backtick, render as-is.
                container = container.child(div().child(remaining.to_string()));
                remaining = "";
                break;
            }
        }
        if !remaining.is_empty() {
            container = container.child(div().child(strip_bold(remaining)));
        }
        container
    } else {
        // No inline code -- render with bold/italic stripping.
        let display = strip_bold(text);
        let is_bold = text.starts_with("**") && text.ends_with("**") && text.len() > 4;
        let mut el = div().text_color(fg);
        if is_bold { el = el.font_weight(FontWeight::BOLD); }
        el.child(display)
    }
}

/// Strip **bold** and *italic* markers.
fn strip_bold(text: &str) -> String {
    text.replace("**", "").replace("__", "")
}
