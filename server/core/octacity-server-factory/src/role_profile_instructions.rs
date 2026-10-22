//! Ordered instruction content, preserving the original wire identity of retained text.
use crate::FactoryError;
use serde::{Deserialize, Serialize};

/// Ordered instruction entries. Legacy text remains one entry and serializes as text,
/// so restoring a retained v1 definition never rewrites its content digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FactoryInstructions(InstructionContent);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
enum InstructionContent {
  Text(String),
  Entries(Vec<String>),
}
impl FactoryInstructions {
  /// Constructs an explicit ordered set; text is kept exactly as supplied.
  pub fn new(entries: Vec<String>) -> Result<Self, FactoryError> {
    let value = Self(InstructionContent::Entries(entries));
    value.validate()?;
    Ok(value)
  }
  /// Borrows ordered entries, including one entry for a retained text definition.
  #[must_use]
  pub fn entries(&self) -> &[String] {
    match &self.0 {
      InstructionContent::Text(text) => std::slice::from_ref(text),
      InstructionContent::Entries(entries) => entries,
    }
  }
  pub(super) fn validate(&self) -> Result<(), FactoryError> {
    if self.entries().len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || self.entries().iter().map(String::len).sum::<usize>() > crate::MAX_FACTORY_ROLE_INSTRUCTION_BYTES
    {
      return Err(super::role_profile::invalid("role instruction set"));
    }
    for entry in self.entries() {
      super::role_profile::validate_instructions(entry)?;
    }
    Ok(())
  }
}
impl From<String> for FactoryInstructions {
  fn from(text: String) -> Self {
    Self(InstructionContent::Text(text))
  }
}
impl From<&str> for FactoryInstructions {
  fn from(text: &str) -> Self {
    text.to_owned().into()
  }
}
