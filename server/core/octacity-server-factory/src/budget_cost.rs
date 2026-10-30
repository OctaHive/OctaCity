//! Measured monetary subtotal and explicit absence of a complete price.
use serde::{Deserialize, Serialize};

/// Monetary usage with an optional unpriced component. A measured zero remains
/// different from unknown cost, including after aggregation and persistence.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(from = "CostWire", into = "CostWire")]
pub struct BudgetCost {
  known_micro_units: u64,
  unpriced: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
enum CostWire {
  Known(u64),
  Unknown {
    status: UnknownStatus,
    known_micro_units: u64,
  },
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum UnknownStatus {
  Unknown,
}

impl BudgetCost {
  /// One unpriced subscription operation, without an invented monetary amount.
  pub const fn unknown() -> Self {
    Self {
      known_micro_units: 0,
      unpriced: true,
    }
  }
  /// Measured subtotal; it is the total only when [`Self::is_known`] is true.
  pub const fn known_micro_units(self) -> u64 {
    self.known_micro_units
  }
  /// Whether every component has a measured price.
  pub const fn is_known(self) -> bool {
    !self.unpriced
  }
  /// Combine known subtotals without losing an unpriced component.
  pub fn checked_add(self, other: Self) -> Option<Self> {
    Some(Self {
      known_micro_units: self.known_micro_units.checked_add(other.known_micro_units)?,
      unpriced: self.unpriced || other.unpriced,
    })
  }
  /// Subtract measured counters; an unpriced total cannot become measured by
  /// subtracting an earlier observation whose own price was also unavailable.
  pub fn checked_sub(self, other: Self) -> Option<Self> {
    if self.is_known() && !other.is_known() {
      return None;
    }
    Some(Self {
      known_micro_units: self.known_micro_units.checked_sub(other.known_micro_units)?,
      unpriced: self.unpriced,
    })
  }
  /// Stable digest encoding. Measured costs preserve their historical eight
  /// bytes; an unpriced component adds a marker so it cannot alias measured zero.
  pub fn canonical_bytes(self) -> Vec<u8> {
    let mut bytes = self.known_micro_units.to_be_bytes().to_vec();
    if self.unpriced {
      bytes.push(1);
    }
    bytes
  }
}
impl From<u64> for BudgetCost {
  fn from(value: u64) -> Self {
    Self {
      known_micro_units: value,
      unpriced: false,
    }
  }
}
impl From<CostWire> for BudgetCost {
  fn from(value: CostWire) -> Self {
    match value {
      CostWire::Known(value) => value.into(),
      CostWire::Unknown { known_micro_units, .. } => Self {
        known_micro_units,
        unpriced: true,
      },
    }
  }
}
impl From<BudgetCost> for CostWire {
  fn from(value: BudgetCost) -> Self {
    if value.unpriced {
      Self::Unknown {
        status: UnknownStatus::Unknown,
        known_micro_units: value.known_micro_units,
      }
    } else {
      Self::Known(value.known_micro_units)
    }
  }
}
impl PartialEq<u64> for BudgetCost {
  fn eq(&self, other: &u64) -> bool {
    self.is_known() && self.known_micro_units == *other
  }
}
impl std::fmt::Display for BudgetCost {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if self.is_known() {
      self.known_micro_units.fmt(formatter)
    } else {
      write!(formatter, "unknown(known_micro_units={})", self.known_micro_units)
    }
  }
}
