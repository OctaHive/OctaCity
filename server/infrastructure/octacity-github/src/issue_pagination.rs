//! Provider continuations carry page numbers, never network authority from Link or caller data.
use octacity_work_source_protocol::{FailureClass, source::ScanIntent};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  page: u32,
  context: String,
}

pub(super) fn current(intent: &ScanIntent) -> Result<(u32, String), FailureClass> {
  let mut context = serde_json::to_vec(&(
    &intent.scan_id,
    &intent.configuration_digest,
    &intent.resource.resource_id,
    &intent.resource.resource_locator,
    &intent.selection,
    &intent.window,
    intent.max_items,
  ))
  .map_err(|_| FailureClass::InvalidRequest)?;
  if !intent.additional_selections.is_empty() {
    context.extend(serde_json::to_vec(&intent.additional_selections).map_err(|_| FailureClass::InvalidRequest)?);
  }
  let context = hex::encode(Sha256::digest(context));
  let page = match &intent.cursor {
    None => 1,
    Some(cursor) => {
      let cursor: Cursor = serde_json::from_str(cursor).map_err(|_| FailureClass::InvalidRequest)?;
      if cursor.page < 2 || cursor.context != context {
        return Err(FailureClass::InvalidRequest);
      }
      cursor.page
    }
  };
  Ok((page, context))
}
pub(super) fn next(
  headers: &http::HeaderMap,
  requested: &url::Url,
  page: u32,
  context: String,
) -> Result<Option<String>, FailureClass> {
  let mut next = None;
  let mut bytes = 0_usize;
  for header in headers.get_all("link") {
    bytes = bytes.saturating_add(header.as_bytes().len());
    if bytes > 8192 {
      return Err(FailureClass::ProtocolFault);
    }
    let header = header.to_str().map_err(|_| FailureClass::ProtocolFault)?;
    for link in header.split(',') {
      let (href, parameters) = link.trim().split_once('>').ok_or(FailureClass::ProtocolFault)?;
      let href = href.strip_prefix('<').ok_or(FailureClass::ProtocolFault)?;
      let mut relation = None;
      for parameter in parameters.split(';').filter(|part| !part.trim().is_empty()) {
        let (name, value) = parameter.trim().split_once('=').ok_or(FailureClass::ProtocolFault)?;
        if name.trim() == "rel" {
          if relation.is_some() {
            return Err(FailureClass::ProtocolFault);
          }
          relation = Some(value.trim().trim_matches('"'));
        }
      }
      let relation = relation.ok_or(FailureClass::ProtocolFault)?;
      if !relation.split_whitespace().any(|rel| rel == "next") {
        continue;
      }
      if next.is_some() {
        return Err(FailureClass::ProtocolFault);
      }
      let target = url::Url::parse(href).map_err(|_| FailureClass::ProtocolFault)?;
      let next_page = page.checked_add(1).ok_or(FailureClass::ProtocolFault)?;
      let mut expected = query(requested)?;
      expected.insert("page".into(), next_page.to_string());
      if target.origin() != requested.origin()
        || target.path() != requested.path()
        || !target.username().is_empty()
        || target.password().is_some()
        || target.fragment().is_some()
        || query(&target)? != expected
      {
        return Err(FailureClass::ProtocolFault);
      }
      next = Some(
        serde_json::to_string(&Cursor {
          page: next_page,
          context: context.clone(),
        })
        .map_err(|_| FailureClass::ProtocolFault)?,
      );
    }
  }
  Ok(next)
}
fn query(url: &url::Url) -> Result<BTreeMap<String, String>, FailureClass> {
  let mut values = BTreeMap::new();
  for (key, value) in url.query_pairs() {
    if values.insert(key.into_owned(), value.into_owned()).is_some() {
      return Err(FailureClass::ProtocolFault);
    }
  }
  Ok(values)
}
