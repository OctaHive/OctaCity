//! Bounded inline presentation metadata; complete raster decoding belongs to application publication.
use serde::{Deserialize, Serialize};

/// Maximum encoded avatar metadata, included in the ordinary role document bound.
pub const MAX_FACTORY_ROLE_AVATAR_BYTES: usize = 96 * 1024;
const PREFIX: &str = "data:image/png;base64,";

/// A private uploaded PNG snapshot, never an external URL or executable image format.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "String")]
pub struct FactoryRoleAvatar(String);
impl TryFrom<String> for FactoryRoleAvatar {
  type Error = crate::FactoryError;
  fn try_from(value: String) -> Result<Self, Self::Error> {
    let invalid = || crate::role_profile::invalid("agent avatar");
    if value.len() > MAX_FACTORY_ROLE_AVATAR_BYTES {
      return Err(invalid());
    }
    let payload = value.strip_prefix(PREFIX).ok_or_else(invalid)?;
    // Presentation metadata owns the URI/size contract. The application performs
    // complete base64 and bounded raster decoding before any publication.
    if !payload.starts_with("iVBORw0KGgo")
      || !payload
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte))
    {
      return Err(invalid());
    }
    Ok(Self(value))
  }
}
impl FactoryRoleAvatar {
  /// Borrows the validated data URI for authorized presentation.
  pub fn as_str(&self) -> &str {
    &self.0
  }
  /// Borrows the bounded encoded payload for the application raster validator.
  pub fn encoded_png(&self) -> &str {
    &self.0[PREFIX.len()..]
  }
}
impl std::fmt::Debug for FactoryRoleAvatar {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("FactoryRoleAvatar").finish_non_exhaustive()
  }
}
