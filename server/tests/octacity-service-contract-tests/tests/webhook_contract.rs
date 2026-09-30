//! Cross-layer contracts for the public webhook ingress.

#[test]
fn public_body_limit_matches_the_provider_process_contract() {
  assert_eq!(
    octacity_server_api_webhook::MAX_WEBHOOK_DELIVERY_BYTES,
    octacity_webhook_provider_protocol::MAX_DELIVERY_BYTES
  );
}
