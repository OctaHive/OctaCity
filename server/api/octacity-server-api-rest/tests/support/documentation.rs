use axum::{
  body::Body,
  http::{Request, request::Builder},
};

#[derive(Debug)]
pub struct DocumentedHttpRequest {
  pub method: String,
  pub target: String,
  headers: Vec<(String, String)>,
  body: String,
}

impl DocumentedHttpRequest {
  pub fn to_request(&self) -> Request<Body> {
    let request = self.headers.iter().fold(
      Request::builder().method(self.method.as_str()).uri(&self.target),
      |request, (name, value)| header(request, name, value),
    );
    request.body(Body::from(self.body.clone())).unwrap()
  }

  pub fn json_body(&self) -> serde_json::Value {
    serde_json::from_str(&self.body).expect("documented request body must be valid JSON")
  }
}

pub fn documented_http_requests(document: &str) -> Vec<DocumentedHttpRequest> {
  let document = document.replace("\r\n", "\n");
  document
    .split("```http\n")
    .skip(1)
    .map(|remainder| remainder.split_once("\n```").expect("HTTP example fence must close").0)
    .map(|block| {
      let (head, body) = block
        .split_once("\n\n")
        .expect("HTTP example must separate headers and body");
      let mut lines = head.lines();
      let request_line = lines.next().expect("HTTP example must have a request line");
      let mut request_line = request_line.split_whitespace();
      let method = request_line.next().expect("HTTP method is required").to_owned();
      let target = request_line.next().expect("HTTP target is required").to_owned();
      assert_eq!(request_line.next(), Some("HTTP/1.1"));
      assert_eq!(request_line.next(), None);
      let headers = lines
        .map(|line| {
          let (name, value) = line.split_once(':').expect("HTTP header must contain ':'");
          (name.trim().to_owned(), value.trim().to_owned())
        })
        .collect();
      DocumentedHttpRequest {
        method,
        target,
        headers,
        body: body.trim_end().to_owned(),
      }
    })
    .collect()
}

fn header(request: Builder, name: &str, value: &str) -> Builder {
  if name.eq_ignore_ascii_case("host") || name.eq_ignore_ascii_case("content-length") {
    request
  } else {
    request.header(name, value)
  }
}
