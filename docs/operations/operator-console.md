# Operator console deployment

This runbook defines the supported same-origin deployment for the OctaCity
operator console. The console and management API have no operator login,
session, RBAC, or browser credential. Deploy this origin only on a trusted
operator network. It is not safe for public Internet exposure.

Access to this origin is management authority, not merely access to a read-only
dashboard. The label `Operator` describes the local console role and is not an
authenticated person. Do not deploy the console until network controls limit
the origin to the intended operators.

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

| Request | Result |
| --- | --- |
| `/api/v1` and `/api/v1/*` | Proxied to management, including upstream 4xx/5xx responses |
| `/health` and `/health/*` | Proxied to management, including upstream 4xx/5xx responses |
| Existing `/assets/<content-hashed-name>` | Served with its registered MIME type and immutable caching |
| Missing asset or any other missing file-like path | Static `404`; never `index.html` |
| `/`, `index.html`, or an extension-free UI deep link | SPA entry point |

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
