# Operator console deployment

This runbook defines the supported same-origin deployment for the OctaCity
operator console. The console and management API have no operator login,
session, RBAC, or browser credential. Deploy this origin only on a trusted
operator network. It is not safe for public Internet exposure.

Access to this origin is management authority, not merely access to a read-only
dashboard. The label `Operator` describes the local console role and is not an
authenticated person. Do not deploy the console until network controls limit
the origin to the intended operators.

## Agent capacity and actions

Pool rows show online, enabled Agents accepting new Jobs divided by all visible
members. Clicking anywhere in a Pool or Agent row selects it; its disclosure arrow
only expands/collapses the branch. The Agent card links the Pool by name, keeps
presence separate from admission permission, and places **Actions** in the header.
**Disconnect Agent / Connect Agent** disables/enables new-Job admission after review,
preserving the connection, registration and current execution. Graceful and forced
process draining remain distinct confirmed actions. **Change pool** opens the existing
idle-only reassignment form instead of occupying the main card.

Work/state disks show observed free bytes alongside total capacity and the observation
time. Missing measurements or host details are explicitly not reported. Hostname and
DNS share one field: `hostname (fqdn)` when both differ, otherwise the single reported
name. Agent and Pool headings omit their IDs; stable IDs remain in routes and APIs.
Host facts come from the Agent; there is no inbound listener/port or CPU-rank metric.
Runtime inventory uses current provider-neutral `executions`, falling back to legacy
`runtimes` only when no current routes were published; the contracts are not added
together. Older Agent binaries continue to work, but need updating/restarting to
publish the new optional host details.

## Task workflow authoring

**Create workflow** opens an empty local draft: no name, statuses or transitions
are copied from the Project default. The first added status becomes the initial
status; operators can select another initial status and configure terminal statuses
and permitted transitions. A draft may be cleared completely, but validation and
publication require at least one status.

The supplied default uses the same editor and version-publication commands as
custom workflows. Removing a draft status also removes its incident transitions
and terminal selection. If it was initial, the first remaining status becomes
initial. These edits affect only the draft until reviewed publication. Published
versions and existing tasks' workflow pins remain unchanged; selecting a default
for new tasks is a separate action.

## Supported topology

Terminate TLS and serve the static console at one reverse proxy. Route only
`/api/v1` and `/health` from that origin to the private management listener:

```text
trusted browser --HTTPS--> nginx --private HTTP--> 127.0.0.1:8080
                              |
                              +-- /srv/octacity-console/current/index.html
                              +-- /srv/octacity-console/current/assets/*
```

Keep `management_bind = "127.0.0.1:8080"`, as shown in
[`server.reverse-proxy.example.toml`](../reference/examples/server.reverse-proxy.example.toml).
Do not publish that port through a host firewall, container port mapping, load
balancer, or another virtual host. The server ignores forwarded identity and
address headers; the proxy does not create an authenticated operator identity.

The browser and API must share one origin. The console uses relative
`/api/v1` and `/health` URLs with browser credentials omitted. Do not add CORS,
cookies, an injected API origin, an authentication token, or a forwarded-user
header to this deployment. The console virtual host strips authorization and
forwarded-identity request headers and does not expose upstream CORS headers.

## Nginx fixture

The checked-in [`deployment/console/nginx`](../../deployment/console/nginx/)
directory is the supported nginx contract. It contains one complete example
and three shared snippets also consumed by the local stand:

- `nginx.conf` terminates TLS, serves the SPA, and proxies to a loopback
  management listener;
- `routes.conf` keeps API, health, static-file, and SPA fallback behavior
  disjoint;
- `cache-map.conf` distinguishes content-hashed assets from mutable content;
- `security-headers.conf` applies the browser security policy to success and
  error responses.

## Release archive

