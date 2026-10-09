# OctaCity Dark Factory Architecture

- Статус: целевая архитектура и снимок реализации
- Создано: 2026-09-27
- Актуализировано: 2026-10-09
- Область: OctaCity, Octa, coding harnesses, evaluation и delivery

Верхнеуровневая карта системы, двухуровневой модели Factory Flow / Octa Task
DAG, подсистем, connectors и полного ticket lifecycle находится в документе
[OctaCity Dark Factory: обзор системы](./dark-factory-system-overview.md).

## 1. Резюме

OctaCity остаётся самостоятельной CI/CD-платформой и дополнительно получает
source-agnostic Dark Factory mode. Обычные Pipelines, Builds, Triggers и ручные
operator commands не требуют Factory Controller, LLM или coding harness. Dark
Factory переиспользует тот же execution substrate и координирует несколько
обычных immutable Builds как более длинный lifecycle работы над кодом.

```text
CI/CD mode
  Trigger / operator
    -> Build -> Attempt -> Job DAG -> Agent -> Octa -> Result

Dark Factory mode
  Work Source -> Factory Run
    -> pinned nested Flow Definition
    -> Node Attempts: Build | reasoning | Decision Signal
    -> deterministic gates / joins / human gates / trusted actions
    -> Delivery / promotion through ordinary CI/CD
```

Octa в обоих режимах остаётся детерминированным execution engine. Codex, Claude
и другие coding harnesses запускаются Octa через проверяемые plugins внутри
изолированного Job. CI/CD и Dark Factory имеют общий substrate, но разные
application lifecycles и могут развиваться, включаться и эксплуатироваться
независимо.

```text
Work Source
  -> Work Envelope
  -> Factory Run
  -> immutable root Flow Definition + pinned subflow closure
  -> triage / requirements / development / verification subflows
  -> immutable Task Envelopes and Context Manifests per node
  -> ordinary Builds, bounded reasoning calls and Decision Signals
  -> exact candidate lineage + deterministic evidence
  -> policy-governed delivery and promotion through existing CI/CD
```

Workers и worktrees stateless и disposable. Durable state хранится в OctaCity,
Git и Artifact Store. Потеря worker или worktree не должна мешать продолжить
Factory Run на другом Agent.

Фабрика не должна быть привязана к GitHub, OpenSpec, Codex, Claude, одному
набору judges, конкретному Git forge или одному sandbox backend.
Описанный ниже ticket-to-production lifecycle является первой конфигурацией,
а не зашитым в domain единственным flow.

## 2. Цели и не-цели

### Цели

1. Сохранить полноценный независимый CI/CD mode и его существующие contracts.
2. Принимать factory work из разных источников через единый intake seam.
3. Нормализовать её в immutable Work Envelope.
4. Выполнять coding и rework в disposable worktree/sandbox.
5. Запускать разные coding harnesses через versioned Octa task/plugin contracts,
   не пропуская provider-specific semantics в OctaCity.
6. Фиксировать результат как immutable ChangeSet для точного base commit.
7. Выполнять deterministic checks до LLM evaluation.
8. Поддерживать произвольные evidence producers, criterion packs и evaluators.
9. Принимать решение детерминированной policy, а не свободным текстом LLM.
10. Подключать взаимозаменяемые decision-модели для bounded routing и tool-risk
    assessment, не передавая им authority.
11. Поддерживать bounded retries, budgets, WIP limits и human escalation.
12. Переживать restart, lease expiry, повтор событий и потерю Agent.
13. Собирать множество immutable вложенных flow из небольшого закрытого набора
    исполнимых и orchestration-примитивов без изменения Factory core.
14. Отделять control dependencies от context/data authority, чтобы порядок
    выполнения не раскрывал predecessor inputs, protected tests или transcript.
15. Использовать один механизм для triage, требований, test-first разработки,
    независимой проверки, delivery и policy-governed production promotion.

### Не-цели первого релиза

- автоматический merge любых изменений;
- замена существующего CI/CD mode или обязательное прохождение Build через
  Factory Controller;
- dynamic infrastructure provisioning;
- одновременная поддержка всех ОС и sandbox backends;
- использование LLM judge как источника фактов;
- разрешение judges изменять проверяемый код;
- выдача coding harness широких forge credentials;
- превращение Octa в backlog scheduler или merge controller;
- зависимость correctness от сохранённой model session.
- привязка Factory domain к JEV, OpenAI Decisions API или wire format одного
  decision-model provider;
- разрешение decision-модели расширять whitelist, создавать произвольный route
  или пропускать обязательный gate.
- unrestricted workflow scripting, provider-authored graph edges и unbounded
  recursion/repeat/fan-out;
- обязательный внешний GitHub/GitLab ticket connector в первом manual-intake
  slice: polling/webhooks подключаются позднее через Work Source seam.

## 3. Текущий фундамент и граница реализации

На 2026-10-06 OctaCity уже владеет нижним execution-слоем: versioned Project,
Pipeline, Repository и Build Configuration, нормализованные Trigger occurrences,
immutable Build и Attempt, Jobs, DAG Orchestrator, Placement Scheduler, fenced
Lease, Agents и Agent Pools, signed JobSpec v1/v2, Artifact Store, cache sessions,
secret grants, audit, retention, observability и release-qualified execution
backends. Management REST и Operator Console позволяют запускать, отменять и
повторять Builds, диагностировать Jobs, outputs, capacity и audit evidence.

Octa уже предоставляет task DAG, headless `octa-runner`, versioned protocol,
process plugins, JSON Schema, structured outputs, progress, cancellation,
artifacts, reports и digest-pinned `Octa.lock`.

В закреплённом Octa release `v0.5.1` с source revision
`e0a3c65fe010220c5a8162c52368b7c8f624ddb5` реализован и упакован официальный
`octa_plugin_codex`. Он запускает Codex CLI
как обычный Octa task, использует строгую schema, проверяет совместимую версию
CLI (сейчас exact `0.161.0`) и явно выбранный absolute executable, передаёт
только явно выбранное environment, завершает всё дерево процессов, санитизирует
JSONL events и публикует versioned trace, result и provenance через обычные Octa
artifacts/reports. Plugin намеренно не добавляет Codex-specific JobSpec, runner
events или OctaCity API.

OctaCity проверяет exact `codex-compatibility.json`, `Octa.lock`, plugin manifest,
plugin executable digest, release checksum inventory и embedded build revision.
Local-stand inputs отдельно закрепляют release/source archive digests, MIT
license и GitHub build-provenance contract. Этот foundation только добавляет
opt-in `codex` task к обычным Pipelines; он не включает Dark Factory и не
изменяет lifecycle существующих Builds.

### 3.1 Матрица готовности

