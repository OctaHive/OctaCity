//! Reuse the adapter's hermetic HTTPS peer without replacing any server handler.
use serde_json::{Value, json};
#[path = "../../../../infrastructure/octacity-github/tests/support/https_peer.rs"]
#[allow(dead_code)]
mod https_peer;
pub(super) use https_peer::{Forge, NativeResource, Reply};
#[path = "../../../../infrastructure/octacity-github/tests/support/reporting.rs"]
#[allow(dead_code)]
pub(super) mod reporting;
pub(super) fn issue(id: u64, number: u64) -> Value {
  json!({"id":id,"number":number,"title":"Native task without execution acceptance","body":null,
    "updated_at":chrono::Utc::now().to_rfc3339(),"state":"open","labels":[]})
}
