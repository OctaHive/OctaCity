//! Authorized current Agent metadata; no Lease or capacity reservation APIs.
use super::*;
/// Existing execution-context and management readers used by normal placement.
pub struct FactoryReadinessPlacement {
  pub(super) contexts: Arc<dyn BuildExecutionContextSource>,
  agents: Arc<dyn AuthorizedManagementQueryHandler<ListAgentsQuery, Error = ApplicationError>>,
  details: Arc<dyn AuthorizedManagementQueryHandler<GetAgentQuery, Error = ApplicationError>>,
  pools: Arc<dyn AuthorizedManagementQueryHandler<GetAgentPoolQuery, Error = ApplicationError>>,
}
pub(super) struct PlacementObservation<'a> {
  pub(super) context: &'a ManagementRequestContext,
  pub(super) policy: &'a dyn ManagementAuthorizationPolicy,
  pub(super) target: &'a FactoryConfiguration,
  pub(super) repository: &'a WorkSourceRepositoryRef,
  pub(super) now: Timestamp,
  pub(super) end: Timestamp,
  pub(super) maximum_age: Duration,
  pub(super) choices: Option<&'a FactoryConfigurationChoices>,
}
impl FactoryReadinessPlacement {
  /// Installs ordinary authorized readers, never providers or execution writers.
  pub fn new<A, P>(contexts: Arc<dyn BuildExecutionContextSource>, agents: Arc<A>, pools: Arc<P>) -> Self
  where
    A: AuthorizedManagementQueryHandler<ListAgentsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetAgentQuery, Error = ApplicationError>
      + 'static,
    P: AuthorizedManagementQueryHandler<GetAgentPoolQuery, Error = ApplicationError> + 'static,
  {
    Self {
      contexts,
      agents: agents.clone(),
      details: agents,
      pools,
    }
  }
  pub(super) async fn observe(&self, observation: PlacementObservation<'_>) -> FactoryReadinessCheck {
    let PlacementObservation {
      context,
      policy,
      target,
      repository,
      now,
      end,
      maximum_age,
      choices,
    } = observation;
    use FactoryReadinessState::*;
    let result = |state, reason, expiry| check("agent", state, reason, true, now, expiry);
    let Ok(page) = self
      .agents
      .handle_authorized_query(
        context,
        ListAgentsQuery {
          pool_id: None,
          after: None,
          limit: octacity_server_store::MAX_AGENT_PAGE_SIZE,
        },
      )
      .await
    else {
      return result(Unknown, "inventory_unavailable", end);
    };
    if page.agents.is_empty() {
      return result(
        if page.next_cursor.is_none() { Blocked } else { Unknown },
        "capacity_absent",
        end,
      );
    }
    let mut remaining = factory_configuration_document_bounds().choice_entries;
    let mut agents = Vec::new();
    let mut incomplete = page.next_cursor.is_some();
    for summary in page
      .agents
      .iter()
      .take(usize::from(octacity_server_store::MAX_AGENT_PAGE_SIZE))
    {
      if !take(&mut remaining) {
        return result(Unknown, "observation_limit", end);
      }
      match self
        .details
        .handle_authorized_query(context, GetAgentQuery { agent_id: summary.id })
        .await
      {
        Ok(agent) if agent.id == summary.id => agents.push(ObservedAgent::new(agent, now, maximum_age)),
        _ => incomplete = true,
      }
    }
    let mut pools = std::collections::BTreeMap::new();
    let mut expiry = end;
    let mut queue = false;
    for node in target
      .flow()
      .closure
      .definitions()
      .iter()
      .flat_map(|definition| definition.nodes())
      .filter(|node| node.is_required())
    {
      for binding in node.build_bindings() {
        // Requested tool authority needs the installed plugin's exact control contract.
        // Inventory capability names cannot supply missing mode/response-limit policy.
        if node.permissions().tools().next().is_some() {
          return result(Unknown, "tool_control_unobserved", end);
        }
        let pin = &binding.build_configuration;
        if crate::factory_configuration_choices::require_visible_reference(
          policy,
          context,
          ManagementResourceKind::BuildConfiguration,
          pin.id().to_string(),
        )
        .await
        .is_err()
        {
          return result(Unknown, "execution_context_unavailable", end);
        }
        if !take(&mut remaining) {
          return result(Unknown, "observation_limit", end);
        }
        let Ok(execution) = self
          .contexts
          .load(target.reference().project_id(), pin.id(), pin.version())
          .await
        else {
          return result(Unknown, "execution_context_unavailable", end);
        };
        let Some(contract) = choices.and_then(|choices| choices.build_pipeline_contract(binding)) else {
          return result(Unknown, "pipeline_contract_unobserved", end);
        };
        if !contract.matches_nodes(execution.pipeline.dag.nodes().iter().map(|node| node.id())) {
          return result(Blocked, "pipeline_incompatible", end);
        }
        let definition = &execution.configuration.definition;
        let Some(execution_repository) = execution.repository.as_ref() else {
          return result(Blocked, "repository_binding_unavailable", end);
        };
        if execution.configuration.id != pin.id()
          || execution.configuration.version != pin.version()
          || execution.configuration.project_id != pin.project_id()
          || execution_repository.id != repository.id
          || execution_repository.project_id != repository.project_id
          || execution_repository.version != repository.version
          || Some(execution_repository.id) != definition.repository_id
          || Some(execution_repository.version) != definition.repository_version
          || execution.pipeline.id != definition.pipeline_id
          || execution.pipeline.version != definition.pipeline_version
          || execution.pipeline.project_id != pin.project_id()
          || !definition.enabled
          || crate::build_configuration_policy::validate_configuration_policy(&execution).is_err()
        {
          return result(Blocked, "execution_policy_incompatible", end);
        }
        if crate::factory_configuration_choices::require_visible_reference(
          policy,
          context,
          ManagementResourceKind::Pipeline,
          execution.pipeline.id.to_string(),
        )
        .await
        .is_err()
        {
          return result(Unknown, "execution_context_unavailable", end);
        }
        // The materializer selects the exact task plugin from the ordinary pinned toolchain.
        if execution
          .job_spec_toolchain
          .octa
          .plugin_digests
          .get(binding.plugin.identity().as_str())
          != Some(&binding.plugin.digest().to_string())
        {
          return result(Blocked, "plugin_incompatible", end);
        }
        let mut toolchain = execution.job_spec_toolchain.clone();
        toolchain
          .octa
          .plugin_digests
          .retain(|name, _| name == binding.plugin.identity().as_str());
        let Ok(job_policy) = crate::build_configuration_job_policy::configuration_definition_job_policy(
          definition,
          &toolchain,
          definition.secrets_profile.clone(),
        ) else {
          return result(Blocked, "execution_policy_incompatible", end);
        };
        for job in execution.pipeline.dag.nodes() {
          let requirements = crate::build_configuration_job_policy::configuration_job_requirements(definition, job);
          let required = octacity_server_scheduler::PlacementRequirements {
            job: &requirements,
            policy: job_policy.placement_policy(octacity_protocol::EXECUTION_CONTRACT_V3),
            factory_enforcement: Some(&octacity_protocol::FactoryEnforcementCapabilityV3::ALL),
            workspace_anchor: true,
            tool_control: None,
          };
          let mut matched = false;
          let mut unobserved = incomplete;
          for observed in &agents {
            let agent = &observed.details;
            if !agent.enabled
              || agent.status != AgentStatusProjection::Online
              || !definition.allowed_pools.contains(&agent.pool_id)
              || !execution.effective_policy.policy.pools.contains(&agent.pool_id)
            {
              continue;
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = pools.entry(agent.pool_id) {
              if !take(&mut remaining) {
                return result(Unknown, "observation_limit", end);
              }
              let pool = self
                .pools
                .handle_authorized_query(
                  context,
                  GetAgentPoolQuery {
                    pool_id: agent.pool_id,
                    version: None,
                  },
                )
                .await
                .ok();
              entry.insert(pool);
            }
            let Some(pool) = pools.get(&agent.pool_id).and_then(Option::as_ref) else {
              unobserved = true;
              continue;
            };
            if pool.id != agent.pool_id
              || !pool.enabled
              || pool.drain_state != AgentPoolDrainStateProjection::Accepting
              || pool.version != agent.pool_version
            {
              continue;
            }
            let Some((snapshot, deadline)) = &observed.availability else {
              unobserved = true;
              continue;
            };
            let inventory = &observed.inventory;
            let Some(contract) =
              octacity_protocol::SUPPORTED_EXECUTION_CONTRACTS.negotiate(inventory.execution_contract)
            else {
              continue;
            };
            if !take(&mut remaining) {
              return result(Unknown, "observation_limit", end);
            }
            let available = octacity_server_scheduler::matches_placement(inventory, snapshot, &required, contract);
            // An active Lease may consume otherwise compatible physical capacity.
            // This separately observed ceiling describes queueing, never free capacity.
            let pending = if !available && agent.current_execution.is_some() {
              if !take(&mut remaining) {
                return result(Unknown, "observation_limit", end);
              }
              let capacity = octacity_protocol::HostSnapshot {
                available_cpu_millis: u64::from(agent.capacity.logical_cpu_count) * 1000,
                available_memory_bytes: agent.capacity.total_memory_bytes,
                work_disk_free_bytes: agent.capacity.work_disk_total_bytes,
                state_disk_free_bytes: agent.capacity.state_disk_total_bytes,
                ..snapshot.clone()
              };
              octacity_server_scheduler::matches_placement(inventory, &capacity, &required, contract)
            } else {
              false
            };
            if available || pending {
              matched = true;
              expiry = expiry.min(*deadline);
              queue |= agent.current_execution.is_some();
              break;
            }
          }
          if !matched {
            return result(
              if unobserved { Unknown } else { Blocked },
              if unobserved { "stale" } else { "capacity_incompatible" },
              end,
            );
          }
        }
        // Metadata reads do not retain authority across a concurrent visibility change.
        if crate::factory_configuration_choices::require_visible_reference(
          policy,
          context,
          ManagementResourceKind::BuildConfiguration,
          pin.id().to_string(),
        )
        .await
        .is_err()
        {
          return result(Unknown, "execution_context_unavailable", end);
        }
      }
    }
    result(Ready, if queue { "queue_pending" } else { "compatible" }, expiry)
  }
}
// A request-local observation; rebuilding policy requirements does not reread Agent facts.
struct ObservedAgent {
  details: AgentProjection,
  inventory: octacity_protocol::AgentInventory,
  availability: Option<(octacity_protocol::HostSnapshot, Timestamp)>,
}
impl ObservedAgent {
  fn new(details: AgentProjection, now: Timestamp, maximum_age: Duration) -> Self {
    let availability = (|| {
      let resources = details.resources.as_ref()?;
      let deadline = fresh_until(resources.observed_at.min(details.last_seen_at), maximum_age)?
        .min(resources.registration_expires_at?);
      if resources.observed_at > now || details.last_seen_at > now || deadline <= now {
        return None;
      }
      Some((
        octacity_protocol::HostSnapshot {
          available_cpu_millis: resources.available_cpu_millis,
          available_memory_bytes: resources.available_memory_bytes,
          work_disk_free_bytes: resources.work_disk_free_bytes,
          state_disk_free_bytes: resources.state_disk_free_bytes,
          active_job: None,
          backends: resources.backends.clone()?,
        },
        deadline,
      ))
    })();
    Self {
      inventory: inventory(&details),
      details,
      availability,
    }
  }
}
fn fresh_until(observed: Timestamp, maximum_age: Duration) -> Option<Timestamp> {
  Timestamp::from_unix_millis(
    observed
      .unix_millis()
      .checked_add(i64::try_from(maximum_age.as_millis()).ok()?)?,
  )
  .ok()
}
fn inventory(agent: &AgentProjection) -> octacity_protocol::AgentInventory {
  let value = &agent.inventory;
  let capacity = &agent.capacity;
  octacity_protocol::AgentInventory {
    agent_id: agent.id.to_string(),
    agent_version: value.agent_version.clone(),
    coordinator_protocols: value.coordinator_protocols.clone(),
    execution_contract: value.execution_contract,
    labels: value.labels.clone(),
    host_platform: value.host_platform,
    host_details: value.host_details.clone(),
    host_capacity: octacity_protocol::HostCapacity {
      logical_cpu_count: capacity.logical_cpu_count,
      total_memory_bytes: capacity.total_memory_bytes,
      work_disk_total_bytes: capacity.work_disk_total_bytes,
      state_disk_total_bytes: capacity.state_disk_total_bytes,
      virtualization_available: capacity.virtualization_available,
    },
    runtimes: value.runtimes.clone(),
    executions: value.executions.clone(),
    factory_executions: value.factory_executions.clone(),
    octa: value.octa.clone(),
    source_plugins: value.source_plugins.clone(),
    cache: value.cache.clone(),
  }
}

fn take(remaining: &mut usize) -> bool {
  if *remaining == 0 {
    false
  } else {
    *remaining -= 1;
    true
  }
}
