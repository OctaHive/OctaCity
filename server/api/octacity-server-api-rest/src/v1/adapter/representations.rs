use super::*;

pub(super) fn project_mutation(outcome: ProjectCommandOutcome) -> MutationResponse<ProjectResource> {
  MutationResponse {
    disposition: mutation_disposition(outcome.disposition),
    resource: project_resource(outcome.project),
  }
}

pub(super) fn delete_project_response(outcome: DeleteProjectCommandOutcome) -> DeleteProjectResponse {
  DeleteProjectResponse {
    disposition: mutation_disposition(outcome.disposition),
    project_id: outcome.project_id.to_string(),
  }
}

pub(super) fn project_details(projection: ProjectProjection) -> ProjectDetails {
  ProjectDetails {
    project: project_resource(projection.project),
    ancestors: projection.ancestors.into_iter().map(project_resource).collect(),
  }
}

pub(super) fn project_page(
  page: ProjectPageProjection,
  request_id: &RequestId,
) -> Result<CursorPage<ProjectResource>, ApiError> {
  Ok(CursorPage {
    items: page.projects.into_iter().map(project_resource).collect(),
    next_cursor: page
      .next_cursor
      .map(|cursor| Cursor::new(cursor.to_string()))
      .transpose()
      .map_err(|_| internal_conversion(request_id))?,
  })
}

fn project_resource(project: octacity_server_application::ProjectSummaryProjection) -> ProjectResource {
  ProjectResource {
    id: project.id.to_string(),
    parent_id: project.parent_id.map(|id| id.to_string()),
    name: project.name.to_string(),
    version: project.version.get(),
    created_at_unix_ms: project.created_at.unix_millis(),
    updated_at_unix_ms: project.updated_at.unix_millis(),
  }
}

pub(super) fn pipeline_mutation(outcome: PipelineCommandOutcome) -> MutationResponse<PipelineResource> {
  MutationResponse {
    disposition: mutation_disposition(outcome.disposition),
    resource: pipeline_resource(outcome.pipeline),
  }
}

pub(super) fn pipeline_resource(projection: PipelineProjection) -> PipelineResource {
  let nodes = projection
    .nodes
    .into_iter()
    .map(|node| PipelineNode {
      id: node.id.to_string(),
      name: node.name.to_string(),
      dependency_policy: match node.dependency_policy {
        ApplicationDependencyPolicy::AllSucceeded => DependencyPolicy::AllSucceeded,
        ApplicationDependencyPolicy::AllCompleted => DependencyPolicy::AllCompleted,
        ApplicationDependencyPolicy::AnySucceeded => DependencyPolicy::AnySucceeded,
      },
      required_capabilities: node
        .required_capabilities
        .into_iter()
        .map(|capability| capability.to_string())
        .collect(),
      execution: JobExecution {
        octafile: node.execution.octafile,
        commands: node.execution.commands,
        arguments: node.execution.arguments,
        concurrency: node.execution.concurrency,
        parallel: node.execution.parallel,
        failfast: node.execution.failfast,
      },
    })
    .collect();
  let edges = projection
    .edges
    .into_iter()
    .map(|edge| PipelineEdge {
      predecessor: edge.predecessor.to_string(),
      dependent: edge.dependent.to_string(),
    })
    .collect();
  PipelineResource {
    id: projection.id.to_string(),
    project_id: projection.project_id.to_string(),
    name: projection.name.to_string(),
    version: projection.version.get(),
    dag: PipelineDag { nodes, edges },
    published_at_unix_ms: projection.published_at.unix_millis(),
  }
}

pub(super) fn repository_mutation(outcome: RepositoryCommandOutcome) -> MutationResponse<RepositoryResource> {
  MutationResponse {
    disposition: mutation_disposition(outcome.disposition),
    resource: repository_resource(outcome.repository),
  }
}

pub(super) fn repository_resource(projection: RepositoryProjection) -> RepositoryResource {
  RepositoryResource {
    id: projection.id.to_string(),
    project_id: projection.project_id.to_string(),
    name: projection.name.to_string(),
    version: projection.version.get(),
    definition: RepositoryDefinition {
      vcs_integration_id: projection.vcs_integration_id.to_string(),
      repository_locator: projection.repository_locator.to_string(),
      selection: RepositorySelectionPolicy {
        allowed_references: projection
          .selection
          .allowed_references
          .into_iter()
          .map(|reference| reference.to_string())
          .collect(),
        default_reference: projection
          .selection
          .default_reference
          .map(|reference| reference.to_string()),
        allow_exact_revision: projection.selection.allow_exact_revision,
      },
    },
    published_at_unix_ms: projection.published_at.unix_millis(),
  }
}

