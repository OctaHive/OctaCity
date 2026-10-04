use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Serialize, de::DeserializeOwned};

pub(crate) fn decode_canonical<T>(encoded: &str, maximum_bytes: usize) -> Result<T, ()>
where
  T: DeserializeOwned + Serialize,
{
  if encoded.is_empty() || encoded.len() > maximum_bytes {
    return Err(());
  }
  let decoded = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| ())?;
  if decoded.len() > maximum_bytes || URL_SAFE_NO_PAD.encode(&decoded) != encoded {
    return Err(());
  }
  let value = serde_json::from_slice(&decoded).map_err(|_| ())?;
  if encode(&value) != encoded {
    return Err(());
  }
  Ok(value)
}

pub(crate) fn encode<T: Serialize>(value: &T) -> String {
  URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).expect("cursor wire values are infallibly serializable"))
}
