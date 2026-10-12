//! The schema-grounded NL→Gremlin pipeline (ADR-0019): context and prompt building with privacy modes
//! (ADR-0021), answer parsing, static validation, read-only dry runs (ADR-0020) and the repair loop.

pub mod answer;
pub mod check;
pub mod context;
pub mod dryrun;
pub mod pipeline;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use answer::{Answer, Intent, Shape};
pub use check::{Issue, IssueKind, check};
pub use context::{Context, Dialect};
pub use dryrun::{DryRunner, inject_limit};
pub use pipeline::{Badges, DryRun, Generator, Options, Outcome, PipelineEvent, Stage, run};
