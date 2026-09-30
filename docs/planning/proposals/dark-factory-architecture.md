# OctaCity Dark Factory Architecture

- Статус: архитектурное предложение
- Дата: 2026-09-27
- Область: OctaCity, Octa, coding harnesses, evaluation и delivery

## 1. Резюме

OctaCity развивается в source-agnostic dark factory control plane. Octa остаётся
детерминированным execution engine. Codex, Claude и другие coding harnesses
запускаются Octa через проверяемый plugin внутри изолированного Job.

```text
Work Source
  -> Work Envelope
  -> Factory Run
  -> immutable Task Envelope
  -> disposable sandbox/worktree
  -> octa-runner
  -> coding plugin
  -> Codex/Claude harness
  -> ChangeSet
  -> deterministic validation
  -> independent evaluation
  -> Accept / Rework / Escalate
  -> trusted delivery adapter
```

Workers и worktrees stateless и disposable. Durable state хранится в OctaCity,
Git и Artifact Store. Потеря worker или worktree не должна мешать продолжить
Factory Run на другом Agent.

Фабрика не должна быть привязана к GitHub, OpenSpec, Codex, Claude, одному
набору judges, конкретному Git forge или одному sandbox backend.

## 2. Цели и не-цели

### Цели

1. Принимать работу из разных источников через единый intake seam.
2. Нормализовать её в immutable Work Envelope.
3. Выполнять coding и rework в disposable worktree/sandbox.
4. Запускать разные coding harnesses через один стабильный Octa plugin contract.
5. Фиксировать результат как immutable ChangeSet для точного base commit.
6. Выполнять deterministic checks до LLM evaluation.
7. Поддерживать произвольные evidence producers, criterion packs и evaluators.
8. Принимать решение детерминированной policy, а не свободным текстом LLM.
9. Поддерживать bounded retries, budgets, WIP limits и human escalation.
10. Переживать restart, lease expiry, повтор событий и потерю Agent.

### Не-цели первого релиза

- автоматический merge любых изменений;
- dynamic infrastructure provisioning;
- одновременная поддержка всех ОС и sandbox backends;
- использование LLM judge как источника фактов;
- разрешение judges изменять проверяемый код;
- выдача coding harness широких forge credentials;
- превращение Octa в backlog scheduler или merge controller;
- зависимость correctness от сохранённой model session.

## 3. Существующий фундамент

OctaCity уже владеет нижним execution-слоем: Trigger, immutable Build и Attempt,
Jobs, DAG Orchestrator, Placement Scheduler, fenced Lease, Agents, signed
JobSpec, Artifact Store, secret grants, audit, observability и sandbox backends.

Octa уже предоставляет task DAG, headless `octa-runner`, versioned protocol,
process plugins, JSON Schema, structured outputs, progress, cancellation,
artifacts, reports и digest-pinned `Octa.lock`.

Dark factory строится поверх этого и не заменяет существующие Orchestrator,
Scheduler, Agent или Octa runtime.

## 4. Главные архитектурные решения

### 4.1 Factory Controller находится над Builds

Build является immutable execution request с точной source revision. Dark
factory меняет код между стадиями, поэтому весь lifecycle нельзя поместить в
один Build или существующий DAG Orchestrator.

```text
Factory Run
  -> Implementation Build at commit A
  -> Validation/Evaluation Builds at commit B
  -> Rework Build at commit B
  -> Validation/Evaluation Builds at commit C
  -> Delivery at commit C
```

Orchestrator продолжает двигать Jobs внутри Attempt. Factory Controller
принимает меж-Build решения.

### 4.2 OctaCity Agent не запускает Codex напрямую

```text
OctaCity Agent
  -> materialize exact revision
  -> create sandbox/worktree
  -> start octa-runner
  -> Octa executes tasks
  -> Octa launches coding/evaluator plugin
  -> plugin supervises selected harness
```

Agent владеет workspace, sandbox, resources, short-lived secrets, cancellation
и cleanup. Octa владеет task execution. Plugin владеет lifecycle harness
process. Harness изменяет файлы или возвращает assessment.

