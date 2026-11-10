//! Independent report qualification reuses immutable Factory records and retained Artifact reads.
use crate::factory::authoring::factory_configuration_choices::require_authorized_reference;
use crate::factory::commands::factory_command_catalog::FactoryCommandCatalog;
use crate::*;
use octacity_artifact_store::ArtifactByteReader;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{
  ArtifactRecordStore, BuildQueryStore, FactoryRunStore, IssueClassificationEvidenceReference,
  IssueClassificationProblem, IssueReproductionClaim,
};
use std::sync::Arc;

/// A current independent evidence reader; implementations must enforce content authority/retention.
#[async_trait::async_trait]
pub trait IssueClassificationEvidenceReader: Send + Sync {
  /// Returns no proof for missing, hidden, expired or inconsistent selected evidence.
  async fn read(
    &self,
    context: &ManagementRequestContext,
    reference: &IssueClassificationEvidenceReference,
    checked_at: Timestamp,
  ) -> Result<Option<VerifiedIssueClassificationEvidence>, ApplicationError>;
}

/// Qualified command finding, with no body or caller-writable verification flag.
#[derive(Clone, Debug)]
pub struct VerifiedIssueClassificationEvidence {
  reference: IssueClassificationEvidenceReference,
  finding: FactoryCommandFinding,
}
impl VerifiedIssueClassificationEvidence {
  /// Independently evaluated observation; ordinary process success alone supplies no finding.
  pub const fn finding(&self) -> FactoryCommandFinding {
    self.finding
  }
  /// Compares a claim without granting execution or changing its unresolved route.
  pub fn review_claim(
    &self,
    reference: &IssueClassificationEvidenceReference,
    claim: &str,
  ) -> Vec<IssueClassificationProblem> {
    let record = reference.record;
    let Some(claim) = IssueReproductionClaim::from_choice(claim) else {
      return vec![IssueClassificationProblem::UnsupportedReproduction];
    };
    if &self.reference != reference {
      return vec![IssueClassificationProblem::EvidenceUnavailable { record }];
    }
    match (claim, self.finding) {
      (_, FactoryCommandFinding::Indeterminate) => vec![IssueClassificationProblem::EvidenceInconclusive { record }],
      (IssueReproductionClaim::Reproduced, FactoryCommandFinding::NotMatched)
      | (IssueReproductionClaim::NotReproduced, FactoryCommandFinding::Matched) => {
        vec![IssueClassificationProblem::EvidenceClaimConflict { record }]
      }
      _ => vec![],
    }
  }
}

/// Reads only the selected ordinary report, without requiring access to a producer transcript.
pub struct FactoryIssueClassificationEvidenceReader<R, A, S, B> {
  runs: Arc<R>,
  reader: Arc<FactoryArtifactReader<A, S, B>>,
  catalog: FactoryCommandCatalog,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<R, A, S, B> FactoryIssueClassificationEvidenceReader<R, A, S, B> {
  /// Uses the same exact installed command profiles as ordinary dispatch/output qualification.
  pub fn new(
    runs: Arc<R>,
    reader: Arc<FactoryArtifactReader<A, S, B>>,
    profiles: Vec<FactoryCommandBuildProfile>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    Ok(Self {
      runs,
      reader,
      catalog: FactoryCommandCatalog::builtin(profiles)?,
      policy,
    })
  }
}
#[async_trait::async_trait]
impl<R: FactoryRunStore, A: ArtifactRecordStore, S: BuildQueryStore, B: ArtifactByteReader>
  IssueClassificationEvidenceReader for FactoryIssueClassificationEvidenceReader<R, A, S, B>
{
  async fn read(
    &self,
    context: &ManagementRequestContext,
    reference: &IssueClassificationEvidenceReference,
    checked_at: Timestamp,
  ) -> Result<Option<VerifiedIssueClassificationEvidence>, ApplicationError> {
    // Per-field unavailable diagnostics preserve the Issue read and disclose no source failures/bodies.
    Ok(self.qualified(context, reference, checked_at).await.ok())
  }
}
impl<R: FactoryRunStore, A: ArtifactRecordStore, S: BuildQueryStore, B: ArtifactByteReader>
  FactoryIssueClassificationEvidenceReader<R, A, S, B>
{
  async fn qualified(
    &self,
    context: &ManagementRequestContext,
    reference: &IssueClassificationEvidenceReference,
    checked_at: Timestamp,
  ) -> Result<VerifiedIssueClassificationEvidence, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let snapshot = self.runs.factory_run_snapshot(reference.run_id).await?;
    if snapshot.work.id() != reference.work_id
      || snapshot.run.id() != reference.run_id
      || checked_at < reference.observed_at
      || checked_at >= reference.fresh_until
    {
      return Err(invalid());
    }
    snapshot
      .flow
      .data
      .validate(&snapshot.work, &snapshot.admitted_flow, snapshot.flow.runtime())
      .map_err(|_| invalid())?;
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| record.digest().ok() == Some(reference.record))
      .ok_or_else(invalid)?;
    let intent = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|intent| intent.node().id() == record.node_attempt_id())
      .ok_or_else(invalid)?;
    let profile = self.catalog.profile(intent).map_err(|_| invalid())?;
    let contract = profile.observation_contract.as_ref().ok_or_else(invalid)?;
    let output = record.primary_build_output().ok_or_else(invalid)?;
    if reference.subject != FactoryTaskSubject::Exact(record.input().exact_resource().map_err(|_| invalid())?.clone())
      || reference.environment != contract.environment
      || reference.criterion != contract.criterion
      || reference.observed_at != record.observed_at()
      || reference.fresh_until > output.fresh_until
      || output.schema != contract.report_schema().map_err(|_| invalid())?.reference().clone()
    {
      return Err(invalid());
    }
    require_authorized_reference(
      self.policy.as_ref(),
      context,
      ManagementAction::Download,
      ManagementResourceKind::Artifact,
      output.artifact.artifact_id(),
    )
    .await?;
    let retained = self
      .reader
      .read(ReadFactoryArtifact {
        project_id: snapshot.work.subject().project_id(),
        artifact: output.artifact.clone(),
        maximum_bytes: MAX_FLOW_DATA_BYTES as u64,
        observed_at: checked_at,
        required_until: reference.fresh_until,
      })
      .await?;
    let value: serde_json::Value = serde_json::from_slice(retained.bytes()).map_err(|_| invalid())?;
    if &value != record.payload().value() {
      return Err(invalid());
    }
    let finding = verify_factory_command_observation(contract, intent, &value).map_err(|_| invalid())?;
    Ok(VerifiedIssueClassificationEvidence {
      reference: reference.clone(),
      finding,
    })
  }
}
