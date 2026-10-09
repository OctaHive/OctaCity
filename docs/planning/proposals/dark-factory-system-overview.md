# OctaCity Dark Factory: обзор системы

- Статус: рабочая карта целевой системы
- Актуализировано: 2026-10-09
- Аудитория: разработчики OctaCity и Octa, архитекторы, операторы фабрики
- Подробные контракты: [OctaCity Dark Factory Architecture](./dark-factory-architecture.md)
- Научная основа: [LLM-as-Code](https://arxiv.org/html/2606.15874v1)

## 1. Назначение документа

Этот документ описывает Dark Factory верхнеуровнево: зачем она нужна, как
устроена, где проходят границы ответственности, какие данные движутся между
подсистемами и в каких точках система расширяется плагинами и коннекторами.

Документ нужен как навигационная карта. Он не заменяет versioned OpenSpec,
протокольные спецификации, threat model или подробный архитектурный документ.
Его задача — удерживать общую модель системы и не позволять реализации
случайно превратиться в набор несвязанных model calls, второй CI executor или
захардкоженный процесс обработки GitHub Issues.

Описанный ниже lifecycle тикета является важным первым сценарием и рекомендуемой
конфигурацией. Он не является встроенным конечным автоматом Factory core.
Другой Project должен иметь возможность опубликовать иной процесс из того же
закрытого набора безопасных строительных блоков.

## 2. Идея продукта

OctaCity остаётся полноценной CI/CD-платформой и дополнительно получает режим
Dark Factory.

В режиме CI/CD OctaCity принимает Trigger, создаёт Build, материализует Pipeline
в Jobs, размещает Jobs на Agents и получает результаты выполнения через Octa.

В режиме Dark Factory OctaCity принимает Work из внешней системы, проводит его
через долговечный versioned Factory Flow, координирует несколько Builds и
независимых reasoning/evaluation вызовов, формирует точный кандидат изменения,
проверяет его и только после policy/human gates разрешает delivery и deployment.

```text
CI/CD

Trigger -> Build -> Attempt -> Job DAG -> Agent -> Octa -> Build Result


Dark Factory

Work Source -> Work Envelope -> Factory Run -> pinned Factory Flow
                                            -> Builds / model calls / gates
                                            -> exact candidate + evidence
                                            -> delivery -> existing CI/CD deployment
```

Оба режима используют общий execution substrate, но имеют разные application
lifecycles. Обычный CI/CD не должен зависеть от доступности Factory workers,
моделей, JEV, ticket connectors или delivery adapters.

## 3. Цели

1. Автоматизировать путь от входящего тикета до проверенного изменения и
   управляемого production rollout.
2. Сохранить OctaCity самостоятельной CI/CD-системой, работающей без Dark
   Factory.
3. Сделать процесс разработки versioned и настраиваемым, а не захардкоженным в
   серверной логике.
4. Исполнять reasoning и coding вызовы как ограниченные typed функции внутри
   программно управляемого процесса.
5. Разделять авторов, разработчиков, reviewers и verifiers по данным,
   credentials, workspace и authority.
6. Не полагаться на память model session: все необходимые handoffs должны быть
   явными, ограниченными и воспроизводимыми.
7. Связывать requirements, tests, code, evidence, review и deployment с exact
   revisions и content digests.
8. Переживать рестарты сервера, Agents, sandboxes и provider sessions без
   потери authoritative state и без повторения уже принятых side effects.
9. Подключать GitHub, GitLab, Jira, Slack, модели, evaluators и delivery systems
   через узкие provider-neutral contracts.
10. Оставлять человеку управляемые gates для риска, требований, review, merge и
    deployment там, где это требует политика Project.

## 4. Не-цели

1. Не превращать LLM в workflow engine, scheduler или источник lifecycle truth.
2. Не давать модели право самостоятельно создавать graph edges, менять policy,
   выполнять merge или выбирать production rollout.
3. Не превращать Octa в backlog manager, ticket tracker или долговечный Factory
   Controller.
4. Не создавать второй executor рядом с существующими Build, Job, Agent и Octa.
5. Не считать GitHub labels или статусы Jira authoritative состоянием фабрики.
6. Не передавать каждому следующему агенту полную историю диалога и результаты
   всех предыдущих этапов.
7. Не использовать mutable provider session, remote branch или worktree как
   единственное место хранения результата.
8. Не внедрять unrestricted workflow scripting, рекурсивные subflows или
   неограниченные fan-out, retry и model budgets.
9. Не давать coding/review/verifier workload постоянные forge-write или
   production credentials.
10. Не привязывать Factory domain к GitHub, Git, Codex, JEV или формату одного
    model provider.

## 5. Основные принципы

### 5.1 Код управляет процессом, модель решает только ограниченную задачу

Flow, допустимые переходы, retries, budgets, permissions и terminal outcomes
задаются versioned конфигурацией и исполняются кодом. Model call получает
ограниченный Task Envelope и возвращает schema-validated результат. Даже если
результат содержит рекомендацию, следующий переход выбирает детерминированная
policy.

### 5.2 Facts, opinions и authority разделены

```text
Evidence producers -> Assessments / Decision Signals -> deterministic policy
      facts                    opinions                    authority
```

Компилятор, тест или benchmark публикует факт. Reviewer или decision model
публикует оценку. Только policy и явно разрешённый human gate создают
authoritative transition.

### 5.3 Control dependency не передаёт данные

То, что узел B запускается после A, не означает, что B получает prompt,
transcript, workspace, artifacts или credentials A. Передача данных задаётся
отдельными Context и Data projections.

### 5.4 Результат имеет exact identity

Любой принятый результат связан с exact base revision, candidate revision,
ChangeSet, Node Attempt, Flow Definition, policy и provenance. Prose вроде
«тесты прошли» не заменяет проверяемое Evidence.

### 5.5 Внешние системы являются integrations, а не источником внутренней истины

GitHub Issue остаётся внешней проекцией Work. Потеря label update не отменяет
принятое сервером решение. Повторный reporter вызов использует стабильную
idempotency identity и восстанавливает внешнюю проекцию.

## 6. Два уровня flow

В системе есть два разных по назначению графа. Они не конкурируют, а образуют
иерархию.

```text
OctaCity Factory Flow
  -> durable semantic Node Attempt
       -> ordinary Build / Job
            -> Octa Task DAG
                 -> commands, plugins, model calls, local conditions
```

### 6.1 Factory Flow в OctaCity

Factory Flow — долговечный versioned macro-flow. Он:

- может длиться часы, дни или недели;
- переживает рестарты и смену Agents;
- имеет immutable Flow Definition и pinned subflow closure;
- управляет Node Attempts, retries, WIP, budgets и phase-ready pools;
- может ждать человека, внешнюю систему или свободную capacity;
- использует разные Builds, sandboxes, Agent Pools и credential profiles;
- фиксирует Context Manifest для каждого model-backed вызова;
- выполняет внешние side effects только через trusted actions и outbox;
- владеет переходами между triage, research, requirements, development,
  verification, delivery и promotion.

### 6.2 Octa Task DAG внутри Job

Octa Task DAG — ограниченный execution flow внутри одного Job и lease. Он:

- выполняется от начала до конца в одном execution boundary;
- запускает commands и versioned plugins;
- параллелит локально независимые задачи;
- поддерживает зависимости и conditions;
- может использовать structured outputs предыдущих tasks;
- управляет process lifecycle, timeout, cancellation и cleanup;
- публикует generic events, reports, artifacts и terminal result;
- не знает о Factory Run, внешнем тикете, phase-ready pool, merge или deploy.

### 6.3 Правило выбора границы

Новый Factory Node нужен, если между частями процесса меняется хотя бы одно из
следующего:

- требуется durable checkpoint;
- результат должен пережить потерю Job или Agent;
- меняется видимый context или source visibility;
- нужны другие credentials, permissions, mounts или network policy;
- нужен другой Agent Pool, sandbox или execution backend;
- требуется отдельный retry, timeout, budget или WIP limit;
- возможна долгая пауза, human gate или внешний callback;
- выполняется внешний side effect;
- промежуточный результат нужен другим Flow Runs или phase-ready pool.

Если всё выполняется в одной trust/context boundary, не требует отдельного
checkpoint и безопасно повторяется целиком, операции можно объединить в один
Octa Task DAG.

### 6.4 Почему не следует помещать весь Factory Flow в один Octafile

Один большой Octafile технически может содержать условия и много tasks, но при
этом сервер увидит его как одну атомарную Job execution:

- частичный прогресс не станет durable Factory state;
- сбой в конце потребует повторить ранние дорогостоящие вызовы;
- невозможно безопасно сменить visibility и credentials между стадиями;
- human gates и внешние ожидания удерживали бы Job и lease;
- сложнее доказать независимость author, implementer и verifier;
- невозможно независимо планировать стадии по pools, priority и capacity.

Поэтому OctaCity владеет крупными смысловыми границами, а Octa — локальной
механикой исполнения каждого узла.

## 7. Общая архитектурная схема

```text
                         OPERATOR PLANE
                  +-------------------------+
                  | Operator Console / REST |
                  +------------+------------+
                               |
                               v
EXTERNAL SYSTEMS        FACTORY CONTROL PLANE
+----------------+     +------------------------------------------+
| GitHub         |<--->| Work Source / Reporter connectors        |
| GitLab         |     | Admission + immutable Work Envelope      |
| Jira / Linear  |     | Factory Configuration + Flow Definitions |
| Slack / queues |     | Durable Flow interpreter + reconciler    |
+----------------+     | Phase-ready pools + policy gates         |
                       | Context builder + Decision Engine         |
                       | Delivery / promotion orchestration        |
                       +------------------+-----------------------+
                                          |
                                          | creates and observes
                                          v
                              EXISTING CI/CD SUBSTRATE
                       +------------------------------------------+
                       | Build -> Attempt -> Job DAG -> Scheduler |
                       +------------------+-----------------------+
                                          |
                                          v
                               AGENT EXECUTION PLANE
                       +------------------------------------------+
                       | signed JobSpec + fenced Lease            |
                       | qualified isolation backend              |
                       | Octa Task DAG                            |
                       | commands + plugins + model harnesses     |
                       | bounded outputs + ChangeSet capture      |
                       +------------------+-----------------------+
                                          |
                                          v
                              DURABLE DATA AND EVIDENCE
                       +------------------------------------------+
                       | PostgreSQL authoritative state           |
                       | Artifact Store immutable bytes           |
                       | Git / ChangeSet exact revisions          |
                       | Repository Knowledge retrieval receipts  |
                       | Audit + outbox                           |
                       +------------------------------------------+
```

## 8. Подсистемы и ответственность

| Подсистема | Отвечает за | Не отвечает за |
| --- | --- | --- |
| Work Source Connector | Получение внешнего тикета, нормализацию provider payload, deduplication identity | Выбор Factory transition |
| Work Reporter Connector | Best-effort projection статуса, labels, комментариев и resolution во внешнюю систему | Хранение authoritative Factory state |
| Factory Admission | Проверку Project/configuration/source, создание immutable Work Envelope и Factory Run | Исполнение repository code |
| Factory Configuration | Root Flow Definition, policies, budgets, connector/profile selectors | Mutable состояние выполняющегося run |
| Flow Interpreter | Durable Node Attempts, переходы, subflows, joins, retries и waits | Выполнение commands или model provider SDK |
| Phase-ready Pools | Детерминированный выбор готовой работы с учётом severity, priority, age, dependencies, WIP и budget | Изменение результата triage |
| Context Builder | Воспроизводимый Context Manifest из разрешённых artifacts, handoffs и retrieval receipts | Автоматическую передачу всей истории |
| Decision Engine | Применение deterministic gates, quorum и policy к facts/assessments/signals | Генерацию model opinion |
| Decision Signal Plane | Ограниченные provider-neutral вопросы routing/tool risk и immutable receipts | Самостоятельный lifecycle transition |
| Build Application | Immutable Build, Attempt и Job DAG | Factory lifecycle |
| Placement Scheduler | Подбор совместимого Agent и fenced Lease | Выбор следующей Factory стадии |
| Agent | Проверка JobSpec, локальных grants и backend capabilities; execution lifecycle | Provider-specific business semantics |
| Octa | Task DAG, plugins, processes, events, outputs, timeout/cancellation | Backlog, Factory Run, delivery и production policy |
| Model Plugin | Один bounded reasoning/coding/evaluation вызов | Merge, deployment и graph mutation |
| ChangeSet Capture | Проверку workspace и создание exact candidate bundle/manifest | Публикацию remote branch |
| Evidence Producers | Deterministic test/security/performance facts | Итоговый delivery decision |
| Evaluators | Assessment exact candidate по Criterion Pack | Изменение candidate или policy |
| Delivery Adapter | Публикацию exact accepted candidate, branch/PR и наблюдение результата | Выбор кандидата и production credentials для моделей |
| Promotion Action | Запуск существующего deployment Build/Pipeline | Самостоятельное изменение Factory policy |
| PostgreSQL | Authoritative definitions, runs, attempts, projections, fences, budgets и audit metadata | Большие payloads и repository bytes |
| Artifact Store | Immutable task bodies, traces, reports, evidence, test bundles и ChangeSets | Lifecycle truth |
| Repository Knowledge | Revision-bound поиск релевантных fragments с Retrieval Receipt | Authority, permissions или переходы |

## 9. Точки расширения

Расширение допускается только через versioned contracts и conformance tests.
Новый provider не должен проникать своими SDK types в Factory domain.

### 9.1 Work Source и Work Reporter connectors

Примеры: GitHub, GitLab, Jira, Linear, Slack, REST, queue, schedule.

Source connector:

- обнаруживает candidates через polling, webhook или explicit command;
- загружает bounded ticket body и metadata;
- присваивает stable source-scoped identity;
- создаёт provider-neutral Work input.

Reporter connector:

- устанавливает labels/status/resolution;
- публикует bounded комментарии и ссылки на review;
- наблюдает результат неизвестного предыдущего запроса перед повтором;
- не становится источником Factory truth.

Один adapter может реализовывать обе роли, но их authority различается.

### 9.2 Source/VCS plugins

Source plugins материализуют exact read-only revision для обычного Build.
Write-capable delivery отделён от read path. Git является первой реализацией,
но Factory contracts используют provider-neutral revision и ChangeSet identity.

### 9.3 Octa task plugins и model harnesses

Octa plugins реализуют commands, templates, containers и model-backed tasks.
Codex является первым закреплённым coding harness. Другой harness подключается
новым versioned plugin с schema, capability metadata, provenance и release
qualification, не меняя Factory domain.

### 9.4 Decision Signal Providers

JEV является первым provider для ограниченных routing и tool-risk вопросов.
Будущий Decisions API подключается через тот же provider-neutral контракт:
purpose, finite answer domain, exact model identity, bounded input, deadline,
budget, policy digest и immutable receipt.

### 9.5 Evidence Producers и Evaluators

Deterministic producers публикуют JUnit, coverage, SARIF, SBOM, benchmark,
visual diff и другие machine-verifiable факты. Evaluator connectors создают
provider-neutral Assessments по Criterion Pack. Новая модель или статический
анализатор не меняет Decision Engine.

### 9.6 Repository Knowledge providers

Будущий provider может объединять exact/symbol search, dependency graph,
lexical ranking и vector retrieval. В Factory он возвращает только immutable
revision-bound fragments и Retrieval Receipt. Retrieval не расширяет authority
и не заменяет обязательные artifacts или evidence.

### 9.7 Delivery и deployment adapters

Delivery adapter публикует exact accepted candidate в GitHub/GitLab и создаёт
или обновляет review target. Deployment предпочтительно выполняется через уже
существующий OctaCity Pipeline/Build. Provider-specific deployment adapter
нужен только там, где существующий CI/CD контракт не подходит.

## 10. Основные данные, передаваемые между этапами

| Объект | Кто создаёт | Для чего используется |
| --- | --- | --- |
| Work Envelope | Admission | Immutable нормализованный тикет, source identity, Project и base subject |
| Factory Configuration | Operator | Закрепляет root flow, policies, budgets и profile selectors |
| Flow Definition | Operator | Versioned graph из закрытого набора node primitives |
| Flow Run | Interpreter | Durable invocation конкретного Flow Definition |
| Node Attempt | Interpreter | Одна fenced bounded попытка узла |
| Task Envelope | Trusted compiler | Exact вход одного command/model-backed Job |
| Context Manifest | Context Builder | Явный ordered список разрешённого context с digests и inclusion reasons |
| Stage Handoff | Завершившийся node | Ограниченный typed итог для явно выбранных successors |
| Decision Signal Receipt | Decision Signal Plane | Exact provider/model/input/output/replay evidence |
| ChangeSet | Trusted capture | Exact candidate lineage, bundle, manifest и optional bounded patch |
| Protected Test Bundle | Test-authoring node | Immutable скрытые acceptance tests |
| Evidence Manifest | Validation | Факты, привязанные к exact candidate и test bundle |
| Assessment | Evaluator | Мнение одного независимого evaluator с findings и provenance |
| Decision | Decision Engine | Authoritative policy outcome над facts и assessments |
| Audit Fact | Каждый accepted command/transition | Неизменяемая корреляция actor, operation, target и outcome |
| Outbox Record | Та же transaction, что и решение | Идемпотентный внешний side effect после durable commit |

Raw provider messages, chain-of-thought, credentials, presigned URLs и полные
transcripts не являются межэтапным state. При необходимости сохраняется только
sanitized bounded trace как evidence, но он не передаётся следующему call без
явной projection.

## 11. Пример целевого ticket flow

Следующий lifecycle является первой рекомендуемой Factory Configuration. Он
может изменяться без изменения Factory core.

```text
External Work Source
  -> intake and immutable Work Envelope
  -> triage phase 1: duplicate + Project-goal fit
       -> reject / escalate
       -> triage phase 2: severity + component + size + route
            -> research
                 -> requirements
            -> requirements
            -> direct development for policy-approved small Work
            -> verification-only / already-fixed resolution

requirements
  -> protected test authoring
  -> isolated implementation + unit tests + documentation
  -> independent code review
  -> trusted validation with protected tests
       -> rework implementation
       -> return to requirements with typed findings
       -> accepted candidate
  -> delivery branch / review target
  -> stage / canary / A-B / production policy
  -> post-deployment checks
  -> completed Work projection to the external source
```

Каждая стрелка является переходом опубликованного Flow Definition, а не
встроенным `if` в Factory Controller. Не все Projects обязаны использовать все
ветви; configuration выбирает допустимые subflows, gates и terminal outcomes.

### 11.1 Intake из GitHub

GitHub connector выбирает Issues без управляемых Factory labels. В будущем ту
же роль могут выполнять GitLab, Jira, Linear, Slack или другой Work Source.

Источник может работать через webhook или периодический reconciliation poll.
Интервал, например 30 минут, page bounds, stable cursor, rate limits и набор
eligible provider states задаются connector profile. Повторное наблюдение того
же Issue дедуплицируется по source-scoped identity и не создаёт второй Work.

Поддерживаемые начальные типы Work:

- `defect`;
- `feature_request`.

Connector нормализует Issue в Work Envelope. GitHub labels являются внешним
отображением состояния и задаются configurable mapping, например:

```text
unclassified -> triage
triage/research-ready/requirements-ready/development-ready
verification/delivery/completed
rejected:duplicate/out-of-scope/cant-reproduce/already-fixed
```

Внутренние state и outcome остаются provider-neutral. Конкретные имена labels,
projects, boards и resolutions принадлежат GitHub adapter configuration.

### 11.2 Triage, фаза 1: eligibility

OctaCity создаёт первый Triage Node Attempt и Build. Внутри Job Octa исполняет
локальный DAG:

1. Поиск возможных дубликатов по authoritative history, Repository Knowledge и
   внешнему source index.
2. Оценка соответствия тикета целям Project.
3. Формирование schema-validated `Eligibility Result`.

Операции могут выполняться параллельно, если их inputs независимы. Model/JEV
возвращает assessment, а не переход.

Пример outcomes:

- `eligible`;
- `duplicate(candidate)`;
- `out_of_scope`;
- `uncertain`;
- `invalid_input`.

После durable сохранения результата OctaCity применяет gate:

- duplicate или out-of-scope -> reject policy;
- uncertain -> human/escalation pool;
- eligible -> вторая фаза triage.

Закрытие Issue и установка resolution выполняются Work Reporter через outbox,
а не Octa task или model plugin.

### 11.3 Triage, фаза 2: classification и routing

Второй Node Attempt получает исходный Work Envelope и только явно разрешённый
bounded handoff первой фазы. Он определяет:

1. severity;
2. component;
3. размер работы;
4. предварительную воспроизводимость для defect;
5. risk и dependencies при наличии данных;
6. рекомендуемый следующий маршрут.

Допустимые route outcomes объявлены Flow Definition, например:

- `research`;
- `requirements`;
- `test_authoring`;
- `development`;
- `verification_only`;
- `already_fixed`;
- `escalate`;
- `reject`.

Model/JEV не может вернуть произвольный node ID. Он выбирает значение из
finite domain, а policy проверяет, разрешён ли маршрут для ticket type, size,
risk и текущей configuration.

Успешно классифицированный Work получает внутренний phase-ready state и
соответствующую внешнюю label projection. Отклонённый Work получает typed
resolution и evidence, объясняющий решение без публикации hidden reasoning.

### 11.4 Research

Research subflow запускается только для выбранного Work.

Для defect:

- строится более полный reproduction environment;
- используется более сильная модель и больший bounded budget;
- формируется deterministic reproduction evidence;
- результатом является reproduced, intermittent, environment-specific,
  cannot-reproduce или needs-human-input.

Если defect невозможно воспроизвести после исчерпания declared attempts,
policy может завершить Work с resolution `cant_reproduce` или отправить его на
human gate.

Для feature request:

- выполняется deep research;
- анализируются текущая архитектура, ограничения, аналоги и варианты;
- создаётся proposal artifact с sources, assumptions и unresolved questions;
- proposal не является готовой спецификацией и не разрешает implementation.

### 11.5 Requirements и design

Policy может пропустить этот subflow для действительно маленького Work. Такое
решение основано на принятом Triage Result, а не на желании coding model.

Для feature request:

- создаётся новая specification/change;
- формируются acceptance criteria;
- при необходимости создаётся architecture/design;
- задаются performance, security, UI, compatibility и operational требования;
- отдельный reviewer получает spec candidate без author transcript.

Для defect:

- определяется, какая существующая requirement/specification нарушена;
- создаётся specification change, уточняющий expected behavior;
- фиксируется regression acceptance criterion.

Принятый requirements candidate доставляется в Git через trusted Delivery
Adapter и становится exact predecessor implementation cycle. Если на позднем
этапе обнаружена ошибка требований, создаётся новый append-only requirements
cycle; старые code/test/evidence сохраняются для audit, но теряют authority.

### 11.6 Protected test authoring

Отдельный node создаёт проверки по exact requirements:

- integration tests;
- end-to-end tests;
- performance/benchmark contracts;
- UI и visual tests;
- security/negative tests;
- migration и compatibility tests;
- другие Project-specific acceptance checks.

Публичная часть Stage Handoff сообщает только типы и общую область проверок.
Исходники hidden tests, assertions, expected values и author transcript
сохраняются как protected immutable bundle в Artifact Store.

### 11.7 Implementation и code review

Implementation node получает:

- exact base revision;
- accepted requirements и design;
- разрешённые repository fragments;
- известные публичные constraints;
- собственные skills/configuration;
- отдельные credentials и permission set.

Он не получает protected test bundle или transcript test author. Coding agent:

- изменяет production code;
- пишет unit tests, доступные обычному разработчику;
- обновляет документацию;
- обновляет build/deployment scripts в разрешённых пределах;
- возвращает typed result и candidate workspace.

Trusted capture создаёт ChangeSet и уничтожает disposable worktree. Затем
независимый code-review node получает exact candidate, requirements и свой
Criterion Pack, но не implementation transcript. Review findings не дают
reviewer права изменить candidate; исправление создаёт новый attempt.

### 11.8 Independent verification

Trusted validation materializes exact implementation candidate и exact
protected test bundle. Deterministic test processes могут исполнять candidate,
но reasoning verifier по умолчанию не получает исходники hidden tests,
implementation transcript или произвольный доступ к repository.

Verification может включать:

- ранее подготовленные acceptance tests;
- compiler, lint и unit tests;
- integration/e2e/UI tests;
- performance и resource regression;
- security scans и adversarial checks;
- migration/rollback verification;
- staged environment validation.

Все факты входят в Evidence Manifest. Если нужен model-backed verifier, он
получает только explicitly projected black-box observations и Criterion Pack.
Таким образом модель не может «подправить» код или тесты для прохождения
проверки и не знает hidden контекст предыдущих авторов.

### 11.9 Delivery, merge и deployment

После успешной verification Decision Engine проверяет mandatory evidence,
quorum, risk, human approvals и exact candidate lineage.

Trusted Delivery Adapter:

- публикует feature branch или другой review target;
- создаёт/обновляет pull request;
- наблюдает external review и remote head;
- выполняет merge только при разрешённой policy.

Deployment выполняется через существующий OctaCity Pipeline/Build. Flow
Definition может выбрать:

- только stage;
- stage -> human approval -> production;
- stage -> deterministic criteria -> production;
- canary;
- A/B experiment;
- progressive rollout;
- immediate production для разрешённого low-risk класса.

JEV или другой Decision Signal Provider может дать bounded recommendation для
одного из заранее разрешённых rollout outcomes. Authoritative решение всё равно
создаёт policy/human gate, а production credentials остаются только у trusted
promotion action.

После deployment выполняются bounded post-deployment checks. Долговременный
production monitoring находится вне текущего Factory Run; обнаруженная им
проблема создаёт новый Work и проходит обычный admission/triage.

### 11.10 Карта участия модулей по этапам

В таблице ниже показано, какой модуль отвечает за каждый этап описанного
процесса. Названия этапов принадлежат первой рекомендуемой Flow Definition, а
названия модулей — стабильной архитектуре. Изменение flow переставляет и
комбинирует узлы, но не переносит ответственность между модулями.

| Этап | Control plane в OctaCity | Исполнение | Данные и доказательства | Внешнее действие и итог |
| --- | --- | --- | --- | --- |
| Обнаружение тикета | **Work Source Connector** опрашивает GitHub или принимает webhook; **Factory Admission** проверяет Project, source identity и configuration | Отдельный Agent не нужен | **PostgreSQL** хранит cursor, deduplication identity, Work Envelope и admission audit; большой ticket body попадает в **Artifact Store** | Создаётся один immutable Work; источник пока не изменяется |
| Triage, фаза 1: duplicate и соответствие целям | **Flow Interpreter** открывает Node Attempt; **Context Builder** выбирает Project goals, history и разрешённые repository fragments; **Decision Engine** применяет eligibility gate | **Build Application** создаёт Build/Job; **Placement Scheduler** выдаёт fenced Lease; **Agent** запускает **Octa Task DAG**; **Model Plugin** и при необходимости **Decision Signal Plane** дают bounded assessments | **Repository Knowledge** возвращает revision-bound candidates и Retrieval Receipt; **Artifact Store** хранит входы и trace; **PostgreSQL** — Stage Handoff и решение | **Work Reporter Connector** через outbox закрывает duplicate/out-of-scope Issue либо ставит следующую label projection |
| Triage, фаза 2: severity, component, size, reproduction и route | **Flow Interpreter** запускает второй изолированный Node Attempt; **Context Builder** передаёт Work Envelope и разрешённый handoff первой фазы; **Decision Engine** проверяет route против policy | Тот же путь **Build → Scheduler → Agent → Octa**; Octa может параллельно вызвать несколько commands/model tasks; JEV или другой **Decision Signal Provider** выбирает только значение из конечного домена | **PostgreSQL** хранит Triage Result, provenance и phase-ready projection; **Artifact Store** — bounded reports | **Phase-ready Pools** помещает Work в research, requirements, development, verification или escalation; **Work Reporter** обновляет labels/status |
| Исследование defect | **Flow Interpreter** выбирает reproduction subflow и его budget; **Context Builder** выдаёт exact revision, environment contract и известные симптомы; **Decision Engine** применяет reproduction policy | **Octa** запускает команды воспроизведения, diagnostic tools и более сильный **Model Plugin** в отдельном Job | **Evidence Producers** публикуют reproduction facts; **Artifact Store** хранит логи и reports; **PostgreSQL** — принятый outcome | **Work Reporter** публикует результат; успешное исследование направляется в requirements, неуспешное — в escalation или `cant_reproduce` по policy |
| Исследование feature request | **Flow Interpreter** запускает research subflow; **Context Builder** собирает Project goals, ограничения и разрешённые sources; **Decision Engine** применяет research gate | **Octa Task DAG** выполняет поиск, анализ и model-backed deep research; **Repository Knowledge** предоставляет точные fragments | Proposal, sources, assumptions, Retrieval Receipts и Stage Handoff сохраняются в **Artifact Store/PostgreSQL** | Work переходит к requirements или human clarification; proposal сам по себе не разрешает разработку |
| Разработка и review требований | **Flow Interpreter** выбирает requirements subflow или policy-approved обход для small Work; **Context Builder** отделяет author и reviewer context; **Decision Engine** принимает или возвращает spec candidate | Первый **Model Plugin** создаёт spec/design, второй независимый вызов выполняет review; оба запускаются через отдельные **Build/Agent/Octa** boundaries | **ChangeSet Capture** фиксирует spec candidate; **Evaluators** создают findings; **Artifact Store** хранит exact bundle; **PostgreSQL** — cycle и decision | **Delivery Adapter** при принятии публикует spec branch/PR; после merge exact spec revision становится base разработки |
| Разработка защищённых acceptance tests | **Flow Interpreter** создаёт test-authoring node; **Context Builder** передаёт requirements, но не будущий implementation context | Отдельный **Agent/Octa/Model Plugin** создаёт integration, E2E, UI, performance и security tests | **ChangeSet Capture** фиксирует test candidate как **Protected Test Bundle**; **Artifact Store** хранит его с закрытыми grants | Наружу передаётся только bounded Stage Handoff о классах проверок; implementation node не получает bundle |
| Реализация кода | **Flow Interpreter** создаёт implementation Node Attempt; **Context Builder** выдаёт requirements, design, public constraints и разрешённые repository fragments | **Build Application → Scheduler → Agent → Octa → Codex Model Plugin**; coding agent меняет code, unit tests, docs и build scripts в выделенном workspace | **ChangeSet Capture** проверяет workspace и создаёт exact candidate bundle/manifest; **Artifact Store** хранит ChangeSet, **PostgreSQL** — lineage | Remote branch ещё не создаётся; terminal result возвращает candidate identity, а не право на delivery |
| Независимый code review | **Flow Interpreter** создаёт sibling/review node; **Context Builder** исключает implementation transcript и выдаёт exact candidate, requirements и Criterion Pack; **Decision Engine** применяет quorum/policy | Отдельный **Agent/Octa/Evaluator Model Plugin** анализирует candidate read-only | **Evaluators** публикуют Assessment и findings; **PostgreSQL** связывает их с exact ChangeSet | При замечаниях создаётся новый implementation attempt; reviewer не изменяет candidate напрямую |
| Независимая verification | **Flow Interpreter** запускает validation plan; **Context Builder** создаёт отдельные manifests для deterministic checks и reasoning verifier; **Decision Engine** проверяет обязательные facts, quorum и risk | Trusted validation Job получает candidate и protected tests; **Evidence Producers** запускают compiler, tests, scans, benchmarks и staged checks; reasoning verifier получает только разрешённые observations | **Evidence Manifest** и Assessments сохраняются по exact identities | Failure возвращает Work в implementation либо requirements с typed findings; success создаёт accepted candidate |
| Публикация branch/PR и merge | **Decision Engine** проверяет acceptance, approvals и delivery policy; **Flow Interpreter** создаёт trusted-action attempt | Model/Octa workload не получает forge-write authority | **PostgreSQL** фиксирует stable delivery identity, attempt и outbox; **Artifact Store** предоставляет exact accepted bundle | **Delivery Adapter** идемпотентно публикует feature branch/PR и после разрешённых gates выполняет merge |
| Stage, canary, A/B и production | **Flow Interpreter** выбирает только объявленные rollout branches; **Decision Signal Plane** может дать recommendation; **Decision Engine** или human gate принимает решение | **Promotion Action** запускает существующий OctaCity deployment Pipeline/Build; Agents и Octa выполняют обычный CI/CD workload | Deployment evidence, health checks и rollout observations попадают в **Evidence Manifest**, audit и outbox | Promotion выполняется на stage, canary или production; credentials принадлежат только trusted action |
| Завершение и обратная связь | **Flow Interpreter** фиксирует terminal outcome; **Decision Engine** проверяет post-deployment policy | Долговременный monitoring остаётся внешней системой; при инциденте он становится новым **Work Source** | **PostgreSQL** сохраняет полный append-only lifecycle и retention state; **Artifact Store** — удерживаемые evidence | **Work Reporter** закрывает или обновляет исходный тикет. Production regression создаёт новый Work, а не переписывает завершённый Run |

На каждом исполняемом этапе повторяется одна и та же инфраструктурная цепочка:

```text
Flow Interpreter
  -> Build Application
  -> Placement Scheduler
  -> Agent with fenced Lease
  -> Octa Task DAG
  -> command / model / evaluator plugin
  -> typed result + Stage Handoff / ChangeSet / Evidence
  -> PostgreSQL durable acceptance
  -> Decision Engine
  -> next declared Flow transition
```

Эта цепочка не означает, что каждый узел обязан создавать Build. Deterministic
gate, bounded join, human approval и trusted outbox transition выполняются в
Factory control plane. Build создаётся тогда, когда нужен изолированный
execution workload. За счёт этого сервер не дублирует executor Octa, а Octa не
становится владельцем долговечного Factory lifecycle.

## 12. JEV, Decisions API и ветвление

JEV можно использовать двумя способами.

### 12.1 Server-side Decision Signal

Подходит для небольшого bounded вопроса над уже подготовленными structured
facts: routing, tool risk, rollout recommendation. Сервер сохраняет request и
receipt, обеспечивает replay, deadline, provider/model pinning и fallback.

### 12.2 JEV как Octa plugin

Подходит, если assessment требует sandbox, repository checkout, локальных
commands или сложного Octa DAG. Plugin возвращает typed output, а Octa может
локально пропустить или запустить заранее объявленные tasks.

В обоих вариантах действуют ограничения:

- provider не создаёт graph edges;
- ответ выбирается из finite domain;
- неизвестный или malformed outcome fail-closed;
- ответ не расширяет permission set;
- macro transition фиксирует OctaCity после durable acceptance результата;
- внешний side effect выполняется только trusted action/outbox.

Один и тот же purpose не должен случайно вызываться обоими путями. Flow
Definition и provider profile явно задают execution mode и exact implementation.

## 13. Изоляция данных и контекста

Разделение на два model calls или два Octa tasks само по себе не создаёт строгой
изоляции. Если они работают в одном Job/workspace, поздний call потенциально
может прочитать файлы раннего.

Строгая изоляция требует отдельного Node Attempt/Job и одновременно:

- чистого workspace;
- отдельного sandbox/execution backend;
- собственного Context Manifest;
- явных Artifact grants;
- отдельных read-only/write mounts;
- stage-scoped credentials;
- ограниченной network policy;
- отсутствия undeclared environment values;
- независимого process/model session;
- cleanup после terminal outcome.

```text
Test author Job
  -> protected test bundle --------------------+
                                                  |
Implementation Job                               |
  requirements -> candidate ChangeSet            |
                                                  v
Trusted validation Job <---------------- candidate + protected tests
  -> Evidence Manifest

Implementation Job не имеет ни mount, ни Artifact grant на test bundle.
```

Независимость может быть усилена отдельным Agent Pool, но другой физический
Agent не заменяет capability isolation. Один и тот же host допустим, если
backend доказывает отсутствие остаточного state и корректно очищает workspace.

## 14. Feature branches и внутренние ChangeSets

### 14.1 Возможные модели

1. Model/coding Agent сразу пишет в remote feature branch.
2. Каждый Factory этап публикует отдельную remote branch.
3. Factory хранит candidate как внутренний exact ChangeSet, а branch создаёт
   только trusted Delivery Adapter.

### 14.2 Рекомендуемая модель

По умолчанию используется третий вариант:

```text
isolated workspace
  -> trusted ChangeSet capture
  -> immutable bundle + manifest in Artifact Store
  -> validation/review by exact identity
  -> trusted Delivery Adapter
  -> feature branch + pull request
```

Преимущества:

- coding model не получает forge-write credential;
- незавершённые и отвергнутые attempts не создают remote branch мусор;
- retry всегда начинается от exact base, а не от mutable remote head;
- можно параллельно проверять несколько candidates;
- hidden tests и internal evidence не попадают в Git;
- публикация branch становится наблюдаемым идемпотентным side effect.

Feature branch остаётся полезной как review/delivery projection. Для
requirements возможно отдельное раннее delivery: accepted spec ChangeSet
публикуется и после merge становится exact base implementation cycle.

Remote branch никогда не является единственным authoritative candidate state.
Перед merge Delivery Adapter повторно проверяет ancestry, exact remote head,
approvals и соответствие принятому ChangeSet.

Поддержка stacked branches или per-stage branches может быть добавлена отдельной
Delivery Policy, но не должна менять core Flow semantics.

## 15. Durability, retry и side effects

Authoritative state разделён следующим образом:

| Хранилище | Содержимое |
| --- | --- |
| PostgreSQL | Definitions, Runs, Node Attempts, current projections, budgets, claims, receipts, decisions, audit и outbox |
| Artifact Store | Ticket/spec bodies, prompts, handoffs, traces, test bundles, ChangeSets, reports и evidence bytes |
| Git | Принятые exact repository revisions после trusted delivery |
| Repository Knowledge | Revision-bound indexes и Retrieval Receipts, но не lifecycle truth |
| External Work Source | Provider identity и best-effort status projection |
| Worktree / model session | Ничего незаменимого |

Правила восстановления:

- любой worker сначала читает durable state;
- claims и leases fenced;
- unknown external response наблюдается по stable operation identity;
- retry создаёт новый append-only attempt;
- принятый immutable result не вычисляется повторно без policy reason;
- partial outputs не публикуют result identity;
- exhausted retry переводит run в declared failure/escalation route;
- timer только будит reconciler и не является источником состояния.

## 16. Security и trust boundaries

1. Внешний ticket body, repository text и retrieved fragments являются
   недоверенным input и могут содержать prompt injection.
2. Factory Flow и Task Envelope компилируются trusted code, а не repository
   files.
3. Effective permissions являются пересечением Factory policy, Task Envelope,
   Agent policy и доказанных backend capabilities.
4. Любой слой может только сузить authority.
5. Coding/review/verifier workloads не получают delivery или production
   credentials.
6. Protected tests не скрываются инструкцией в prompt; у implementation
   отсутствуют соответствующие mounts и Artifact grants.
7. Model outputs проходят schema, size, subject и provenance validation.
8. Secret-bearing input не сохраняется в prompt, trace, log, patch или audit.
9. ChangeSet capture и materialization не исполняют repository hooks/filters и
   проверяют paths, symlinks, limits и secret patterns.
10. Невозможность обеспечить требуемую isolation приводит к отказу до spawn,
    а не к fallback на более слабый backend.

## 17. Наблюдаемость и audit

Для каждого Factory Run оператор должен видеть:

- source Work и exact subject;
- pinned Flow Definition closure;
- текущий Flow Run, node и phase-ready state;
- Node Attempts, retries и budgets;
- linked Builds, Attempts и Jobs;
- Context projection без скрытого содержимого;
- model/provider/plugin/prompt/policy digests;
- Decision Signal receipts и deterministic decision reasons;
- ChangeSet/candidate lineage;
- Evidence, Assessments и quorum;
- human gates, delivery, merge и promotion attempts;
- request/audit correlation.

UI не выводит hidden tests, raw prompts, chain-of-thought, credentials,
presigned URLs или permanent object locations. Operator может перейти от
Factory node к связанному Build и обратно, но Build UI не пытается сам вывести
Factory decision из логов.

## 18. Что является настраиваемым

Без изменения Factory core Project может заменить:

- Work Source/Reporter connector;
- root Flow Definition и состав nested subflows;
- обязательность research, requirements и human review;
- модель, reasoning effort и budget конкретного node;
- JEV/Decision Signal provider profile;
- Agent Pool, execution backend и resource limits;
- Context/Data projections;
- Criterion Packs и evaluator quorum;
- retry и escalation policy;
- delivery target и branch strategy;
- stage/canary/A-B/production rollout policy;
- post-deployment checks.

Не настраиваются произвольным provider output:

- новые node kinds;
- undeclared edges;
- unbounded loops/fan-out;
- permission widening;
- forge или production credentials;
- accepted candidate identity;
- обязательные deterministic gates.

## 19. Текущее состояние и целевые шаги

Уже существует фундамент:

- обычный CI/CD server -> Agent -> Octa lifecycle;
- versioned Projects, Pipelines, Builds, Attempts и Jobs;
- PostgreSQL authority, Artifact Store, audit и outbox patterns;
- signed JobSpec v3 и protected Factory execution;
- pinned Octa/Codex plugin integration;
- Factory Configuration/Run, fixed Stage Attempts и fenced reconciliation;
- Decision Signal/JEV foundation и tool authorization broker;
- Task Envelope, Context Manifest и Stage Handoff contracts;
- trusted ChangeSet capture, acceptance и exact rematerialization.

Следующие крупные возможности:

1. Общий bounded structural graph kernel для Octa и Factory validators без
   объединения executors.
2. Immutable nested Flow Definitions и durable interpreter.
3. Triage Result, phase-ready pools и manual source-agnostic journey.
4. Research, requirements и correction cycles.
5. Protected test-first development и context-absence contracts.
6. Independent evaluation и configurable verification plans.
7. GitHub Work Source/Reporter и Delivery adapters.
8. Factory REST и Operator Console workbench.
9. Released-product pilot, shadow JEV calibration и opt-in unattended mode.

## 20. Архитектурные инварианты

1. CI/CD работает при полностью выключенной Factory.
2. Factory использует Builds и Jobs, а не создаёт второй execution runtime.
3. Octa исполняет bounded Task DAG и не владеет backlog или lifecycle.
4. Factory Flow состоит только из закрытого набора versioned primitives.
5. Provider не создаёт edge и не расширяет permission.
6. Любой macro transition основан на durable accepted observation.
7. Control edge не передаёт данные.
8. Context передаётся только через immutable explicit projection.
9. Model session не является памятью системы.
10. Tests, code, evidence и delivery связаны exact identities и digests.
11. Independent agent не получает undeclared transcript или artifacts.
12. Hidden tests отсутствуют у implementation на уровне capabilities.
13. External labels и branches являются projections, а не Factory truth.
14. Side effect следует только после durable commit и использует stable
    idempotency identity.
15. JEV и будущие Decisions APIs возвращают signals, а не authority.
16. Merge и deployment выполняются только trusted actions после policy/human
    gates.
17. Retry и correction создают новую append-only history.
18. Невозможность доказать требуемую isolation приводит к fail-closed.

## 21. Вопросы для дальнейшей детализации

Эти решения не блокируют базовую архитектуру, но должны быть закреплены до
production pilot:

1. Конкретная provider-neutral taxonomy ticket status/resolution и её mapping в
   GitHub labels/projects.
2. Какие triage операции используют deterministic code, Codex, JEV или hybrid.
3. Граница small Work, позволяющая пропустить requirements authoring.
4. Какие test artifacts являются hidden, а какие должны быть публичны coding
   agent.
5. Минимальный обязательный evaluator quorum по risk class.
6. Когда feature branch публикуется: перед human code review или только после
   automated verification.
7. Какие rollout strategies входят в первый production pilot.
8. Какие Factory Configuration изменения требуют отдельного human approval.
9. Retention сроки для prompts, sanitized traces, protected tests и rejected
   ChangeSets.
10. Нужен ли отдельный Agent Pool для verifier или достаточно доказанной
    sandbox/context isolation.

## 22. Итоговая модель

OctaCity Dark Factory — не один большой автономный агент и не один гигантский
Octafile. Это долговечный программно управляемый Factory Flow, состоящий из
безопасных semantic nodes. Каждый execution node переиспользует существующий
CI/CD substrate и может запускать локальный Octa Task DAG с commands, plugins,
Codex, JEV и deterministic checks.

OctaCity владеет Work, состоянием, graph transitions, budgets, isolation,
candidate lineage, decisions, delivery и promotion. Octa владеет ограниченным
исполнением одного Job. Models создают typed результаты и assessments. Policy и
human gates владеют authority.

Такое разделение позволяет начать с описанного ticket lifecycle и затем менять
его, добавлять исследования, reviewers, новые тесты, providers, connectors и
rollout strategies без переписывания Factory core и без ослабления границ
доверия.
