//! Stable management REST wire contract below `/api/v1`.
//!
//! These DTOs intentionally contain only wire primitives. They do not expose
//! application projections, domain entities, persistence rows, or provider
//! payloads. Route adapters perform explicit conversion at the boundary.

mod adapter;
mod agent;
mod artifact;
mod audit;
mod cache;
mod common;
mod configuration;
mod definition_discovery;
mod execution;
mod job_event;
mod log_search;
mod openapi;
mod operations;
mod operator_attention;
mod pipeline;
mod pool;
mod project;
mod resource_search;
mod retention;
mod trigger;

pub use adapter::{
  AgentManagementApplication, ArtifactManagementApplication, AuditManagementApplication,
  BuildLogSearchManagementApplication, BuildManagementApplication, BuildResultRetentionManagementApplication,
  CacheManagementApplication, CatalogManagementApplication, ConfigurationManagementApplication,
  DefinitionManagementApplication, ExecutionManagementApplication, InternalTriggerManagementApplication,
  JobEventManagementApplication, ManagementApplication, ManagementApplicationHandlers,
  ManagementAuthorizationOperation, ManualTriggerManagementApplication, OperationalMetadataManagementApplication,
  OperatorAttentionManagementApplication, PipelineManagementApplication, ProjectManagementApplication,
  ResourceSearchManagementApplication, ScheduleManagementApplication, management_authorization_operations,
  router as management_routes,
};
pub use agent::*;
pub use artifact::*;
pub use audit::*;
pub use cache::*;
pub use common::*;
pub use configuration::*;
pub use definition_discovery::*;
pub use execution::*;
pub use job_event::*;
pub use log_search::*;
pub use openapi::{MANAGEMENT_OPERATIONS, ManagementOperation, openapi_document};
pub use operations::*;
pub use operator_attention::*;
pub use pipeline::*;
pub use pool::*;
pub use project::*;
pub use resource_search::*;
pub use retention::*;
pub use trigger::*;

/// URL prefix for the first management API contract.
pub const API_PREFIX: &str = "/api/v1";
