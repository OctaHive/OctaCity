//! Loads optional operator values at the executable boundary, never from Jobs.

use std::{collections::BTreeMap, fmt};

use octacity_config::AgentConfig;
use octacity_job::{FactoryCredentialProfile, JobError};
use octacity_protocol::MAX_FACTORY_PERMISSION_ENTRIES;
use serde::{
  Deserialize, Deserializer,
  de::{MapAccess, Visitor},
};
use zeroize::Zeroizing;

// Independent encoded-file ceiling allows JSON escaping of the Runner's
// bounded private value set without accepting an unbounded installation file.
const MAX_PROFILE_FILE_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn load(config: &AgentConfig) -> Result<Vec<FactoryCredentialProfile>, JobError> {
  config
    .factory_credential_profiles
    .iter()
    .map(|(identity, path)| {
      let parent = path.parent().ok_or_else(invalid)?;
      octacity_private_fs::validate_trusted_directory_chain(parent).map_err(|_| invalid())?;
      octacity_private_fs::validate_private_access(parent).map_err(|_| invalid())?;
      octacity_private_fs::validate_trusted_owner(path).map_err(|_| invalid())?;
      octacity_private_fs::validate_private_access(path).map_err(|_| invalid())?;
      let bytes = Zeroizing::new(
        octacity_private_fs::read_bounded_regular_file(path, MAX_PROFILE_FILE_BYTES).map_err(|_| invalid())?,
      );
      let values: PrivateValues = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
      let values = values
        .0
        .into_iter()
        .map(|(key, mut value)| (key, std::mem::take(&mut *value)))
        .collect();
      FactoryCredentialProfile::new(identity.clone(), values)
    })
    .collect()
}

fn invalid() -> JobError {
  JobError::Invalid("Factory credential installation is invalid".to_owned())
}

// Already decoded values, including a partially decoded map on error, are
// erased on drop. Neither serde errors nor file paths reach public diagnostics.
struct PrivateValue(Zeroizing<String>);
impl<'de> Deserialize<'de> for PrivateValue {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
  }
}

struct PrivateValues(BTreeMap<String, Zeroizing<String>>);
impl<'de> Deserialize<'de> for PrivateValues {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    struct ValuesVisitor;
    impl<'de> Visitor<'de> for ValuesVisitor {
      type Value = PrivateValues;
      fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded private named string map")
      }
      fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut values = BTreeMap::new();
        while let Some((key, PrivateValue(value))) = map.next_entry::<String, PrivateValue>()? {
          if values.len() >= MAX_FACTORY_PERMISSION_ENTRIES || values.contains_key(&key) {
            return Err(serde::de::Error::custom("private map exceeds its bound"));
          }
          values.insert(key, value);
        }
        Ok(PrivateValues(values))
      }
    }
    deserializer.deserialize_map(ValuesVisitor)
  }
}
