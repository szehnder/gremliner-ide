//! Persisted UI state: the dock layout and the window geometry, stored as JSON in `ui_state`.
//!
//! Reads are blocking and happen only while building the window (before its first frame). Writes go
//! through the storage thread. Anything that fails to parse is ignored with a warning, never an error
//! dialog: the user just gets the default layout.

use gpui_kit::component::dock::DockAreaState;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use storage::{Storage, StorageError, Table};

/// Bump when the default layout changes incompatibly; older saved layouts are then discarded.
pub const LAYOUT_VERSION: usize = 1;
pub const DOCK_KEY: &str = "layout.dock";
pub const WINDOW_KEY: &str = "layout.window";

/// Why a saved layout was not used.
#[derive(Debug)]
pub enum LayoutProblem {
    Unreadable(StorageError),
    OldVersion(Option<usize>),
}

/// The saved dock layout, if there is a usable one.
pub fn load_dock(storage: &Storage) -> Result<Option<DockAreaState>, LayoutProblem> {
    let state: Option<DockAreaState> = storage
        .get_json_blocking(Table::UiState, DOCK_KEY)
        .map_err(LayoutProblem::Unreadable)?;
    match state {
        Some(state) if state.version != Some(LAYOUT_VERSION) => {
            Err(LayoutProblem::OldVersion(state.version))
        }
        other => Ok(other),
    }
}

pub fn save_dock(storage: &Storage, state: &DockAreaState, cx: &App) {
    let write = storage.set_json(Table::UiState, DOCK_KEY, state);
    cx.background_spawn(async move {
        if let Err(err) = write.await {
            log::warn!("failed to save dock layout: {err}");
        }
    })
    .detach();
}

/// Window placement in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

impl WindowGeometry {
    pub fn from_bounds(bounds: WindowBounds) -> Self {
        let (rect, maximized) = match bounds {
            WindowBounds::Windowed(b) => (b, false),
            WindowBounds::Maximized(b) | WindowBounds::Fullscreen(b) => (b, true),
        };
        Self {
            x: f32::from(rect.origin.x),
            y: f32::from(rect.origin.y),
            width: f32::from(rect.size.width),
            height: f32::from(rect.size.height),
            maximized,
        }
    }

    /// Back to window bounds, rejecting sizes too small to be a deliberate choice.
    pub fn to_bounds(self, min: Size<Pixels>) -> Option<WindowBounds> {
        let valid = self.width.is_finite()
            && self.height.is_finite()
            && self.width >= f32::from(min.width)
            && self.height >= f32::from(min.height);
        if !valid {
            return None;
        }
        let rect = Bounds::new(
            point(px(self.x), px(self.y)),
            size(px(self.width), px(self.height)),
        );
        Some(if self.maximized {
            WindowBounds::Maximized(rect)
        } else {
            WindowBounds::Windowed(rect)
        })
    }
}

pub fn load_window(storage: &Storage) -> Option<WindowGeometry> {
    match storage.get_json_blocking(Table::UiState, WINDOW_KEY) {
        Ok(geometry) => geometry,
        Err(err) => {
            log::warn!("ignoring unreadable window geometry: {err}");
            None
        }
    }
}

pub fn save_window(storage: &Storage, geometry: WindowGeometry, cx: &App) {
    let write = storage.set_json(Table::UiState, WINDOW_KEY, &geometry);
    cx.background_spawn(async move {
        if let Err(err) = write.await {
            log::warn!("failed to save window geometry: {err}");
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    // `gpui_kit::*` (test-support) exports a `test` macro that would shadow the built-in one.
    use ::core::prelude::v1::test;

    use super::*;

    #[test]
    fn geometry_round_trips_and_rejects_tiny_windows() {
        let min = size(px(640.0), px(400.0)); // token-check: allow (window geometry)
        let g = WindowGeometry {
            x: 10.0,
            y: 20.0,
            width: 1200.0,
            height: 800.0,
            maximized: false,
        };
        let bounds = g.to_bounds(min).unwrap();
        assert_eq!(WindowGeometry::from_bounds(bounds), g);
        let tiny = WindowGeometry { width: 10.0, ..g };
        assert!(tiny.to_bounds(min).is_none());
    }

    #[test]
    fn old_layout_versions_are_discarded() {
        let storage = Storage::open_in_memory().unwrap();
        let state = DockAreaState {
            version: Some(LAYOUT_VERSION + 99),
            ..Default::default()
        };
        futures::executor::block_on(storage.set_json(Table::UiState, DOCK_KEY, &state)).unwrap();
        assert!(matches!(
            load_dock(&storage),
            Err(LayoutProblem::OldVersion(_))
        ));
    }
}