### 4.3 Implementation и evaluation разделены

```text
octa_plugin_coding
  workspace: writable
  purpose: implement or repair

octa_plugin_evaluator
  workspace: read-only
  purpose: assess immutable candidate
```

Они могут использовать общую библиотеку Harness Adapters, но evaluator не
получает право менять код.

### 4.4 Exact commit является identity результата

Нельзя проверять плавающую branch. Checks и assessments привязаны к точному
candidate commit SHA, ChangeSet digest, evidence digest и policy version. Любое
изменение кода создаёт новую Evaluation Round.

### 4.5 LLM не принимает merge decision

LLM возвращает schema-validated Assessment. Детерминированный Decision Engine
создаёт `Accept`, `Rework`, `Escalate`, `Reject` или `Cancel`.

## 5. Канонические термины

**Work Source** — источник потенциальной работы: tracker, webhook, REST, CLI,
queue, schedule, файл, OpenSpec change или другой Pipeline.

**Work Submission** — недоверенное сообщение до нормализации и admission.

**Work Envelope** — immutable provider-neutral описание принятой работы:
identity, subject, repository reference, задача, acceptance criteria, priority,
risk и specification artifact references.

**Work Reporter** — опциональный outbound adapter для статусов и результатов.

**Factory Configuration** — versioned admission, stages, budgets, connector
selectors, evaluation и delivery policy.

**Factory Run** — durable lifecycle одной принятой работы.

**Factory Stage** — implementation, validation, evaluation, rework или delivery.
Это не Octa task и не OctaCity Job.

**Stage Attempt** — одна bounded попытка стадии; retry не переписывает историю.

**Task Envelope** — immutable input Job: задача, revision, permissions, budget,
policy versions и ожидаемые outputs.

**Coding Harness** — Codex, Claude или другой исполнитель LLM coding session.
Его не следует называть Agent: Agent уже означает execution worker OctaCity.

**Harness Adapter** — provider-specific реализация стабильного plugin interface.

**ChangeSet** — immutable base/candidate relationship, commit или bundle/patch,
changed paths, digest и provenance.

**Evidence Producer** — task, создающий факты: JUnit, coverage, SARIF, SBOM,
dependency graph, benchmark, diff или repository inventory.

**Evidence Manifest** — immutable список evidence для exact candidate commit.

**Criterion Pack** — versioned rubric, не привязанный к model provider.

**Evaluator Connector** — adapter, создающий Assessment из request и evidence.

**Evaluation Round** — assessments одного subject digest и Evaluation Plan.

**Assessment** — outcome, findings, evidence references и provenance одного
evaluator. Это не delivery decision.

**Decision** — детерминированный результат Evaluation Policy.

**Delivery Adapter** — доверенный adapter для branch, PR и optional merge.

## 6. Source-agnostic intake

Поддерживаемые источники включают GitHub/GitLab, Linear/Jira, OpenSpec, REST,
CLI, расписание, queue, Slack, internal events, другой Factory Run и task-файл.

OpenSpec является опциональным specification artifact, а не обязательной
factory dependency:

```yaml
work:
  source:
    kind: github
    external_id: OctaHive/OctaCity#321
  subject:
    repository: OctaHive/OctaCity
    base_ref: main
  task:
    body_artifact: artifact://work/body/sha256:...
  specifications:
    - kind: openspec
      path: openspec/changes/add-evaluation-policy
      revision: 8e4d...
  risk: medium
```

Правила intake:

- внешний payload недоверенный;
- deduplication scoped к source installation и source version;
- повторная доставка не создаёт второй Factory Run;
- external label/claim не является единственным correctness state;
- Factory Run сохраняется до внешних side effects;
- reporter failure не откатывает внутреннее решение.

Предлагаемые interfaces:

```rust
trait WorkResolver {
    async fn resolve(&self, reference: WorkReference)
        -> Result<WorkEnvelope, WorkResolutionFailure>;
}

trait WorkReporter {
    async fn publish(&self, update: WorkUpdate)
        -> Result<(), WorkReportingFailure>;
}
```

