//! Provider-local MCP/PreToolUse adapter. All filesystem authority stays at the Agent.
use octacity_lifecycle::{CodexMcpReadRecipe, CodexPreToolUseInput, codex_denied_output, codex_permitted_output};
use octacity_protocol::{
  FactoryToolActionProposalV3, MAX_FACTORY_TOOL_RESPONSE_BYTES, ProtectedReadChannelOperationV1,
  ProtectedReadChannelRequestV1, ScopedValidationAction, ScopedValidationChannelOperationV1,
  ScopedValidationChannelRequestV1, ScopedValidationVerdict,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
  io::{BufRead as _, Read as _, Write as _},
  path::PathBuf,
};

#[cfg(unix)]
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
  #[serde(default)]
  socket: Option<PathBuf>,
  #[serde(default)]
  port: Option<u32>,
  hook_tool_name: String,
  recipe: FactoryToolActionProposalV3,
  #[serde(default)]
  scoped_validation: bool,
}

const SCOPED_TOOL: &str = "verify_candidate";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopedArguments {
  action: ScopedValidationAction,
}
trait Stream: std::io::Read + std::io::Write {}
impl<T: std::io::Read + std::io::Write> Stream for T {}

impl Configuration {
  fn load() -> Option<Self> {
    let path = std::env::var_os("OCTACITY_CODEX_READER_CONFIG")
      .map(PathBuf::from)
      .unwrap_or_else(|| PathBuf::from("/workspace/scratch/.octacity/controlled-reader.json"));
    let bytes = octacity_private_fs::read_bounded_regular_file(&path, 64 * 1024).ok()?;
    let config: Self = serde_json::from_slice(&bytes).ok()?;
    if config.socket.is_some() == config.port.is_some()
      || !config.recipe.arguments.is_empty()
      || !config.recipe.paths.is_empty()
    {
      return None;
    }
    config.reader()?;
    Some(config)
  }
  fn reader(&self) -> Option<CodexMcpReadRecipe> {
    let r = &self.recipe;
    if !r.network_hosts.is_empty()
      || !r.secret_profiles.is_empty()
      || !r.workload_identity_profiles.is_empty()
      || r.descendants != 1
    {
      return None;
    }
    CodexMcpReadRecipe::new(
      self.hook_tool_name.clone(),
      r.tool.clone(),
      r.executable.clone(),
      r.resources,
      r.outputs.clone(),
    )
    .ok()
  }
  fn connect(&self) -> Option<Box<dyn Stream>> {
    #[cfg(unix)]
    if let Some(path) = &self.socket {
      let socket = std::os::unix::net::UnixStream::connect(path).ok()?;
      socket.set_read_timeout(Some(Duration::from_secs(60))).ok()?;
      socket.set_write_timeout(Some(Duration::from_secs(60))).ok()?;
      return Some(Box::new(socket));
    }
    #[cfg(target_os = "linux")]
    if let Some(port) = self.port {
      return vsock::connect(port).map(|socket| Box::new(socket) as Box<dyn Stream>);
    }
    None
  }
  fn exchange_read(&self, input: &CodexPreToolUseInput, operation: ProtectedReadChannelOperationV1) -> Option<Vec<u8>> {
    let proposal = input.normalize_read(&self.reader()?).ok()?.into_proposal();
    let bytes = serde_json::to_vec(&ProtectedReadChannelRequestV1 {
      schema_version: 1,
      operation,
      proposal,
    })
    .ok()?;
    let result = self.exchange_frame(&bytes)?;
    (operation != ProtectedReadChannelOperationV1::Authorize || result.is_empty()).then_some(result)
  }
  fn exchange_frame(&self, bytes: &[u8]) -> Option<Vec<u8>> {
    let mut socket = self.connect()?;
    socket
      .write_all(&(u32::try_from(bytes.len()).ok()?).to_be_bytes())
      .ok()?;
    socket.write_all(bytes).ok()?;
    socket.flush().ok()?;
    let mut header = [0; 5];
    socket.read_exact(&mut header).ok()?;
    let length = u32::from_be_bytes(header[1..].try_into().ok()?) as usize;
    if header[0] != 0 || length as u64 > MAX_FACTORY_TOOL_RESPONSE_BYTES {
      return None;
    }
    let mut bytes = vec![0; length];
    socket.read_exact(&mut bytes).ok()?;
    Some(bytes)
  }
  fn scoped_verdict(&self, arguments: &Value) -> Option<String> {
    let arguments: ScopedArguments = serde_json::from_value(arguments.clone()).ok()?;
    let request = serde_json::to_vec(&ScopedValidationChannelRequestV1 {
      schema_version: 1,
      operation: ScopedValidationChannelOperationV1::ScopedValidation,
      action: arguments.action,
    })
    .ok()?;
    let verdict: ScopedValidationVerdict = serde_json::from_slice(&self.exchange_frame(&request)?).ok()?;
    serde_json::to_string(&verdict).ok()
  }
  fn allow_hook(&self, input: &CodexPreToolUseInput) -> bool {
    if self.scoped_validation
      && let Some(server) = self.hook_tool_name.strip_suffix("__read_file")
      && let Some(arguments) = input.arguments_for_tool(&format!("{server}__{SCOPED_TOOL}"))
    {
      // The hook checks only the shape. The invocation still needs fresh
      // host authorization and no Build is launched during this check.
      return serde_json::from_value::<ScopedArguments>(arguments.clone()).is_ok();
    }
    self
      .exchange_read(input, ProtectedReadChannelOperationV1::Authorize)
      .is_some()
  }
  fn mcp(&self, request: Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request.get("method").and_then(Value::as_str);
    let result = match method {
      Some("initialize") => Some(
        json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"octacity-controlled-reader","version":env!("CARGO_PKG_VERSION")}}),
      ),
      Some("tools/list") => {
        let mut tools = vec![
          json!({"name":"read_file","description":"Read one bounded file through Agent policy.","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}),
        ];
        if self.scoped_validation {
          tools.push(json!({"name":SCOPED_TOOL,"description":"Start verification of the admitted candidate or read its verdict.","inputSchema":{"type":"object","properties":{"action":{"enum":["start","status"]}},"required":["action"],"additionalProperties":false}}));
        }
        Some(json!({"tools":tools}))
      }
      Some("tools/call") => {
        let params = request.get("params")?;
        match params.get("name").and_then(Value::as_str) {
          Some("read_file") => {
            let event = json!({"hook_event_name":"PreToolUse","tool_name":self.hook_tool_name,"tool_use_id":"mcp-read","tool_input":params.get("arguments")?});
            let input = CodexPreToolUseInput::parse(&serde_json::to_vec(&event).ok()?).ok();
            input.and_then(|input| self.exchange_read(&input, ProtectedReadChannelOperationV1::Read))
              .and_then(|bytes| String::from_utf8(bytes).ok())
          },
          Some(SCOPED_TOOL) if self.scoped_validation => self.scoped_verdict(params.get("arguments")?),
          _ => None,
        }
        .map(|text| json!({"content":[{"type":"text","text":text}],"isError":false}))
      }
      _ => None,
    };
    Some(match result {
      Some(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
      None => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Controlled read denied."}}),
    })
  }
}
fn main() {
  let arguments = std::env::args().skip(1).collect::<Vec<_>>();
  let config = Configuration::load();
  if arguments.is_empty() {
    let mut input = Vec::new();
    let allowed = std::io::stdin()
      .lock()
      .take(64 * 1024 + 1)
      .read_to_end(&mut input)
      .ok()
      .filter(|_| input.len() <= 64 * 1024)
      .and_then(|_| CodexPreToolUseInput::parse(&input).ok())
      .is_some_and(|input| config.as_ref().is_some_and(|config| config.allow_hook(&input)));
    println!(
      "{}",
      if allowed {
        codex_permitted_output()
      } else {
        codex_denied_output()
      }
    );
  } else if arguments == ["mcp"] {
    let Some(config) = config else {
      fail();
      return;
    };
    let mut input = std::io::stdin().lock();
    loop {
      let mut line = Vec::new();
      let count = std::io::Read::take(&mut input, 64 * 1024 + 1).read_until(b'\n', &mut line);
      let Ok(count) = count else {
        fail();
        return;
      };
      if count == 0 {
        break;
      }
      if count > 64 * 1024 {
        fail();
        return;
      }
      let Ok(request) = serde_json::from_slice::<Value>(&line) else {
        fail();
        return;
      };
      if let Some(response) = config.mcp(request) {
        println!("{response}");
        if std::io::stdout().flush().is_err() {
          return;
        }
      }
    }
  } else {
    fail();
  }
}
fn fail() {
  eprintln!("controlled reader adapter rejected its invocation");
  std::process::exit(64);
}

