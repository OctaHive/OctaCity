//! Work protocol negotiation/terminal framing, independent of the existing Delivery capability.
//! Issue reads and effect mappings use independently installed scopes.
use crate::GithubWorkProvider;
use octacity_work_source_protocol::{Failure, FailureClass, Request, Response, reporter, review, source};
use std::path::Path;

pub(super) fn same_cancellation(original: &Request, bytes: &[u8]) -> bool {
  let Ok(cancel) = octacity_work_source_protocol::decode_request(bytes) else {
    return false;
  };
  match (original, cancel) {
    (
      Request::Review(original),
      Request::Review(review::Request {
        command: review::Command::Cancel { target_read_id },
        ..
      }),
    ) => original
      .command
      .intent()
      .is_some_and(|intent| intent.read_id == target_read_id),
    (
      Request::Source(source::Request {
        command: source::Command::Check(intent),
        ..
      }),
      Request::Source(source::Request {
        command: source::Command::Cancel { target_scan_id },
        ..
      }),
    ) => intent.check_id == target_scan_id,

    (
      Request::Source(source::Request {
        command: source::Command::Scan(intent) | source::Command::Preview(intent),
        ..
      }),
      Request::Source(source::Request {
        command: source::Command::Cancel { target_scan_id },
        ..
      }),
    ) => intent.scan_id == target_scan_id,
    (
      Request::Source(source::Request {
        command: source::Command::Authorize(intent),
        ..
      }),
      Request::Source(source::Request {
        command: source::Command::Cancel { target_scan_id },
        ..
      }),
    ) => intent.scan_id == target_scan_id,
    (
      Request::Reporter(original),
      Request::Reporter(reporter::Request {
        command: reporter::Command::Cancel { target_projection_id },
        ..
      }),
    ) => match &original.command {
      reporter::Command::Check(intent) => intent.check_id == target_projection_id,
      command => command
        .intent()
        .is_some_and(|intent| intent.projection_id == target_projection_id),
    },
    _ => false,
  }
}

pub(super) async fn execute(
  request: &Request,
  directory: &Path,
  cancellation: tokio_util::sync::CancellationToken,
) -> Response {
  if matches!(
    request,
    Request::Source(source::Request {
      command: source::Command::Cancel { .. },
      ..
    }) | Request::Review(review::Request {
      command: review::Command::Cancel { .. },
      ..
    }) | Request::Reporter(reporter::Request {
      command: reporter::Command::Cancel { .. },
      ..
    })
  ) {
    return response(request, FailureClass::Unsupported);
  }
  if std::fs::symlink_metadata(directory.join("github-work.toml"))
    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
  {
    return response(request, FailureClass::Unsupported);
  }
  match GithubWorkProvider::load(directory) {
    Err(_) => response(request, FailureClass::Permanent),
    Ok(provider) => provider.execute(request, cancellation).await,
  }
}

pub(super) fn response(request: &Request, class: FailureClass) -> Response {
  let unavailable = || Failure {
    class,
    retry_after_millis: None,
  };
  match request {
    Request::Review(request) => Response::Review(review::Response {
      protocol_version: request.protocol_version,
      request_id: request.request_id.clone(),
      outcome: match &request.command {
        review::Command::Cancel { target_read_id } => review::Outcome::Acknowledged {
          read_id: target_read_id.clone(),
        },
        _ => review::Outcome::Failure(unavailable()),
      },
    }),
    Request::Source(request) => Response::Source(source::Response {
      protocol_version: request.protocol_version,
      request_id: request.request_id.clone(),
      outcome: match &request.command {
        source::Command::Cancel { target_scan_id } => source::Outcome::Acknowledged {
          scan_id: target_scan_id.clone(),
        },
        source::Command::Check(intent) => source::Outcome::Checked(check_failure(intent, class)),
        source::Command::Scan(_) | source::Command::Preview(_) | source::Command::Authorize(_) => {
          source::Outcome::Failure(unavailable())
        }
      },
    }),
    Request::Reporter(request) => Response::Reporter(reporter::Response {
      protocol_version: request.protocol_version,
      request_id: request.request_id.clone(),
      outcome: match &request.command {
        reporter::Command::Check(intent) => reporter::Outcome::Checked(check_failure(intent, class)),
        reporter::Command::Cancel { target_projection_id } => reporter::Outcome::Acknowledged {
          projection_id: target_projection_id.clone(),
        },
        _ => reporter::Outcome::Failure(unavailable()),
      },
    }),
  }
}

fn check_failure(
  intent: &octacity_work_source_protocol::check::CheckIntent,
  class: FailureClass,
) -> octacity_work_source_protocol::check::CheckReport {
  use octacity_work_source_protocol::check::{CheckReport, CheckState};
  let mut report = CheckReport::unverified(intent);
  report.configuration = match class {
    FailureClass::Unsupported => CheckState::Missing,
    FailureClass::Transient | FailureClass::Cancelled => CheckState::Unavailable,
    _ => CheckState::Invalid,
  };
  report
}