Adapter может реализовывать один или оба interface.

## 7. Stateless execution model

Stateless/disposable:

- Agent process между Jobs;
- coding/evaluator session;
- plugin process;
- sandbox;
- worktree;
- local checkout и temporary files.

Durable state:

| Место | Authoritative state |
| --- | --- |
| Git / ChangeSet Store | код и immutable revisions |
| PostgreSQL OctaCity | Factory Run, stages, attempts, leases, budgets, policies, decisions |
| Artifact Store | logs, reports, evidence, assessments, transcripts, ChangeSets |
| Work Source | внешний identity и best-effort status projection |
| Worktree | ничего незаменимого |

Другой Agent должен продолжить работу, имея только Factory Run state, exact
commit, Task Envelope digest, policy/connector versions, artifact references и
current fenced Lease. Session resume допустим только как оптимизация.

## 8. Factory lifecycle

```text
Submitted
  -> Admitted
  -> Claimed
  -> Implementing
  -> Validating
  -> Evaluating
       -> Reworking -> Validating
       -> ReadyForDelivery
       -> Escalated
       -> Rejected
  -> Delivering
  -> Delivered

Любое нетерминальное состояние -> Cancelled
Infrastructure failure -> bounded retry or Escalated
```

Каждый переход основан на persisted observation. Таймер только будит
reconciler. Attempts append-only. Lease fencing исключает двух владельцев.
Delivery разрешён только для exact accepted candidate commit.

## 9. Coding execution через Octa plugin

Octafile использует один стабильный task key, а не provider-specific `codex:`
и `claude:` contracts:

```yaml
tasks:
  implement:
    coding:
      harness: codex
      mode: implement
      task_envelope: .octa/factory/task-envelope.json
      result: .octa/factory/coding-result.json
```

Это концептуальная схема; финальный contract фиксируется JSON Schema.

`octa_plugin_coding` владеет выбором Harness Adapter, process supervision,
streaming, cancellation, budgets, provider-result normalization, diagnostics и
registration reports.

Он не владеет Factory Run, source admission, worktree creation, sandbox policy,
VCS push/PR/merge, delivery decision или меж-Job retries.

Harness seam:

```rust
trait CodingHarness {
    async fn run(
        &self,
        request: HarnessRequest,
        events: EventSink,
        cancellation: CancellationToken,
    ) -> Result<HarnessOutcome, HarnessFailure>;
}
```

Adapters: Codex, Claude, Gemini, OpenCode, Aider и локальные runtimes.

Task Envelope фиксирует protocol version, identities, mode, exact revision,
task/spec artifacts, previous findings, permissions, network policy, secret
handles, budgets, expected output schema и prompt/policy digests.

Coding result содержит status, changed-path summary, summary, usage, diagnostics,
transcript и provider provenance. Изменения остаются в sandbox до trusted
ChangeSet capture.

Coding task заранее не знает все reads/writes и по умолчанию возвращает
`CachePlanUnavailable`. Reproducibility identity включает Task Envelope, base
commit, plugin/harness/model/image/prompt digests и ChangeSet digest.

## 10. ChangeSet и delivery

После coding stage trusted capture:

1. Проверяет resolved workspace root и ownership.
2. Проверяет changed paths, symlinks, size и file-count limits.
3. Исключает secrets и запрещённые paths.
4. Вычисляет diff и content digest.
5. Создаёт candidate commit или git bundle/patch.
6. Связывает ChangeSet с base commit и Stage Attempt.
7. Загружает artifacts/reports.
8. Уничтожает worktree.

Coding harness не получает постоянный forge write credential. Delivery Adapter
проверяет accepted candidate, ancestry, gates и policy; затем публикует branch,
создаёт/обновляет PR и выполняет merge только для явно разрешённого risk class.

Read-only VCS и write-capable delivery protocols следует разделить.

## 11. Validation и Evaluation Plane

```text
Evidence Producers -> Evaluators -> Decision Engine
       facts            opinions       authority
```

Deterministic failures поступают Decision Engine напрямую, а не через пересказ
LLM.

