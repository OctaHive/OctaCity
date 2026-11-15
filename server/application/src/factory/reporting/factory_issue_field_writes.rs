//! Automatic guarded Issue effects from an ordinary agent's exact accepted form.
use super::FactoryIssueClassificationAction;
use crate::*;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::Arc;

pub(crate) fn effect_kind(writer: &ImmutableReference) -> Result<FactoryKey, ApplicationError> {
  let bytes = serde_json::to_vec(writer).map_err(|_| ApplicationError::invalid())?;
  FactoryKey::new(format!("issue.fields.{}", FactoryDigest::content_sha256(&bytes)))
    .map_err(|_| ApplicationError::invalid())
}
pub(crate) fn accepted_effect(
  record: &FlowNodeRecord,
  write: &FlowIssueFieldWrite,
) -> Result<FactoryNodeOutboxEffect, ApplicationError> {
  let digest = record.digest().map_err(|_| ApplicationError::invalid())?;
  let binding = serde_json::to_vec(write).map_err(|_| ApplicationError::invalid())?;
  Ok(FactoryNodeOutboxEffect {
    kind: effect_kind(&write.writer)?,
    operation_id: FactoryDigest::sha256("octacity.issue-field-write.v1", &[&digest.as_bytes(), &binding]),
  })
}

