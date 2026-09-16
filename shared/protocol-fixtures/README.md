# Protocol fixtures

This directory contains language-neutral golden documents for shared wire
protocols. Tests deserialize these fixtures with the corresponding protocol
implementation to detect incompatible schema changes.

`coordinator/` covers server-agent coordination and `artifact/` covers the
backend-neutral logical artifact transfer contract.

The fixtures are not a Cargo crate and must not contain implementation code.
