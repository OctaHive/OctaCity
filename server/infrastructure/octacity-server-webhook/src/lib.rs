//! Discovers trusted webhook-provider adapters and supervises their processes.
//!
//! Registry verification and process hosting form one security boundary: a
//! request is sent only after the operator-installed manifest, protocol range,
//! capability, executable path, and executable digest have been verified. The
//! host clears inherited environment variables and exchanges only bounded v1
//! protocol frames, so provider credentials remain behind opaque handles.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod host;
mod registry;

pub use host::{HostFailureClass, WebhookHostError};
pub use registry::{InstalledWebhookAdapter, RegistryError, WebhookAdapterRegistry};
