//! Read-only explanation of existing validated Flow ceilings; never a policy or reservation owner.
use octacity_server_factory::*;

/// One retained ceiling and the scope in which its existing owner charges usage.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryBudgetConstraintSource {
  /// Technical owner, independent of logical stage names.
  pub source: &'static str,
  /// A run total, nested Flow total or individual node execution.
  pub scope: &'static str,
  /// Exact configured ceiling, without silent normalization.
  pub limit: BudgetLimit,
}
/// Requested allowance and its effective authoring bound, with the enclosing owners retained.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryBudgetConstraint {
  /// Charging scope; shared totals are not renewed by retries.
  pub scope: &'static str,
  /// Exact prospective or retained definition for a node allowance.
  pub definition: Option<FlowDefinitionRef>,
  /// Arbitrary node key; absent for the admitted root total.
  pub node: Option<FactoryKey>,
  /// Exact declared candidate; absent for static nodes and the admitted root total.
  pub profile: Option<FactoryRoleProfileRef>,
  /// Operator's published request, unchanged.
  pub requested: BudgetLimit,
  /// Allowance after the already validated enclosing policy.
  pub effective: BudgetLimit,
  /// Ordered node/definition/root ceilings, preserving each owner's scope.
  pub sources: Vec<FactoryBudgetConstraintSource>,
  /// Retries retain cumulative root/Flow charges instead of resetting consumption.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub retry_charging: Option<&'static str>,
}
/// Distinct scheduling scopes; an unknown Configuration is not inferred from a Flow.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryConcurrencyScope {
  /// Active nodes in one admitted root Run.
  RunNodes,
  /// Active nodes in one nested Flow execution.
  FlowNodes,
  /// Admitted root Runs sharing a Configuration's capacity.
  ConfigurationRuns,
  /// Active nodes sharing that Configuration's capacity.
  ConfigurationNodes,
}
impl FactoryConcurrencyScope {
  /// Closed public scope vocabulary shared with transport schemas.
  pub const ALL: [Self; 4] = [
    Self::RunNodes,
    Self::FlowNodes,
    Self::ConfigurationRuns,
    Self::ConfigurationNodes,
  ];
}
use FactoryConcurrencyScope as Scope;
/// Requested and effective capacity in one explicitly named existing scheduling scope.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryConcurrencyConstraint {
  /// Configuration-wide runs/nodes, one root Run, or one nested Flow.
  pub scope: FactoryConcurrencyScope,
  /// Exact definition when the scope is a nested Flow execution.
  pub definition: Option<FlowDefinitionRef>,
  /// Owning existing policy field.
  pub source: &'static str,
  /// Operator request, absent when its owner was not supplied.
  pub requested: Option<u32>,
  /// Enforced bound for this scope; absent rather than guessed.
  pub effective: Option<u32>,
  /// Actionable prerequisite when this owner is unavailable.
  pub unresolved: Option<&'static str>,
}
/// Static authoring ceilings; live Agent/local readiness remains independently checked at dispatch.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryEffectiveConstraints {
  /// Entire admitted Run, including all nested execution and retries.
  pub run_budget: FactoryBudgetConstraint,
  /// Individual execution allowances with their shared cumulative parents.
  pub node_budgets: Vec<FactoryBudgetConstraint>,
  /// Distinct concurrency scopes, without an inferred Configuration-wide value.
  pub concurrency: Vec<FactoryConcurrencyConstraint>,
}
impl FactoryEffectiveConstraints {
  pub(super) fn include_configuration(&mut self, configuration: &FactoryConfiguration) {
    let ceiling = FactoryBudgetConstraintSource {
      source: "configuration",
      scope: "run_total",
      limit: configuration.hard_budget(),
    };
    self.run_budget.sources.push(ceiling.clone());
    for node in &mut self.node_budgets {
      node.sources.push(ceiling.clone());
    }
    for concurrency in &mut self.concurrency {
      let value = match concurrency.scope {
        Scope::ConfigurationRuns => configuration.wip_limits().max_active_runs(),
        Scope::ConfigurationNodes => configuration.wip_limits().max_active_nodes(),
        Scope::RunNodes | Scope::FlowNodes => continue,
      };
      concurrency.requested = Some(value);
      concurrency.effective = Some(value);
      concurrency.unresolved = None;
    }
  }
  pub(super) fn for_flow(flow: &FactoryFlowConfiguration) -> Self {
    let root = FactoryBudgetConstraintSource {
      source: "flow_admission",
      scope: "run_total",
      limit: flow.limits.budget(),
    };
    let mut node_budgets = vec![];
    let mut concurrency = vec![FactoryConcurrencyConstraint {
      scope: Scope::RunNodes,
      definition: None,
      source: "flow_admission",
      requested: Some(flow.limits.max_wip().into()),
      effective: Some(flow.limits.max_wip().into()),
      unresolved: None,
    }];
    for definition in flow.closure.definitions() {
      let enclosing = FactoryBudgetConstraintSource {
        source: "flow_definition",
        scope: "flow_run_total",
        limit: definition.execution().budget(),
      };
      concurrency.push(FactoryConcurrencyConstraint {
        scope: Scope::FlowNodes,
        definition: Some(definition.reference()),
        source: "flow_definition",
        requested: Some(definition.execution().max_active_nodes().into()),
        effective: Some(definition.execution().max_active_nodes().into()),
        unresolved: None,
      });
      for node in definition.nodes() {
        // FactoryFlowConfiguration construction has already rejected a request
        // above either parent. This projection never clamps or grants an allowance.
        let node_budget = FactoryBudgetConstraint {
          scope: "node_execution",
          definition: Some(definition.reference()),
          node: Some(node.key().clone()),
          profile: None,
          requested: node.budget(),
          effective: node.budget(),
          sources: vec![
            FactoryBudgetConstraintSource {
              source: "node",
              scope: "node_execution",
              limit: node.budget(),
            },
            enclosing.clone(),
            root.clone(),
          ],
          retry_charging: Some("cumulative_in_run"),
        };
        if let Some(selection) = node.profile_selection() {
          for candidate in selection.candidates.values() {
            let mut candidate_budget = node_budget.clone();
            candidate_budget.profile = Some(candidate.profile.reference().clone());
            candidate_budget.effective = candidate.profile.definition().budget;
            candidate_budget.sources.insert(
              0,
              FactoryBudgetConstraintSource {
                source: "role_profile",
                scope: "node_execution",
                limit: candidate.profile.definition().budget,
              },
            );
            node_budgets.push(candidate_budget);
          }
        } else {
          node_budgets.push(node_budget);
        }
      }
    }
    for scope in [Scope::ConfigurationRuns, Scope::ConfigurationNodes] {
      concurrency.push(FactoryConcurrencyConstraint {
        scope,
        definition: None,
        source: "configuration_wip",
        requested: None,
        effective: None,
        unresolved: Some("configuration_required"),
      });
    }
    Self {
      run_budget: FactoryBudgetConstraint {
        scope: "run_total",
        definition: None,
        node: None,
        profile: None,
        requested: root.limit,
        effective: root.limit,
        sources: vec![root],
        retry_charging: None,
      },
      node_budgets,
      concurrency,
    }
  }
}
