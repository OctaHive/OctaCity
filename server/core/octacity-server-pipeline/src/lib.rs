//! Immutable Pipeline versions, DAG validation, and dependency policy.
//!
//! Attempt materialization derives server Jobs from validated snapshots owned
//! by this module.
//!
//! A **Pipeline** is the immutable DAG definition. An **Attempt** is one
//! numbered materialization of a Pipeline into server Jobs; neither term means
//! Build or retry. See the [canonical glossary] and [server ownership guide].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
