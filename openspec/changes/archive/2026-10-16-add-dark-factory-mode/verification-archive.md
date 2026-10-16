# Archive qualification: 2026-10-16

The operator requested archival after the remaining CI passed. The verified
implementation revision is `08c44acf9a80c7eae1863750d51e6c51af1a4235` on `main`.
All 121 tasks are complete under the explicitly approved offline scope for
task 16.8. Live provider qualification is deferred, not reported as performed.

## Confirmed CI

- [Hosted CI](https://github.com/OctaHive/OctaCity/actions/runs/38890858814)
  completed successfully on that exact revision: Windows, Linux and macOS
  quality, coverage, Operator Console and both hosted local-stand jobs passed.
- [Windows quality](https://github.com/OctaHive/OctaCity/actions/runs/38890858814/job/116939752876)
  passed all 10 OpenAI tests, including the literal IPv4 HTTPS regression,
  unknown-usage/no-redispatch cases, and TLS/hostname/response bounds. The
  original 300 ms failure timeout and every original failure assertion remain
  intact. The two fixture corrections and their Red/Green evidence are recorded
  in `verification-16.8.md`; production transport was unchanged by those fixes.
- [Released Host contracts](https://github.com/OctaHive/OctaCity/actions/runs/38890858737)
  passed on the same revision for Linux AMD64, macOS ARM64 and Windows AMD64,
  with a successful evidence aggregate. This push-triggered run covers the
  Host matrix, not the complete hosted backend/security matrix.
- [Security scanning](https://github.com/OctaHive/OctaCity/actions/runs/38874030161)
  passed at `b5da11ae195fc6da60a43b68d5f4f25da4109943`. Subsequent changes before
  archival affect only test fixtures/tests and verification documentation;
  production sources, dependency manifests and the lockfile are identical.
  The workflow's existing path filters did not trigger another security scan.

The optional self-hosted Apple Silicon stand job was skipped; it is not counted
as successful qualification. The earlier complete hosted backend and native
performance evidence retains its original exact source scope in
`verification-16.6.md` and `verification-16.7.md`. No skipped job, fixture or
older-source run is reassigned to a new qualification claim.

## Archive checks

All seven delta capabilities were already synchronized: all 52 added/modified
requirement blocks match their main specifications. Archival therefore used
`openspec archive add-dark-factory-mode --yes --skip-specs` after strict change
validation, preserving main specs unchanged.

The graph semantics contract now reads the stable main specification instead
of an active change path. Its three existing behavioral tests passed both
before and after that path-only refactor:
`python3 -m unittest tools.tests.test_factory_graph_semantics -v`.

`openspec validate --specs --strict --json` passed all 11 main specifications;
`openspec validate --archived --strict --json` passed all four archived changes.
All seven relative Markdown links in this archive resolve after relocation,
the planning reference points to the archived design and stable main spec,
and `git diff --check` passed. Archival/documentation has no executable behavior
requiring a new Red/Green cycle; existing test behavior was preserved.

Live shadow/advisory observations, representative holdouts, exact-model
calibration, audited promotion and live demotion/fallback remain separate work.
Archival grants no real-model approval or bounded-control authority. The
periodic `factory-16-6-16-9` automation was deleted at the operator's request.
