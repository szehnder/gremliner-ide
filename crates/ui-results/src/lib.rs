//! Results views: a streaming `ResultSet` model and its Table, JSON and Console views, hosted by
//! `ResultsView` in the results dock.

pub mod console_view;
pub mod json_view;
pub mod result_set;
pub mod results_view;
pub mod table_view;

pub use result_set::{ResultSet, ResultSetEvent, RunError, RunStatus, error_range};
pub use results_view::{ResultsView, ResultsViewEvent, ViewKind};