The platform-independent `octacity-console-<version>.tar.gz` release contains
the same `ui/dist` application consumed by the local-stand gateway. Packaging
does not rebuild or transform the SPA, so local stand and the downloadable
console cannot acquire different browser code paths. The archive is rooted at
`index.html` and contains content-hashed assets, release metadata,
`SHA256SUMS`, this deployment guide, the repository license, and the complete
copy-ready nginx fixture under `share/nginx/`. It contains no Node runtime,
package-manager metadata, source maps, or runtime API origin.

The canonical release contract declares both `application` (`index.html`) and
`proxy` (`share/nginx/nginx.conf`) components. Deploy a console only with an
`octacity-server` archive whose manifest has the same release version and exact
`build_inputs.octacity_revision`. The released-product verifier rejects a
mixed-version or mixed-revision pair before installation. The console remains
optional: the server archive and every REST workflow are complete without it.

After the locked UI build completes, create and verify the archive from the
repository root:

```shell
VERSION=0.1.0
REVISION="$(git rev-parse HEAD)"

python3 tools/package_console_release.py package \
  --distribution ui/dist \
  --version "$VERSION" \
  --octacity-revision "$REVISION" \
  --output "dist/octacity-console-${VERSION}.tar.gz"

python3 tools/package_console_release.py verify \
  --archive "dist/octacity-console-${VERSION}.tar.gz" \
  --expected-version "$VERSION" \
  --expected-octacity-revision "$REVISION"
```

The packager writes an external `.sha256` sidecar and verifies both it and the
archive before returning. The verifier independently rejects unsafe archive
paths and entry types, missing or extra checksum entries, modified payloads,
unhashed assets, development tooling, executable runtimes, source-bearing
output, and environment-specific management origins. Repeated packaging of
the same `ui/dist`, version, revision, license, contract, guide, and nginx
fixture produces the same archive bytes and release manifest.

For a downloaded release, verify the external sidecar before extraction and
then extract the complete archive into a new staging directory. Do not merge a
new asset set into a prior deployment directory.

For a source deployment, build the console and select the repository-owned
static and proxy inputs:

```shell
cd ui
corepack pnpm install --frozen-lockfile
corepack pnpm api:generate
corepack pnpm build
cd ..

STATIC_SOURCE="$PWD/ui/dist"
NGINX_SOURCE="$PWD/deployment/console/nginx"
RELEASE_ID="0.1.0-$(git rev-parse --short=12 HEAD)"
```

For a downloaded release, extract the already verified archive into a new
private staging directory, enter its root, and select the self-contained
inputs instead:

```shell
STATIC_SOURCE="$PWD"
NGINX_SOURCE="$PWD/share/nginx"
RELEASE_ID="$(python3 -c 'import json; print(json.load(open("release-manifest.json"))["version"])')-$(python3 -c 'import json; print(json.load(open("release-manifest.json"))["build_inputs"]["octacity_revision"][:12])')"
```

Install every release into a previously unused versioned directory. Validate
the proxy configuration before switching the stable `current` symlink with a
single same-filesystem rename:

```shell
RELEASE_DIR="/srv/octacity-console/releases/$RELEASE_ID"
test ! -e "$RELEASE_DIR"
sudo install -d -m 0755 "$RELEASE_DIR" /etc/nginx/octacity-console
sudo cp "$STATIC_SOURCE/index.html" "$RELEASE_DIR/index.html"
sudo cp -R "$STATIC_SOURCE/assets" "$RELEASE_DIR/assets"
sudo install -m 0644 "$NGINX_SOURCE/cache-map.conf" \
  "$NGINX_SOURCE/routes.conf" \
  "$NGINX_SOURCE/security-headers.conf" \
  /etc/nginx/octacity-console/
sudo install -m 0644 "$NGINX_SOURCE/nginx.conf" /etc/nginx/nginx.conf
sudo nginx -t
sudo ln -sfn "releases/$RELEASE_ID" /srv/octacity-console/.current-next
sudo mv -Tf /srv/octacity-console/.current-next /srv/octacity-console/current
sudo nginx -s reload
```

The source-build example reads `ui/dist` directly because that is also the
input to release packaging. Release metadata may remain in the private staging
directory. Never copy new files into an existing release directory.