| Область | Состояние | Фактическая граница |
| --- | --- | --- |
| Build execution substrate | Реализовано | Exact revision, immutable configuration snapshots, signed JobSpec, placement, fenced lifecycle и generic result publication работают end-to-end |
| Isolation и resource policy | Реализовано для квалифицированных backends | v2 target фиксирует mode, platform и required guarantees; containerd, Microsandbox, Apple VF и Native имеют отдельные contracts без fallback на более слабый mode |
| Network, secret и identity restrictions | Реализован Factory foundation | JobSpec v3, permission intersection, placement, Agent/backend enforcement, protected tool-action gate и fenced Codex authorization broker имеют positive/negative contracts; полная generic-flow qualification остаётся частью released pilot |
| Codex execution в Octa | Реализовано и закреплено | Официальный `codex` plugin, blocking `PreToolUse` hook, Agent broker, fixtures, conformance и release metadata поставляются в pinned Octa `v0.5.1` |
| Source intake | Реализован manual foundation | Provider-neutral Work Envelope, manual admission, exact replay, PostgreSQL authority и Project/configuration visibility реализованы; REST и внешние polling/webhook Work Source adapters ещё впереди |
| Factory Controller | Реализован fixed-stage core и persistence foundation | Factory Run, Stage Attempts, budgets, WIP, fenced reconciliation, operator retry authorization, ordinary-Build links и terminal provenance работают в pure core, in-memory и PostgreSQL paths; переход к immutable nested Flow Definitions ещё не выполнен |
| ChangeSet capture | Реализованы capture, acceptance и exact rematerialization | Agent создаёт hook-free commit/bundle/manifest, server принимает только verified generic outputs, а поздние Builds воспроизводят candidate от исходного base без внешней ветки; crash/retry completion ещё не завершён |
| Evaluation Plane | Частично реализовано | Provider-neutral ChangeSet, Evidence Manifest, Evaluation Plan, Assessment и deterministic Decision Engine реализованы в core; trusted output projection, evaluators и durable production flow ещё не подключены |
| Decision Signal Plane | Реализован foundation | Provider-neutral typed request/result/receipt, exact replay, deadlines/budgets, rollout/fallback policy, persistence, registry, fake conformance, bounded JEV adapter и tool-risk exchange реализованы; generic-flow runtime wiring и calibration rollout ещё впереди |
| Delivery | Не реализовано | VCS reads и revision resolution существуют; write-capable branch/PR/merge adapter отсутствует |
| Factory operator UX | Не реализовано | Console управляет Projects, Builds, Agents и audit, но не показывает Factory Runs, evaluation rounds или delivery decisions |

Dark factory строится поверх реализованного execution substrate и не заменяет
существующие Orchestrator, Scheduler, Agent, REST control plane или Octa runtime.
Следующая архитектурная граница — PostgreSQL-backed production composition и
execution protocols; новые Job backends для этого не требуются.

## 4. Главные архитектурные решения

### 4.1 CI/CD и Dark Factory являются двумя first-class режимами

CI/CD остаётся базовой возможностью продукта. Пользователь может создавать
Project, Pipeline, Repository и Build Configuration, запускать Build из Trigger
или Operator Console, наблюдать DAG, artifacts и результаты, не создавая
Factory Configuration и не подключая LLM provider.

Dark Factory является opt-in application layer над теми же Build APIs. Он не
ветвится внутри Scheduler и не добавляет специальный тип Agent. Factory stage
создаёт обычный Build с immutable configuration snapshot; дальнейшее placement,
lease fencing, execution, cancellation, retry, artifacts и audit выполняются
существующим CI/CD контуром. Поэтому развитие Factory не должно менять семантику
обычного Build и не должно делать доступность model provider условием готовности
CI/CD control plane.

### 4.2 Управляющий flow исполняется кодом, а не выбирается LLM