/// Deferred local writes use the installed trusted action; input provenance has a read-only owner.
/// The existing outbox owns retries; no model dispatch or Flow routing occurs here.
pub struct FactoryIssueFieldWrites<S, F> {
  action: Arc<FactoryIssueClassificationAction<S, F>>,
  issues: Arc<dyn IssueStore>,
  settings: FactoryReporterWorkerSettings,
}
impl<S, F> FactoryIssueFieldWrites<S, F> {
  /// Reuses host write authority and the existing bounded outbox settings.
  pub fn new(
    action: Arc<FactoryIssueClassificationAction<S, F>>,
    issues: Arc<dyn IssueStore>,
    settings: FactoryReporterWorkerSettings,
  ) -> Self {
    Self {
      action,
      issues,
      settings,
    }
  }
}
impl<S: IssueClassificationStore, F: FactoryRunStore + FactoryAdmissionStore> FactoryIssueFieldWrites<S, F> {
  async fn apply(
    &self,
    row: &FactoryOutboxRecord,
    at: Timestamp,
  ) -> Result<(), ManagementHandlerError<ApplicationError>> {
    let invalid = || ManagementHandlerError::Application(ApplicationError::invalid());
    let run = row.subject.run_id().ok_or_else(invalid)?;
    let snapshot = self
      .action
      .runs
      .factory_run_snapshot(run)
      .await
      .map_err(ApplicationError::from)
      .map_err(ManagementHandlerError::Application)?;
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| record.digest().ok() == Some(row.input_digest))
      .ok_or_else(invalid)?;
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(record.input().definition())
      .ok_or_else(invalid)?;
    let node = definition.node(record.input().node()).ok_or_else(invalid)?;
    let outcome = node.outcome(record.outcome()).ok_or_else(invalid)?;
    let selected = outcome.issue_fields().ok_or_else(invalid)?;
    let expected = accepted_effect(record, selected).map_err(ManagementHandlerError::Application)?;
    let completion = snapshot
      .flow
      .completions
      .iter()
      .find(|completion| {
        completion.node_attempt_id() == record.node_attempt_id()
          && completion.output_digest() == row.input_digest
          && completion.output_schema() == record.payload().schema()
          && completion.outcome() == record.outcome()
      })
      .ok_or_else(invalid)?;
    if node.kind() != FlowNodeKind::Reasoning
      || outcome.kind() != FlowOutcomeKind::Success
      || selected.writer != self.action.identity
      || row.kind != expected.kind
      || row.operation_id != expected.operation_id
      || selected.form.project_id != snapshot.work.subject().project_id()
      || outcome.schema() != record.payload().schema()
      || !snapshot
        .admitted_flow
        .execution_permissions(node.permissions())
        .plugins()
        .any(|allowed| allowed == &selected.writer)
    {
      return Err(invalid());
    }
    let baseline = record.input().issue_field_baseline().ok_or_else(invalid)?;
    let association = self
      .action
      .associations
      .issue_work_association(snapshot.work.subject().project_id(), snapshot.work.id())
      .await
      .map_err(ApplicationError::from)
      .map_err(ManagementHandlerError::Application)?
      .filter(|a| a.run_id == run && a.issue_id == baseline.issue_id && a.requirements_version.is_some())
      .ok_or_else(invalid)?;
    let values = record
      .payload()
      .value()
      .as_object()
      .ok_or_else(invalid)?
      .iter()
      .map(|(key, value)| Ok((FactoryKey::new(key.clone()).map_err(|_| invalid())?, value.clone())))
      .collect::<Result<_, _>>()?;
    // Construct only from the accepted form. Catalog/value revalidation belongs inside the
    // existing receipt transaction, so replay survives later definition edits and lost acks.
    let fields = IssueClassificationUpdate {
      selection: selected.form.clone(),
      values,
      invocation: Some(IssueFieldInvocation {
        node: record.input().node().clone(),
        attempt_id: record.node_attempt_id(),
        baseline: baseline.version,
      }),
      inputs: IssueClassificationAssessmentInputs {
        subject: record
          .input()
          .task_subject()
          .ok()
          .filter(|subject| subject.exact().is_ok()),
        materials: self
          .material_dependencies(&snapshot, record)
          .await
          .map_err(ManagementHandlerError::Application)?,
      },
    };
    let claim = row.claim.ok_or_else(invalid)?;
    let now = self
      .action
      .clock
      .now()
      .map_err(ManagementHandlerError::Application)?
      .max(at);
    claim.verify_fence(claim.fence(), now).map_err(|_| invalid())?;
    let remaining = u64::try_from(claim.expires_at().unix_millis() - now.unix_millis()).map_err(|_| invalid())?;
    self
      .action
      .commands
      .handle_command(
        &self.action.context,
        ApplyIssueClassificationCommand(ApplyIssueClassification {
          association,
          policy: selected.policy.clone(),
          record: row.input_digest,
          operation_id: row.operation_id,
          accepted_at: completion.observed_at(),
          applied_at: now,
          priority: None,
          size_label: None,
          managed_labels: Default::default(),
          fields: Some(fields),
          authority: Some(IssueClassificationAuthority {
            claim: IssueClassificationClaim::Outbox(row.id),
            deadline: std::time::Instant::now()
              .checked_add(std::time::Duration::from_millis(remaining))
              .ok_or_else(invalid)?,
          }),
        }),
      )
      .await
      .map(|_| ())
  }
}
#[async_trait::async_trait]
impl<S: IssueClassificationStore + 'static, F: FactoryRunStore + FactoryAdmissionStore + 'static>
  FactoryOutboxDispatcher for FactoryIssueFieldWrites<S, F>
{
  async fn dispatch_once(&self, at: Timestamp) -> Result<FactoryOutboxDispatchSummary, ApplicationError> {
    let at = self.action.clock.now()?.max(at);
    let expires = at
      .unix_millis()
      .checked_add(self.settings.claim_millis as i64)
      .and_then(|v| Timestamp::from_unix_millis(v).ok())
      .ok_or_else(ApplicationError::invalid)?;
    let claims = self
      .action
      .runs
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(self.settings.owner.clone(), at, expires, self.settings.limit)?
          .only_kind(effect_kind(&self.action.identity)?),
      )
      .await?;
    let mut summary = FactoryOutboxDispatchSummary {
      claimed: claims.len().try_into().map_err(|_| ApplicationError::invalid())?,
      ..Default::default()
    };
    for claimed in claims {
      let row = claimed.record;
      let settlement = match self.apply(&row, at).await {
        Ok(()) => {
          summary.delivered += 1;
          FactoryOutboxSettlement::Delivered
        }
        Err(ManagementHandlerError::Application(error @ ApplicationError::Store(StoreError::Unavailable))) => {
          return Err(error);
        }
        Err(ManagementHandlerError::Forbidden(_))
        | Err(ManagementHandlerError::Application(ApplicationError::CapabilityUnavailable)) => {
          summary.failed += 1;
          FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Forbidden)
        }
        Err(ManagementHandlerError::Application(ApplicationError::Store(StoreError::Conflict { .. }))) => {
          summary.failed += 1;
          FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Conflict)
        }
        Err(_) => {
          summary.failed += 1;
          FactoryOutboxSettlement::Rejected(FactoryOutboxFailure::Unsupported)
        }
      };
      self
        .action
        .runs
        .settle_factory_outbox(SettleFactoryOutbox {
          operation_id: row.operation_id,
          owner: self.settings.owner.clone(),
          fence: row.claim.ok_or_else(ApplicationError::invalid)?.fence(),
          observed_at: self.action.clock.now()?.max(at),
          settlement,
        })
        .await?;
    }
    Ok(summary)
  }
}

#[path = "factory_issue_field_materials.rs"]
mod materials;