pub(super) fn configuration_mutation(
  outcome: BuildConfigurationCommandOutcome,
) -> MutationResponse<BuildConfigurationResource> {
  MutationResponse {
    disposition: mutation_disposition(outcome.disposition),
    resource: configuration_resource(outcome.configuration),
  }
}

pub(super) fn configuration_resource(projection: BuildConfigurationProjection) -> BuildConfigurationResource {
  let parameters = ParameterSchema {
    parameters: projection
      .parameters
      .parameters
      .into_iter()
      .map(|(name, parameter)| {
        let value_type = match parameter.value_type {
          ApplicationParameterType::String => ParameterType::String,
          ApplicationParameterType::Integer => ParameterType::Integer,
          ApplicationParameterType::Boolean => ParameterType::Boolean,
        };
        (
          name,
          ParameterDefinition {
            value_type,
            required: parameter.required,
            default: parameter.default,
          },
        )
      })
      .collect(),
    deny_unknown: projection.parameters.deny_unknown,
  };
  let triggers = projection
    .triggers
    .into_iter()
    .map(|kind| match kind {
      ApplicationTriggerKind::Manual => TriggerKind::Manual,
      ApplicationTriggerKind::Scheduled => TriggerKind::Scheduled,
      ApplicationTriggerKind::External => TriggerKind::External,
      ApplicationTriggerKind::Internal => TriggerKind::Internal,
    })
    .collect();
  let agent_requirements = AgentRequirements {
    capabilities: projection.agent_requirements.capabilities,
    labels: projection.agent_requirements.labels,
    minimum_cpu_millis: projection.agent_requirements.minimum_cpu_millis,
    minimum_memory_bytes: projection.agent_requirements.minimum_memory_bytes,
    minimum_disk_bytes: projection.agent_requirements.minimum_disk_bytes,
  };
  let class = match projection.runtime.class {
    ApplicationRuntimeClass::Native => RuntimeClass::Native,
    ApplicationRuntimeClass::OciProcess => RuntimeClass::OciProcess,
    ApplicationRuntimeClass::OciHypervisor => RuntimeClass::OciHypervisor,
  };
  let operating_system = match projection.runtime.operating_system {
    ApplicationPlatformOs::Linux => PlatformOs::Linux,
    ApplicationPlatformOs::Windows => PlatformOs::Windows,
    ApplicationPlatformOs::Macos => PlatformOs::Macos,
  };
  let architecture = match projection.runtime.architecture {
    ApplicationPlatformArchitecture::Amd64 => PlatformArchitecture::Amd64,
    ApplicationPlatformArchitecture::Arm64 => PlatformArchitecture::Arm64,
  };
  let network = match projection.runtime.network {
    ApplicationNetworkPolicy::Disabled => NetworkPolicy::Disabled,
    ApplicationNetworkPolicy::Unrestricted => NetworkPolicy::Unrestricted,
    ApplicationNetworkPolicy::Restricted(allowed_hosts) => NetworkPolicy::Restricted { allowed_hosts },
  };
  let runtime = RuntimePolicy {
    class,
    operating_system,
    architecture,
    immutable_image: projection.runtime.immutable_image,
    cpu_millis: projection.runtime.cpu_millis,
    memory_bytes: projection.runtime.memory_bytes,
    writable_disk_bytes: projection.runtime.writable_disk_bytes,
    timeout_seconds: projection.runtime.timeout_seconds,
    network,
    workload_identity_profile: projection
      .runtime
      .workload_identity_profile
      .map(|profile| profile.to_string()),
  };
  let retry = RetryPolicy {
    max_attempts: projection.retry.max_attempts,
    retry_on: projection
      .retry
      .retry_on
      .into_iter()
      .map(|class| match class {
        ApplicationRetryClass::ExecutionFailure => RetryClass::ExecutionFailure,
        ApplicationRetryClass::InfrastructureFailure => RetryClass::InfrastructureFailure,
      })
      .collect(),
  };
  let definition = BuildConfigurationDefinition {
    enabled: projection.enabled,
    repository_id: projection.repository_id.to_string(),
    repository_version: projection.repository_version.get(),
    pipeline_id: projection.pipeline_id.to_string(),
    pipeline_version: projection.pipeline_version.get(),
    parameters,
    triggers,
    agent_requirements,
    allowed_pools: projection
      .allowed_pools
      .into_iter()
      .map(|pool| pool.to_string())
      .collect(),
    runtime,
    cache: CachePolicy {
      namespace: projection.cache.namespace.map(|namespace| namespace.to_string()),
      read: projection.cache.read,
      write: projection.cache.write,
    },
    artifacts: ArtifactPolicy {
      artifact_count: projection.artifacts.artifact_count,
      artifact_bytes: projection.artifacts.artifact_bytes,
      report_count: projection.artifacts.report_count,
      report_bytes: projection.artifacts.report_bytes,
      single_output_bytes: projection.artifacts.single_output_bytes,
    },
    retry,
  };
  BuildConfigurationResource {
    id: projection.id.to_string(),
    project_id: projection.project_id.to_string(),
    name: projection.name.to_string(),
    version: projection.version.get(),
    definition,
    published_at_unix_ms: projection.published_at.unix_millis(),
  }
}

