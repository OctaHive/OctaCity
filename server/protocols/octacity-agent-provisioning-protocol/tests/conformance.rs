use std::collections::{BTreeMap, BTreeSet};

use octacity_agent_provisioning_protocol::{
  AGENT_PROVISIONING_PROTOCOL_VERSION, AdapterManifest, Command, Failure, FailureClass, Outcome, ProtocolError,
  Request, Response, decode_request, decode_response,
};
use serde::Deserialize;
use serde_json::Value;

const FIXTURE_BYTES: &[u8] = include_bytes!("../fixtures/conformance-v1.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConformanceFixture {
  protocol_version: u16,
  manifest: AdapterManifest,
  exchanges: Vec<Exchange>,
  failures: Vec<Failure>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
  name: String,
  request: Value,
  response: Value,
}

fn load_fixture() -> ConformanceFixture {
  serde_json::from_slice(FIXTURE_BYTES).expect("the checked-in conformance fixture must be valid JSON")
}

fn decoded_exchanges(fixture: &ConformanceFixture) -> BTreeMap<String, (Request, Response)> {
  fixture
    .exchanges
    .iter()
    .map(|exchange| {
      let request_bytes = serde_json::to_vec(&exchange.request).unwrap();
      let request = decode_request(&request_bytes).unwrap();
      let response_bytes = serde_json::to_vec(&exchange.response).unwrap();
      let response = decode_response(&response_bytes, &request).unwrap();

      assert_eq!(serde_json::to_value(&request).unwrap(), exchange.request);
      assert_eq!(serde_json::to_value(&response).unwrap(), exchange.response);
      (exchange.name.clone(), (request, response))
    })
    .collect()
}

#[test]
fn fixture_covers_the_complete_provider_neutral_lifecycle() {
  let fixture = load_fixture();
  assert_eq!(fixture.protocol_version, AGENT_PROVISIONING_PROTOCOL_VERSION);
  fixture.manifest.validate().unwrap();
  assert_eq!(
    fixture.manifest.protocol.negotiate(fixture.manifest.protocol).unwrap(),
    AGENT_PROVISIONING_PROTOCOL_VERSION
  );

  let names = fixture
    .exchanges
    .iter()
    .map(|exchange| exchange.name.as_str())
    .collect::<BTreeSet<_>>();
  assert_eq!(
    names,
    BTreeSet::from([
      "cancel",
      "observe",
      "provision",
      "provision_replay",
      "terminate",
      "terminate_replay",
    ])
  );

  let exchanges = decoded_exchanges(&fixture);
  let (provision, provisioned) = &exchanges["provision"];
  let (provision_replay, provisioned_replay) = &exchanges["provision_replay"];
  assert_eq!(provision.command, provision_replay.command);
  assert_eq!(provisioned.outcome, provisioned_replay.outcome);

  let (terminate, terminated) = &exchanges["terminate"];
  let (terminate_replay, terminated_replay) = &exchanges["terminate_replay"];
  assert_eq!(terminate.command, terminate_replay.command);
  assert_eq!(terminated.outcome, terminated_replay.outcome);

  let Command::Provision(provision) = &provision.command else {
    panic!("provision exchange must contain a provision command");
  };
  assert!(!format!("{provision:?}").contains(&provision.bootstrap.enrollment_credential_handle));
}

#[test]
fn fixture_covers_every_classified_failure() {
  let fixture = load_fixture();
  assert_eq!(
    fixture.failures.iter().map(|failure| failure.class).collect::<Vec<_>>(),
    vec![
      FailureClass::InvalidRequest,
      FailureClass::Unsupported,
      FailureClass::Permanent,
      FailureClass::Transient,
      FailureClass::Cancelled,
      FailureClass::ProtocolFault,
    ]
  );
  let exchanges = decoded_exchanges(&fixture);
  let request = &exchanges["observe"].0;
  for failure in fixture.failures {
    failure.validate().unwrap();
    Response {
      protocol_version: request.protocol_version,
      request_id: request.request_id.clone(),
      outcome: Outcome::Failure(failure),
    }
    .validate_for(request)
    .unwrap();
  }
}

#[test]
fn responses_must_match_the_requested_operation_and_identity() {
  let fixture = load_fixture();
  let exchanges = decoded_exchanges(&fixture);
  let (provision, provision_response) = &exchanges["provision"];
  let mut wrong_pool = provision_response.clone();
  let Outcome::Machine(machine) = &mut wrong_pool.outcome else {
    panic!("provision response must contain a machine");
  };
  machine.pool_id = "another-pool".to_owned();
  assert_eq!(wrong_pool.validate_for(provision), Err(ProtocolError::OutcomeMismatch));

  let mut wrong_platform = provision_response.clone();
  let Outcome::Machine(machine) = &mut wrong_platform.outcome else {
    panic!("provision response must contain a machine");
  };
  machine.platform = Some(octacity_agent_provisioning_protocol::Platform {
    os: "windows".to_owned(),
    architecture: "x86_64".to_owned(),
  });
  assert_eq!(
    wrong_platform.validate_for(provision),
    Err(ProtocolError::OutcomeMismatch)
  );

  let observe = &exchanges["observe"].0;
  let mut wrong_machine = exchanges["observe"].1.clone();
  let Outcome::Machine(machine) = &mut wrong_machine.outcome else {
    panic!("observe response must contain a machine");
  };
  machine.machine_id = "another-machine".to_owned();
  assert_eq!(wrong_machine.validate_for(observe), Err(ProtocolError::OutcomeMismatch));

  let terminate = &exchanges["terminate"].0;
  let wrong_termination = Response {
    protocol_version: terminate.protocol_version,
    request_id: terminate.request_id.clone(),
    outcome: Outcome::Terminated {
      machine_id: "another-machine".to_owned(),
    },
  };
  assert_eq!(
    wrong_termination.validate_for(terminate),
    Err(ProtocolError::OutcomeMismatch)
  );

  let cancel = &exchanges["cancel"].0;
  let wrong_operation = Response {
    protocol_version: cancel.protocol_version,
    request_id: cancel.request_id.clone(),
    outcome: Outcome::Acknowledged {
      operation_id: "another-operation".to_owned(),
    },
  };
  assert_eq!(
    wrong_operation.validate_for(cancel),
    Err(ProtocolError::OutcomeMismatch)
  );
}
