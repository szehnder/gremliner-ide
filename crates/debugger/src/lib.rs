//! The Gremlin step-through debugger (query-debugger): a session over one read-only statement with
//! navigation, and the queries it runs at each step: prefixes, capped counts, traversal simulation and
//! single-element inspection. Pure logic; the app runs the queries (ADR-0004).

pub mod query;
pub mod session;

pub use query::{ElementKind, IDS, format_count};
pub use session::{DebugStep, Session, StartError};
