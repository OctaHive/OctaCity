//! Applicability follows exact frozen content projections, not every material on the Issue.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

impl<S: IssueClassificationStore, F: FactoryRunStore + FactoryAdmissionStore> FactoryIssueFieldWrites<S, F> {
  pub(super) async fn material_dependencies(
    &self,
    snapshot: &FactoryRunSnapshot,
    record: &FlowNodeRecord,
  ) -> Result<Vec<IssueMaterialReference>, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let baseline = record.input().issue_field_baseline().ok_or_else(invalid)?;
    let project = snapshot.work.subject().project_id();
    let issue = self
      .issues
      .issue(project, baseline.issue_id, Some(baseline.version))
      .await?;
    let mut revisions = BTreeMap::from([(issue.version, issue)]);
    let mut materials = Vec::new();
    let mut material_contract = None;
    let mut seen = BTreeSet::new();
    let mut pending = vec![(record.input(), String::new(), true)];
    while let Some((input, path, include_context)) = pending.pop() {
      if !seen.insert((input.digest().map_err(|_| invalid())?, path.clone(), include_context)) {
        continue;
      }
      // Context is a dependency of a result, but is never forwarded by InputPayload/CallerInput.
      if include_context && let Some(context) = input.context().filter(|context| !context.entries().is_empty()) {
        // A selected accepted result carries its producer's frozen material selection.
        // The final writer's newer baseline remains only the destination write fence.
        let origin = input.issue_field_baseline().ok_or_else(invalid)?;
        if origin.issue_id != baseline.issue_id {
          return Err(invalid());
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = revisions.entry(origin.version) {
          entry.insert(
            self
              .issues
              .issue(project, origin.issue_id, Some(origin.version))
              .await?,
          );
        }
        for entry in context.entries() {
          if let FactoryContextReference::Artifact(artifact) = entry.source() {
            for selected in revisions[&origin.version].materials.entries() {
              if matches!(&selected.reference,
                IssueMaterialReference::IssueDocument { artifact: source, .. }
                | IssueMaterialReference::ProjectDocument { artifact: source, .. } if source == artifact)
              {
                retain(&mut materials, selected.reference.clone());
              }
            }
          }
        }
      }
      let reads = snapshot
        .flow
        .data
        .input_reads(
          &snapshot.work,
          &snapshot.admitted_flow,
          snapshot.flow.runtime(),
          input,
          &path,
        )
        .map_err(|_| invalid())?;
      for read in reads {
        match read {
          FlowInputRead::Caller { digest, path } => {
            let source = snapshot
              .flow
              .data
              .inputs
              .iter()
              .find(|source| source.digest().ok() == Some(digest))
              .ok_or_else(invalid)?;
            pending.push((source, path, false));
          }
          FlowInputRead::Record { reference, view, path } => {
            let source = snapshot
              .flow
              .data
              .records
              .iter()
              .find(|source| source.digest().ok() == Some(reference.digest()))
              .ok_or_else(invalid)?;
            match view {
              FlowRecordView::Payload => pending.push((
                returned_input(&snapshot.flow.data.records, source)?,
                String::new(),
                true,
              )),
              FlowRecordView::InputPayload => pending.push((source.input(), path, false)),
              FlowRecordView::Metadata | FlowRecordView::PrimaryArtifact => {}
            }
          }
          FlowInputRead::Incoming { digest, path } => {
            let incoming = snapshot
              .flow
              .data
              .incoming
              .iter()
              .find(|source| source.digest().ok() == Some(digest))
              .ok_or_else(invalid)?;
            let Some(header) = incoming.issue_snapshot() else {
              continue;
            };
            if header.project_id != project || header.issue_id != baseline.issue_id {
              return Err(invalid());
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = revisions.entry(header.version) {
              entry.insert(
                self
                  .issues
                  .issue(project, header.issue_id, Some(header.version))
                  .await?,
              );
            }
            if material_contract.is_none() {
              let configuration = self
                .action
                .runs
                .factory_admission_context(ReadFactoryAdmissionContext {
                  project_id: project,
                  configuration_id: snapshot.work.configuration().id(),
                  configuration_version: snapshot.work.configuration().version(),
                  repository_id: None,
                  repository_version: None,
                })
                .await?;
              material_contract = Some(
                configuration
                  .configuration
                  .configuration
                  .flow()
                  .issue_snapshot
                  .clone()
                  .ok_or_else(invalid)?,
              );
            }
            let contract = material_contract.as_ref().ok_or_else(invalid)?;
            for (alias, selection) in &contract.materials {
              if !incoming
                .payload()
                .value()
                .as_object()
                .is_some_and(|values| values.contains_key(alias.as_str()))
              {
                continue;
              }
              let prefix = format!("/{alias}");
              let entries =
                crate::issues::issue_execution_snapshot_materials::selections(&revisions[&header.version], selection)
                  .map_err(|_| invalid())?;
              for (index, entry) in entries.into_iter().enumerate() {
                if path.is_empty() || path == prefix || path == format!("{prefix}/{index}") {
                  retain(&mut materials, entry.reference);
                }
              }
            }
          }
        }
      }
    }
    Ok(materials)
  }
}
// A call forwards the child's payload verbatim. Its own input is not the producing analysis;
// follow exact retained returns, including nested calls, before walking content dependencies.
fn returned_input<'a>(
  records: &'a [FlowNodeRecord],
  mut result: &'a FlowNodeRecord,
) -> Result<&'a FlowNodeInput, ApplicationError> {
  for _ in 0..=records.len() {
    let Some(child) = result.subflow_source() else {
      return Ok(result.input());
    };
    result = records
      .iter()
      .find(|record| record.digest().ok() == Some(child.digest()))
      .ok_or_else(ApplicationError::invalid)?;
  }
  Err(ApplicationError::invalid())
}
fn retain(materials: &mut Vec<IssueMaterialReference>, reference: IssueMaterialReference) {
  if !materials.contains(&reference) {
    materials.push(reference);
  }
}
