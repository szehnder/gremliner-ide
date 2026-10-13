//! Dockable panels and the registration API feature crates use to add their own.
//!
//! A panel implements gpui-kit's two dock traits plus [`WorkspacePanel`]. [`register`] makes it
//! restorable from a saved layout by its [`WorkspacePanel::NAME`]; never rename a shipped name.

mod assistant;
mod connections;
pub mod debug_view;
mod editor_tab;
mod empty;
mod inspector;
mod results;
mod schema_graph;
mod settings_view;
mod welcome;

use gpui_kit::component::dock::{DockPlacement, Panel, panel_handle, register_panel};
use gpui_kit::*;

pub use assistant::AssistantPanel;
pub use connections::ConnectionsPanel;
pub use editor_tab::{EditorPanel, EditorTabs, EditorTabsEvent, RunRequest};
pub use empty::empty_state;
pub use inspector::InspectorPanel;
pub use results::ResultsPanel;
pub use schema_graph::SchemaGraphPanel;
pub use settings_view::{SettingsPanel, confirm_reset_general};
pub use welcome::WelcomePanel;

/// A panel the workspace knows how to build, place and restore.
pub trait WorkspacePanel: Panel + Sized {
    /// Persisted identity, also returned by `panel_name`.
    const NAME: &'static str;
    /// Where the panel goes in the default layout.
    const DEFAULT_PLACEMENT: DockPlacement;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self>;
}

/// Register a panel so saved layouts can rebuild it.
pub fn register<P: WorkspacePanel>(cx: &mut App) {
    register_panel(cx, P::NAME, |_, window, cx| {
        panel_handle(P::build(window, cx))
    });
}

/// Register every built-in panel.
pub fn register_builtin(cx: &mut App) {
    register::<ConnectionsPanel>(cx);
    register::<WelcomePanel>(cx);
    register::<ResultsPanel>(cx);
    register::<InspectorPanel>(cx);
    register::<AssistantPanel>(cx);
    register::<SettingsPanel>(cx);
    editor_tab::register(cx);
    schema_graph::register(cx);
}

/// Implements the boilerplate traits every simple panel needs.
macro_rules! panel_basics {
    ($ty:ty) => {
        impl gpui_kit::EventEmitter<gpui_kit::component::dock::PanelEvent> for $ty {}

        impl gpui_kit::Focusable for $ty {
            fn focus_handle(&self, _: &gpui_kit::App) -> gpui_kit::FocusHandle {
                self.focus_handle.clone()
            }
        }
    };
}
pub(crate) use panel_basics;
