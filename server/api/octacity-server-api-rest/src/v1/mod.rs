//! Stable management REST wire contract below `/api/v1`.
//!
//! These DTOs intentionally contain only wire primitives. They do not expose
//! application projections, domain entities, persistence rows, or provider
//! payloads. Route adapters perform explicit conversion at the boundary.

mod adapter;
mod agent;
mod common;
mod configuration;
mod execution;
mod job_event;
mod openapi;
mod operations;
mod pipeline;
mod pool;
mod project;
mod trigger;

pub use adapter::{
  AgentManagementApplication, BuildManagementApplication, CatalogManagementApplication,
  ConfigurationManagementApplication, DefinitionManagementApplication, ExecutionManagementApplication,
  JobEventManagementApplication, ManagementApplication, ManagementApplicationHandlers,
  ManualTriggerManagementApplication, PipelineManagementApplication, ProjectManagementApplication,
  ScheduleManagementApplication, router as management_routes,
};
pub use agent::*;
pub use common::*;
pub use configuration::*;
pub use execution::*;
pub use job_event::*;
pub use openapi::{MANAGEMENT_OPERATIONS, ManagementOperation, openapi_document};
pub use operations::*;
pub use pipeline::*;
pub use pool::*;
pub use project::*;
pub use trigger::*;

/// URL prefix for the first management API contract.
pub const API_PREFIX: &str = "/api/v1";
