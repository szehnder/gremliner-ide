//! Gremlin restrictions per database and TinkerPop version; the table lives with the language
//! (`gremlin_syntax::dialect`) so the editor and the assistant share it.

pub use gremlin_syntax::dialect::{Restrictions, gremlin_server, neptune, parse_version};
