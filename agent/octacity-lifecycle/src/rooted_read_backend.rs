//! Exact installed-reader checks above retained source-directory authority.

use std::{
  future::Future,
  path::{Path, PathBuf},
  pin::Pin,
};

use octacity_job::AuthorizedFactoryToolAction;
use octacity_private_fs::RootedReadDirectory;
use tokio_util::sync::CancellationToken;

use crate::{AgentToolActionBrokerError, ProtectedReadBackend, ProtectedReadRecipe};

/// Trusted Agent inventory of the installed rooted-reader implementation.
///
/// Implementations must resolve the real reader binary and entrypoint from
/// operator/release inventory, independently of blocked action metadata. A
/// caller-supplied executable reference is not installation evidence. Missing
/// or unqualified implementations return `None`.
pub trait ProtectedReadInventory {
  /// Returns the frozen technical recipe for the installed rooted reader.
  fn installed_rooted_reader(&self) -> Option<ProtectedReadRecipe>;

  /// Resolves the actual standalone worker from the same frozen installation.
  /// Recipe metadata alone is not evidence of an installed confined process.
  fn installed_rooted_reader_program(&self) -> Option<octacity_execution::ExecutionProgram> {
    None
  }
}

/// Built-in bounded reader for one trusted Job source root.
///
/// It executes the installed read operation rather than spawning a supplied
/// executable or interpreting arguments as a command. Source-root isolation,
/// inventory qualification and the supervised helper process remain required
/// before this component can enable controlled Jobs. Blocking filesystem calls
/// must run inside that helper, whose owner enforces physical teardown.
pub struct RootedProtectedReadBackend {
  root: RootedReadDirectory,
  portable_root: PathBuf,
  recipe: ProtectedReadRecipe,
}

impl RootedProtectedReadBackend {
  /// Resolves the actual reader from trusted inventory and retains its source
  /// directory. The blocked action cannot choose implementation metadata.
  pub fn from_inventory(
    inventory: &dyn ProtectedReadInventory,
    source_root: &Path,
    portable_root: &str,
  ) -> Result<Self, AgentToolActionBrokerError> {
    let recipe = inventory
      .installed_rooted_reader()
      .ok_or(AgentToolActionBrokerError::Denied)?;
    recipe.action(portable_root)?;
    let root = RootedReadDirectory::open(source_root).map_err(|_| AgentToolActionBrokerError::Denied)?;
    Ok(Self {
      root,
      portable_root: PathBuf::from(portable_root),
      recipe,
    })
  }
}

impl ProtectedReadBackend for RootedProtectedReadBackend {
  fn read<'a>(
    &'a self,
    action: AuthorizedFactoryToolAction,
    max_bytes: u64,
    cancellation: CancellationToken,
  ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AgentToolActionBrokerError>> + Send + 'a>> {
    Box::pin(async move {
      if cancellation.is_cancelled() {
        return Err(AgentToolActionBrokerError::Cancelled);
      }
      let proposal = action.into_proposal();
      let path = proposal.paths.first().ok_or(AgentToolActionBrokerError::Mismatch)?;
      let installed_action = self
        .recipe
        .action(&path.path)
        .map_err(|_| AgentToolActionBrokerError::Mismatch)?;
      if installed_action.into_proposal() != proposal {
        return Err(AgentToolActionBrokerError::Mismatch);
      }
      let relative = Path::new(&path.path)
        .strip_prefix(&self.portable_root)
        .map_err(|_| AgentToolActionBrokerError::Mismatch)?;
      self
        .root
        .read_bounded(relative, max_bytes)
        .map_err(|_| AgentToolActionBrokerError::Denied)
    })
  }
}
