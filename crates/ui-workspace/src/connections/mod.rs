//! Connections in the app: the manager entity, the connection dialog, and shared visuals.

pub mod form;
pub mod manager;
pub mod style;

pub use manager::{ConnStatus, ConnectionManager, SaveError, Saved};
