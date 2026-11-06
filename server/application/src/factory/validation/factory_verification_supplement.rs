//! Validates operator-selected private verification inputs before publication.
use crate::ApplicationError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_server_domain::{ImmutableRevision, RepositoryId, RepositoryVersion};
use octacity_server_factory::{
  FactoryArtifactReference, FactoryDigest, FactoryFlowConfiguration, FactoryKey, FactoryRepositoryPath,
  FactoryRepositoryRef, FlowDefinition, FlowNodeDefinition,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_FILES: usize = 64;
const MAX_OCTA_TASK_BYTES: usize = 128;
pub(crate) const MAX_BYTES: usize = 256 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationSubmission {
  #[serde(default)]
  files: Vec<UploadedFile>,
  pub(crate) repository: Option<VerificationRepositorySelection>,
  entries: Vec<VerificationEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationRepositorySelection {
  pub(crate) id: RepositoryId,
  pub(crate) version: RepositoryVersion,
  pub(crate) revision: ImmutableRevision,
  pub(crate) paths: Vec<FactoryRepositoryPath>,
}

pub(crate) struct ResolvedVerificationRepository {
  pub(crate) reference: FactoryRepositoryRef,
  pub(crate) files: BTreeMap<FactoryRepositoryPath, Vec<u8>>,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FrozenRepository {
  repository: FactoryRepositoryRef,
  revision: ImmutableRevision,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadedFile {
  path: FactoryRepositoryPath,
  content_base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum VerificationEntry {
  Shell {
    path: FactoryRepositoryPath,
  },
  Octafile {
    path: FactoryRepositoryPath,
    #[serde(deserialize_with = "deserialize_octa_task")]
    task: String,
  },
}

fn deserialize_octa_task<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
  let task = String::deserialize(deserializer)?;
  // Octa joins case-sensitive names with colons. Reject wildcard, parent and root selectors.
  if task.len() <= MAX_OCTA_TASK_BYTES
    && !task.chars().any(|character| character == '*' || character.is_control())
    && task.split(':').all(|part| !part.is_empty())
  {
    return Ok(task);
  }
  Err(serde::de::Error::custom("invalid Octa task name"))
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
/// Frozen private files, their exact digests, and the explicitly selected entrypoints.
pub struct VerificationSupplement {
  files: Vec<VerificationFile>,
  entries: Vec<VerificationEntry>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  repository: Option<FrozenRepository>,
}
impl std::fmt::Debug for VerificationSupplement {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("VerificationSupplement").finish_non_exhaustive()
  }
}
pub(super) fn supplement_matches(
  reference: Option<&FactoryArtifactReference>,
  supplement: Option<&VerificationSupplement>,
) -> bool {
  match (reference, supplement) {
    (None, None) => true,
    (Some(reference), Some(supplement)) => serde_json::to_vec(supplement).is_ok_and(|bytes| {
      supplement.has_closed_octafiles()
        && reference.encoded_size() == bytes.len() as u64
        && reference.content_digest() == FactoryDigest::content_sha256(&bytes)
    }),
    _ => false,
  }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct VerificationFile {
  path: FactoryRepositoryPath,
  content_base64: String,
  sha256: FactoryDigest,
}

fn portable(path: &FactoryRepositoryPath) -> bool {
  path.as_str().len() <= 512
    && path.as_str().split('/').all(|part| {
      part
        .as_bytes()
        .first()
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        && part
          .bytes()
          .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
    })
}

impl VerificationSupplement {
  // Octa otherwise resolves includes and discovers monorepo definitions from
  // the live filesystem. Bound the entire expansion to exact frozen files.
  fn has_closed_octafiles(&self) -> bool {
    let mut expansions = BTreeMap::new();
    self.entries.iter().all(|entry| match entry {
      VerificationEntry::Shell { .. } => true,
      VerificationEntry::Octafile { path, .. } => self.octafile_expansion(path, &mut expansions).is_some(),
    })
  }

  fn octafile_expansion(
    &self,
    path: &FactoryRepositoryPath,
    expansions: &mut BTreeMap<FactoryRepositoryPath, Option<usize>>,
  ) -> Option<usize> {
    use serde_yaml_ng::Value;
    if let Some(size) = expansions.get(path) {
      return *size;
    }
    // An unfinished entry is a cycle. Completed counts can be reused by any
    // task, while repeated includes still count toward each root's expansion.
    expansions.insert(path.clone(), None);
    let file = self.files.iter().find(|file| &file.path == path)?;
    let bytes = STANDARD.decode(&file.content_base64).ok()?;
    let document: Value = serde_yaml_ng::from_slice(&bytes).ok()?;
    let root = document.as_mapping()?;
    if root.contains_key("monorepo") || root.contains_key("<<") {
      return None;
    }
    let mut size = 1;
    if let Some(includes) = root.get("includes") {
      let parent = path.as_str().rsplit_once('/').map(|(parent, _)| parent);
      for include in includes.as_mapping()?.values() {
        let relative = include.as_str().or_else(|| include.get("octafile")?.as_str())?;
        let relative = FactoryRepositoryPath::new(relative).ok()?;
        if !portable(&relative) {
          return None;
        }
        let child = match parent {
          Some(parent) => FactoryRepositoryPath::new(format!("{parent}/{}", relative.as_str())).ok()?,
          None => relative,
        };
        size += self.octafile_expansion(&child, expansions)?;
        if size > MAX_FILES {
          return None;
        }
      }
    }
    expansions.insert(path.clone(), Some(size));
    Some(size)
  }
}

impl VerificationSubmission {
  pub(crate) fn validate_paths(&self) -> Result<(), ApplicationError> {
    let invalid = ApplicationError::invalid;
    let paths = match &self.repository {
      Some(repository) if self.files.is_empty() => repository.paths.iter().collect::<Vec<_>>(),
      None => self.files.iter().map(|file| &file.path).collect(),
      Some(_) => return Err(invalid()),
    };
    if paths.is_empty() || paths.len() > MAX_FILES || self.entries.is_empty() || self.entries.len() > MAX_FILES {
      return Err(invalid());
    }
    let exact = paths.iter().copied().collect::<BTreeSet<_>>();
    let portable_names = paths
      .iter()
      .map(|path| path.as_str().to_ascii_lowercase())
      .collect::<BTreeSet<_>>();
    if portable_names.len() != paths.len()
      || paths.iter().any(|path| !portable(path))
      || portable_names.iter().any(|path| {
        path
          .match_indices('/')
          .any(|(i, _)| portable_names.contains(&path[..i]))
      })
    {
      return Err(invalid());
    }
    let mut seen = BTreeSet::new();
    for entry in &self.entries {
      let path = match entry {
        VerificationEntry::Shell { path } if path.as_str().ends_with(".sh") => path,
        VerificationEntry::Octafile { path, .. } => path,
        _ => return Err(invalid()),
      };
      if !exact.contains(path) || !seen.insert(entry) {
        return Err(invalid());
      }
    }
    Ok(())
  }

  pub(crate) fn freeze(
    self,
    repository: Option<ResolvedVerificationRepository>,
  ) -> Result<VerificationSupplement, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let (files, repository) = match (self.repository, repository) {
      (None, None) => {
        let files = self
          .files
          .into_iter()
          .map(|file| {
            let bytes = STANDARD.decode(&file.content_base64).map_err(|_| invalid())?;
            if STANDARD.encode(&bytes) != file.content_base64 {
              return Err(invalid());
            }
            Ok((file.path, bytes))
          })
          .collect::<Result<BTreeMap<_, _>, ApplicationError>>()?;
        (files, None)
      }
      (Some(selection), Some(resolved)) => (
        resolved.files,
        Some(FrozenRepository {
          repository: resolved.reference,
          revision: selection.revision,
        }),
      ),
      _ => return Err(invalid()),
    };
    if files.values().map(Vec::len).sum::<usize>() > MAX_BYTES {
      return Err(invalid());
    }
    for entry in &self.entries {
      let (VerificationEntry::Shell { path } | VerificationEntry::Octafile { path, .. }) = entry;
      let bytes = files.get(path).ok_or_else(invalid)?;
      if bytes.is_empty() || bytes.contains(&0) {
        return Err(invalid());
      }
    }
    let supplement = VerificationSupplement {
      files: files
        .into_iter()
        .map(|(path, bytes)| VerificationFile {
          path,
          content_base64: STANDARD.encode(&bytes),
          sha256: FactoryDigest::content_sha256(&bytes),
        })
        .collect(),
      entries: self.entries,
      repository,
    };
    if !supplement.has_closed_octafiles() {
      return Err(invalid());
    }
    Ok(supplement)
  }
}

pub(crate) fn validate_verification_capabilities(
  flow: &FactoryFlowConfiguration,
  choices: &octacity_server_factory::FactoryConfigurationChoices,
) -> Result<(), ApplicationError> {
  use octacity_server_factory::{
    EvidenceOutputKind, FactoryProtectedTestBuildContract::*, FlowInputSource, FlowRecordView, FlowTransitionTarget,
  };
  let nodes = || {
    flow
      .closure
      .definitions()
      .iter()
      .flat_map(|definition| definition.nodes().iter().map(move |node| (definition, node)))
  };
  let mut has_supplement = false;
  for (definition, node) in nodes().filter(|(_, node)| node.protected_verification().is_some()) {
    has_supplement = true;
    let invalid = || {
      crate::FactoryValidationDiagnostics::invalid_binding(
        definition.reference(),
        node.key(),
        "/protected_verification",
      )
    };
    let mut bindings = node.build_bindings().peekable();
    if bindings.peek().is_none() {
      return Err(invalid());
    }
    for binding in bindings {
      let Some(Author {
        result_output,
        inventory,
      }) = choices
        .build_pipeline_contract(binding)
        .and_then(|contract| contract.protected_tests.as_ref())
      else {
        return Err(invalid());
      };
      if binding.result_output != *result_output
        || binding.result_output_kind != EvidenceOutputKind::Report
        || !binding.evidence.contains(inventory)
      {
        return Err(invalid());
      }
    }
  }
  if has_supplement {
    for (definition, node) in nodes() {
      let (Some(context), Some(input)) = (node.context_binding(), node.input_binding()) else {
        continue;
      };
      let input_shape = node
        .input_schema()
        .and_then(|reference| flow.data_schemas.iter().find(|schema| schema.reference() == reference))
        .map(|schema| schema.shape());
      for binding in node.build_bindings() {
        let Some(Verifier {
          author_profile,
          inventory,
          accepts_supplement: false,
        }) = choices
          .build_pipeline_contract(binding)
          .and_then(|contract| contract.protected_tests.as_ref())
        else {
          continue;
        };
        let inventory_path = format!("/build/outputs/{}/artifact", inventory.kind());
        let supplemental_author = |key: &FactoryKey, outcome: &FactoryKey| {
          definition.node(key).is_some_and(|producer| {
            producer.protected_verification().is_some()
              && producer.build_bindings().any(|build| {
                &build.model_or_tool == author_profile
                  && &build.result_outcome == outcome
                  && build.evidence.contains(inventory)
              })
          })
        };
        if context.selects_required_artifact_reference(input_shape, |path| {
          input.selects_only(path, |source, tail| match source {
            FlowInputSource::Record {
              node,
              outcome,
              view: FlowRecordView::Metadata,
              path,
            } if format!("{path}{tail}") == inventory_path => supplemental_author(node, outcome),
            FlowInputSource::Predecessor {
              view: FlowRecordView::Metadata,
              path,
            } if format!("{path}{tail}") == inventory_path => definition.transitions().iter().any(|edge| {
              (matches!(edge.target(), FlowTransitionTarget::Node(key) if key == node.key())
                || edge.exhausted_node() == Some(node.key()))
                && supplemental_author(edge.predecessor(), edge.outcome())
            }),
            _ => false,
          })
        }) {
          return Err(crate::FactoryValidationDiagnostics::invalid_binding(
            definition.reference(),
            node.key(),
            "/protected_verification",
          ));
        }
      }
    }
  }
  Ok(())
}

pub(crate) fn verification_selections(
  flow: &FactoryFlowConfiguration,
) -> impl Iterator<Item = (&FlowDefinition, &FlowNodeDefinition, &FactoryArtifactReference)> {
  flow.closure.definitions().iter().flat_map(|definition| {
    definition.nodes().iter().filter_map(move |node| {
      node
        .protected_verification()
        .map(|reference| (definition, node, reference))
    })
  })
}

/// Checks pinned verification metadata before publication or execution. The
/// operator who attaches a file must have Download permission. Admission uses
/// the reviewed pin and reads only metadata from the existing private owner.
pub(crate) async fn validate_verification_selections(
  flow: &FactoryFlowConfiguration,
  project: octacity_server_domain::ProjectId,
  inputs: Option<&dyn octacity_server_store::ProjectInputQueryStore>,
  observed_at: octacity_server_domain::Timestamp,
  authorization: Option<(
    &dyn crate::ManagementAuthorizationPolicy,
    &crate::ManagementRequestContext,
  )>,
) -> Result<(), ApplicationError> {
  use crate::factory::authoring::factory_configuration_choices::require_authorized_reference;
  use std::collections::btree_map::Entry;
  let budget = flow.limits.budget().max_elapsed_millis();
  let mut observed = BTreeMap::new();
  for (definition, node, reference) in verification_selections(flow) {
    let diagnostic = |code| {
      let ApplicationError::FactoryValidation(mut report) =
        crate::FactoryValidationDiagnostics::unavailable_with_code("/protected_verification", code)
      else {
        unreachable!()
      };
      report.errors[0].definition = Some(definition.reference());
      report.errors[0].node = Some(node.key().clone());
      ApplicationError::FactoryValidation(report)
    };
    let unavailable = || diagnostic("unavailable_reference");
    let input = match observed.entry(reference.artifact_id()) {
      Entry::Occupied(entry) => entry.into_mut(),
      Entry::Vacant(entry) => {
        if let Some((policy, context)) = authorization {
          require_authorized_reference(
            policy,
            context,
            crate::ManagementAction::Download,
            crate::ManagementResourceKind::Artifact,
            reference.artifact_id(),
          )
          .await
          .map_err(|_| unavailable())?;
        }
        entry.insert(
          inputs
            .ok_or_else(unavailable)?
            .project_input_artifact(project, reference.artifact_id())
            .await?
            .ok_or_else(unavailable)?,
        )
      }
    };
    if input.project_id != project
      || input.object.artifact != *reference
      || input.mapping.as_str() != octacity_server_store::PROJECT_PROTECTED_VERIFICATION_MAPPING
      || input.published_at > observed_at
    {
      return Err(unavailable());
    }
    if input.expires_at <= observed_at
      || i64::try_from(budget)
        .ok()
        .and_then(|millis| observed_at.unix_millis().checked_add(millis))
        .is_none_or(|deadline| deadline > input.expires_at.unix_millis())
    {
      return Err(diagnostic("verification_retention_unavailable"));
    }
  }
  Ok(())
}
