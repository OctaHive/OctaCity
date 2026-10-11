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

Factory provenance in the unreleased v3 contract binds the exact Flow, workflow
cycle, Node Attempt and frozen input digest. It has no stage role or fixed
predecessor category. Its required `execution_deadline_unix_ms` is independent
of signature validity and preserves the exact Node deadline. Source mount access comes from signed permissions;
capture requires explicit write authority, and configured materialization still
requires the exact protected bundle and manifest. The v1/v2 fixtures are unchanged.

The fixtures are not a Cargo crate and must not contain implementation code.
