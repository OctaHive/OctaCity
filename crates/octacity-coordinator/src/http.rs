//! Reqwest adapter for the versioned outbound coordinator protocol.

use std::{fs, io::Read as _, path::PathBuf, time::Duration};

use async_trait::async_trait;
use octacity_protocol::{
  AcquireLeaseRequest, AcquireLeaseResponse, AgentInventory, AppendEventsRequest, AppendEventsResponse,
  AttemptEventEnvelope, COORDINATOR_PROTOCOL_VERSION, CompleteLeaseRequest, CompleteLeaseResponse,
  CoordinatorErrorResponse, HeartbeatDirective, HeartbeatRequest, HeartbeatResponse, HostCapacity, HostSnapshot,
  LeaseAssignment, RegisterAgentRequest, RegisterAgentResponse,
};
use reqwest::{StatusCode, Url, header};
use serde::{Serialize, de::DeserializeOwned};
use tokio::time::{Instant, sleep, timeout_at};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{CoordinatorClient, CoordinatorError, Registration, RetryPolicy, invalid, unix_now};

const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;
const IDEMPOTENCY_KEY: &str = "idempotency-key";

/// Explicit transport, credential, memory, timeout, and retry configuration.
#[derive(Clone, Debug)]
pub struct HttpCoordinatorConfig {
  /// HTTPS coordinator origin, or loopback HTTP for local integration tests.
  pub server_url: String,
  /// Permissions-restricted bearer enrollment credential file.
  pub credential_file: PathBuf,
  /// Timeout for non-long-poll coordinator requests.
  pub request_timeout: Duration,
  /// Maximum serialized bytes in one request or response body.
  pub max_body_bytes: usize,
  /// Retry policy for protocol-defined idempotent calls.
  pub retry: RetryPolicy,
}

/// Bounded HTTPS implementation of [`CoordinatorClient`].
pub struct HttpCoordinatorClient {
  base_url: Url,
  authorization: header::HeaderValue,
  client: reqwest::Client,
  request_timeout: Duration,
  max_body_bytes: usize,
  retry: RetryPolicy,
}

struct PostCall<'a> {
  operation: &'static str,
  path: &'a [&'a str],
  request_id: &'a str,
  operation_timeout: Duration,
  server_max_retry: Duration,
  cancellation: CancellationToken,
}

impl HttpCoordinatorClient {
  /// Validates configuration, reads the credential once, and builds the TLS client.
  pub fn new(config: HttpCoordinatorConfig) -> Result<Self, CoordinatorError> {
    if config.request_timeout.is_zero() || config.max_body_bytes == 0 {
      return Err(invalid("request timeout and body limit must be greater than zero"));
    }
    config.retry.validate()?;
    let base_url =
      Url::parse(&config.server_url).map_err(|error| invalid(format!("server_url is invalid: {error}")))?;
    let loopback_http = base_url.scheme() == "http" && base_url.host_str().is_some_and(is_loopback_host);
    if base_url.scheme() != "https" && !loopback_http {
      return Err(invalid(
        "server_url must use HTTPS, except for loopback integration testing",
      ));
    }
    if base_url.cannot_be_a_base()
      || base_url.username() != ""
      || base_url.password().is_some()
      || base_url.query().is_some()
      || base_url.fragment().is_some()
      || base_url.path() != "/"
    {
      return Err(invalid(
        "server_url must be an origin without credentials, path, query, or fragment",
      ));
    }
    let credential = read_credential(&config.credential_file)?;
    let mut authorization = header::HeaderValue::from_str(&format!("Bearer {credential}"))
      .map_err(|_| invalid("credential cannot be represented as an HTTP bearer token"))?;
    authorization.set_sensitive(true);
    let client = reqwest::Client::builder()
      .connect_timeout(config.request_timeout)
      .user_agent(concat!("octacity-agent/", env!("CARGO_PKG_VERSION")))
      .build()
      .map_err(|source| CoordinatorError::Transport {
        operation: "build HTTP client",
        source: Box::new(source),
      })?;
    Ok(Self {
      base_url,
      authorization,
      client,
      request_timeout: config.request_timeout,
      max_body_bytes: config.max_body_bytes,
      retry: config.retry,
    })
  }