Run the proxy and server under separately constrained service identities.
Certificate and private-key ownership, rotation, process supervision, firewall
policy, and access-log retention remain deployment responsibilities. Never put
certificate keys in the static root or console archive.

## Routing and fallback contract

The proxy applies these mutually exclusive rules:

| Request                                              | Result                                                      |
| ---------------------------------------------------- | ----------------------------------------------------------- |
| `/api/v1` and `/api/v1/*`                            | Proxied to management, including upstream 4xx/5xx responses |
| `/health` and `/health/*`                            | Proxied to management, including upstream 4xx/5xx responses |
| Existing `/assets/<content-hashed-name>`             | Served with its registered MIME type and immutable caching  |
| Missing asset or any other missing file-like path    | Static `404`; never `index.html`                            |
| `/`, `index.html`, or an extension-free UI deep link | SPA entry point                                             |

Do not replace the exact and `^~` API/health locations with a catch-all proxy
or an error-page rewrite. In particular, upstream `404`, `429`, and `503`
responses must retain their status and body rather than becoming a successful
HTML response. File-like misses such as `/missing.js` also remain `404`, which
prevents a JavaScript request from receiving HTML with a misleading MIME type.

The fixture includes nginx's standard MIME database and defaults unknown files
to `application/octet-stream`. JavaScript and CSS therefore retain their
registered media types, while `X-Content-Type-Options: nosniff` prevents a
browser from guessing a different type.

## Cache and browser security policy

`index.html`, API responses, health responses, deep-link fallbacks, and
unhashed files receive `Cache-Control: no-store`. Only filenames under
`/assets/` with a Vite-style content hash of at least eight characters receive
`Cache-Control: public, max-age=31536000, immutable`. Never make `index.html`
immutable: it is the pointer to the current hashed asset set and must change
immediately during rollout or rollback.

Every response carries:

- a Content Security Policy that permits scripts, styles, images, and API
  connections only from the same origin, prohibits objects, workers, forms,
  inline scripts, and framing, and allows only the one dynamic explorer-width
  style attribute required by the application;
- `X-Frame-Options: DENY` as a compatibility framing defense;
- `Referrer-Policy: no-referrer`;
- `X-Content-Type-Options: nosniff`;
- a restrictive `Permissions-Policy`; and
- one-year HTTP Strict Transport Security.

Serve this origin only over HTTPS. The fixture permits TLS 1.2 and TLS 1.3.
Use a certificate trusted by operator browsers and complete certificate
rotation before retiring the previous chain. Do not enable HSTS on a shared
hostname whose other applications are not ready for HTTPS-only operation.

## Factory workbench

