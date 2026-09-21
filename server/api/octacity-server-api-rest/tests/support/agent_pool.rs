pub fn create_body() -> &'static str {
  r#"{"name":"linux","definition":{"enabled":true,"drain_state":"accepting","admission_policy":{"mode":"any"},"concurrency_limit":1,"fairness_policy":"priority_fifo","static_capacity_limit":4}}"#
}

pub fn publish_body() -> &'static str {
  r#"{"definition":{"enabled":true,"drain_state":"graceful_drain","admission_policy":{"mode":"allowlist","platforms":[{"operating_system":"linux","architecture":"amd64"}]},"concurrency_limit":2,"fairness_policy":"priority_fifo","static_capacity_limit":4}}"#
}
