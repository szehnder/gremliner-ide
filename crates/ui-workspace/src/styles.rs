//! Wires the shared graph stylesheet library (`ui_results::style_library`) to storage: loads stylesheets
//! and per-connection defaults at startup and persists every change in the background.

use std::collections::HashMap;

use gpui_kit::*;
use graph_store::style::Stylesheet;
use storage::{Storage, StylesheetRow, Table};
use ui_results::style_library::{LibraryChange, StyleLibrary};

use crate::settings::AppSettings;

/// `ui_state` key of the connection id → stylesheet id map.
pub const DEFAULTS_KEY: &str = "graph.stylesheet.defaults";

pub fn init(cx: &mut App) {
    let library = StyleLibrary::global(cx);
    let Some(storage) = AppSettings::storage(cx).cloned() else {
        return;
    };
    let writer = storage.clone();
    library.update(cx, |l, _| {
        l.set_persistence(move |change, cx| persist(&writer, change, cx));
    });
    let rows = storage.list_stylesheets();
    let defaults = storage.get_json::<HashMap<String, String>>(Table::UiState, DEFAULTS_KEY);
    cx.spawn(async move |cx| {
        let sheets: Vec<Stylesheet> = match rows.await {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|r| match Stylesheet::from_json(&r.json) {
                    Ok(s) => Some(s),
                    Err(err) => {
                        log::warn!("skipping stylesheet {:?}: {err}", r.name);
                        None
                    }
                })
                .collect(),
            Err(err) => {
                log::warn!("failed to load stylesheets: {err}");
                Vec::new()
            }
        };
        let defaults = defaults.await.ok().flatten().unwrap_or_default();
        library.update(cx, |l, cx| l.load(sheets, defaults, cx));
    })
    .detach();
}

fn persist(storage: &Storage, change: &LibraryChange, cx: &mut App) {
    let work: futures::future::BoxFuture<'static, storage::Result<()>> = match change {
        LibraryChange::Saved(sheet) => Box::pin(storage.save_stylesheet(StylesheetRow {
            id: sheet.id.clone(),
            name: sheet.name.clone(),
            json: sheet.to_json(),
        })),
        LibraryChange::Deleted(id) => Box::pin(storage.delete_stylesheet(id)),
        LibraryChange::ConnectionDefaults(map) => {
            Box::pin(storage.set_json(Table::UiState, DEFAULTS_KEY, map))
        }
    };
    cx.background_spawn(async move {
        if let Err(err) = work.await {
            log::warn!("failed to save stylesheets: {err}");
        }
    })
    .detach();
}
