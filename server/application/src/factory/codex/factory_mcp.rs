//! Common transport-shape validation, shared by Runtime authoring and compiler adapters.
use octacity_server_factory::{FactoryError, FactoryMcpServer};
/// Returns a verified HTTP endpoint, or None for a structured stdio command. No I/O.
pub(crate) fn validate_transport(server: &FactoryMcpServer) -> Result<Option<url::Url>, FactoryError> {
  let invalid = || FactoryError::InvalidConfiguration { field: "MCP transport" };
  match server {
    FactoryMcpServer::Http {
      url,
      bearer_token_env_var,
      ..
    } => {
      let url = url::Url::parse(url).map_err(|_| invalid())?;
      if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || (bearer_token_env_var.is_some() && url.scheme() != "https")
      {
        return Err(invalid());
      }
      Ok(Some(url))
    }
    FactoryMcpServer::Stdio { command, args, .. } => {
      // Preserve the existing finite command contract; qualification separately resolves exact installed paths.
      if !command.starts_with('/')
        || command.contains('\0')
        || args.len() > octacity_server_factory::MAX_COMMAND_ARGUMENTS
        || args
          .iter()
          .any(|arg| arg.len() > octacity_server_factory::MAX_FACTORY_MCP_ARGUMENT_BYTES || arg.contains('\0'))
      {
        return Err(invalid());
      }
      Ok(None)
    }
  }
}
