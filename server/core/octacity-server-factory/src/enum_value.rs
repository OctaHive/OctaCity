use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{FactoryEnumKind, FactoryError};

macro_rules! factory_enum {
  ($name:ident, $kind:expr, $documentation:literal, { $($variant:ident => $value:literal),+ $(,)? }) => {
    #[doc = $documentation]
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub enum $name {
      $(
        #[doc = concat!("Canonical `", $value, "` value.")]
        $variant,
      )+
    }

    impl $name {
      /// Returns the canonical stable representation.
      #[must_use]
      pub const fn as_str(self) -> &'static str {
        match self {
          $(Self::$variant => $value,)+
        }
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
      }
    }

    impl FromStr for $name {
      type Err = FactoryError;

      fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
          $($value => Ok(Self::$variant),)+
          _ => Err(FactoryError::UnsupportedEnum { kind: $kind }),
        }
      }
    }

    impl Serialize for $name {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
        S: Serializer,
      {
        serializer.serialize_str(self.as_str())
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
        D: Deserializer<'de>,
      {
        String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
      }
    }
  };
}

factory_enum!(
  FactoryRunState,
  FactoryEnumKind::RunState,
  "Durable high-level state of one Factory Run.",
  {
    Admitted => "admitted",
    Implementing => "implementing",
    Validating => "validating",
    Evaluating => "evaluating",
    Reworking => "reworking",
    ReadyForDelivery => "ready_for_delivery",
    Delivering => "delivering",
    Escalated => "escalated",
    Rejected => "rejected",
    Cancelled => "cancelled",
    Completed => "completed",
  }
);
factory_enum!(
  PermissionCategory,
  FactoryEnumKind::PermissionCategory,
  "Provider-neutral capability required to enforce one Factory permission category.",
  {
    PluginIdentity => "plugin_identity",
    ExecutableIdentity => "executable_identity",
    ToolIdentity => "tool_identity",
    CommandArguments => "command_arguments",
    DescendantProcesses => "descendant_processes",
    FilesystemPaths => "filesystem_paths",
    MountModes => "mount_modes",
    NetworkHosts => "network_hosts",
    SecretProfiles => "secret_profiles",
    WorkloadIdentityProfiles => "workload_identity_profiles",
    Cpu => "cpu",
    Memory => "memory",
    Disk => "disk",
    ProcessCount => "process_count",
    ElapsedTime => "elapsed_time",
    Outputs => "outputs",
  }
);
factory_enum!(
  MountMode,
  FactoryEnumKind::MountMode,
  "Access granted to one canonical Factory filesystem root.",
  {
    ReadOnly => "read_only",
    ReadWrite => "read_write",
  }
);

