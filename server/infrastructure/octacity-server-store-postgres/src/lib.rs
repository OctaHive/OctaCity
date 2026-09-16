//! PostgreSQL persistence adapter.
//!
//! Migrations, SQL rows, transaction mechanics, locks, and conversion to the
//! backend-neutral store contract are isolated here.

#![forbid(unsafe_code)]
