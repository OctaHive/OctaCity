//! Discovers trusted source plugins and supervises source materialization.
//!
//! Registry verification and process hosting live together because they form
//! one trust boundary: only an operator-installed and digest-verified plugin
//! may be started. Provider behavior remains outside this crate behind the
//! versioned `octacity-source-plugin` process protocol.

mod host;
mod registry;

pub use host::{MaterializedSource, SourceHostError, SourceMaterializationRequest};
pub use registry::{InstalledSourcePlugin, RegistryError, SourcePluginRegistry};
