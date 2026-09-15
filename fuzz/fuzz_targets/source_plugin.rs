#![no_main]

use libfuzzer_sys::fuzz_target;
use octacity_source_plugin::{
  MAX_SOURCE_FRAME_BYTES, SourceCommand, SourceMessage, SourcePluginManifest, decode_frame,
};

// Source plugins accept both JSONL process frames and an operator-owned TOML
// manifest, so arbitrary input is presented to all three public decoders.
fuzz_target!(|data: &[u8]| {
  let _ = decode_frame::<SourceCommand>(data);
  let _ = decode_frame::<SourceMessage>(data);
  if data.len() < MAX_SOURCE_FRAME_BYTES {
    let mut framed = Vec::with_capacity(data.len() + 1);
    framed.extend_from_slice(data);
    framed.push(b'\n');
    let _ = decode_frame::<SourceCommand>(&framed);
    let _ = decode_frame::<SourceMessage>(&framed);
  }
  if let Ok(text) = std::str::from_utf8(data) {
    let _ = SourcePluginManifest::from_toml(text);
  }
});
