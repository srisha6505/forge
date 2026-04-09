#![allow(dead_code)]

mod agent;
mod app;
mod auth;
mod buffer;
mod chat;
mod editor;
mod embedder;
mod graph;
mod icons;
mod links;
mod llm;
mod markdown;
mod search;
mod settings;
mod theme;

fn main() {
    app::run_app();
}