Evidence Producers: compiler, lint, tests, JUnit, coverage, mutation testing,
SARIF, dependency audit, SBOM, licenses, dependency graph, inventory,
benchmarks, diff, OpenSpec, ADR и project context snapshots.

Evidence Manifest привязан к exact base/candidate/ChangeSet digests:

```yaml
subject:
  base_revision: 31aa...
  candidate_revision: 79bd...
  changeset_digest: sha256:...
evidence:
  - schema: junit.v1
    artifact: artifact://...
    digest: sha256:...
  - schema: sarif.v2.1.0
    artifact: artifact://...
    digest: sha256:...
```

Новый аспект оценки добавляет Criterion Pack, не новый метод interface. Базовые
packs: `spec-compliance`, `architecture`, `code-quality`, `security`,
`test-quality`, `performance`, `dependency-risk`, `documentation`.

Operator-owned packs обязательны. Repository-owned packs могут усиливать, но не
ослаблять operator policy.

Один evaluator interface:

```rust
trait Evaluator {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<Assessment, EvaluationFailure>;
}
```

Request содержит subject, Criterion Pack, Evidence Manifest, budget/deadline,
data-handling policy и Assessment schema version.

Assessment:

```yaml
outcome: satisfied | violated | indeterminate
findings:
  - rule_id: dependency-direction
    severity: high
    confidence: 0.93
    message: application imports infrastructure implementation
    locations:
      - file: server/application/src/example.rs
        line: 42
    evidence_refs:
      - dependency-graph.v1#edge-991
    remediation: depend on the core port instead
    fingerprint: sha256:...
provenance:
  connector_digest: sha256:...
  model: provider/model-version
  prompt_digest: sha256:...
  criterion_pack_digest: sha256:...
  evidence_manifest_digest: sha256:...
```

Transport failure, timeout и cancellation отделены от Assessment outcome.

Если evidence недостаточно, evaluator возвращает `indeterminate` и bounded
typed evidence requirements. Он не получает произвольные tools. Factory может
запустить allowlisted producer и новую bounded attempt.

Connector manifest фиксирует protocol range, evidence schemas, languages,
provider/model family, input limits, network, data residency, cost,
concurrency и executable/image digest. Policy выбирает по capabilities, а
Evaluation Plan закрепляет exact connector и digest.

Decision Engine применяет явные правила, а не средний score:

```yaml
required:
  tests:
    outcome: satisfied
  security:
    deny_severity: high
  spec-compliance:
    outcome: satisfied
quorum:
  architecture:
    required: 2
    total: 3
advisory:
  - code-quality
on_required_indeterminate: escalate
```

Test failure ведёт к Rework. High security finding ведёт к Rework/Reject.
Required failure или indeterminate ведёт к Escalate, а не pass. High-risk
архитектура может требовать quorum независимых evaluators.

## 12. End-to-end flow

1. Work Source доставляет Work Submission.
2. Adapter проверяет transport/authenticity и создаёт reference.
3. WorkResolver создаёт Work Envelope.
4. Admission создаёт ровно один Factory Run.
5. Source ref разрешается в exact immutable base revision.
6. Factory Controller создаёт Implementation Stage Attempt.
7. Agent создаёт disposable sandbox/worktree и запускает octa-runner.
8. Octa запускает coding plugin, harness изменяет workspace.
9. Trusted capture создаёт ChangeSet/candidate commit.
10. Implementation worktree уничтожается.
11. Validation Jobs создают свежие worktrees exact candidate commit.
12. Octa запускает tests/scanners/evidence producers.
13. Evaluation Planner фиксирует immutable Evaluation Plan.
14. Evaluator Jobs параллельно работают в read-only worktrees.
15. Decision Engine создаёт Accept, Rework или Escalate.
16. Accept вызывает trusted Delivery Adapter; Rework создаёт новый worktree.
17. Work Reporter best-effort публикует status/result projection.
18. Retention policy очищает disposable и просроченные artifacts.

## 13. Proposed module ownership

Названия предварительные.

