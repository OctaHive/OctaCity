//! Shared deterministic fixtures for adapter contract modules.

use std::{
  fmt::Debug,
  future::Future,
  pin::pin,
  str::FromStr,
  task::{Context, Poll, Waker},
};

use octacity_server_domain::Timestamp;

pub(crate) fn id<T>(value: u64) -> T
where
  T: FromStr,
  T::Err: Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}

pub(crate) fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}

pub(crate) fn run_ready(future: impl Future<Output = ()>, failure: &str) {
  let mut future = pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  assert!(
    matches!(Future::poll(future.as_mut(), &mut context), Poll::Ready(())),
    "{failure}"
  );
}
