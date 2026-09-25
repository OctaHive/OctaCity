use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    "POST",
    "/api/v1/trigger-definitions/manual",
    "createManualTriggerDefinition",
    "Triggers",
    "Create a manual Trigger definition",
    Some("CreateManualTriggerDefinitionRequest"),
    "TriggerDefinitionMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/trigger-definitions/scheduled",
    "createScheduledTriggerDefinition",
    "Triggers",
    "Create a durable scheduled Trigger definition",
    Some("CreateScheduledTriggerDefinitionRequest"),
    "TriggerDefinitionMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/trigger-definitions/internal",
    "createInternalTriggerDefinition",
    "Triggers",
    "Create a source-scoped internal Trigger",
    Some("InternalTriggerDefinitionRequest"),
    "InternalTriggerMutationResponse",
    "201",
    true,
    false
  ),
  operation!(
    "GET",
    "/api/v1/trigger-definitions/internal",
    "listInternalTriggerDefinitions",
    "Triggers",
    "List current internal Trigger versions",
    None,
    "InternalTriggerPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::InternalTriggerList),
  operation!(
    "POST",
    "/api/v1/trigger-definitions/internal/{trigger_id}/versions",
    "publishInternalTriggerDefinitionVersion",
    "Triggers",
    "Publish an internal Trigger version",
    Some("InternalTriggerDefinitionRequest"),
    "InternalTriggerMutationResponse",
    "200",
    true,
    true
  ),
  operation!(
    "GET",
    "/api/v1/trigger-definitions/internal/{trigger_id}/versions/{version}",
    "getInternalTriggerDefinitionVersion",
    "Triggers",
    "Get an internal Trigger version",
    None,
    "InternalTriggerResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/unmanaged",
    "createUnmanagedWebhookIntegration",
    "Webhook Integrations",
    "Create an unmanaged webhook integration",
    Some("CreateUnmanagedWebhookRequest"),
    "UnmanagedWebhookResource",
    "201",
    true,
    false
  ),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/managed",
    "createManagedWebhookIntegration",
    "Webhook Integrations",
    "Create a provider-managed webhook integration",
    Some("CreateManagedWebhookRequest"),
    "ManagedWebhookResource",
    "201",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/managed/{integration_id}/observe",
    "observeManagedWebhookIntegration",
    "Webhook Integrations",
    "Observe a managed remote webhook registration",
    None,
    "ManagedWebhookResource",
    "200",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "POST",
    "/api/v1/webhook-integrations/managed/{integration_id}/rotate",
    "rotateManagedWebhookIntegration",
    "Webhook Integrations",
    "Rotate a managed remote webhook registration",
    None,
    "ManagedWebhookResource",
    "200",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "DELETE",
    "/api/v1/webhook-integrations/managed/{integration_id}",
    "deleteManagedWebhookIntegration",
    "Webhook Integrations",
    "Delete a managed remote webhook registration",
    None,
    "ManagedWebhookResource",
    "200",
    true,
    false
  )
  .with_capability_unavailable_response(),
  operation!(
    "GET",
    "/api/v1/schedules/{trigger_id}/versions/{version}",
    "getSchedule",
    "Triggers",
    "Get a durable schedule",
    None,
    "ScheduleResource",
    "200",
    false,
    false
  ),
  operation!(
    "POST",
    "/api/v1/triggers/manual",
    "acceptManualTrigger",
    "Triggers",
    "Accept a manual Trigger",
    Some("AcceptManualTriggerRequest"),
    "TriggerEvaluationResponse",
    "200",
    true,
    false
  ),
];
