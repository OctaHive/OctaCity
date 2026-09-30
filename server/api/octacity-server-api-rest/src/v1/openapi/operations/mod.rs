use std::any::{TypeId, type_name};

use octacity_server_application::{ManagementAuthorizationMapping, ManagementAuthorizationTarget};

/// One management operation in the versioned REST contract.
///
/// The router and OpenAPI drift test use this registry as the stable inventory
/// that later feature tasks extend when they add complete application use cases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagementOperation {
  /// Uppercase HTTP method.
  pub method: &'static str,
  /// Templated OpenAPI path.
  pub path: &'static str,
  /// Stable OpenAPI operation identifier.
  pub operation_id: &'static str,
  /// OpenAPI tag grouping the operation.
  pub tag: &'static str,
  /// Short operator-facing operation summary.
  pub summary: &'static str,
  /// Request component schema, when the operation accepts JSON.
  pub request_schema: Option<&'static str>,
  /// Successful response component schema.
  pub response_schema: &'static str,
  /// Successful HTTP status code.
  pub success_status: &'static str,
  /// Whether the operation requires `Idempotency-Key`.
  pub idempotent_mutation: bool,
  /// Whether the operation requires an optimistic `If-Match` precondition.
  pub optimistic_precondition: bool,
  authorization: Option<ManagementOperationAuthorization>,
  pub(super) parameter_profile: ParameterProfile,
  pub(super) capability_unavailable_response: bool,
  pub(super) precondition_failed_response: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ManagementOperationAuthorization {
  request_type_id: fn() -> TypeId,
  request_type_name: fn() -> &'static str,
  mapping: fn() -> ManagementAuthorizationMapping,
}

impl ManagementOperationAuthorization {
  pub(crate) fn request_type_id(self) -> TypeId {
    (self.request_type_id)()
  }

  pub(crate) fn request_type_name(self) -> &'static str {
    (self.request_type_name)()
  }

  pub(crate) fn mapping(self) -> ManagementAuthorizationMapping {
    (self.mapping)()
  }
}

impl std::fmt::Debug for ManagementOperationAuthorization {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ManagementOperationAuthorization")
      .field("request_type", &self.request_type_name())
      .field("mapping", &self.mapping())
      .finish()
  }
}

impl PartialEq for ManagementOperationAuthorization {
  fn eq(&self, other: &Self) -> bool {
    self.request_type_id() == other.request_type_id() && self.mapping() == other.mapping()
  }
}

impl Eq for ManagementOperationAuthorization {}

const fn authorization<T: ManagementAuthorizationTarget + 'static>() -> ManagementOperationAuthorization {
  ManagementOperationAuthorization {
    request_type_id: TypeId::of::<T>,
    request_type_name: type_name::<T>,
    mapping: || T::AUTHORIZATION,
  }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ParameterProfile {
  #[default]
  None,
  ProjectList,
  AgentPoolList,
  AgentList,
  JobEvents,
  ArtifactList,
  CacheSessionList,
  InternalTriggerList,
  BuildLogSearch,
  AuditFacts,
}

impl ManagementOperation {
  pub(crate) fn authorization(&self) -> ManagementOperationAuthorization {
    self
      .authorization
      .expect("published management operations carry authorization metadata")
  }

  const fn with_parameters(mut self, profile: ParameterProfile) -> Self {
    self.parameter_profile = profile;
    self
  }

  const fn with_capability_unavailable_response(mut self) -> Self {
    self.capability_unavailable_response = true;
    self
  }

  const fn with_precondition_failed_response(mut self) -> Self {
    self.precondition_failed_response = true;
    self
  }
}

macro_rules! operation {
  ($target:ty, $method:literal, $path:literal, $id:literal, $tag:literal, $summary:literal, $request:expr, $response:literal, $status:literal, $mutation:literal, $precondition:literal) => {
    ManagementOperation {
      method: $method,
      path: $path,
      operation_id: $id,
      tag: $tag,
      summary: $summary,
      request_schema: $request,
      response_schema: $response,
      success_status: $status,
      idempotent_mutation: $mutation,
      optimistic_precondition: $precondition,
      authorization: Some($crate::v1::openapi::operations::authorization::<$target>()),
      parameter_profile: ParameterProfile::None,
      capability_unavailable_response: false,
      precondition_failed_response: false,
    }
  };
}

mod agents;
mod artifacts;
mod builds;
mod core;
mod definitions;
mod triggers;

const GROUPS: &[&[ManagementOperation]] = &[
  core::OPERATIONS,
  definitions::OPERATIONS,
  triggers::OPERATIONS,
  builds::OPERATIONS,
  agents::OPERATIONS,
  artifacts::OPERATIONS,
];

const OPERATION_COUNT: usize = core::OPERATIONS.len()
  + definitions::OPERATIONS.len()
  + triggers::OPERATIONS.len()
  + builds::OPERATIONS.len()
  + agents::OPERATIONS.len()
  + artifacts::OPERATIONS.len();

const EMPTY_OPERATION: ManagementOperation = ManagementOperation {
  method: "",
  path: "",
  operation_id: "",
  tag: "",
  summary: "",
  request_schema: None,
  response_schema: "",
  success_status: "",
  idempotent_mutation: false,
  optimistic_precondition: false,
  authorization: None,
  parameter_profile: ParameterProfile::None,
  capability_unavailable_response: false,
  precondition_failed_response: false,
};

const fn collect_operations() -> [ManagementOperation; OPERATION_COUNT] {
  let mut operations = [EMPTY_OPERATION; OPERATION_COUNT];
  let mut output_index = 0;
  let mut group_index = 0;
  while group_index < GROUPS.len() {
    let group = GROUPS[group_index];
    let mut operation_index = 0;
    while operation_index < group.len() {
      operations[output_index] = group[operation_index];
      output_index += 1;
      operation_index += 1;
    }
    group_index += 1;
  }
  operations
}

const OPERATIONS: [ManagementOperation; OPERATION_COUNT] = collect_operations();

/// Complete inventory of registered management operations.
pub const MANAGEMENT_OPERATIONS: &[ManagementOperation] = &OPERATIONS;
