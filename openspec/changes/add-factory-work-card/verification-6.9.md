# 6.9 — Provider-neutral code-review contracts and memory behavior

## Test boundaries and observed Red → Green

- `cargo test --locked -p octacity-server-application --lib issue_code_reviews`: Red on missing public association/observation contracts, then Green for several canonical PR/MR references, original scoped replay, independent native reader selection, retained safe facts and unchanged manual Issue/primary ticket.
- `cargo test --locked -p octacity-server-application --lib issue_code_review_refresh`: Red on absent explicit refresh/history contracts, then Green for original/new heads, exact refresh acknowledgement after read revocation and independently paged retained observations.
- `cargo test --locked -p octacity-server-application --lib issue_code_review_binding`: Red on absent optional existing-journal composition, then Green for original association/record/candidate validation, foreign/missing/hidden Run rejection and no Factory progress. A further Red proved that mismatched optional receipt output must not establish another provenance; the request/receipt contract is now checked explicitly.
- `cargo test --locked -p octacity-server-application --lib issue_code_review_delivery`: Red first rejected a real retained publication because Delivery verification was absent. Green reused the actual installed candidate-publication fixture and common retained record. A receipt-shaped result from a differently declared action then established a second behavioral Red; Green requires the existing declared publication action, whose name is shared with its original validator.
- `cargo test --locked -p octacity-server-application --lib issue_code_review_pages`: Red returned a hidden native repository; Green supplies the existing typed read-visibility mechanism before membership/count/order/cursor queries and checks metadata visibility before replay. Security/Issue/visibility changes, foreign targets and invalid continuations remain isolated.

## Implementation and limits

The optional handler/ports remain outside the Issue domain and manual creation. Identity uses stable installed/native facts, independently from a primary ticket, display number, URL and configuration pin. The existing execution reader verifies optional lineage and retained Delivery facts. There is no provider SDK, new credential mechanism, execution owner, provider GET or automatic poller in these handlers. Current protected-read revocation is separate from retained metadata visibility. Unsupported fields are optional; external merge/check summaries confer no Factory acceptance or task-status authority.

Memory uses the existing atomic Issue transaction, audit and scoped replay publication. Safe observation history is the only source of latest facts; PostgreSQL parity subsequently qualified timestamp ties and old-observation replay in 6.10. Concrete installed read adapters and HTTP/Console review composition remain tasks 6.11–6.13.


Final cross-slice regression and cleanup results: [6.10 verification](verification-6.10.md#final-regression-verification).
