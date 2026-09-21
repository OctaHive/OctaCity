use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  net::TcpStream,
};

#[derive(Debug)]
pub struct DocumentedHttpRequest {
  pub method: String,
  pub target: String,
  headers: Vec<(String, String)>,
  body: String,
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

pub async fn send_documented_request(address: std::net::SocketAddr, request: &DocumentedHttpRequest) -> u16 {
  let mut stream = TcpStream::connect(address).await.unwrap();
  let mut encoded = format!("{} {} HTTP/1.1\r\nHost: {address}\r\n", request.method, request.target);
  for (name, value) in &request.headers {
    if !name.eq_ignore_ascii_case("host") && !name.eq_ignore_ascii_case("content-length") {
      encoded.push_str(&format!("{name}: {value}\r\n"));
    }
  }
  encoded.push_str(&format!(
    "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
    request.body.len(),
    request.body
  ));
  stream.write_all(encoded.as_bytes()).await.unwrap();

  let mut response = Vec::new();
  stream.read_to_end(&mut response).await.unwrap();
  let response = String::from_utf8(response).unwrap();
  response
    .lines()
    .next()
    .expect("HTTP response must have a status line")
    .split_whitespace()
    .nth(1)
    .expect("HTTP response status is required")
    .parse()
    .unwrap()
}