| Module | Owns | Explicitly does not own |
| --- | --- | --- |
| `octacity-server-factory` | Factory Run, stages, attempts, budgets, reconciliation | Job placement, harness execution, provider SDKs |
| `octacity-server-work` | Work Envelope, admission, deduplication | Provider payloads, tracker SDKs |
| `octacity-server-evaluation` | Evaluation Round, Plan, Assessment, Decision | Model calls, scanners, repo execution |
| `octacity-server-delivery` | Delivery policy and commands | Forge SDKs, repo execution |

Provider-neutral protocols:

- `octacity-work-source-protocol`;
- `octacity-work-reporter-protocol`;
- `octacity-delivery-protocol`;
- `octacity-evaluator-protocol`.

Octa plugins:

- `octa_plugin_coding` — writable implementation/rework;
- `octa_plugin_evaluator` — read-only structured Assessment;
- существующие shell/test plugins — deterministic evidence.

Shared internal library `octa-harness-adapters` содержит CodexHarness,
ClaudeHarness и будущие adapters.

## 14. Security and trust model

Недоверенными являются Work Submission, repository contents, filenames,
symlinks, build scripts, OpenSpec/README/comments, harness/evaluator output,
tool reports, URLs и provider metadata.

Основные правила:

- repository text передаётся как evidence, не instructions;
- system rubric хранится вне repo и фиксируется digest;
- permissions enforced кодом, а не prompt;
- evaluator по умолчанию не имеет shell/write/arbitrary network;
- LLM output schema-validated и никогда не исполняется напрямую;
- secrets и cross-tenant data не попадают в context;
- model credentials short-lived, scoped и redacted;
- delivery credential доступен только trusted adapter;
- filesystem operations ограничены resolved owned workspace root;
- plugins, harness runtime, image, prompts и policies pinned digests;
- implementation harness не является единственным reviewer;
- deterministic gates имеют приоритет;
- required indeterminate никогда не становится pass;
- auto-merge только opt-in для allowlisted low-risk policy.

## 15. Failure, retry и fencing

- Worker loss: lease expiry fences owner; другой Agent materializes exact state.
- Duplicate intake: scoped dedup identity возвращает существующий Factory Run.
- Stale completion: принимается только current lease/stage/subject digest.
- Connector outage: bounded retry; exhaustion required connector -> Escalate.
- Partial evidence: required evaluation не становится accepted.
- Delivery lost response: stable idempotency key и observe-before-retry.
- Rework: новый ChangeSet/Evaluation Round, bounded attempts/time/tokens/cost.

Multi-instance correctness хранится в authoritative store. Process-local timers
только будят workers. Все claims имеют owner, deadline и fence.

## 16. Observability и масштабирование

Audit фиксирует source identity, Factory transitions, exact revisions,
connector/plugin/image/policy digests, usage, lease fence, evidence freshness,
decision reasons, delivery identity и escalation cause.

Task text, paths высокой кардинальности, issue IDs, prompts, secrets и raw
findings не попадают в metric labels.

Масштабирование:

- WIP limits на Project/Factory Configuration;
- provider/model concurrency pools;
- раздельные coding/evaluation quotas;
- source admission rate limits;
- provider circuit breakers;
- fair scheduling;
- bounded evaluator fan-out.

Новый источник добавляет Work Source Adapter. Новый coding provider добавляет
Harness Adapter. Новый аспект качества добавляет Criterion Pack/Evidence
Producer. Новый delivery target добавляет Delivery Adapter. Factory Controller
не изменяется.

## 17. Минимальный безопасный пилот

Предпосылки:

1. Полный server -> Agent -> Octa lifecycle через PostgreSQL.
2. Typed cancel/retry/diagnostics.
3. Один квалифицированный isolation backend.
4. Recovery/failure tests.
5. Security tests для malicious repository и secret isolation.
6. Надёжные Artifact/report contracts.

Первый slice:

```text
manual REST/CLI Work Submission
  -> Work Envelope
  -> Factory Run
  -> Codex through octa_plugin_coding
  -> trusted ChangeSet capture
  -> Octa fmt/lint/test
  -> spec + architecture + security + test-quality evaluation
  -> at most one rework
  -> PR creation
  -> human merge
```

