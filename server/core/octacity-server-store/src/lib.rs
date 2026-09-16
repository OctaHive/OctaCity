//! Backend-neutral persistence ports shaped around complete atomic use cases.
//!
//! Table CRUD, SQL rows, transactions, and database-specific error types do
//! not belong in this crate.

#![forbid(unsafe_code)]
