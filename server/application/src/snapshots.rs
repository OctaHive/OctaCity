use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EffectiveProjectPolicy, JobSpecToolchainPolicy, ManualSourceSelection};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuildInputSnapshot {
  pub(crate) parameters: BTreeMap<String, Value>,
  pub(crate) source: ManualSourceSelection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuildPolicySnapshot {
  pub(crate) project: EffectiveProjectPolicy,
  pub(crate) job_spec_toolchain: JobSpecToolchainPolicy,
}
