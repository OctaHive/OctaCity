use std::{
  collections::BTreeSet,
  future::Future,
  pin::Pin,
  str::FromStr as _,
  sync::Arc,
  task::{Context, Poll, Waker},
};

use octacity_server_application::{
  AgentPoolDrainStateProjection, AgentPoolHandlers, CommandHandler as _, CreateAgentPoolCommand,
  DeleteAgentPoolCommand, GetAgentPoolQuery, ListAgentPoolsQuery, PublishAgentPoolVersionCommand, QueryHandler as _,
};
use octacity_server_domain::{PoolId, PoolName, PoolVersion, Timestamp};
use octacity_server_scheduler::PoolDrainState;
use octacity_server_store::{
  AgentPlatform, AgentPoolDefinition, IdempotencyKey, PoolAdmissionPolicy, PoolFairnessPolicy,
  testing::{InMemoryAgentPoolStore, PoolReferenceKind},
};

#[test]
fn typed_agent_pool_handlers_preserve_versions_and_guard_references() {
  run_ready(async {
    let store = Arc::new(InMemoryAgentPoolStore::new());
    let handlers = AgentPoolHandlers::new(Arc::clone(&store));
    let pool_id = id(1);
    let created = handlers
      .handle_command(CreateAgentPoolCommand {
        id: pool_id,
        name: PoolName::new("linux-native").unwrap(),
        definition: definition(PoolDrainState::Accepting),
        idempotency_key: key("create-pool"),
        published_at: time(10),
      })
      .await
      .unwrap();
    assert_eq!(created.pool.drain_state, AgentPoolDrainStateProjection::Accepting);

    let published = handlers
      .handle_command(PublishAgentPoolVersionCommand {
        id: pool_id,
        expected_current_version: PoolVersion::INITIAL,
        definition: definition(PoolDrainState::GracefulDrain),
        idempotency_key: key("drain-pool"),
        published_at: time(20),
      })
      .await
      .unwrap();
    assert_eq!(published.pool.version.get(), 2);
    assert_eq!(published.pool.drain_state, AgentPoolDrainStateProjection::GracefulDrain);

    let original = handlers
      .handle_query(GetAgentPoolQuery {
        pool_id,
        version: PoolVersion::INITIAL,
      })
      .await
      .unwrap();
    assert_eq!(original.drain_state, AgentPoolDrainStateProjection::Accepting);
    let current = handlers
      .handle_query(ListAgentPoolsQuery { after: None, limit: 10 })
      .await
      .unwrap();
    assert_eq!(current.pools.as_slice(), std::slice::from_ref(&published.pool));

    store
      .seed_reference(pool_id, PoolReferenceKind::BuildConfiguration)
      .unwrap();
    assert!(
      handlers
        .handle_command(DeleteAgentPoolCommand {
          id: pool_id,
          expected_current_version: published.pool.version,
          idempotency_key: key("delete-referenced-pool"),
          deleted_at: time(30),
        })
        .await
        .is_err()
    );
  });
}

fn run_ready<F: Future>(future: F) -> F::Output {
  let mut context = Context::from_waker(Waker::noop());
  let mut future = Box::pin(future);
  match Pin::as_mut(&mut future).poll(&mut context) {
    Poll::Ready(output) => output,
    Poll::Pending => panic!("in-memory application future unexpectedly yielded"),
  }
}

fn definition(drain_state: PoolDrainState) -> AgentPoolDefinition {
  AgentPoolDefinition {
    enabled: true,
    drain_state,
    admission_policy: PoolAdmissionPolicy::Allowlist {
      platforms: BTreeSet::from([AgentPlatform::new("linux", "amd64").unwrap()]),
    },
    concurrency_limit: 2,
    fairness_policy: PoolFairnessPolicy::PriorityFifo,
    static_capacity_limit: 4,
  }
}

fn id(value: u128) -> PoolId {
  PoolId::from_str(&format!("{value:08x}-0000-4000-8000-000000000000")).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
