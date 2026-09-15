#![no_main]

use libfuzzer_sys::fuzz_target;

// Exercise both raw command deserialization and the exact bounded JSONL parser
// used by the runner supervisor. Appending a newline lets arbitrary JSON reach
// the decoder while the original bytes still explore malformed framing.
fuzz_target!(|data: &[u8]| {
  let _ = serde_json::from_slice::<octa_runner_protocol::RunnerCommand>(data);
  let _ = octacity_runner::decode_message_frame(data);
  if data.len() < octacity_runner::MAX_RUNNER_OUTPUT_FRAME_BYTES {
    let mut framed = Vec::with_capacity(data.len() + 1);
    framed.extend_from_slice(data);
    framed.push(b'\n');
    let _ = octacity_runner::decode_message_frame(&framed);
  }
});
