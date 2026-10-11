//! Graph database providers (ADR-0015): saved connection settings, the provider abstraction, the Gremlin
//! Server provider, connection testing, the read-only guard, and the connection store.

pub mod config;
pub mod endpoint;
pub mod error;
pub mod gremlin_server;
pub mod provider;
pub mod read_only;
pub mod store;

pub use config::{
    ColorTag, ConnectionConfig, Environment, GremlinServerConfig, NeptuneConfig, ProviderConfig,
    ProviderKind,
};
pub use error::{FailureKind, ProviderError};
pub use provider::{
    Capabilities, GraphProvider, ServerInfo, Session, TestReport, provider_for, test_connection,
};
pub use store::{ConnectionStore, NameError, PasswordChange, StoreError, validate_name};
