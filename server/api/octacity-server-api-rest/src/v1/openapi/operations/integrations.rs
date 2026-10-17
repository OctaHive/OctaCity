use super::{ManagementOperation, ParameterProfile};
use octacity_server_application::{
  CleanupIntegrationPreparationsCommand, GetIntegrationAdministrationQuery, GetIntegrationConnectionQuery,
  GetIntegrationProjectAccessQuery, ListIntegrationProjectResourcesQuery, ManageIntegrationCredentialCommand,
  PublishIntegrationConnectionCommand, PublishIntegrationProjectAccessCommand, SetIntegrationConnectionStateCommand,
};
pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    PublishIntegrationProjectAccessCommand,
    "POST",
    "/api/v1/integrations/{integration_id}/projects/{project_id}/access",
    "publishIntegrationProjectAccess",
    "Integrations",
    "Publish explicit Project resource/purpose grants; requires both connection and Project administration",
    Some("IntegrationProjectAccessPublication"),
    "IntegrationProjectAccessMutation",
    "200",
    true,
    true
  )
  .with_protected_ingress(),
  operation!(
    GetIntegrationProjectAccessQuery,
    "GET",
    "/api/v1/integrations/{integration_id}/projects/{project_id}/access",
    "getIntegrationProjectAccess",
    "Integrations",
    "Read current Project grant metadata without granting use",
    None,
    "IntegrationProjectAccessState",
    "200",
    false,
    false
  )
  .with_protected_ingress(),
  operation!(
    ListIntegrationProjectResourcesQuery,
    "GET",
    "/api/v1/integrations/{integration_id}/projects/{project_id}/resources",
    "listIntegrationProjectResources",
    "Integrations",
    "List only visible Project-granted resource/purpose choices without credentials",
    None,
    "IntegrationProjectResources",
    "200",
    false,
    false
  )
  .with_protected_ingress(),
  operation!(
    ManageIntegrationCredentialCommand,
    "POST",
    "/api/v1/integrations/{integration_id}/credentials",
    "manageIntegrationCredential",
    "Integrations",
    "Submit, adopt, rotate or revoke an installed purpose credential without read-back",
    Some("IntegrationCredentialSubmission"),
    "IntegrationAdministrationReceipt",
    "200",
    true,
    true
  )
  .with_protected_ingress(),
  operation!(
    PublishIntegrationConnectionCommand,
    "POST",
    "/api/v1/integrations/{integration_id}/versions",
    "publishIntegrationConnection",
    "Integrations",
    "Publish immutable installed connection metadata without granting Project use",
    Some("IntegrationConnectionPublication"),
    "IntegrationConnectionMutation",
    "200",
    true,
    true
  )
  .with_protected_ingress(),
  operation!(
    GetIntegrationConnectionQuery,
    "GET",
    "/api/v1/integrations/{integration_id}",
    "getIntegrationConnection",
    "Integrations",
    "Read current safe connection metadata",
    None,
    "IntegrationConnection",
    "200",
    false,
    false
  )
  .with_protected_ingress(),
  operation!(
    GetIntegrationConnectionQuery,
    "GET",
    "/api/v1/integrations/{integration_id}/versions/{version}",
    "getIntegrationConnectionVersion",
    "Integrations",
    "Read retained immutable connection metadata",
    None,
    "IntegrationConnection",
    "200",
    false,
    false
  )
  .with_protected_ingress(),
  operation!(
    SetIntegrationConnectionStateCommand,
    "POST",
    "/api/v1/integrations/{integration_id}/state",
    "setIntegrationConnectionState",
    "Integrations",
    "Explicitly activate or disable a connection while retaining referenced metadata and material",
    Some("IntegrationStateChange"),
    "IntegrationAdministrationReceipt",
    "200",
    true,
    true
  )
  .with_protected_ingress(),
  operation!(
    GetIntegrationAdministrationQuery,
    "GET",
    "/api/v1/integrations/{integration_id}/state",
    "getIntegrationAdministration",
    "Integrations",
    "Read safe configured and disabled state without credential contents",
    None,
    "IntegrationAdministrationState",
    "200",
    false,
    false
  )
  .with_protected_ingress(),
  operation!(
    CleanupIntegrationPreparationsCommand,
    "POST",
    "/api/v1/integrations/{integration_id}/preparations/cleanup",
    "cleanupIntegrationPreparations",
    "Integrations",
    "Erase a bounded batch of expired private preparations without collecting runtime material",
    Some("IntegrationPreparationCleanup"),
    "IntegrationPreparationCleanupReceipt",
    "200",
    false,
    false
  )
  .with_protected_ingress(),
];
