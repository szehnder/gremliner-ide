//! Graph database providers (ADR-0015): saved connection settings, the provider abstraction, the Gremlin
//! Server and Neptune providers, connection testing, the read-only guard, and the connection store.

pub mod config;
pub mod endpoint;
pub mod error;
pub mod gremlin_server;
pub mod neptune;
pub mod provider;
pub mod read_only;
pub mod schema;
pub mod store;

pub use config::{
    ColorTag, ConnectionConfig, Environment, GremlinServerConfig, NeptuneConfig, ProviderConfig,
    ProviderKind,
};
pub use error::{FailureKind, ProviderError};
pub use provider::{
    Capabilities, GraphProvider, ServerApi, ServerInfo, Session, TestReport, provider_for,
    restrictions_for, test_connection,
};
pub use store::{ConnectionStore, NameError, PasswordChange, StoreError, validate_name};