pub(super) fn trigger_response(outcome: ManualTriggerOutcome) -> TriggerEvaluationResponse {
  match outcome {
    ManualTriggerOutcome::Accepted {
      disposition,
      trigger_occurrence_id,
      build_id,
      attempt_id,
      ready_job_ids,
    } => TriggerEvaluationResponse::Accepted {
      disposition: mutation_disposition(disposition),
      trigger_occurrence_id: trigger_occurrence_id.to_string(),
      build_id: build_id.to_string(),
      attempt_id: attempt_id.to_string(),
      ready_job_ids: ready_job_ids.into_iter().map(|id| id.to_string()).collect(),
    },
    ManualTriggerOutcome::Suppressed {
      disposition,
      trigger_occurrence_id,
    } => TriggerEvaluationResponse::Suppressed {
      disposition: mutation_disposition(disposition),
      trigger_occurrence_id: trigger_occurrence_id.to_string(),
    },
  }
}

pub(super) fn job_event_page(page: JobEventPageProjection) -> JobEventPage {
  JobEventPage {
    items: page
      .events
      .into_iter()
      .map(|event| JobEventResource {
        sequence: event.sequence,
        kind: event.kind,
        occurred_at_unix_ms: event.occurred_at_unix_ms,
        payload: event.payload,
      })
      .collect(),
    cursor: page.cursor,
  }
}

pub(super) fn pipeline_document(dag: PipelineDag) -> Value {
  json!({
    "schema_version": 1,
    "nodes": dag.nodes.into_iter().map(|node| json!({
      "id": node.id,
      "name": node.name,
      "dependency_policy": node.dependency_policy,
      "required_capabilities": node.required_capabilities,
      "template": node.execution,
    })).collect::<Vec<_>>(),
    "edges": dag.edges,
  })
}

pub(super) fn configuration_document(
  definition: BuildConfigurationDefinition,
  request_id: &RequestId,
) -> Result<Value, ApiError> {
  let mut document = encode(definition, request_id)?;
  let triggers = document
    .get_mut("triggers")
    .ok_or_else(|| internal_conversion(request_id))?;
  *triggers = json!({"allowed": std::mem::take(triggers)});
  Ok(document)
}

pub(super) fn encode<T: Serialize>(value: T, request_id: &RequestId) -> Result<Value, ApiError> {
  serde_json::to_value(value).map_err(|_| internal_conversion(request_id))
}

fn mutation_disposition(value: ApplicationMutationDisposition) -> MutationDisposition {
  match value {
    ApplicationMutationDisposition::Applied => MutationDisposition::Applied,
    ApplicationMutationDisposition::Replayed => MutationDisposition::Replayed,
  }
}

fn internal_conversion(request_id: &RequestId) -> ApiError {
  ApiError::new(
    StatusCode::INTERNAL_SERVER_ERROR,
    ErrorCode::Internal,
    "application result cannot be represented by the REST contract",
    request_id,
  )
}
