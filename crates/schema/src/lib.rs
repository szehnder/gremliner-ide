//! Schema discovery: the model of a connected graph's effective schema (labels, typed properties,
//! counts, samples and connectivity), bounded sampling discovery over any [`QueryRunner`], and the
//! statistics-summary fast path. No I/O of its own; `providers` runs the queries.

pub mod discover;
pub mod graph;
pub mod model;
pub mod summary;

pub use discover::{Options, QueryError, QueryRunner, discover};
pub use model::{Cardinality, Count, LabelInfo, Partial, PropertyInfo, Schema, Source, Triple};
pub use summary::Summary;