Для проверки seams пилот должен иметь минимум два Work Source adapters, Codex и
Claude adapters, один coding plugin contract, несколько Criterion Packs через
один evaluator interface и fake adapters для deterministic tests.

## 18. Этапы развития

### Phase A: contracts

Утвердить glossary; Work/Task Envelope; ChangeSet; Assessment/Decision schemas;
plugin provenance; threat model; ADR о разделении Factory Run и Build.

### Phase B: manual factory to PR

Manual intake, coding plugin, Codex adapter, ChangeSet capture, deterministic
validation, один evaluator connector, human merge.

### Phase C: pluggability

Claude adapter, второй Work Source, Work Reporter, multiple Criterion Packs,
connector registry и bounded rework.

### Phase D: unattended backlog

Polling/webhooks, durable claiming, WIP limits, reconciliation, circuit
breakers и multi-instance tests.

### Phase E: policy-based auto-merge

Risk classification, independent/quorum reviewers, calibration corpus, holdout
tests, low-risk allowlist, rollback/escalation policy и audited enablement.

## 19. Архитектурные инварианты

1. Work Source не определяет внутренний lifecycle Factory Run.
2. OpenSpec — optional specification format, не factory dependency.
3. Factory Controller не вызывает model provider напрямую.
4. OctaCity Agent не знает provider-specific harness semantics.
5. Octa не владеет backlog, Factory Run или delivery.
6. Coding plugin не выполняет push, PR или merge.
7. Evaluator не изменяет candidate.
8. Assessment не является Decision.
9. LLM output не исполняется без schema/policy validation.
10. Gates относятся к exact candidate/evidence digests.
11. Worktree не содержит authoritative state.
12. Retry не переписывает историю.
13. Required missing/indeterminate evidence не означает pass.
14. External status — projection, не correctness state.
15. Auto-merge opt-in и ограничен risk policy.

## 20. Решения для отдельных ADR

1. ChangeSet format: commit, git bundle, patch series или комбинация.
2. Где выполняется trusted commit creation.
3. Один universal coding plugin или independently distributed provider plugins.
4. Installation/versioning model Criterion Packs.
5. Evaluator process на Agent или remote evaluator pool.
6. Data residency classes для внешних LLM providers.
7. Quorum/model independence для high-risk изменений.
8. Граница auto-merge и аварийное отключение.
9. Хранение resumable sessions как optional optimization.
10. Разделение Git publication и forge PR operations.

## 21. Критерии подтверждения архитектуры

1. Одна задача принимается из REST и GitHub без изменения Factory Controller.
2. Один Octafile работает через Codex и Claude без provider-specific factory logic.
3. Agent погибает после coding, другой продолжает без hidden local state.
4. Completion старого lease после takeover отклоняется.
5. Architecture/security используют один evaluator interface и разные packs.
6. Тесты failed, LLM сказал pass, но Decision Engine создаёт Rework.
7. Required evaluator недоступен — система Escalates.
8. Prompt injection в repo не расширяет permissions evaluator.
9. Candidate изменился — старые gates не принимаются.
10. Delivery retry после lost response не создаёт второй PR.
11. Coding harness не имеет push/merge credential.
12. Удаление worktree не уничтожает возможность продолжить Factory Run.

## 22. Итог

Dark factory является новым верхним coordination-слоем OctaCity, а не
расширением Octa до orchestrator. Octa исполняет DAG и plugins, OctaCity владеет
durable lifecycle, coding harnesses меняют одноразовые worktrees, Evaluation
Plane независимо оценивает exact candidate, а доверенный Delivery Adapter
публикует принятый результат.

```text
новый источник        -> Work Source Adapter
новый coding provider -> Harness Adapter
новый аспект качества -> Criterion Pack / Evidence Producer
новый target delivery -> Delivery Adapter
```

Так система сохраняет stateless execution, не связывается с GitHub, OpenSpec,
Codex или Claude и может постепенно перейти от human-approved PR factory к
unattended low-risk delivery.