For a task-oriented walkthrough from prepared inputs to an exact result, start
with [Admit and follow one Work item](factory.md#admit-and-follow-one-work-item).
This workbench exposes the current advanced configuration and Artifact-based
intake. The proposed text quickstart and unified Work card are follow-up work;
they are not required controls hidden elsewhere in this interface.

Open `/factory` from the Factory activity. Its explorer lazily loads visible
Projects, Factory Configurations and Runs in bounded pages. Run links remain
stable at `/factory/:factoryRunId`; lifecycle, configuration, source, decision,
delivery and admission-time filters can be shared in the query string. Search
Factory focuses these server-supported filters. Collapse and resize preserve
loaded branches. Favorites persist only the Run kind and identity.

A Project link opens its Factory workbench. Configuration and Flow lists expose
current publications and exact immutable versions, including copied links to
versions outside the first page. History controls load at most 100 version
choices per window. Named phase-ready queues use the policy identities returned
by the configuration detail endpoint. An empty configuration collection describes
Factory as optional and keeps ordinary CI/CD available.

Draft actions become available after the Project's configuration-choice request
succeeds, including an available catalog with no visible choices. A
`capability_unavailable` response disables creation and editing, explains that
Factory authoring is unavailable on this server, and retains access to published
definitions. Retry rechecks availability. The default server binary and local
stand keep Factory disabled; enabling it requires the explicit runtime
installation described in the [server architecture](../architecture/server.md#optional-factory-runtime-installation).

Create a draft or edit an existing publication. The workbench owns one draft at a
time and discards it when selecting another resource or leaving the page. Drafts,
validation previews and confirmed intents never enter browser persistence or
query strings. New configurations start disabled. New Flows contain no invented
logical nodes and request no execution permissions; operators choose names,
contracts, resource limits and routes through the forms.

The editor covers the closed technical node grammar, typed outcomes and
terminals, bounded control repeats, nested immutable subflows, independent
Context/data projections, input mappings, budgets and permission choices.
Logical Flow/node names have no special meaning. Execution, model and gate
bindings come from compatible published Flow profiles and visible exact resource
choices. Missing dependencies leave a profile unavailable. The editor preserves
frozen adapter parameters and task provenance instead of asking for provider
payloads, digests, scripts or mutable model aliases. Data-schema normalization
returns a server-computed exact reference without publishing a document.

**Navigation:** open **Factory**, click the Project name in its explorer, then
**AI agents**. This catalog is scoped to the selected Project.

The Project workbench also has **AI agents** (**AI-агенты**). These are ordinary
Factory Roles with immutable profiles, separate from the enrolled CI/CD Agents
and Agent Pools. Their catalog remains available without an execution installation.
Create a custom role, then add profiles, or copy a release template into independent
role/profile identities. Name, uploaded avatar, description, ordered shared instructions, shared skill
folders and concurrency belong to the role. A profile shows its exact shared-instruction pin
and additional instructions; adopting a newer role version is explicit.
Publishing definitions does not install an execution runtime or the source that
constructs the first retained package. Native Codex assembly supplies that source
for readonly observations, with exact historical content and current integration
authorization. Other adapters require their own qualified installation. See
[installation and compilation](factory.md#role-packages-in-ordinary-build-compilation).

Upload a PNG/JPEG/WebP avatar (up to 4 MiB and 4096 × 4096 source pixels); the
browser crops/resizes it to a 96 × 96 PNG. Server publication revalidates the
bounded raster through the common preview decoder. The image is immutable role
metadata; no external image URL is fetched.

Profile Runtime/Model choices come from the authorized capability catalog. Each profile, including arbitrary names such as default/pro, pins its own
Runtime/Model; names have no implicit provider meaning. With no installed choices,
configure Factory Runtime/Model settings on the server before creating execution
profiles; agent identity, shared instructions and skill authoring remain available.
The follow-up Console configuration workflow is planned in section 10 of
`add-factory-role-profiles`; it is not yet an available provider-setup screen. Thinking
uses the installed parameter schema, without provider-specific field names.
Environment entries, logical secret references, MCP/integrations, tools and budgets
stay within installed declarations. CLI arguments are shown as the exact approved
vector; changing it requires an installation change. Validation reports unsupported
settings before a reviewed independent profile publication.
Permission controls use the capability's separate plugin, executable, tool,
secret-profile and workload-identity sets; a tool is not offered as a plugin or
executable. Each optional grant can be removed and restored within its own set.

**Delete AI agent** in the agent card requires explicit confirmation against the
displayed role version. It removes the agent and its profiles from current choices,
including a pending stage's new manual selection. Existing Run history and exact
versions remain readable; deleting an agent does not cancel accepted Builds.
After success the card closes, the catalog refreshes and keyboard focus returns to
**Create AI agent**. Transport retry uses the same reviewed command and replay key.

**Import skill folder** stays inside the AI-agent draft. Choose a folder with root
`SKILL.md`, review its relative file inventory, then confirm import. Cancellation,
read/import failure and lost-response retry preserve the other draft fields.
A successful import selects its exact skill version in the current AI-agent draft,
preserving other selected skills. Saving the agent remains a separate reviewed publication.
Use **View skill** to inspect its retained relative files and open a bounded plain-text
preview. Markdown, HTML and scripts are shown as text. Binary/large files retain
metadata without an inline text preview. Browsing preserves all draft fields.
Import alone grants no execution compatibility: execution still uses the pinned role version and the existing retained-input and
permission boundaries. Historical profile skill additions remain readable; new
profiles use the shared agent set. Folder contents, settings and instructions never
enter navigation or browser persistence, and local edits require explicit reimport.

Reasoning stages can replace an existing static Build binding with role-backed
candidates while retaining its result contract and task provenance. Manual selection
can wait for an operator. Ordered rules use the common typed predicate grammar;
Decision API uses an installed profile-purpose policy and an explicit input mapping.
Its safe catalog metadata shows the provider/model, semantic criteria version,
rollout, calibration, baseline and probability/margin thresholds in millionths.
For each candidate, explicitly choose its **Selector answer** from the installed
finite domain. These keys are independent of profile names/IDs; remapping a
candidate preserves its selected reserve recovery. Every installed answer must
have one pinned candidate, and Flow/configuration validation rejects missing or
mismatched keys before publication when that exact policy declares the domain.
These facts are derived from the validated installed selector, not another editable
policy. Restricted catalogs omit details whose dependencies are hidden. Missing
metadata does not establish qualification. Runtime also checks the exact installed
input projection, call budget and authority before selection; publication is not
a guarantee that the capability will remain available at execution time.
Choose an explicit reserve or manual recovery, and review exact candidate pins and
failure policy before publishing. Rules and semantic criteria are never executed
by the browser.

**Override stage profiles** in manual Work admission reads the selected immutable
configuration and offers only its pinned candidates, including nested stage paths.
Omitting overrides preserves existing static/manual admission. A Run awaiting a
choice shows its exact input, eligible profiles, safe failure reason and original
deadline. Confirmation fixes the target, profile and Run version. A recorded intent
is not a consumed choice or a Build; use the existing profile-selection journal for
rule/provider/reserve history, supersession and Build links. New commands are blocked
at the deadline; a transport replay retains its original idempotency key. Stale
commands require refresh and another review.

Admission permission presets come from visible `permission_ceiling` choices with
deployment-declared `metadata.permission_declarations`. Selecting a preset only
offers bounded hosts, mounts, exact command patterns and output kinds; it does
not copy grants into the draft. Each requested value still needs an explicit
selection, and execution/node requests remain separate. A command selection also
requests its exact executable identity. Presets containing hidden or unavailable
capability/credential references are omitted from the declarations. Without a
declared preset, an existing immutable baseline supplies its own declarations;
an empty draft has none. The preset and baseline survive closing Flow settings.

Subflow selection imports the exact schemas of its published closure without
replacing the parent's existing catalogue or granting input/context access.
Native gate profiles use the server's `native_gate_policies` discovery facts;
the browser does not recognize native evaluators by their logical names. Schema
and mapping depth limits come from the generated OpenAPI `x-max-depth` contract,
and enum alternatives retain their selected string, integer, boolean or null type.

Validate the draft before review. Generated OpenAPI constraints and local graph
checks give early field feedback; server validation remains authoritative.
Normalized values, required capabilities and effective Flow permission narrowing
appear in the review area. Project, configuration, task and Agent policy may
narrow these ceilings again. Field-error buttons focus the affected control or
section without modifying the URL or unrelated draft values.

Review opens the shared confirmed-intent dialog. Confirmation publishes one
immutable version with one idempotency key and, for replacement, the discovered
current-version precondition. A historical version may supply the draft while the
precondition still names the current publication. A safe transport retry reuses
the frozen request and key. A stale baseline requires explicit refresh and a new
validation; refresh keeps the draft values. Abandoning closes the intent.
Successful publication invalidates only the owning collections, version/current
reads and audit projection. Existing Runs keep their pinned definitions.

The Factory authoring schemas load with the Factory views. The initial bundle
retains its 450 KiB limit. The complete console, including the Issue journal and
PR/MR cards and Checks/Artifacts panes, has a 1,130 KiB JavaScript and 1,216 KiB
static-output ceiling. The measured result-panel build and qualification are
recorded in [task 6.15 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-6.15.md).
The 85 KiB CSS limit and 53-file cap remain unchanged. The result panels add
20 KiB to each aggregate allowance over the review-card baseline; the architecture
test rejects an additional unplanned 16 KiB chunk. Runtime dependencies, network
and persistence boundaries and product-asset allowances remain unchanged. Generated
validation contracts omit documentation strings while OpenAPI retains them.

### Manual Work and Run investigation

Select a visible Project and **Admit Work**. Choose an enabled immutable Factory
Configuration, a versioned Repository and its permitted source selection. Task,
acceptance criteria and optional specifications are selected by name from the
Artifacts of a visible input Build. Prepare those immutable inputs through the
existing Build/Artifact workflow; the form sends their exact references and never
fetches their bodies. Specification counts and Work classification use generated
API bounds. Review shows the selected configuration identity, Repository version and exact revision
or permitted reference from the frozen request. Review freezes the selected versions and inputs; confirmation opens
the Run identity returned by the server. Discard, leaving the workbench or reloading
removes unsubmitted values. Inputs and confirmation bodies remain in memory.

A Run separates its recorded overview and budget consumption, available commands,
execution journal, Flow hierarchy and pinned configuration. Exact identities and
configuration details are disclosures. Choose a node in the pinned graph to see
its outgoing transitions and bounded repeats. Context and data connections have
separate views; the adjacent table preserves full outcomes, targets and bounds.
This is configuration, not inferred execution history. The Flow hierarchy graph
and table show relationships from loaded journal pages only. The tree groups loaded
children under their parents and routes links outside neighboring cards. Missing
parents on a bounded page are not reconstructed.

Choose a journal collection and inspect a record to open its exact safe metadata.
`diagnostic`, `diagnostic_id`, `diagnostic_after`, `flow_run_id`, `cycle_id` and `node_attempt_id` support
copied diagnostic links. Missing rows on a bounded page do not prove missing work.
Accepted completions, confirmed stops, active attempts and retry eligibility are
identified from server facts. Signals expose typed answers, purpose, mode,
fallback, exact provenance, deterministic baseline and final disposition. Their
recommendations and probabilities never become Decisions or delivery success.
Historical signal records retain their original representation; unavailable new
metadata is not reconstructed. Calls show immutable node identities and frozen
Task/Context provenance; current `active` and `retryable` flags belong to the
snapshot-backed Node Attempts projection and are absent from Calls. Unavailable
completion/stop details are labeled explicitly. Supporting diagnostic requests
retain their own loading, stale, failure and retry presentations even when another
journal collection is selected. Payloads, prompts, traces, findings,
credentials and transfer URLs have no renderer.

Cancellation, eligible retry, declared HumanGate responses and configured
escalation/delivery/promotion requests share the confirmed-intent dialog. A
HumanGate editor follows the exact pinned outcome schema; operator dispositions
follow the pinned closed binding. Delivery and promotion require a recorded
accepted candidate. Confirmation captures the Run version and exact node/cycle
and candidate context. The server rechecks authority. Transport replay retains
the same body, idempotency key and strong precondition. A stale version or state
conflict offers explicit refresh and requires a new confirmation. No command
claims a merge or manufactures a Decision. Accepted commands invalidate only the
owning Run, diagnostics, relevant Run/pool collections and audit projection;
the result links to the Run's audit journal.

Factory errors retain scoped request correlation and loaded data with explicit
stale notices. Ordinary CI/CD navigation remains available when Factory providers
fail. Keyboard confirmations contain focus, Escape abandons before submission,
and dismissal restores the invoking control. Read-only, secret-free fixtures
supply theme/language/narrow visual snapshots. Browser scenarios that type
unsubmitted content disable screenshots, videos and traces.

## Verification

Set the deployed origin and verify the route classes independently:

```shell
ORIGIN=https://console.internal.example.test
INDEX_SHA256="$(sha256sum /srv/octacity-console/current/index.html | cut -d' ' -f1)"

curl --fail --silent --show-error "$ORIGIN/builds/example" \
  | sha256sum | grep "$INDEX_SHA256"

ASSET="$(find /srv/octacity-console/current/assets -maxdepth 1 -type f -name '*.js' -print -quit)"
curl --fail --silent --show-error --head \
  "$ORIGIN/assets/$(basename "$ASSET")"

test "$(curl --silent --output /tmp/octacity-api-miss \
  --write-out '%{http_code}' "$ORIGIN/api/v1/unknown")" = 404
test "$(curl --silent --output /tmp/octacity-health-miss \
  --write-out '%{http_code}' "$ORIGIN/health/unknown")" = 404
test "$(curl --silent --output /tmp/octacity-file-miss \
  --write-out '%{http_code}' "$ORIGIN/missing.js")" = 404

! cmp -s /tmp/octacity-api-miss /srv/octacity-console/current/index.html
! cmp -s /tmp/octacity-health-miss /srv/octacity-console/current/index.html
! cmp -s /tmp/octacity-file-miss /srv/octacity-console/current/index.html

curl --fail --silent --show-error --dump-header - --output /dev/null \
  "$ORIGIN/projects/example"
```

Inspect the final command for `Cache-Control: no-store`, the declared CSP,
HSTS, framing, referrer, and `nosniff` headers. Inspect the asset response for
the correct JavaScript MIME type and immutable cache policy. Verify that the
server itself remains reachable only from the proxy host or protected proxy
network.

Browser developer tools must show relative same-origin `/api/v1` requests with
no `Cookie` or `Authorization` request header, and API responses must not add
`Access-Control-Allow-Origin` or `Access-Control-Allow-Credentials`. Active Job
event requests must retain the published bounded `after`, `limit`, and
`wait_ms` parameters; do not replace them with an unbounded polling proxy.

CI verifies the packaged—not development—console through the packaged proxy
against the matching released PostgreSQL-backed server. That slice exercises
Project and Build discovery, a lost-response idempotent mutation replay,
bounded live Job following, absence of browser credentials and CORS, and a
headless REST workflow after nginx is stopped. Treat failure of that slice as
a release blocker rather than substituting the Vite development server.

The local-stand gateway image executes the same routing snippets against a
mock management upstream. Its contract check compares deep-link output with
the real `index.html`, validates JavaScript MIME and cache headers, and proves
that API, health, and file misses do not return the SPA.

## Rollout and rollback

Stage a complete immutable release directory and validate nginx before
atomically replacing the `current` symlink. Because assets are
content-addressed, an old `index.html` can continue to use its matching old
assets during a gradual rollout.

Rollback by atomically switching `current` to the retained previous release
directory and reloading the proxy. Removing the console static route does not
remove or alter any REST workflow; headless clients continue to use the
private management API.

## Legacy tasks and unavailable requirements

A migrated task keeps its human key and original execution identity. **Runs**
separates reviewed execution requests from **Legacy Artifact executions**, which
have no invented request or requirements revision. Selecting original Work is
retained in the card URL while switching sections. The original Run link opens
its existing diagnostics/log/control surface. A Run's **Open task** link is
shown only with independently authorized association metadata; pending/hidden
association is explicitly unavailable.

The same stage/Job hierarchy, original event stream and Results/Checks/Artifacts
views also accept selected Artifact Work without an execution request. Selecting
a reviewed request clears the previous Work selection and its event/result detail.
If legacy history changes between pages, **First page** remains available after
the cursor conflict; retrying the stale cursor is not the only recovery option.

Expired or never-retained requirements use localized unavailable text in both
current and historical views. This does not remove original execution history,
renew Artifact retention or make a new empty draft. A later explicit edit creates
a new revision. Content retention/holds are administrative REST operations,
separate from ordinary task editing; see [Issue lifetime](../reference/issues.md#collaboration-content-lifetime).

The PostgreSQL/browser pilots use the same generated clients with a real runtime,
not fixture responses. Their disposable service requirements and exact commands
are recorded in [section 8 verification](../../openspec/changes/archive/2026-10-22-add-factory-work-card/verification-8.md).
