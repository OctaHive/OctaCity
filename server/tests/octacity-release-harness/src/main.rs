//! Command-line entry point for the isolated Agent Ready release harness.

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use octacity_release_harness::{
  AgentRuntimeBundles, AgentRuntimePlatforms, BrowserReleaseBundles, HarnessError, ReleaseBundles,
  derive_job_spec_policy, install, install_browser_release, load_job_spec_policy, verify_job_spec_policy,
};
use serde::Serialize;

const DEFAULT_JOB_SPEC_VALIDITY_SECONDS: u64 = 900;

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
  /// Copy a verified matching server and static console into one installation.
  PrepareBrowser {
    /// Extracted checksummed server bundle root.
    #[arg(long)]
    server_root: PathBuf,
    /// Extracted checksummed console bundle root.
    #[arg(long)]
    console_root: PathBuf,
    /// New directory that will own this scenario's installation.
    #[arg(long)]
    install_root: PathBuf,
  },
  /// Emit a server JobSpec policy derived from one installed Agent runtime.
  JobSpecPolicy {
    /// Installed checksummed Agent release root.
    #[arg(long)]
    agent_root: PathBuf,
    /// Installed checksummed Octa release root.
    #[arg(long)]
    octa_root: PathBuf,
    /// Expected Agent release platform.
    #[arg(long)]
    agent_platform: String,
    /// Expected Octa runner platform.
    #[arg(long)]
    octa_platform: String,
    /// Signed JobSpec lifetime in seconds.
    #[arg(long, default_value_t = DEFAULT_JOB_SPEC_VALIDITY_SECONDS)]
    validity_seconds: u64,
  },
  /// Verify that a server JobSpec policy exactly matches installed Agent assets.
  VerifyJobSpecPolicy {
    /// Installed checksummed Agent release root.
    #[arg(long)]
    agent_root: PathBuf,
    /// Installed checksummed Octa release root.
    #[arg(long)]
    octa_root: PathBuf,
    /// Expected Agent release platform.
    #[arg(long)]
    agent_platform: String,
    /// Expected Octa runner platform.
    #[arg(long)]
    octa_platform: String,
    /// Server JobSpec policy JSON file.
    #[arg(long)]
    policy: PathBuf,
    /// Expected signed JobSpec lifetime in seconds.
    #[arg(long, default_value_t = DEFAULT_JOB_SPEC_VALIDITY_SECONDS)]
    validity_seconds: u64,
  },
}

fn main() -> ExitCode {
  let Cli { command } = Cli::parse();
  let result = run(command);
  match result {
    Ok(document) => {
      println!("{document}");
      ExitCode::SUCCESS
    }
    Err(error) => {
      eprintln!("octacity-release-harness: {error}");
      ExitCode::FAILURE
    }
  }
}

fn run(command: Command) -> Result<String, HarnessError> {
  match command {
    Command::Prepare {
      server_root,
      agent_root,
      octa_root,
      install_root,
    } => encode(&install(
      &ReleaseBundles {
        server: server_root,
        agent: agent_root,
        octa: octa_root,
      },
      &install_root,
    )?),
    Command::PrepareBrowser {
      server_root,
      console_root,
      install_root,
    } => encode(&install_browser_release(
      &BrowserReleaseBundles {
        server: server_root,
        console: console_root,
      },
      &install_root,
    )?),
    Command::JobSpecPolicy {
      agent_root,
      octa_root,
      agent_platform,
      octa_platform,
      validity_seconds,
    } => encode(&derive_job_spec_policy(
      &AgentRuntimeBundles {
        agent: agent_root,
        octa: octa_root,
      },
      &AgentRuntimePlatforms::new(agent_platform, octa_platform),
      validity_seconds,
    )?),
    Command::VerifyJobSpecPolicy {
      agent_root,
      octa_root,
      agent_platform,
      octa_platform,
      policy,
      validity_seconds,
    } => {
      let configured = load_job_spec_policy(&policy)?;
      verify_job_spec_policy(
        &AgentRuntimeBundles {
          agent: agent_root,
          octa: octa_root,
        },
        &AgentRuntimePlatforms::new(agent_platform, octa_platform),
        &configured,
        validity_seconds,
      )?;
      Ok("{\"verified\":true}".to_owned())
    }
  }
}

fn encode(value: &impl Serialize) -> Result<String, HarnessError> {
  serde_json::to_string(value).map_err(HarnessError::Json)
}
