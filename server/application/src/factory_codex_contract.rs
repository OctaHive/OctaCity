//! Private compatibility contract shared by Codex compilation and observation.

pub(super) const CODEX_PLUGIN_IDENTITY: &str = "codex";
pub(super) const CODEX_PLUGIN_PROVENANCE_IDENTITY: &str = "octa_plugin_codex";
pub(super) const CODEX_EXECUTABLE_IDENTITY: &str = "codex-cli";
pub(super) const CODEX_RESULT_FORMAT: &str = "octa.codex.result.v1";
pub(super) const CODEX_RUN_TRACE: &str = "codex-run-trace";
pub(super) const CODEX_RUN_PROVENANCE: &str = "codex-run-provenance";
pub(super) const CODEX_RUN_RESULT: &str = "codex-run-result";
pub(super) const CODEX_STAGE_SUMMARY: &str = "stage-summary";
pub(super) const JSON_MEDIA_TYPE: &str = "application/json";
pub(super) const TRACE_MEDIA_TYPE: &str = "application/x-ndjson";
pub(super) const MAX_CODEX_PROMPT_BYTES: usize = 1024 * 1024;
