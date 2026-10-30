//! Unpriced subscription usage stays distinct from measured zero-cost usage.
use octacity_server_factory::{BudgetLimit, BudgetResource, BudgetUsage, FactoryError};
use serde_json::json;

#[test]
fn subscription_usage_preserves_unknown_cost_and_known_subtotals_under_token_and_time_limits() {
  let subscription: BudgetUsage = serde_json::from_value(json!({
    "attempts":1,"elapsed_millis":100,"tokens":7,"output_bytes":8,
    "cost_micro_units":{"status":"unknown","known_micro_units":0}
  }))
  .unwrap();
  let priced: BudgetUsage = serde_json::from_value(json!({
    "attempts":1,"elapsed_millis":20,"tokens":2,"output_bytes":3,"cost_micro_units":5
  }))
  .unwrap();
  let combined = priced.checked_add(subscription).unwrap();
  let wire = serde_json::to_value(combined).unwrap();
  assert_eq!(
    wire["cost_micro_units"],
    json!({"status":"unknown","known_micro_units":5})
  );
  assert_eq!(wire["tokens"], 9);
  assert_eq!(wire["elapsed_millis"], 120);
  assert_eq!(serde_json::from_value::<BudgetUsage>(wire).unwrap(), combined);
  assert_eq!(combined.checked_sub(priced), Some(subscription));
  assert_eq!(serde_json::to_value(priced).unwrap()["cost_micro_units"], 5);
  assert!(combined.validate(BudgetLimit::new(2, 120, 9, 5, 11).unwrap()).is_ok());
  for (limit, resource) in [
    (BudgetLimit::new(2, 120, 8, 5, 11).unwrap(), BudgetResource::Tokens),
    (BudgetLimit::new(2, 119, 9, 5, 11).unwrap(), BudgetResource::ElapsedTime),
    (BudgetLimit::new(2, 120, 9, 4, 11).unwrap(), BudgetResource::Cost),
  ] {
    assert_eq!(combined.validate(limit), Err(FactoryError::BudgetExceeded { resource }));
  }
}
