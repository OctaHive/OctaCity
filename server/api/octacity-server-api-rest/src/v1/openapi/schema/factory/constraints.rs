//! Safe read-only policy explanations use the same finite Flow and budget contracts.
use super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  let scope = || json!({"enum":["run_total","flow_run_total","node_execution"]});
  let source = || json!({"enum":["flow_admission","flow_definition","node","configuration","role_profile"]});
  schemas.insert(
    "FactoryBudgetConstraintSource".into(),
    object(
      [
        ("source", source()),
        ("scope", scope()),
        ("limit", schema_ref("FactoryBudget")),
      ],
      &["source", "scope", "limit"],
    ),
  );
  schemas.insert(
    "FactoryBudgetConstraint".into(),
    object(
      [
        ("scope", scope()),
        ("definition", nullable(schema_ref("FactoryFlowDefinitionReference"))),
        ("node", nullable(key())),
        ("profile", nullable(schema_ref("FactoryRoleProfileReference"))),
        ("requested", schema_ref("FactoryBudget")),
        ("effective", schema_ref("FactoryBudget")),
        ("sources", bounded_array(schema_ref("FactoryBudgetConstraintSource"), 5)),
        ("retry_charging", json!({"const":"cumulative_in_run"})),
      ],
      &[
        "scope",
        "definition",
        "node",
        "profile",
        "requested",
        "effective",
        "sources",
      ],
    ),
  );
  schemas.insert(
    "FactoryConcurrencyConstraint".into(),
    object(
      [
        (
          "scope",
          json!({"enum":octacity_server_application::FactoryConcurrencyScope::ALL}),
        ),
        ("definition", nullable(schema_ref("FactoryFlowDefinitionReference"))),
        (
          "source",
          json!({"enum":["flow_admission","flow_definition","configuration_wip"]}),
        ),
        ("requested", nullable(positive_integer())),
        ("effective", nullable(positive_integer())),
        ("unresolved", nullable(json!({"const":"configuration_required"}))),
      ],
      &["scope", "definition", "source", "requested", "effective", "unresolved"],
    ),
  );
  schemas.insert(
    "FactoryEffectiveConstraints".into(),
    object(
      [
        ("run_budget", schema_ref("FactoryBudgetConstraint")),
        (
          "node_budgets",
          bounded_array(
            schema_ref("FactoryBudgetConstraint"),
            bounds.definitions * bounds.nodes * bounds.role_entries,
          ),
        ),
        (
          "concurrency",
          bounded_array(schema_ref("FactoryConcurrencyConstraint"), bounds.definitions + 3),
        ),
      ],
      &["run_budget", "node_budgets", "concurrency"],
    ),
  );
}