impl PermissionCategory {
  /// Every permission category that a Factory execution backend must enforce.
  pub const ALL: [Self; 16] = [
    Self::PluginIdentity,
    Self::ExecutableIdentity,
    Self::ToolIdentity,
    Self::CommandArguments,
    Self::DescendantProcesses,
    Self::FilesystemPaths,
    Self::MountModes,
    Self::NetworkHosts,
    Self::SecretProfiles,
    Self::WorkloadIdentityProfiles,
    Self::Cpu,
    Self::Memory,
    Self::Disk,
    Self::ProcessCount,
    Self::ElapsedTime,
    Self::Outputs,
  ];
}
factory_enum!(
  FactoryStageKind,
  FactoryEnumKind::Stage,
  "Program-owned kind of Factory Stage Attempt.",
  {
    Implementation => "implementation",
    Validation => "validation",
    Evaluation => "evaluation",
    Rework => "rework",
  }
);
factory_enum!(
  MacroCallKind,
  FactoryEnumKind::Call,
  "Purpose of one bounded reasoning call within a Stage Attempt.",
  {
    Implement => "implement",
    Evaluate => "evaluate",
    Summarize => "summarize",
  }
);
factory_enum!(
  RiskClass,
  FactoryEnumKind::Risk,
  "Bounded risk assigned to admitted Work.",
  {
    Low => "low",
    Medium => "medium",
    High => "high",
    Critical => "critical",
  }
);
factory_enum!(
  DecisionSignalPurpose,
  FactoryEnumKind::DecisionSignalPurpose,
  "Permitted non-authoritative Decision Signal seam.",
  {
    Routing => "routing",
    ToolRisk => "tool_risk",
  }
);
factory_enum!(
  DecisionSignalState,
  FactoryEnumKind::DecisionSignalState,
  "Terminal classification of one Decision Signal request.",
  {
    Completed => "completed",
    TimedOut => "timed_out",
    Invalid => "invalid",
    Unavailable => "unavailable",
    Cancelled => "cancelled",
  }
);
factory_enum!(
  DecisionSignalMode,
  FactoryEnumKind::DecisionSignalMode,
  "Configured authority of a Decision Signal relative to deterministic policy.",
  {
    Shadow => "shadow",
    Advisory => "advisory",
    BoundedControl => "bounded_control",
  }
);
factory_enum!(
  DecisionSignalFallback,
  FactoryEnumKind::DecisionSignalFallback,
  "Fail-closed disposition when a configured Decision Signal is unavailable or unusable.",
  {
    Deny => "deny",
    Escalate => "escalate",
  }
);
factory_enum!(
  AssessmentOutcome,
  FactoryEnumKind::AssessmentOutcome,
  "Schema-valid outcome of one independent evaluator.",
  {
    Satisfied => "satisfied",
    Violated => "violated",
    Indeterminate => "indeterminate",
  }
);
factory_enum!(
  FindingSeverity,
  FactoryEnumKind::FindingSeverity,
  "Ordered severity of one typed evaluator violation.",
  {
    Advisory => "advisory",
    Low => "low",
    Medium => "medium",
    High => "high",
    Critical => "critical",
  }
);
factory_enum!(
  DeterministicGateOutcome,
  FactoryEnumKind::DeterministicGateOutcome,
  "Authoritative outcome projected from exact deterministic evidence.",
  {
    Passed => "passed",
    Failed => "failed",
    Indeterminate => "indeterminate",
  }
);
factory_enum!(
  IndeterminatePolicy,
  FactoryEnumKind::IndeterminatePolicy,
  "Versioned policy for optional indeterminate evaluator Assessments.",
  {
    RequiredOnly => "required_only",
    Any => "any",
  }
);
factory_enum!(
  DecisionOutcome,
  FactoryEnumKind::DecisionOutcome,
  "Deterministic candidate decision produced from evidence and assessments.",
  {
    Accept => "accept",
    Rework => "rework",
    Reject => "reject",
    Escalate => "escalate",
    Cancel => "cancel",
  }
);
factory_enum!(
  DeliveryState,
  FactoryEnumKind::DeliveryState,
  "Terminal classification of a delivery-for-review attempt.",
  {
    Succeeded => "succeeded",
    Failed => "failed",
    Unknown => "unknown",
  }
);
factory_enum!(
  ReportingState,
  FactoryEnumKind::ReportingState,
  "Terminal classification of a Work Reporter attempt.",
  {
    Succeeded => "succeeded",
    Failed => "failed",
    Unknown => "unknown",
  }
);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_open_enum_rejects_unknown_values() {
    let errors = [
      "future".parse::<FactoryRunState>().expect_err("unknown run state"),
      "future".parse::<FactoryStageKind>().expect_err("unknown stage"),
      "future".parse::<MacroCallKind>().expect_err("unknown call"),
      "future".parse::<RiskClass>().expect_err("unknown risk"),
      "future"
        .parse::<DecisionSignalPurpose>()
        .expect_err("unknown signal purpose"),
      "future"
        .parse::<DecisionSignalState>()
        .expect_err("unknown signal state"),
      "future".parse::<DecisionSignalMode>().expect_err("unknown signal mode"),
      "future"
        .parse::<DecisionSignalFallback>()
        .expect_err("unknown signal fallback"),
      "future".parse::<AssessmentOutcome>().expect_err("unknown assessment"),
      "future"
        .parse::<FindingSeverity>()
        .expect_err("unknown finding severity"),
      "future"
        .parse::<DeterministicGateOutcome>()
        .expect_err("unknown deterministic gate outcome"),
      "future"
        .parse::<IndeterminatePolicy>()
        .expect_err("unknown indeterminate policy"),
      "future".parse::<DecisionOutcome>().expect_err("unknown decision"),
      "future".parse::<DeliveryState>().expect_err("unknown delivery state"),
      "future".parse::<ReportingState>().expect_err("unknown reporting state"),
      "future"
        .parse::<PermissionCategory>()
        .expect_err("unknown permission category"),
      "future".parse::<MountMode>().expect_err("unknown mount mode"),
    ];

    assert!(
      errors
        .into_iter()
        .all(|error| matches!(error, FactoryError::UnsupportedEnum { .. }))
    );
  }
}
