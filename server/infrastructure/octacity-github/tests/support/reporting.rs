//! Stateful native HTTPS peer: mutations survive a deliberately lost response.
use super::{Reply, issue};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub enum LostResponse {
  Empty,
  Malformed,
  Disconnect,
  Truncated,
  OversizedDeclaration,
  OversizedBody,
  UnavailableBeforeApply,
  UnavailableAfterApply,
}
pub struct Issue {
  pub labels: BTreeSet<String>,
  pub state: String,
  pub catalog: BTreeMap<String, u64>,
  pub fault: Option<(&'static str, LostResponse)>,
  pub confirmation: Option<Reply>,
  changed: bool,
}
impl Issue {
  pub fn new(labels: &[&str]) -> Self {
    Self {
      labels: labels.iter().map(|v| (*v).into()).collect(),
      state: "open".into(),
      catalog: BTreeMap::from([("ready / review".into(), 101), ("needs context".into(), 102)]),
      fault: None,
      confirmation: None,
      changed: false,
    }
  }
  pub fn respond(&mut self, request: &str) -> Reply {
    let (method, rest) = request.split_once(' ').unwrap();
    let path = rest.split(' ').next().unwrap();
    let root = "/api/v3/repos/owner/tickets";
    if method == "GET" {
      if path == format!("{root}/labels?per_page=100") {
        let page: Vec<_> = self
          .catalog
          .iter()
          .take(100)
          .map(|(name, id)| json!({"name":name,"id":id}))
          .collect();
        return Reply::json(json!(page));
      }
      for (name, id) in &self.catalog {
        let mut url = url::Url::parse("https://localhost/").unwrap();
        url.path_segments_mut().unwrap().extend(["labels", name.as_str()]);
        if path == format!("{root}{}", url.path()) {
          return Reply::json(json!({"name":name,"id":id}));
        }
      }
      if path == format!("{root}/issues/17") {
        if self.changed
          && let Some(reply) = self.confirmation.take()
        {
          return reply;
        }
        let mut value = issue(9001, 17);
        value["state"] = json!(self.state);
        return Reply::json(value);
      }
      if path.starts_with(&format!("{root}/labels/")) {
        return Reply {
          status: 404,
          ..Reply::json(json!({"message":"private-provider-error"}))
        };
      }
      assert_eq!(path, format!("{root}/issues/17/labels?per_page=100"));
      return self.labels_reply();
    }
    if matches!(self.fault.as_ref(), Some((verb, LostResponse::UnavailableBeforeApply)) if *verb == method) {
      self.fault.take();
      return Reply {
        status: 503,
        ..Reply::json(json!({"message":"private-provider-error"}))
      };
    }
    match method {
      "DELETE" => {
        assert_eq!(path, format!("{root}/issues/17/labels/needs%20context"));
        assert!(self.labels.remove("needs context"), "never replay a completed removal");
      }
      "POST" => {
        assert_eq!(path, format!("{root}/issues/17/labels"));
        let value: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(value, json!({"labels":["ready / review"]}));
        assert!(
          self.labels.insert("ready / review".into()),
          "never replay a completed addition"
        );
      }
      "PATCH" => {
        assert_eq!(path, format!("{root}/issues/17"));
        let value: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(value, json!({"state":"closed"}));
        assert_ne!(self.state, "closed", "never replay a completed state update");
        self.state = "closed".into();
      }
      _ => panic!("unexpected native method: {method}"),
    }
    self.changed = true;
    if self.fault.as_ref().is_some_and(|(verb, _)| *verb == method) {
      return match self.fault.take().unwrap().1 {
        LostResponse::Empty => Reply {
          body: vec![],
          ..Reply::json(Value::Null)
        },
        LostResponse::Malformed => Reply {
          body: b"{\"private".to_vec(),
          ..Reply::json(Value::Null)
        },
        LostResponse::Disconnect => Reply {
          disconnect: true,
          ..Reply::json(Value::Null)
        },
        LostResponse::Truncated => Reply {
          headers: vec![("content-length".into(), "100".into())],
          body: b"{\"private".to_vec(),
          ..Reply::json(Value::Null)
        },
        LostResponse::OversizedDeclaration => Reply {
          headers: vec![(
            "content-length".into(),
            (octacity_work_source_protocol::MAX_RESPONSE_BYTES + 1).to_string(),
          )],
          body: vec![],
          ..Reply::json(Value::Null)
        },
        LostResponse::OversizedBody => Reply {
          chunked: true,
          body: vec![b'x'; octacity_work_source_protocol::MAX_RESPONSE_BYTES + 1],
          ..Reply::json(Value::Null)
        },
        LostResponse::UnavailableAfterApply => Reply {
          status: 503,
          ..Reply::json(json!({"message":"private-provider-error"}))
        },
        LostResponse::UnavailableBeforeApply => unreachable!(),
      };
    }
    self.labels_reply()
  }
  fn labels_reply(&self) -> Reply {
    Reply::json(json!(
      self
        .labels
        .iter()
        .map(|name| json!({"name":name,"id":self.catalog.get(name).copied().unwrap_or(9000)}))
        .collect::<Vec<_>>()
    ))
  }
}
