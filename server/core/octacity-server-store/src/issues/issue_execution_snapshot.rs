//! Private selected input retained by the existing Project text preparation owner.
use crate::*;
use octacity_server_domain::{IssueId, IssueVersion, ProjectId};
use octacity_server_factory::{FactoryDigest, FactoryKey, FlowIssueSnapshotInput};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[path = "issue_execution_snapshot_definitions.rs"]
mod definitions;
pub use definitions::*;
#[path = "issue_execution_snapshot_materials.rs"]
mod materials;
pub use materials::*;

/// Exact selected values and their interpretation; never a second mutable Issue model.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueExecutionSnapshot {
  /// Canonical owner, independent of a producing Run.
  pub project_id: ProjectId,
  /// Canonical Issue.
  pub issue_id: IssueId,
  /// Exact reviewed Issue revision.
  pub version: IssueVersion,
  /// Requirements applicability, distinct from unrelated planning edits.
  pub applicability_version: IssueVersion,
  /// Reviewed immutable target and source policy. The preparation freezes its resolved code.
  pub target: IssueExecutionTarget,
  /// Requirements identities, also checked against the published task/acceptance objects.
  pub title_digest: FactoryDigest,
  /// Original description identity.
  pub description_digest: FactoryDigest,
  /// Original acceptance identity.
  pub acceptance_digest: FactoryDigest,
  /// Exact consumer contract, including required/optional selection and schema identity.
  pub input: FlowIssueSnapshotInput,
  /// Catalog head observed during review; a concurrent publication requires fresh review.
  /// Absent only on retained snapshots created before the admission fence.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub definition_head: Option<u64>,
  /// Exact policy result sealed with this snapshot; absent on older explicit-classification contracts.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub admission_classification: Option<octacity_server_factory::WorkClassification>,
  /// Selected declarations and original publication pins used to interpret values and origins.
  pub definitions: BTreeMap<u64, IssueSnapshotDefinitions>,
  /// Only explicitly selected canonical scalar/dependency values, under consumer aliases.
  pub values: BTreeMap<FactoryKey, serde_json::Value>,
  /// Original assessment provenance for selected effective fields.
  pub attribution: BTreeMap<FactoryKey, IssueClassificationAttribution>,
  /// Explicit optional omissions; missing values are never fabricated defaults.
  pub omissions: BTreeMap<FactoryKey, IssueSnapshotOmission>,
  /// Exact selected source lineage; material text occurs once in `values`.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub materials: BTreeMap<FactoryKey, Vec<IssueSnapshotMaterial>>,
}
impl std::fmt::Debug for IssueExecutionSnapshot {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueExecutionSnapshot").finish_non_exhaustive()
  }
}
/// Why a declared optional projection has no frozen value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSnapshotOmission {
  /// No canonical value is present.
  Missing,
  /// An independent content/evidence reader cannot supply the selected input.
  Unavailable,
  /// The assessment no longer applies to the selected requirements.
  Inapplicable,
}
impl IssueExecutionSnapshot {
  /// Binds the complete retained selection and payload to the ordinary prepared admission.
  /// Matching a caller-supplied digest alone does not establish payload integrity.
  pub fn validate_prepared_admission(
    &self,
    preparation: &ProjectTextPreparation,
    request: &AdmitFactoryWork,
  ) -> Result<(), StoreError> {
    let schema = request
      .flow
      .data_schema(&self.input.schema)
      .ok_or_else(snapshot_conflict)?;
    let target = &preparation.intent.target;
    let classification = self
      .input
      .admission
      .as_ref()
      .map(|policy| policy.classify(&self.values).map_err(|_| snapshot_conflict()))
      .transpose()?;
    if classification != self.admission_classification
      || classification.as_ref().is_some_and(|expected| {
        expected != &preparation.intent.classification
          || expected.priority() != request.work.priority()
          || expected.risk() != request.work.risk()
          || expected.metadata() != request.work.metadata()
      })
      || Some(self.digest()?) != preparation.intent.snapshot_digest
      || self.project_id != target.project_id
      || self.target.configuration_id != target.configuration_id
      || self.target.configuration_version != target.configuration_version
      || Some(self.target.repository_id) != target.repository_id
      || Some(self.target.repository_version) != target.repository_version
      || self.target.source != preparation.intent.source
      || self.target.predecessor_run.is_some()
      || request.incoming.as_ref() != Some(&self.incoming(&request.work, &request.flow, schema)?)
      || self
        .incompatible_subject_field(&octacity_server_factory::FactoryTaskSubject::Exact(
          request
            .work
            .subject()
            .repository()
            .map_err(|_| snapshot_conflict())?
            .clone(),
        ))
        .is_some()
    {
      return Err(snapshot_conflict());
    }
    Ok(())
  }
  /// Identifies the first selected assessment tied to another exact code subject.
  /// Fields with no code dependency remain applicable without inventing a subject pin.
  pub fn incompatible_subject_field(
    &self,
    subject: &octacity_server_factory::FactoryTaskSubject,
  ) -> Option<&FactoryKey> {
    self.attribution.iter().find_map(|(alias, assessment)| {
      assessment
        .inputs
        .subject
        .as_ref()
        .is_some_and(|expected| expected != subject)
        .then_some(alias)
    })
  }
  /// Freezes selected values and their reference under the exact resolved Work/Run.
  pub fn incoming(
    &self,
    work: &octacity_server_factory::WorkEnvelope,
    flow: &octacity_server_factory::AdmittedFlow,
    schema: &octacity_server_factory::FlowDataSchema,
  ) -> Result<octacity_server_factory::FlowIncomingData, StoreError> {
    use octacity_server_factory::{FlowIncomingData, FlowIncomingIssueSnapshot, FlowPayload};
    let payload = FlowPayload::new(
      schema,
      serde_json::to_value(&self.values).map_err(|_| invalid_snapshot())?,
    )
    .map_err(|_| invalid_snapshot())?;
    FlowIncomingData::from_issue_snapshot(
      work,
      flow,
      payload,
      FlowIncomingIssueSnapshot {
        digest: self.digest()?,
        project_id: self.project_id,
        issue_id: self.issue_id,
        version: self.version,
        applicability_version: self.applicability_version,
        definition_version: self.input.definition_version,
      },
    )
    .map_err(|_| invalid_snapshot())
  }
  /// Canonical private selection identity, independent of transport retries and generated request IDs.
  pub fn digest(&self) -> Result<FactoryDigest, StoreError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid_snapshot())?;
    if bytes.len() > MAX_SOURCE_INPUT_BYTES as usize {
      return Err(invalid_snapshot());
    }
    Ok(FactoryDigest::content_sha256(&bytes))
  }
}
fn snapshot_conflict() -> StoreError {
  StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::Issue,
  }
}
fn invalid_snapshot() -> StoreError {
  StoreError::invalid(
    StoreOperation::ReserveProjectTextInput,
    StoreInputError::InvalidProjectTextInput,
  )
}

#[cfg(test)]
#[path = "issue_execution_snapshot_tests.rs"]
mod tests;