#[cfg(target_os = "linux")]
mod vsock {
  use std::{
    io,
    os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd},
    time::Duration,
  };
  pub(super) fn connect(port: u32) -> Option<std::fs::File> {
    if port == 0 || port == u32::MAX {
      return None;
    }
    // SAFETY: AF_VSOCK socket has one owned descriptor; all pointers below refer
    // to initialized ABI structures for exactly the duration of each syscall.
    let raw = unsafe {
      libc::socket(
        libc::AF_VSOCK,
        libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
        0,
      )
    };
    if raw < 0 {
      return None;
    }
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    let address = libc::sockaddr_vm {
      svm_family: libc::AF_VSOCK as _,
      svm_reserved1: 0,
      svm_port: port,
      svm_cid: 2,
      svm_zero: [0; 4],
    };
    let connected = unsafe {
      libc::connect(
        raw,
        (&address as *const libc::sockaddr_vm).cast(),
        std::mem::size_of_val(&address) as _,
      )
    };
    if connected != 0 {
      if io::Error::last_os_error().raw_os_error() != Some(libc::EINPROGRESS) {
        return None;
      }
      let mut poll = libc::pollfd {
        fd: raw,
        events: libc::POLLOUT,
        revents: 0,
      };
      if unsafe { libc::poll(&mut poll, 1, 60000) } != 1 || poll.revents != libc::POLLOUT {
        return None;
      }
      let mut error: libc::c_int = 0;
      let mut length = std::mem::size_of_val(&error) as libc::socklen_t;
      if unsafe {
        libc::getsockopt(
          raw,
          libc::SOL_SOCKET,
          libc::SO_ERROR,
          (&mut error as *mut libc::c_int).cast(),
          &mut length,
        )
      } != 0
        || error != 0
      {
        return None;
      }
    }
    if unsafe { libc::fcntl(raw, libc::F_SETFL, 0) } != 0 {
      return None;
    }
    let timeout = libc::timeval {
      tv_sec: Duration::from_secs(60).as_secs() as _,
      tv_usec: 0,
    };
    for option in [libc::SO_RCVTIMEO, libc::SO_SNDTIMEO] {
      if unsafe {
        libc::setsockopt(
          owned.as_raw_fd(),
          libc::SOL_SOCKET,
          option,
          (&timeout as *const libc::timeval).cast(),
          std::mem::size_of_val(&timeout) as _,
        )
      } != 0
      {
        return None;
      }
    }
    Some(std::fs::File::from(owned))
  }
}