  async fn post<TRequest, TResponse>(&self, call: PostCall<'_>, body: &TRequest) -> Result<TResponse, CoordinatorError>
  where
    TRequest: Serialize + Sync,
    TResponse: DeserializeOwned,
  {
    let PostCall {
      operation,
      path,
      request_id,
      operation_timeout,
      server_max_retry,
      cancellation,
    } = call;
    let url = self.endpoint(path)?;
    let encoded = serde_json::to_vec(body).map_err(|source| CoordinatorError::Encode {
      operation,
      source: Box::new(source),
    })?;
    if encoded.len() > self.max_body_bytes {
      return Err(CoordinatorError::RequestTooLarge {
        operation,
        maximum: self.max_body_bytes,
      });
    }
    let idempotency = header::HeaderValue::from_str(request_id)
      .map_err(|_| invalid("request_id cannot be represented as an idempotency header"))?;
    let mut last = None;
    for attempt in 1..=self.retry.max_attempts {
      debug!(operation, request_id, attempt, "sending coordinator request");
      let deadline = Instant::now()
        .checked_add(operation_timeout)
        .ok_or_else(|| invalid("coordinator operation timeout is too large"))?;
      let result = tokio::select! {
        () = cancellation.cancelled() => return Err(CoordinatorError::Cancelled),
        result = timeout_at(deadline, self.client
          .post(url.clone())
          .header(header::AUTHORIZATION, self.authorization.clone())
          .header(header::CONTENT_TYPE, "application/json")
          .header(IDEMPOTENCY_KEY, idempotency.clone())
          .body(encoded.clone())
          .send()) => result,
      };
      let response = match result {
        Err(_) => {
          last = Some(CoordinatorError::TimedOut { operation });
          None
        }
        Ok(Err(source)) => {
          last = Some(CoordinatorError::Transport {
            operation,
            source: Box::new(source),
          });
          None
        }
        Ok(Ok(response)) => Some(response),
      };

      let mut server_suggestion = None;
      if let Some(mut response) = response {
        let status = response.status();
        let header_delay = retry_after(response.headers());
        match read_bounded(operation, &mut response, self.max_body_bytes, deadline, &cancellation).await {
          Ok(bytes) if status.is_success() => {
            return serde_json::from_slice(&bytes).map_err(|source| CoordinatorError::Json {
              operation,
              source: Box::new(source),
            });
          }
          Ok(bytes) => {
            let error: CoordinatorErrorResponse =
              serde_json::from_slice(&bytes).map_err(|source| CoordinatorError::Json {
                operation,
                source: Box::new(source),
              })?;
            error.validate(request_id)?;
            let retryable = error.retryable && retryable_status(status);
            server_suggestion = error.retry_after_ms.map(Duration::from_millis).or(header_delay);
            last = Some(CoordinatorError::Rejected {
              operation,
              status: status.as_u16(),
              code: error.code,
              message: error.message,
              retryable,
            });
            if !retryable {
              return Err(last.expect("rejection was recorded"));
            }
          }
          Err(error @ (CoordinatorError::TimedOut { .. } | CoordinatorError::Transport { .. })) => {
            last = Some(error);
          }
          Err(error) => return Err(error),
        }
      }

      if attempt == self.retry.max_attempts {
        break;
      }
      let mut delay = self.retry.delay(request_id, attempt, server_max_retry);
      if let Some(suggested) = server_suggestion {
        delay = delay.max(suggested.min(server_max_retry).min(self.retry.max_delay));
      }
      warn!(
        operation,
        request_id,
        attempt,
        ?delay,
        "retrying idempotent coordinator request"
      );
      tokio::select! {
        () = cancellation.cancelled() => return Err(CoordinatorError::Cancelled),
        () = sleep(delay) => {}
      }
    }
    Err(CoordinatorError::RetriesExhausted {
      operation,
      attempts: self.retry.max_attempts,
      last: Box::new(last.unwrap_or_else(|| invalid("retry loop completed without an error"))),
    })
  }

  fn endpoint(&self, path: &[&str]) -> Result<Url, CoordinatorError> {
    let mut url = self.base_url.clone();
    let mut segments = url
      .path_segments_mut()
      .map_err(|_| invalid("server_url cannot contain path segments"))?;
    segments.pop_if_empty();
    for segment in path {
      segments.push(segment);
    }
    drop(segments);
    Ok(url)
  }
}

#[async_trait]
impl CoordinatorClient for HttpCoordinatorClient {
  async fn register(
    &self,
    inventory: &AgentInventory,
    cancellation: CancellationToken,
  ) -> Result<Registration, CoordinatorError> {
    inventory.validate()?;
    let request_id = request_id();
    let request = RegisterAgentRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.clone(),
      inventory: inventory.clone(),
    };
    request.validate()?;
    let response: RegisterAgentResponse = self
      .post(
        PostCall {
          operation: "register agent",
          path: &["api", "v1", "agents", "register"],
          request_id: &request_id,
          operation_timeout: self.request_timeout,
          server_max_retry: self.retry.max_delay,
          cancellation,
        },
        &request,
      )
      .await?;
    response.validate(&request_id)?;
    Ok(Registration {
      agent_id: inventory.agent_id.clone(),
      registration_id: response.registration_id,
      max_retry_delay: Duration::from_millis(response.max_retry_delay_ms).min(self.retry.max_delay),
    })
  }

