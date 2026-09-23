//! Read-only Git implementation of the provider-neutral VCS process protocol.
//!
//! The adapter invokes one operator-selected Git executable without a shell,
//! clones only into an ephemeral bare object database, and inspects objects
//! with plumbing commands. It never creates a working tree, evaluates an
//! Octafile, loads repository plugins, follows repository symlinks, or runs
//! repository hooks.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod git;
mod protocol_io;

pub use config::{GitAdapterConfig, GitAdapterConfigError};
pub use git::GitVcsAdapter;
pub use protocol_io::serve;
