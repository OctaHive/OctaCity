# Protocol fixtures

This directory contains language-neutral golden documents for shared wire
protocols. Tests deserialize these fixtures with the corresponding protocol
implementation to detect incompatible schema changes.

`coordinator/` covers server-agent coordination and `artifact/` covers the
backend-neutral logical artifact transfer contract.

`job-spec/` contains the exact deterministic JSON bytes emitted for execution
contracts v1, v2, and v3. The v1/v2 fixtures freeze their released wire shape;
the v3 fixture defines protected managed execution without transfer URLs or
credentials.

The fixtures are not a Cargo crate and must not contain implementation code.