  async fn acquire_lease(
    &self,
    registration: &Registration,
    wait: Duration,
    lease_safety_margin: Duration,
    cancellation: CancellationToken,
  ) -> Result<AcquireLeaseResponse, CoordinatorError> {
    registration.validate()?;
    if wait.is_zero() || wait.as_secs() == 0 {
      return Err(invalid("lease poll wait must be at least one second"));
    }
    let request_id = request_id();
    let request = AcquireLeaseRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.clone(),
      registration_id: registration.registration_id.clone(),
      wait_seconds: wait.as_secs(),
    };
    request.validate()?;
    let operation_timeout = wait
      .checked_add(self.request_timeout)
      .ok_or_else(|| invalid("lease poll timeout is too large"))?;
    let response: AcquireLeaseResponse = self
      .post(
        PostCall {
          operation: "acquire lease",
          path: &["api", "v1", "agents", &registration.agent_id, "leases:acquire"],
          request_id: &request_id,
          operation_timeout,
          server_max_retry: registration.max_retry_delay,
          cancellation,
        },
        &request,
      )
      .await?;
    response.validate(&request_id, unix_now()?, lease_safety_margin.as_secs())?;
    Ok(response)
  }

  async fn heartbeat(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    snapshot: &HostSnapshot,
    capacity: &HostCapacity,
    lease_safety_margin: Duration,
    cancellation: CancellationToken,
  ) -> Result<HeartbeatDirective, CoordinatorError> {
    registration.validate()?;
    snapshot.validate(capacity)?;
    let request_id = request_id();
    let request = HeartbeatRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.clone(),
      registration_id: registration.registration_id.clone(),
      lease: lease.into(),
      snapshot: snapshot.clone(),
    };
    request.validate(capacity)?;
    let response: HeartbeatResponse = self
      .post(
        PostCall {
          operation: "heartbeat lease",
          path: &["api", "v1", "leases", &lease.lease_id, "heartbeat"],
          request_id: &request_id,
          operation_timeout: self.request_timeout,
          server_max_retry: registration.max_retry_delay,
          cancellation,
        },
        &request,
      )
      .await?;
    response.validate(&request_id, unix_now()?, lease_safety_margin.as_secs())?;
    Ok(response.directive)
  }

  async fn append_events(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    events: &[AttemptEventEnvelope],
    cancellation: CancellationToken,
  ) -> Result<AppendEventsResponse, CoordinatorError> {
    registration.validate()?;
    let request_id = request_id();
    let request = AppendEventsRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.clone(),
      registration_id: registration.registration_id.clone(),
      lease: lease.into(),
      events: events.to_vec(),
    };
    request.validate()?;
    let first = request
      .events
      .first()
      .expect("validated non-empty batch")
      .stream_sequence;
    let last = request
      .events
      .last()
      .expect("validated non-empty batch")
      .stream_sequence;
    let response: AppendEventsResponse = self
      .post(
        PostCall {
          operation: "append job events",
          path: &["api", "v1", "leases", &lease.lease_id, "events:append"],
          request_id: &request_id,
          operation_timeout: self.request_timeout,
          server_max_retry: registration.max_retry_delay,
          cancellation,
        },
        &request,
      )
      .await?;
    response.validate(&request_id, first, last)?;
    Ok(response)
  }

  async fn complete_lease(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    completion: &CompleteLeaseRequest,
    cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    registration.validate()?;
    completion.validate()?;
    if completion.registration_id != registration.registration_id || completion.lease != lease.into() {
      return Err(invalid("completion does not match its registration and lease"));
    }
    let response: CompleteLeaseResponse = self
      .post(
        PostCall {
          operation: "complete lease",
          path: &["api", "v1", "leases", &lease.lease_id, "complete"],
          request_id: &completion.request_id,
          operation_timeout: self.request_timeout,
          server_max_retry: registration.max_retry_delay,
          cancellation,
        },
        completion,
      )
      .await?;
    response.validate(&completion.request_id, &completion.completion_id)?;
    Ok(())
  }
}

