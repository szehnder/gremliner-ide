//! Results views: a streaming `ResultSet` model and its Graph, Table, JSON, Console, Profile and Explain
//! views, hosted by
//! `ResultsView` in the results dock.

pub mod console_view;
pub mod explain_view;
pub mod graph_view;
pub mod inspector;
pub mod json_view;
pub mod profile_view;
pub mod result_set;
pub mod results_view;
pub mod style_editor;
pub mod style_library;
pub mod table_view;

pub use graph_view::{FetchKind, FetchRequest, GraphView, GraphViewEvent};
pub use result_set::{Analysis, ResultSet, ResultSetEvent, RunError, RunStatus, error_range};
pub use results_view::{ResultsView, ResultsViewEvent, ViewKind};
