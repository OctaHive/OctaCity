use async_trait::async_trait;
use octacity_server_domain::EntityKind;
use octacity_server_store::{
  AuditActorKind, StoreOperation,
  testing::{ManagementAuditProbe, MutationEvidenceCounts, MutationEvidenceProbe, RecordedManagementAuditFact},
};
use sqlx::PgPool;
use std::str::FromStr as _;

pub struct PostgresEvidenceProbe(pub PgPool);

#[async_trait]
impl MutationEvidenceProbe for PostgresEvidenceProbe {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let (idempotency, audit, outbox): (i64, i64, i64) = sqlx::query_as(
      "SELECT (SELECT COUNT(*) FROM idempotency_records), \
              (SELECT COUNT(*) FROM audit_facts), \
              (SELECT COUNT(*) FROM outbox_entries)",
    )
    .fetch_one(&self.0)
    .await
    .expect("contract evidence must remain queryable");
    MutationEvidenceCounts {
      idempotency: usize::try_from(idempotency).unwrap(),
      audit: usize::try_from(audit).unwrap(),
      outbox: usize::try_from(outbox).unwrap(),
    }
  }
}

#[async_trait]
impl ManagementAuditProbe for PostgresEvidenceProbe {
  async fn management_audit_facts(&self) -> Vec<RecordedManagementAuditFact> {
    let rows: Vec<(String, Option<String>, String, String, String, String)> = sqlx::query_as(
      "SELECT actor_kind, actor_identity, operation, target_kind, target_identity, request_identity \
       FROM audit_facts \
       WHERE actor_kind IN ('unauthenticated_management', 'authenticated_management') \
       ORDER BY actor_kind, actor_identity, operation, target_kind, target_identity, request_identity",
    )
    .fetch_all(&self.0)
    .await
    .expect("management audit evidence must remain queryable");
    let mut facts = rows
      .into_iter()
      .map(
        |(actor_kind, actor_identity, operation, target_kind, target_identity, request_identity)| {
          RecordedManagementAuditFact {
            actor_kind: AuditActorKind::from_str(&actor_kind).expect("stored actor kind is canonical"),
            actor_identity,
            operation: management_store_operation(&operation),
            target_kind: management_entity_kind(&target_kind),
            target_identity,
            request_identity,
          }
        },
      )
      .collect::<Vec<_>>();
    facts.sort_unstable();
    facts
  }
}

fn management_store_operation(operation: &str) -> StoreOperation {
  match operation {
    "publish-factory-role-profiles" => StoreOperation::PublishFactoryRoleProfiles,
    "create-factory-configuration" => StoreOperation::CreateFactoryConfiguration,
    "replace-factory-configuration" => StoreOperation::ReplaceFactoryConfiguration,
    "create-project" => StoreOperation::CreateProject,
    "rename-project" => StoreOperation::RenameProject,
    "move-project" => StoreOperation::MoveProject,
    "delete-project" => StoreOperation::DeleteProject,
    "create-agent-pool" => StoreOperation::CreateAgentPool,
    "publish-agent-pool-version" => StoreOperation::PublishAgentPoolVersion,
    "delete-agent-pool" => StoreOperation::DeleteAgentPool,
    "create-pipeline" => StoreOperation::CreatePipeline,
    "publish-pipeline-version" => StoreOperation::PublishPipelineVersion,
    "create-repository" => StoreOperation::CreateRepository,
    "publish-repository-version" => StoreOperation::PublishRepositoryVersion,
    "create-build-configuration" => StoreOperation::CreateBuildConfiguration,
    "publish-build-configuration-version" => StoreOperation::PublishBuildConfigurationVersion,
    "issue-agent-enrollment" => StoreOperation::IssueAgentEnrollment,
    "revoke-agent-enrollment" | "revoke-agent-registration" => StoreOperation::RevokeAgentCredential,
    "accept-trigger" => StoreOperation::AcceptTrigger,
    "suppress-trigger" => StoreOperation::SuppressTrigger,
    "cancel-build" => StoreOperation::CancelBuild,
    "retry-build" => StoreOperation::RetryBuild,
    _ => panic!("unexpected management audit operation: {operation}"),
  }
}

fn management_entity_kind(target_kind: &str) -> EntityKind {
  match target_kind {
    "factory_configuration" => EntityKind::FactoryConfiguration,
    "project" => EntityKind::Project,
    "pool" => EntityKind::Pool,
    "pipeline" => EntityKind::Pipeline,
    "repository" => EntityKind::Repository,
    "build_configuration" => EntityKind::Configuration,
    "agent_enrollment_credential" => EntityKind::AgentEnrollmentCredential,
    "agent_registration" => EntityKind::AgentRegistration,
    "build" => EntityKind::Build,
    "trigger" => EntityKind::Trigger,
    _ => panic!("unexpected management audit target kind: {target_kind}"),
  }
}