async fn read_bounded(
  operation: &'static str,
  response: &mut reqwest::Response,
  maximum: usize,
  deadline: Instant,
  cancellation: &CancellationToken,
) -> Result<Vec<u8>, CoordinatorError> {
  if response.content_length().is_some_and(|length| length > maximum as u64) {
    return Err(CoordinatorError::ResponseTooLarge { operation, maximum });
  }
  let mut bytes = Vec::new();
  loop {
    let chunk = tokio::select! {
      () = cancellation.cancelled() => return Err(CoordinatorError::Cancelled),
      result = timeout_at(deadline, response.chunk()) => match result {
        Err(_) => return Err(CoordinatorError::TimedOut { operation }),
        Ok(result) => result.map_err(|source| CoordinatorError::Transport {
          operation,
          source: Box::new(source),
        })?,
      },
    };
    let Some(chunk) = chunk else {
      return Ok(bytes);
    };
    if bytes.len().saturating_add(chunk.len()) > maximum {
      return Err(CoordinatorError::ResponseTooLarge { operation, maximum });
    }
    bytes.extend_from_slice(&chunk);
  }
}

fn read_credential(path: &std::path::Path) -> Result<String, CoordinatorError> {
  let file = fs::File::open(path).map_err(|source| CoordinatorError::Credential {
    path: path.to_owned(),
    source,
  })?;
  let mut credential = String::new();
  file
    .take(MAX_CREDENTIAL_BYTES + 1)
    .read_to_string(&mut credential)
    .map_err(|source| CoordinatorError::Credential {
      path: path.to_owned(),
      source,
    })?;
  if credential.len() as u64 > MAX_CREDENTIAL_BYTES {
    return Err(invalid(format!(
      "credential file exceeds the {MAX_CREDENTIAL_BYTES}-byte limit"
    )));
  }
  let credential = credential.trim_end_matches(['\r', '\n']);
  if credential.is_empty() || credential.chars().any(char::is_whitespace) {
    return Err(invalid("credential must not be empty or contain whitespace"));
  }
  Ok(credential.to_owned())
}

fn request_id() -> String {
  Uuid::new_v4().simple().to_string()
}

fn retryable_status(status: StatusCode) -> bool {
  matches!(
    status,
    StatusCode::REQUEST_TIMEOUT
      | StatusCode::TOO_EARLY
      | StatusCode::TOO_MANY_REQUESTS
      | StatusCode::INTERNAL_SERVER_ERROR
      | StatusCode::BAD_GATEWAY
      | StatusCode::SERVICE_UNAVAILABLE
      | StatusCode::GATEWAY_TIMEOUT
  )
}

fn retry_after(headers: &header::HeaderMap) -> Option<Duration> {
  headers
    .get(header::RETRY_AFTER)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.parse::<u64>().ok())
    .map(Duration::from_secs)
}

fn is_loopback_host(host: &str) -> bool {
  matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}
