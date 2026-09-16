//! Transport-independent OctaCity server use cases.
//!
//! Typed commands, queries, transaction coordination, projections, and
//! application error mapping belong here. Transport and concrete persistence
//! types must remain outside this crate.

#![forbid(unsafe_code)]
