//! Bounded raw-webhook ingress adapter.
//!
//! This crate owns transport decoding and error mapping, not provider
//! authentication, event normalization, or trigger decisions.

#![forbid(unsafe_code)]
