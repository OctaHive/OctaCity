use super::{ManagementOperation, ParameterProfile};

pub(super) const OPERATIONS: &[ManagementOperation] = &[
  operation!(
    "GET",
    "/api/v1/operations/metadata",
    "getOperationalMetadata",
    "Operations",
    "Get deployment security and capability status",
    None,
    "OperationalMetadata",
    "200",
    false,
    false
  ),
  operation!(
    "GET",
    "/api/v1/audit-facts",
    "listAuditFacts",
    "Audit",
    "List immutable audit facts",
    None,
    "AuditFactPage",
    "200",
    false,
    false
  )
  .with_parameters(ParameterProfile::AuditFacts),
];
