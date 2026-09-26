//! Command-line entry point for the isolated Agent Ready release harness.

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use octacity_release_harness::{ReleaseBundles, install};

#[derive(Debug, Parser)]
#[command(version, about = "Install verified release bundles for an Agent Ready scenario")]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Copy verified bundle contents into one fresh isolated installation.
  Prepare {
    /// Extracted checksummed server bundle root.
    #[arg(long)]
    server_root: PathBuf,
    /// Extracted checksummed Agent bundle root.
    #[arg(long)]
    agent_root: PathBuf,
    /// Extracted checksummed Octa bundle root.
    #[arg(long)]
    octa_root: PathBuf,
    /// New directory that will own this scenario's installation.
    #[arg(long)]
    install_root: PathBuf,
  },
}

fn main() -> ExitCode {
  let Cli { command } = Cli::parse();
  let result = match command {
    Command::Prepare {
      server_root,
      agent_root,
      octa_root,
      install_root,
    } => install(
      &ReleaseBundles {
        server: server_root,
        agent: agent_root,
        octa: octa_root,
      },
      &install_root,
    ),
  };
  match result {
    Ok(installed) => match serde_json::to_string(&installed) {
      Ok(document) => {
        println!("{document}");
        ExitCode::SUCCESS
      }
      Err(error) => {
        eprintln!("octacity-release-harness: failed to encode installation receipt: {error}");
        ExitCode::FAILURE
      }
    },
    Err(error) => {
      eprintln!("octacity-release-harness: {error}");
      ExitCode::FAILURE
    }
  }
}