Архитектура следует модели Agentic Programming / LLM-as-Code из статьи
[LLM-as-Code: Agentic Programming for Agent Harness](https://arxiv.org/html/2606.15874v1):
детерминированные loop, branch, sequence, retry, timeout, budget, join и stop
conditions принадлежат программе. LLM вызывается только в узлах, где нужны
понимание, генерация или оценочное суждение.

Factory Controller, Octa task DAG и Decision Engine образуют программный
control flow. Harness может свободно рассуждать и пользоваться разрешёнными
tools внутри bounded call, но не может пропустить обязательную validation stage,
самостоятельно объявить Factory Run доставленным или изменить policy переходов.

Программный control flow не означает один hard-coded pipeline. Operator
публикует immutable versioned Flow Definition из закрытого набора примитивов:
обычный Build/command, reasoning call, Decision Signal, deterministic gate,
bounded fan-out/join, human gate, trusted action и exact nested subflow call.
Interpreter создаёт Node Attempts только по declared edges и проверяет общие
limits глубины, node count, repeat, fan-out, WIP и budget. Так ticket triage,
requirements, development и verification могут меняться как конфигурация, не
разрастаясь в `FactoryStageKind` и новые ветви domain-кода.

Control dependency и context/data dependency являются разными рёбрами. Узел
может ждать predecessor, но не получать его prompt, inputs, outputs, mounts или
Artifacts. Это позволяет скрыть написанные заранее tests от coding node,
изолировать requirements author от reviewer и запускать black-box verifier без
source authority. Отсутствие context обеспечивается Context Manifest, Artifact
grants и mounts, а не обещанием в prompt.

Длительная работа представляется durable DAG вызовов и стадий. Каждый reasoning
call получает immutable Context Manifest с необходимым ancestor context,
Stage Handoffs и typed результатами завершённых ветвей, а не бесконечно растущий
общий transcript. Полный trace сохраняется как evidence для audit/replay, но не
обязан целиком возвращаться в model context.
Параллельные reviewers являются sibling calls, а их join и failure policy
остаются детерминированным кодом. Self-improvement публикуется как обычный
ChangeSet и проходит те же tests/evaluation/delivery gates; модель не изменяет
рабочий workflow скрытым состоянием.

### 4.3 Decision-модель возвращает сигнал, а не исполняет решение

Для узких вероятностных задач OctaCity вводит provider-neutral
`DecisionSignalProvider`. Первый adapter использует JEV. Позже OpenAI Decisions
API или другой сервис может быть подключён отдельным adapter без изменения
Factory domain, когда его официальный public contract и probability semantics
станут стабильными.

Decision Signal применяется только в двух seams:

1. routing между конечным набором заранее объявленных outgoing edges;
2. tool-risk assessment для уже нормализованного действия, которое прошло
   Factory Permission Set, Task Envelope, Project policy, локальную Agent policy
   и backend capability checks.

```text
authoritative state + declared choices
  -> canonical redacted Decision Signal Request
  -> selected provider/model (JEV сейчас, другой adapter позже)
  -> typed answers + probability/confidence
  -> immutable Decision Signal Receipt
  -> deterministic policy
  -> declared route | unchanged allowed action | deny | escalate
```

Signal не может создать новый stage, пропустить mandatory gate, увеличить
budget, расширить command/path/network/secret whitelist или принять финальный
Evaluation Decision. Agent и backend остаются последней enforcement boundary.
Provider outage, invalid output и недостаточная confidence приводят только к
явному fail-closed fallback. Retry использует сохранённый receipt и не вызывает
другую модель для того же logical decision.

Для `tool_risk` недостаточно post-factum telemetry: выбранный pinned harness
adapter обязан иметь blocking pre-execution hook. Target flow требует, чтобы
trusted exact-action adapter передал canonical proposal в Agent-local broker,
связанный с текущими Job, lease и fence. Broker применяет signed hard policy и
отправляет server-side Decision Signal adapter только redacted ambiguous
in-envelope request. Provider credential остаётся на server; workload ждёт
deterministic disposition, после чего Agent/backend повторно проверяют
неизменённое действие. Timeout, cancellation, stale fence, broker loss или
receipt mismatch означают deny. Сейчас shared gate и redacted exchange покрыты
component tests, но Agent-local helper/IPC и exact Codex mapping ещё не
release-qualified, поэтому bounded control не рекламируется и не включается.

Каждый provider/model и каждая purpose (`routing`, `tool_risk`) имеют отдельные
versioned questions, thresholds, margin rules и calibration corpus. Rollout
проходит через `shadow`, `advisory` и только затем `bounded_control`. Общая
абстракция нормализует typed choice/score/yes-no result, но не притворяется, что
probability разных моделей одинаково откалибрована.

### 4.4 Factory Controller находится над Builds

Build является immutable execution request с точной source revision. Dark
factory меняет код между стадиями, поэтому весь lifecycle нельзя поместить в
один Build или существующий DAG Orchestrator.

```text
Factory Run at pinned root Flow Definition
  -> Triage subflow
  -> optional Requirements subflow -> specification candidate S
  -> Test-authoring node (protected bundle T)
  -> Implementation Build at S (T absent from context)
  -> Validation/Review/Verification subflows at candidate C
  -> trusted Delivery/Promotion through ordinary CI/CD
```

Orchestrator продолжает двигать Jobs внутри Attempt. Factory Controller
принимает меж-Build решения.

### 4.5 OctaCity Agent не запускает Codex напрямую

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

Эта граница уже доказана `octa_plugin_codex`: OctaCity видит только обычные
runner events, artifacts, reports и outputs. Agent и server не импортируют
Codex types и не знают его machine protocol. Пока существует один production
harness, отдельный universal `coding` abstraction не вводится; общий seam
выделяется только после появления второго реально интегрированного harness.

### 4.6 Implementation и evaluation разделены

```text
octa_plugin_codex (сейчас) / другой implementation plugin (в будущем)
  workspace: writable
  purpose: implement or repair

octa_plugin_evaluator
  workspace: read-only
  purpose: assess immutable candidate
```

После появления нескольких реализаций они могут переиспользовать доказанную
общую process/result library, но evaluator не получает право менять код.

### 4.7 Exact commit является identity результата

Нельзя проверять плавающую branch. Checks и assessments привязаны к точному
candidate commit SHA, ChangeSet digest, evidence digest и policy version. Любое
изменение кода создаёт новую Evaluation Round.

### 4.8 LLM не принимает merge decision

LLM возвращает schema-validated Assessment. Детерминированный Decision Engine
создаёт `Accept`, `Rework`, `Escalate`, `Reject` или `Cancel`.

## 5. Канонические термины

**CI/CD mode** — существующий самостоятельный lifecycle Trigger/command ->
Build -> Attempt -> Jobs -> Result. Он не является сокращённым Factory Run.

**Dark Factory mode** — opt-in lifecycle, который координирует несколько Builds
для реализации, проверки, оценки и доставки изменения.

**Work Source** — источник потенциальной работы: tracker, webhook, REST, CLI,
queue, schedule, файл, OpenSpec change или другой Pipeline.

**Work Submission** — недоверенное сообщение до нормализации и admission.

**Work Envelope** — immutable provider-neutral описание принятой работы:
identity, subject, repository reference, задача, acceptance criteria, priority,
risk и specification artifact references.

**Work Reporter** — опциональный outbound adapter для статусов и результатов.

**Factory Configuration** — versioned admission policy, exact root Flow
Definition, budgets, connector selectors, evaluation, delivery и promotion
policy.

**Factory Run** — durable lifecycle одной принятой работы.

**Flow Definition** — immutable versioned typed graph с entry, outcomes,
declared transitions, failure routes, budgets, permissions, Context projections
и exact nested subflow versions. Admitted run закрепляет всю reachable closure.

**Flow Run** — durable invocation одного Flow Definition внутри Factory Run;
nested subflow создаёт child Flow Run с exact parent node identity.

**Flow Node** — один узел закрытого набора: ordinary Build/command, reasoning,
Decision Signal, deterministic gate, bounded fan-out/join, human gate, trusted
action или exact subflow call.

**Node Attempt** — одна bounded fenced попытка Flow Node; retry не переписывает
историю.

**Workflow Cycle** — append-only цикл требований или реализации, связывающий
exact predecessor/candidate lineage. Возврат к требованиям создаёт новый cycle,
а не переписывает прошлое.

**Stage Handoff** — immutable schema-validated межэтапный результат с outcome,
bounded summary, влияющими на candidate решениями и assumptions, unresolved
items, changed-component references, validation observations, findings и
точными Artifact/ChangeSet/Evidence/provenance references. Он не содержит
credentials, hidden chain-of-thought или полный transcript.

**Task Envelope** — immutable input Job: задача, revision, permissions, budget,
policy versions и ожидаемые outputs.

**Context Manifest** — канонический immutable список точных входов reasoning
call. Каждая ordered entry содержит source kind, logical identity, revision или
subject binding, Artifact либо repository range reference, content digest,
размер, inclusion reason и provenance; digest манифеста является частью Task
Envelope и call node.

**Coding Harness** — Codex, Claude или другой исполнитель LLM coding session.
Его не следует называть Agent: Agent уже означает execution worker OctaCity.

**Harness Adapter** — provider-specific реализация стабильного plugin interface.

**Reasoning Call** — bounded invocation модели с immutable input и typed output;
модель не владеет переходом Factory lifecycle.

**Call DAG** — durable структура function/reasoning calls. Active call видит
scoped ancestor context, а завершённая child branch возвращает typed result и
summary; полный trace остаётся evidence, а не общим model context.

**Control edge** — зависимость готовности: successor ждёт outcome predecessor,
но не получает его данные автоматически.

**Context/data projection** — отдельное явное право включить bounded typed
result, Artifact, repository range, mount или summary в successor input.

**Phase-ready pool** — durable projection Flow Runs, готовых к triage,
requirements, development, verification или promotion; выбор определяется
детерминированной severity/priority/age/dependency/WIP/budget policy.

**Decision Signal Provider** — взаимозаменяемый adapter узкой decision-модели,
который принимает bounded canonical state и versioned typed questions и
возвращает один из заранее объявленных answers или score с probability/confidence.
JEV является первым provider; будущий Decisions API подключается через тот же
application port, а не становится новым Factory domain concept.

**Decision Signal Request** — immutable provider-neutral запрос с purpose,
subject/policy digests, finite answer domain, deadline, budget и redacted state.

**Decision Signal Receipt** — immutable replay/audit запись exact provider,
adapter, model, question set, policy и input digests, typed answers,
probability semantics, usage, terminal classification и применённого кодом
disposition.

**Routing Assessment** — Decision Signal, который может выбрать только один из
predeclared outgoing edges текущего Factory state.

**Tool Risk Assessment** — Decision Signal для уже разрешённого permission
intersection действия; он может привести к unchanged allow, deny или escalation,
но не расширяет authority.

**Repository Knowledge** — отдельный non-authoritative discovery subsystem для
revision-bound hybrid symbol, lexical, dependency-graph и vector retrieval по
коду и документации. Он помогает найти context, но не заменяет Git, Stage
Handoff, Evidence Manifest, policy или Decision Engine.

**Retrieval Receipt** — immutable описание одного retrieval: exact repository
revision, index/embedding identities, normalized query, retrieval policy,
stable ranking и ranges/digests возвращённых fragments.

**ChangeSet** — immutable predecessor/candidate relationship, purpose
(requirements, tests, implementation или repair), commit либо bundle/patch,
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

| Место | Durable role |
| --- | --- |
| Git / ChangeSet Store | код и immutable revisions |
| PostgreSQL OctaCity | Factory Configuration, Flow Definitions/Runs, Node Attempts, Workflow Cycles, leases, budgets, candidate lineage, Stage Handoffs, Context Manifest metadata/digests, decisions |
| Artifact Store | task/spec bodies, Context Manifest payloads, handoffs, logs, reports, evidence, assessments, transcripts, ChangeSets |
| Repository Knowledge | revision-bound non-authoritative indexes и immutable Retrieval Receipts; не lifecycle truth |
| Work Source | внешний identity и best-effort status projection |
| Worktree | ничего незаменимого |

Другой Agent должен продолжить работу, имея только Factory Run state, exact
commit, Stage Handoffs, Context Manifest и Task Envelope digests,
policy/connector versions, artifact references и current fenced Lease. Session
resume допустим только как оптимизация.

## 8. Программируемый nested-flow lifecycle

Factory lifecycle не является одним конечным автоматом со стадиями
`implementation/validation/evaluation`. Factory Configuration закрепляет root
Flow Definition и immutable closure всех referenced subflows. Interpreter
материализует durable Flow Runs и Node Attempts; definition после admission не
может измениться под выполняющимся run.

```text
Factory Run
  root Flow Run
    triage subflow
      duplicate? -> project fit? -> classify severity/size/risk
      -> reject | escalate | requirements-ready | development-ready
    optional requirements subflow
      author -> independent review -> human/deterministic gate
    development subflow
      protected tests -> isolated implementation -> validation -> code review
    verification subflow
      e2e + performance + UI + security + staged deployment
    delivery/promotion subflow
      review target -> human/policy gates -> existing CI/CD deployment
      -> bounded post-deployment verification
```

Это пример versioned configuration, а не перечень обязательных domain stages.
Другой Project может заменить, вложить, убрать или переставить optional nodes,
если graph проходит schema, reachability, cycle и resource validation.

Исполнимый набор намеренно закрыт:

1. ordinary Build/command node;
2. reasoning node через pinned Octa plugin, сначала Codex;
3. Decision Signal node через JEV или другой совместимый provider;
4. deterministic gate;
5. bounded fan-out/join;
6. human gate;
7. trusted side-effect action;
8. exact nested subflow call.

Definition не содержит unrestricted script и не позволяет provider создать
новый edge. Loop выражается bounded repeat с явным максимумом. Recursive
subflow closure, unbounded fan-out/depth/node count/budget/WIP и graph без
terminal outcome отклоняются до publication.

Каждый переход основан на persisted observation. Таймер только будит
reconciler. Attempts и Workflow Cycles append-only. Lease fencing исключает
двух владельцев. Requirements Defect создаёт новый requirements cycle и делает
downstream evidence старого specification candidate непригодным для promotion.
Delivery и deployment разрешены только для exact accepted lineage.

### 8.1 Triage и phase-ready pools

Triage является обычным nested flow: duplicate search, соответствие целям
Project, category, severity, size, risk и dependencies могут быть отдельными
deterministic, reasoning или Decision Signal nodes. Их typed outputs сводятся
в immutable Triage Result; только policy переводит Work в reject, escalation,
requirements-ready или development-ready.

Ready-state хранится как authoritative projection. Scheduler выбирает работу
детерминированно по severity, Project priority, age, dependency readiness,
capability, WIP и budget. Внешний label может отображать это состояние, но не
является его источником.

### 8.2 Requirements и append-only correction

Small Work может по policy миновать authoring. Large Work проходит отдельные
author и reviewer calls; reviewer не получает author transcript. Принятая
спецификация фиксируется exact ChangeSet и только после gates становится
implementation-ready.

Если implementation или verification возвращает typed Requirements Defect,
run начинает новый requirements cycle с bounded defect evidence. Спецификация
исправляется и проходит review заново; предыдущие code/test/review results
сохраняются для audit, но не могут авторизовать новый cycle.

### 8.3 Protected test-first development

Test-authoring node создаёт immutable protected test bundle. Coding node имеет
control dependency на его завершение, но не context/data edge: bundle, source,
assertions, expected outputs и transcript отсутствуют в его Task Envelope,
mounts и Artifact grants. Trusted validation node получает exact candidate и
exact test bundle и публикует Evidence Manifest. Это предотвращает подгонку
implementation под известные hidden tests и доказывает полезность разделения
control и context graphs.

### 8.4 Independent verification и production promotion

Verification flow может включать e2e, performance, UI, security, staging и
bounded post-deployment checks. Каждый node получает собственную source
visibility, credentials и environment authority; black-box verifier может не
видеть ни source, ни implementation transcript. Trusted action вызывает уже
существующий CI/CD Pipeline/Build для deployment. Reasoning и Decision Signal
nodes не получают forge-write или production credentials. Долгий production
monitoring остаётся внешней системой и при проблеме создаёт новое Work, которое
проходит обычный admission.

## 9. Coding execution через Octa plugin

### 9.1 Реализованный Codex task

Текущая реализация в Octa использует официальный provider-specific task key
`codex`. Это обычный Octa task, а не специальный OctaCity backend:

```yaml
codex:
  prompt_file: .octa/factory/implement.md
  model: gpt-codex
  reasoning_effort: high
  result_schema:
    type: object
    properties:
      status:
        type: string
        enum: [completed, blocked, needs_input, budget_exhausted, failed]
      summary:
        type: string
    required: [status, summary]
    additionalProperties: false
  environment:
    secret:
      OPENAI_API_KEY: CODEX_AUTH
  source_revision: 8de7f1c
  deliverables:
    - kind: report
      name: coding-result
      path: .octa/factory/coding-result.json
      format: octa.codex.result.v1
```

`octa_plugin_codex` уже владеет проверкой совместимой Codex CLI, построением
минимального child environment, process-tree supervision, bounded sanitized
JSONL, cancellation/timeout, schema validation и публикацией versioned
`trace.jsonl`, `result.json` и `provenance.json`. Он не владеет Factory Run,
source admission, worktree creation, sandbox policy, VCS push/PR/merge,
delivery decision или меж-Build retries.

OctaCity остаётся provider-neutral не потому, что сейчас существует фиктивный
универсальный `coding` task, а потому, что provider boundary заканчивается в
Octa plugin. Выше этой границы OctaCity получает только generic runner events,
artifacts, reports, outputs и terminal status. Общий `CodingHarness` interface
следует выделять лишь после второго production implementation, когда Codex и,
например, Claude докажут фактический общий seam. До этого предпочтительнее
конкретный глубокий module, чем преждевременная abstraction.

### 9.2 Factory Task Envelope

Task Envelope фиксирует protocol version, identities, mode, exact revision,
task/spec artifacts, previous findings, permissions, network policy, secret
handles, budgets, expected output schema и prompt/policy digests.

Factory Controller не передаёт Task Envelope в Codex как право управлять
lifecycle. Trusted adapter детерминированно компилирует envelope в immutable
Octa input: prompt artifact, result schema, explicit environment mappings,
source revision, deliverables и execution policy. Неподдерживаемое поле или
невозможность выразить требуемое ограничение приводит к admission failure.

Permissions являются typed, deny-by-default контрактом, а не свободной map. Они
отдельно задают разрешённые tools/commands, read/write filesystem roots, mount
modes, network hosts, secret profiles, workload identity и output capabilities.
Идентичность executable или plugin фиксируется digest/version; разрешение tool
не означает разрешение произвольной команды, аргументов, working directory или
дочернего процесса.

Effective permissions вычисляются только как пересечение Factory Policy, Task
Envelope, локальной Agent policy и реально обеспечиваемых backend capabilities.
Каждый слой может только сузить полномочия. Неизвестная permission, отсутствующая
локальная grant или невозможность backend'а обеспечить требуемое ограничение
отклоняет Job до запуска; переход на Host или другой более слабый backend
запрещён.

### 9.3 Межэтапная память и Repository Knowledge

Завершённая model-backed Node Attempt публикует Stage Handoff. Следующий call
не восстанавливает состояние по transcript или mutable provider session:
Factory Controller канонически собирает Context Manifest только из declared
context/data projections, обязательных exact artifacts/evidence и явно выбранных
typed child results или bounded summaries. Control edge определяет только
готовность и сам по себе не передаёт данные. Stable ordering, content digests,
размеры, inclusion reasons и provenance делают manifest воспроизводимым; retry
использует сохранённый manifest, а не повторяет discovery.

```text
Stage Handoffs --------+
Exact artifacts -------+--> Context Manifest --> Task Envelope --> reasoning call
Evidence --------------+
Repository fragments --+    (optional, frozen with Retrieval Receipt)
```

Repository Knowledge решает другую задачу: поиск дополнительного релевантного
кода и документации в большом exact repository revision. Предпочтителен hybrid
retrieval: symbol и exact search, lexical/BM25, dependency/call graph, vector
similarity и deterministic reranking. Его результат становится входом call
только после фиксации concrete paths/ranges, bytes digests, stable order и
Retrieval Receipt в Context Manifest. Изменение индекса, embedding model или
ranking policy не меняет уже dispatched call.

Repository fragments считаются недоверенным текстом и могут содержать prompt
injection. Retrieval применяется после Project visibility и никогда не может
заменить required Stage Handoff или deterministic evidence, расширить
permissions, выбрать lifecycle transition или передать implementation
transcript независимому evaluator. Cross-Project и cross-run knowledge по
умолчанию изолированы.

Первый Dark Factory slice использует exact references, typed handoffs и bounded
workspace tools concrete harness. Полноценные indexes, ingestion, hybrid/vector
retrieval, обновление по revision, retention и operator diagnostics относятся
к отдельному Repository Knowledge change. До появления concrete implementation
не создаётся пустой retrieval port; versioned Context Manifest и Retrieval
Receipt являются совместимой точкой будущей интеграции.

Сегодня применимая основа уже включает allowlist Octa task/plugin identities,
execution target и required guarantees, resource bounds, restricted network
hosts, logical secret/workload-identity profiles и output limits. Factory-level
contract для tool identities, command arguments, filesystem read/write roots,
mounts, descendants и outputs реализован вместе с blocking Codex authorization
broker. Перед unattended enablement его ещё требуется квалифицировать в полном
nested-flow released pilot, включая context-absence contracts.

Coding result содержит status, changed-path summary, summary, usage,
diagnostics, trace и provider provenance. Изменения остаются в sandbox до
trusted ChangeSet capture. Model result является typed return value одного
reasoning call, но не authority на следующий control-flow transition.

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
6. Связывает ChangeSet с exact predecessor, candidate purpose, Node Attempt и
   Workflow Cycle.
7. Загружает artifacts/reports.
8. Уничтожает worktree.

Coding harness не получает постоянный forge write credential. Delivery Adapter
проверяет accepted candidate, ancestry, gates и policy; затем публикует branch и
создаёт/обновляет PR. Merge и deployment являются отдельными trusted actions,
разрешёнными только immutable policy/human gates; deployment переиспользует
существующий CI/CD Pipeline/Build и не выдаёт production credential модели.

Read-only VCS и write-capable delivery protocols следует разделить.

## 11. Validation и Evaluation Plane

```text
Evidence Producers -> Evaluators -> Decision Engine
       facts            opinions       authority
```

Decision Signal Plane не подменяет эту цепочку. `Routing Assessment` и
`Tool Risk Assessment` отвечают на bounded operational questions до Evaluation
Decision и сохраняются отдельно. Они не являются candidate Assessment,
Evidence или основанием для merge.

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
4. Admission разрешает exact immutable base revision, создаёт ровно один
   Factory Run и закрепляет root Flow Definition со всей subflow closure.
5. Interpreter создаёт triage Flow Run. Команды, Codex calls и optional JEV
   signals возвращают typed observations; deterministic policy выбирает только
   declared outcome и phase-ready pool.
6. Small Work может перейти прямо к development. Large Work проходит isolated
   requirements authoring и independent review; accepted spec становится exact
   candidate lineage.
7. Test-authoring Node Attempt создаёт protected bundle. Implementation ждёт
   его по control edge, но не получает bundle или transcript в context.
8. Agent создаёт disposable sandbox/worktree и запускает ordinary Build через
   octa-runner; каждый tool action проходит permission gate и optional bounded
   Tool Risk Assessment.
9. Trusted capture создаёт exact candidate ChangeSet и уничтожает worktree.
10. Validation materializes exact candidate вместе с exact protected tests и
    публикует deterministic Evidence Manifest.
11. Independent code review и verification subflows выполняют declared
    fan-out/join с отдельными Context Manifests и authority.
12. Requirements Defect создаёт новый append-only requirements cycle; code или
    test defect следует своему declared bounded rework edge.
13. Decision Engine применяет exact evidence, Assessments и gates. Model result
    не становится transition.
14. Trusted Delivery Adapter публикует exact accepted spec/code candidate и
    создаёт review target с observe-before-retry.
15. После human/policy gates trusted promotion action запускает существующий
    deployment Pipeline/Build; bounded post-deployment checks возвращают typed
    evidence.
16. Work Reporter best-effort публикует status projection, а retention policy
    очищает disposable и просроченные artifacts.

Пункты 5–15 описывают один целевой flow. Их состав и вложенность могут меняться
immutable configuration, но node kinds, authority boundaries, exact lineage,
budgets и deterministic transition ownership остаются неизменными.

## 13. Целевое module ownership и текущая реализация

Core contracts и in-memory coordination уже реализуются в
`octacity-server-factory`, `octacity-server-store` и application use cases.
Остальные названия описывают целевое владение и не означают существующий crate.
Эти модули дополняют CI/CD, а не заменяют Build application, Orchestrator,
Scheduler, Agent protocol или management API.

| Module | Owns | Explicitly does not own |
| --- | --- | --- |
| `octacity-server-factory` | Factory Configuration, immutable Flow Definition/Run, Node Attempt, Workflow Cycle, typed transitions, budgets, candidate lineage, evaluation и Decision Signal contracts | Job placement, harness execution, provider SDKs, unrestricted workflow scripting |
| `octacity-server-work` | Work Envelope, admission, deduplication | Provider payloads, tracker SDKs |
| `octacity-server-evaluation` | Evaluation Round, Plan, Assessment, Decision | Model calls, scanners, repo execution |
| `octacity-server-delivery` | Delivery/promotion policy and trusted-action commands | Forge/deployment SDKs, repo execution, model decisions |

`DecisionSignalProvider` начинается как узкий application port рядом с Factory
use cases, а canonical request/result/receipt принадлежат deep factory core.
JEV HTTP types живут только в infrastructure adapter. Отдельный protocol crate
создаётся лишь если появится реальная out-of-process distribution boundary;
будущий OpenAI Decisions API adapter должен реализовать тот же conformance
contract, но сохраняет собственные wire types и calibration semantics внутри.

Provider-neutral protocols:

- `octacity-work-source-protocol`;
- `octacity-work-reporter-protocol`;
- `octacity-delivery-protocol`;
- `octacity-evaluator-protocol`.

Decision Signal boundary provider-neutral, но не обязана немедленно становиться
отдельным process protocol: сначала JEV adapter и второй fake adapter доказывают
стабильность seam без пустого пакета.

Octa plugins:

- `octa_plugin_codex` — реализованный и закреплённый в OctaCity writable
  implementation/rework task;
- будущий `octa_plugin_evaluator` — read-only structured Assessment;
- существующие shell/test plugins — deterministic evidence.

Универсального `octa_plugin_coding` и shared `octa-harness-adapters` сейчас нет.
Они не вводятся только ради симметрии: общий internal interface появляется после
интеграции второго production harness и включает лишь реально совпавшие
process/result semantics. Provider-specific configuration остаётся внутри
соответствующего Octa plugin.

## 14. Security and trust model

Недоверенными являются Work Submission, repository contents, filenames,
symlinks, build scripts, OpenSpec/README/comments, harness/evaluator output,
tool reports, URLs и provider metadata.

Основные правила:

- repository text передаётся как evidence, не instructions;
- system rubric хранится вне repo и фиксируется digest;
- permissions enforced кодом, а не prompt;
- effective permissions являются пересечением Factory Policy, Task Envelope,
  локальной Agent policy и backend capabilities;
- tools/commands, executable identity, arguments, child-process policy,
  filesystem roots, mount modes, network hosts, secret profiles и output
  capabilities проверяются до spawn и при каждом защищённом действии;
- backend, не способный обеспечить требуемую permission, завершает admission
  fail-closed без fallback на более слабый execution mode;
- evaluator по умолчанию не имеет shell/write/arbitrary network;
- LLM output schema-validated и никогда не исполняется напрямую;
- Decision Signal получает только bounded redacted state и finite answer domain;
- routing signal не создаёт edge и не пропускает mandatory gate;
- tool-risk signal вызывается только после permission intersection и hard-deny
  checks, может сузить решение, но не расширить whitelist;
- exact provider/model/question/policy/input закрепляются в receipt; timeout,
  invalid result и low confidence используют `deny` или `escalate`;
- provider credentials server-side и не доступны repository, harness, Agent
  workload или Operator Console;
- tool proposal блокируется trusted hook до fenced disposition; отсутствие hook,
  broker или matching receipt означает deny, а не best-effort execution;
- secrets и cross-tenant data не попадают в context;
- model credentials short-lived, scoped и redacted;
- delivery credential доступен только trusted adapter;
- filesystem operations ограничены resolved owned workspace root;
- plugins, harness runtime, image, prompts и policies pinned digests;
- implementation harness не является единственным reviewer;
- deterministic gates имеют приоритет;
- required indeterminate никогда не становится pass;
- auto-merge только opt-in для allowlisted low-risk policy.

OctaCity Agent остаётся доверенным execution worker и может владеть
привилегированным доступом к containerd, KVM или Virtualization.framework.
Изоляционной границей продукта является Job/coding harness, а не процесс Agent.
Hardening и выделенная машина/VM для самого Agent относятся к deployment
profile; backend sockets и host devices никогда не передаются workload.

### 14.1 Что whitelist означает в текущей реализации

Уже enforced кодом, а не prompt:

- разрешённые Pool и execution targets для Project;
- exact plugin/toolchain identities и signed JobSpec;
- execution mode, platform и required isolation guarantees;
- CPU, memory, disk, process и output bounds;
- `disabled`, `unrestricted` или `restricted` network policy с
  `allowed_hosts`;
- заранее объявленные secret и workload-identity profiles;
- отказ от запуска без перехода на более слабый backend;
- typed Factory Permission Set для tools, commands/arguments, descendants,
  read/write roots, mounts, hosts, identities, resources и outputs;
- signed managed JobSpec v3, protected inputs и backend capability matching;
- blocking Codex tool proposal, fenced Agent broker, server-side Decision
  Signal exchange и повторная Agent/backend проверка неизменённого действия.

Внутренний sandbox Codex полезен как defense in depth, но внешней security
boundary остаётся OctaCity execution backend. Unattended mode включается только
после generic nested-flow и released-pilot negative contracts для каждой
используемой категории permission и context isolation.

Опциональный Tool Risk Assessment не заполняет этот gap и не считается
enforcement boundary. Порядок неизменяем:

```text
normalize proposal
  -> Factory Permission Set ∩ Task Envelope ∩ Project/Agent/backend policy
  -> deterministic hard deny / hard allow
  -> optional Decision Signal только для in-envelope ambiguity
  -> deterministic disposition
  -> Agent/backend revalidation and enforcement
```

## 15. Failure, retry и fencing

- Worker loss: lease expiry fences owner; другой Agent materializes exact state.
- Duplicate intake: scoped dedup identity возвращает существующий Factory Run.
- Stale completion: принимается только current lease/node/cycle/subject digest.
- Connector outage: bounded retry; exhaustion required connector -> Escalate.
- Decision Signal lost response: observe/reuse stable request receipt; не
  re-query другой model; невозможность доказать результат -> Deny/Escalate.
- Decision Signal provider/model drift: новые calls требуют новой immutable
  Factory Configuration; active runs сохраняют exact identity и policy.
- Partial evidence: required evaluation не становится accepted.
- Delivery lost response: stable idempotency key и observe-before-retry.
- Rework: новый ChangeSet/Evaluation Round, bounded attempts/time/tokens/cost.

Multi-instance correctness хранится в authoritative store. Process-local timers
только будят workers. Все claims имеют owner, deadline и fence.

## 16. Observability и масштабирование

Audit фиксирует source identity, Factory transitions, exact revisions,
connector/plugin/image/policy digests, usage, lease fence, evidence freshness,
decision reasons, Decision Signal purpose/provider/model/question/input/policy
digests, rollout mode, deterministic disposition, delivery identity и
escalation cause.

Task text, paths высокой кардинальности, issue IDs, prompts, secrets и raw
findings не попадают в metric labels.

Масштабирование:

- WIP limits на Project/Factory Configuration;
- provider/model concurrency pools;
- раздельные coding/evaluation quotas;
- source admission rate limits;
- provider circuit breakers;
- отдельные decision-signal concurrency, latency и cost budgets;
- purpose/provider/model-specific calibration и drift monitors;
- fair scheduling;
- bounded evaluator fan-out.

Новый источник добавляет Work Source Adapter. Новый coding provider добавляет
Octa plugin и, только при наличии доказанного общего seam, Harness Adapter.
Новый аспект качества добавляет Criterion Pack/Evidence Producer. Новый delivery
target добавляет Delivery Adapter. Factory Controller не изменяется.

## 17. Минимальный безопасный пилот

Состояние предпосылок:

| Предпосылка | Состояние |
| --- | --- |
| Полный server -> Agent -> Octa lifecycle через PostgreSQL | Готово |
| Typed cancel/retry/diagnostics и fenced recovery | Готово |
| Квалифицированные isolation backends и запрет слабого fallback | Готово; pilot выбирает один поддерживаемый deployment profile |
| Generic Artifact/report/result contracts | Готово |
| Codex plugin conformance и release packaging в Octa | Готово upstream |
| OctaCity release, закрепляющий Octa с Codex plugin | Готово: Octa `v0.5.1`, revision `e0a3c65fe010220c5a8162c52368b7c8f624ddb5` |
| Factory Work/Task/ChangeSet/Assessment contracts и durable storage | Core contracts и in-memory history готовы; PostgreSQL и Task/Context schemas не готовы |
| Decision Signal provider seam и JEV adapter | Foundation готов и fail-closed; runtime composition/persistence и shadow calibration не готовы |
| Factory Permission Set и malicious-repository negative contracts | Частично; требуется закрыть gap из 14.1 |
| Trusted ChangeSet capture, evaluation и delivery | Не готово |

Первый slice:

```text
manual REST/CLI Work Submission
  -> Work Envelope
  -> Factory Run at pinned nested Flow Definition
  -> triage and phase-ready pool
  -> optional requirements author + independent reviewer
  -> protected tests -> isolated Codex implementation
  -> trusted ChangeSet capture and deterministic validation
  -> independent code/acceptance/security verification
  -> JEV routing/tool-risk shadow receipts (без влияния на execution)
  -> bounded requirements/code/test correction cycles
  -> PR creation -> human/policy gate
  -> deployment through existing CI/CD -> bounded post-deploy checks
```

Пилот использует manual REST/CLI admission через реальный provider-neutral Work
Source seam и один реальный Codex plugin; source, evaluator и delivery ports
проверяются также fake adapters в deterministic tests. Реальный polling/webhook
adapter для GitHub/GitLab и Claude не являются условием минимального pilot: они
входят в Phase E и позднее доказывают provider-neutral выбранных seams.

## 18. Этапы развития

### Foundation: CI/CD substrate и первый harness — выполнено

Завершены CI/CD Build/Attempt/Job lifecycle, PostgreSQL-backed orchestration,
Agents/Pools, signed JobSpec, isolation backends, artifacts/cache/secrets,
management REST, Operator Console и release/local-stand contracts. Официальный
Codex task/plugin входит в закреплённый Octa `v0.5.1`, а OctaCity проверяет его
release metadata, plugin lock, blocking-hook capability и совместимость CLI.

### Phase A: factory contracts — частично выполнено

Утвердить glossary; Work/Task Envelope; Factory Run/Node Attempt; ChangeSet; Evidence Manifest;
Assessment/Decision schemas; typed permission vocabulary; effective-permission
intersection; backend capability/admission matrix; plugin provenance; threat
model; provider-neutral Decision Signal request/receipt и rollout policy; ADR о
разделении Factory Run и Build. Обычный CI/CD path остаётся
неизменным и проходит regression contracts без factory configuration.

### Phase B: composable flow runtime и manual factory

Immutable nested Flow Definitions, durable interpreter, separate control/context
edges, triage, phase-ready pools, manual intake, `octa_plugin_codex`, trusted
ChangeSet capture и deterministic validation. Все loops, retries, joins и stop
conditions исполняются программно; model calls возвращают typed results. JEV
работает в shadow mode без изменения baseline flow.

### Phase C: requirements и protected test-first development

Conditional requirements, isolated author/reviewer, exact specification
ChangeSet, append-only Requirements Defect cycles, protected test authoring,
test-hidden implementation, independent code review и bounded rework.

### Phase D: verification, delivery и promotion

E2E/performance/UI/security/staged verification, trusted PR delivery, human and
policy gates, deployment через existing CI/CD и bounded post-deployment checks.

### Phase E: pluggability и unattended backlog

Claude plugin/adapter, второй Work Source, Work Reporter, multiple Criterion
Packs, connector registry, второй Decision Signal adapter (например OpenAI
Decisions API после публикации стабильного contract), polling/webhooks, durable
claiming, circuit breakers и multi-instance tests. Только здесь по двум
production harnesses принимается решение о shared coding interface.

На этом этапе доказанные Decision Signal profiles могут независимо перейти из
shadow в advisory и затем в bounded control. Promotion привязан к exact
provider/model/policy и откатывается без изменения Factory graph.

### Phase F: policy-based auto-merge

Risk classification, independent/quorum reviewers, calibration corpus, holdout
tests, low-risk allowlist, rollback/escalation policy и audited enablement.

## 19. Архитектурные инварианты

1. OctaCity остаётся работоспособной CI/CD-платформой без Factory Configuration,
   model provider и coding harness.
2. Dark Factory использует обычные immutable Builds и не меняет их семантику.
3. Work Source не определяет внутренний lifecycle Factory Run.
4. OpenSpec — optional specification format, не factory dependency.
5. Program владеет loop, branch, sequence, retry, join и termination; LLM
   возвращает typed result bounded call и не оркестрирует Factory Run.
6. Factory Controller не вызывает model provider напрямую.
7. OctaCity Agent не знает provider-specific harness semantics.
8. Octa не владеет backlog, Factory Run или delivery.
9. Coding plugin не выполняет push, PR или merge.
10. Evaluator не изменяет candidate.
11. Assessment не является Decision.
12. LLM output не исполняется без schema/policy validation.
13. Gates относятся к exact candidate/evidence digests.
14. Worktree и model session не содержат authoritative state.
15. Retry не переписывает историю.
16. Required missing/indeterminate evidence не означает pass.
17. External status — projection, не correctness state.
18. Auto-merge opt-in и ограничен risk policy.
19. Effective permissions могут только сужаться на каждом trust boundary.
20. Невозможность обеспечить permission приводит к отказу до spawn, а не к
    fallback на более слабый backend.
21. OctaCity Agent является доверенным worker; изолируется недоверенный
    Job/harness и его descendants.
22. Завершённые reasoning branches сохраняют full trace как evidence, но в
    последующий model context входят только scoped typed outputs/summaries.
23. Self-evolution создаёт обычный ChangeSet и не обходит validation,
    evaluation, delivery policy или human gates.
24. Decision Signal provider взаимозаменяем; JEV и будущий Decisions API не
    входят в Factory domain types.
25. Routing signal выбирает только predeclared edge и не пропускает mandatory
    gate; code-owned policy остаётся единственным автором transition.
26. Tool-risk signal никогда не расширяет effective permission intersection;
    Agent/backend остаются enforcement boundary.
27. Один logical Decision Signal request закрепляет exact provider/model,
    question/policy/input digests и receipt; retry не подменяет модель.
28. Shadow, advisory и bounded control продвигаются отдельно для каждой purpose
    и exact provider/model только по versioned calibration evidence.
29. Описанный ticket lifecycle является Flow Definition, а не hard-coded
    Factory domain state machine.
30. Flow graph состоит только из закрытого набора typed primitives; provider не
    создаёт node, edge, loop или authority.
31. Admitted Factory Run закрепляет root definition и exact reachable subflow
    closure; replacement действует только на новые runs.
32. Control dependency не передаёт context, Artifact, mount или credential.
33. Protected tests отсутствуют у implementation не по prompt, а по Context
    Manifest, Artifact grants и mount policy.
34. Requirements correction создаёт новый append-only Workflow Cycle; evidence
    superseded lineage не авторизует delivery или deployment.
35. Production promotion выполняет trusted action через существующий CI/CD, а
    reasoning и Decision Signal nodes не получают production credentials.
36. Long-running production monitoring остаётся внешним Work Source и не
    переписывает завершённый Factory Run.

## 20. Решения для отдельных ADR

1. ChangeSet format: commit, git bundle, patch series или комбинация.
2. Где выполняется trusted commit creation.
3. После второго production harness: общий internal coding interface или
   независимо распространяемые provider plugins. Для первого implementation
   зафиксирован конкретный официальный `octa_plugin_codex` без преждевременного
   universal facade.
4. Installation/versioning model Criterion Packs.
5. Evaluator process на Agent или remote evaluator pool.
6. Data residency classes для внешних LLM providers.
7. Quorum/model independence для high-risk изменений.
8. Граница auto-merge и аварийное отключение.
9. Хранение resumable sessions как optional optimization.
10. Разделение Git publication и forge PR operations.
11. Каноническая идентичность tool/command, допустимые arguments и descendants.
12. Формат filesystem roots и mount policy для implementation и evaluator.
13. Разделение Factory Policy, Task Envelope и локальной Agent policy.
14. Versioned representation call DAG, context summaries и replay evidence для
    LLM-as-Code execution.
15. Canonical Decision Signal request/result/receipt, provider capability model
    и граница между structural normalization и provider-specific probability
    semantics.
16. Purpose-specific calibration corpus, promotion/demotion policy и drift
    limits для routing и tool-risk.
17. Canonical Flow Definition schema, closed node set, graph validation и exact
    nested-version closure.
18. Separate control/context/data edge representation и absence proofs для
    protected tests и independent agents.
19. Candidate lineage и append-only requirements/test/implementation correction
    cycles.
20. Trusted deployment action через existing CI/CD и граница bounded
    post-deployment verification.

## 21. Критерии подтверждения архитектуры

### 21.1 Уже подтверждённый substrate

1. Обычный CI/CD Build проходит server -> Agent -> Octa -> result без Factory
   Configuration и LLM provider.
2. Agent restart, lease expiry, stale completion и retry проверяются lifecycle
   и released-product contracts.
3. Signed JobSpec и backend contracts не допускают fallback с требуемой
   isolation/virtualization на Host.
4. Resource, network host, secret profile и workload identity restrictions
   проходят positive и negative contracts в существующих границах.
5. `octa_plugin_codex` работает как обычный Octa task, не добавляя
   provider-specific OctaCity protocol, и проверяет secrets, process cleanup,
   cancellation, bounded output, schema и provenance.
6. Operator Console и REST сохраняют обычные Project/Build/Agent/Audit workflows
   независимо от будущего Factory UX.

### 21.2 Критерии готовности Dark Factory

1. Одна задача принимается из REST и GitHub без изменения Factory Controller.
2. Codex и второй production harness выполняются без provider-specific factory
   logic; общий plugin/library seam извлекается только если он подтверждён обеими
   реализациями.
3. Agent погибает после coding, другой продолжает без hidden local state.
4. Completion старого lease или Node Attempt после takeover отклоняется.
5. Architecture/security используют один evaluator interface и разные packs.
6. Тесты failed, LLM сказал pass, но Decision Engine создаёт Rework.
7. Required evaluator недоступен — система Escalates.
8. Prompt injection в repo не расширяет permissions evaluator или harness.
9. Candidate изменился — старые gates не принимаются.
10. Delivery retry после lost response не создаёт второй PR.
11. Coding harness не имеет push/merge credential.
12. Удаление worktree и model session не уничтожает возможность продолжить
    Factory Run.
13. Запрос tool, command, argument, path, mount, host или secret profile вне
    allowlist отклоняется до spawn или соответствующего защищённого действия.
14. Реальный backend contract одновременно доказывает разрешённые и запрещённые
    tool, filesystem, network и secret операции.
15. Обязательные validation/evaluation nodes, bounded retries, joins и stop
    conditions выполняются кодом даже при противоречащем LLM result.
16. Parallel reasoning calls получают scoped ancestor context, возвращают typed
    values и не используют общий бесконечно растущий transcript как state.
17. Self-evolution создаёт candidate ChangeSet и не может изменить production
    workflow до прохождения обычных gates и delivery policy.
18. JEV и второй fake adapter проходят один Decision Signal conformance suite;
    замена adapter не требует изменения Factory core или REST domain DTO.
19. Signal с undeclared route, out-of-envelope tool action, malformed result,
    low confidence или provider outage не запускает действие и приводит к
    configured Deny/Escalate.
20. Restart и unknown response переиспользуют immutable Decision Signal Receipt,
    не вызывая другую model для того же logical decision.
21. Shadow JEV результаты доступны в audit/diagnostics, но не меняют baseline;
    bounded control требует отдельного audited promotion exact model/policy.
22. Один и тот же Factory core исполняет разные валидные nested Flow Definitions
    без добавления новых hard-coded lifecycle branches.
23. Recursive/unbounded flow, provider-created edge и graph без terminal outcome
    отклоняются до admission.
24. Coding node, следующий после hidden-test authoring, не может прочитать test
    source, assertions, transcript или Artifact; validation воспроизводимо
    соединяет exact candidate и protected bundle.
25. Requirements Defect создаёт новый cycle и делает downstream evidence
    прежнего specification candidate непригодным для promotion.
26. Black-box verifier выполняется без source/implementation context, если это
    объявлено его projections, а deployment идёт через ordinary CI/CD Build.

## 22. Итог

OctaCity остаётся CI/CD-платформой и получает Dark Factory как независимый
opt-in interpreter immutable nested flows над обычными Builds, а не как замену
существующего режима и не как расширение Octa до backlog orchestrator. Octa
исполняет DAG и plugins, OctaCity владеет durable Flow Runs, Node Attempts,
Workflow Cycles и exact candidate lineage. Coding harnesses меняют одноразовые
worktrees, независимые nodes оценивают exact candidate, а trusted actions
публикуют и продвигают результат через существующий CI/CD. Детерминированный код
владеет control flow; LLM используется как bounded reasoning/generation/review
call. Control graph отдельно от context/data graph, поэтому разные этапы могут
быть как явно связанными handoff, так и доказуемо изолированными.
Decision-модели подключаются через replaceable Decision Signal Provider: JEV
первым, будущий Decisions API отдельным adapter. Они дают typed probabilistic
signal для заранее ограниченного routing или tool-risk, но никогда не получают
право расширить permissions или самостоятельно совершить transition.

```text
новый источник        -> Work Source Adapter
новый coding provider -> Octa plugin -> optional shared Harness Adapter
новый decision model  -> Decision Signal Provider adapter
новый аспект качества -> Criterion Pack / Evidence Producer
новый target delivery -> Delivery Adapter
новый lifecycle       -> immutable Flow Definition from bounded primitives
```

Так система сохраняет stateless execution, не связывается с GitHub, OpenSpec,
Codex или Claude и может постепенно перейти от human-approved PR factory к
unattended low-risk delivery.
