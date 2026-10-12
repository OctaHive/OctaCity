use octacity_server_application::{
  AdmitManualFactoryWorkCommand, CreateFactoryConfigurationCommand, CreateFactoryFlowDefinitionCommand,
  GetCurrentFactoryConfigurationQuery, GetFactoryConfigurationQuery, GetFactoryFlowDefinitionQuery,
  ListFactoryConfigurationChoicesQuery, ListFactoryPhaseReadyQuery, ListFactoryRunsQuery,
  ListProjectFactoryConfigurationsQuery, ListProjectFactoryFlowDefinitionsQuery, ReplaceFactoryConfigurationCommand,
  ReplaceFactoryFlowDefinitionCommand, ValidateFactoryConfigurationQuery, ValidateFactoryFlowDefinitionQuery,
};

use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    ListFactoryPhaseReadyQuery,
    "GET",
    "/api/v1/factory-phase-ready",
    "listFactoryPhaseReady",
    "Factory",
    "List visible phase-ready entries in immutable policy order",
    None,
    "FactoryPhaseReadyPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::FactoryPhaseReady)
  .with_visibility::<ListFactoryPhaseReadyQuery>(),
  operation!(
    ListFactoryPhaseReadyQuery,
    "GET",
    "/api/v1/projects/{project_id}/factory-phase-ready",
    "listProjectFactoryPhaseReady",
    "Factory",
    "List visible Project phase-ready entries",
    None,
    "FactoryPhaseReadyPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::FactoryPhaseReady)
  .with_visibility::<ListFactoryPhaseReadyQuery>(),
  operation!(
    ListFactoryRunsQuery,
    "GET",
    "/api/v1/factory-runs",
    "listFactoryRuns",
    "Factory",
    "List visible Factory Runs newest first",
    None,
    "FactoryRunSummaryPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::FactoryRunList)
  .with_visibility::<ListFactoryRunsQuery>(),
  operation!(
    ListFactoryRunsQuery,
    "GET",
    "/api/v1/projects/{project_id}/factory-runs",
    "listProjectFactoryRuns",
    "Factory",
    "List visible Project Factory Runs newest first",
    None,
    "FactoryRunSummaryPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::FactoryRunList)
  .with_visibility::<ListFactoryRunsQuery>(),
  operation!(
    AdmitManualFactoryWorkCommand,
    "POST",
    "/api/v1/projects/{project_id}/factory-work",
    "admitManualFactoryWork",
    "Factory",
    "Admit bounded manual Work or replay the original result",
    Some("AdmitManualFactoryWorkRequest"),
    "FactoryAdmissionResponse",
    "201",
    true,
    false
  ),
  operation!(
    CreateFactoryFlowDefinitionCommand,
    "POST",
    "/api/v1/flow-definitions",
    "createFactoryFlowDefinition",
    "Factory",
    "Publish an immutable Flow root",
    Some("CreateFactoryFlowDefinitionRequest"),
    "FactoryFlowDefinitionMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    ReplaceFactoryFlowDefinitionCommand,
    "POST",
    "/api/v1/flow-definitions/{flow_definition_id}/versions",
    "replaceFactoryFlowDefinition",
    "Factory",
    "Publish the next immutable Flow root",
    Some("ReplaceFactoryFlowDefinitionRequest"),
    "FactoryFlowDefinitionMutationResponse",
    "201",
    true,
    true
  )
  .with_precondition_failed_response(),
  operation!(
    GetFactoryFlowDefinitionQuery,
    "GET",
    "/api/v1/flow-definitions/{flow_definition_id}",
    "getCurrentFactoryFlowDefinition",
    "Factory",
    "Read the current Flow root",
    None,
    "FactoryFlowDefinitionDetails",
    "200",
    false,
    false
  ),
  operation!(
    GetFactoryFlowDefinitionQuery,
    "GET",
    "/api/v1/flow-definitions/{flow_definition_id}/versions/{version}",
    "getFactoryFlowDefinition",
    "Factory",
    "Read an exact Flow root",
    None,
    "FactoryFlowDefinitionDetails",
    "200",
    false,
    false
  ),
  operation!(
    ListProjectFactoryFlowDefinitionsQuery,
    "GET",
    "/api/v1/projects/{project_id}/flow-definitions",
    "listProjectFactoryFlowDefinitions",
    "Factory",
    "List visible immutable Flow and subflow choices",
    None,
    "FactoryFlowDefinitionPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::CurrentDefinitionList)
  .with_visibility::<ListProjectFactoryFlowDefinitionsQuery>(),
  operation!(
    ValidateFactoryFlowDefinitionQuery,
    "POST",
    "/api/v1/flow-definitions/validate",
    "validateFactoryFlowDefinition",
    "Factory",
    "Validate an exact reachable Flow graph",
    Some("CreateFactoryFlowDefinitionRequest"),
    "FactoryFlowConfiguration",
    "200",
    false,
    false
  ),
  operation!(
    ValidateFactoryConfigurationQuery,
    "POST",
    "/api/v1/factory-configurations/validate",
    "validateFactoryConfiguration",
    "Factory",
    "Validate a configuration without publishing",
    Some("CreateFactoryConfigurationRequest"),
    "FactoryConfigurationValidation",
    "200",
    false,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    ReplaceFactoryConfigurationCommand,
    "POST",
    "/api/v1/factory-configurations/{factory_configuration_id}/versions",
    "replaceFactoryConfiguration",
    "Factory",
    "Publish the next immutable Factory Configuration version",
    Some("ReplaceFactoryConfigurationRequest"),
    "FactoryConfigurationMutationResponse",
    "201",
    true,
    true
  )
  .with_capability_unavailable_response()
  .with_precondition_failed_response(),
  operation!(
    CreateFactoryConfigurationCommand,
    "POST",
    "/api/v1/factory-configurations",
    "createFactoryConfiguration",
    "Factory",
    "Publish an initial immutable Factory Configuration",
    Some("CreateFactoryConfigurationRequest"),
    "FactoryConfigurationMutationResponse",
    "201",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    ListFactoryConfigurationChoicesQuery,
    "GET",
    "/api/v1/projects/{project_id}/factory-configuration-choices",
    "listFactoryConfigurationChoices",
    "Factory",
    "List exact selectable configuration references",
    None,
    "FactoryConfigurationChoicePage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::FactoryConfigurationChoices)
  .with_visibility::<ListFactoryConfigurationChoicesQuery>(),
  operation!(
    GetFactoryConfigurationQuery,
    "GET",
    "/api/v1/factory-configurations/{factory_configuration_id}/versions/{version}",
    "getFactoryConfiguration",
    "Factory",
    "Read an exact Factory Configuration version",
    None,
    "FactoryConfigurationDetails",
    "200",
    false,
    false
  ),
  operation!(
    GetCurrentFactoryConfigurationQuery,
    "GET",
    "/api/v1/factory-configurations/{factory_configuration_id}",
    "getCurrentFactoryConfiguration",
    "Factory",
    "Read the current Factory Configuration version",
    None,
    "FactoryConfigurationDetails",
    "200",
    false,
    false
  ),
  operation!(
    ListProjectFactoryConfigurationsQuery,
    "GET",
    "/api/v1/projects/{project_id}/factory-configurations",
    "listProjectFactoryConfigurations",
    "Factory",
    "List current Factory Configurations in a Project",
    None,
    "FactoryConfigurationSummaryPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::CurrentDefinitionList)
  .with_visibility::<ListProjectFactoryConfigurationsQuery>(),
];
