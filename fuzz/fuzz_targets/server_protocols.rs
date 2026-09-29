#![no_main]

use libfuzzer_sys::fuzz_target;
use octacity_protocol::{
  ARTIFACT_PROTOCOL_VERSION, ArtifactCommand, ArtifactProtocolRequest, CancelArtifactOperation,
  decode_artifact_request, decode_artifact_response,
};
use octacity_vcs_protocol as vcs;
use octacity_webhook_provider_protocol as webhook;

fuzz_target!(|data: &[u8]| {
  let artifact_request = ArtifactProtocolRequest {
    protocol_version: ARTIFACT_PROTOCOL_VERSION,
    request_id: "fuzz".to_owned(),
    command: ArtifactCommand::Cancel(CancelArtifactOperation {
      target_operation_id: "operation".to_owned(),
    }),
  };
  let _ = decode_artifact_request(data);
  let _ = decode_artifact_response(data, &artifact_request);

  let vcs_request = vcs::Request {
    protocol_version: vcs::VCS_PROTOCOL_VERSION,
    request_id: "fuzz".to_owned(),
    command: vcs::Command::Cancel(vcs::CancelOperation {
      target_operation_id: "operation".to_owned(),
    }),
  };
  let _ = vcs::decode_request(data);
  let _ = vcs::decode_response(data, &vcs_request);

  let webhook_request = webhook::Request {
    protocol_version: webhook::WEBHOOK_PROTOCOL_VERSION,
    request_id: "fuzz".to_owned(),
    command: webhook::Command::Cancel(webhook::CancelOperation {
      target_operation_id: "operation".to_owned(),
    }),
  };
  let _ = webhook::decode_request(data);
  let _ = webhook::decode_response(data, &webhook_request);
});
