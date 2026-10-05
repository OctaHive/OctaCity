ARG RUST_IMAGE=scratch
ARG NODE_IMAGE=scratch
ARG NGINX_IMAGE=scratch
ARG ALPINE_IMAGE=scratch

FROM ${RUST_IMAGE} AS rust-builder
WORKDIR /workspace
COPY --from=octa-source . /workspace/octa
COPY . /workspace/octacity
WORKDIR /workspace/octacity

FROM rust-builder AS server-builder
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/octacity/target,sharing=locked \
    set -eu; \
    cargo build --locked --release -p octacity-server --bin octacity-server; \
    binary=target/release/octacity-server; \
    install -D -m 0755 "$binary" /out/rootfs/usr/local/bin/octacity-server; \
    ldd "$binary" | awk '{ for (field = 1; field <= NF; field++) if ($field ~ /^\//) print $field }' | \
      sort -u | while IFS= read -r library; do cp --parents -L "$library" /out/rootfs; done; \
    mkdir -p /out/rootfs/etc; \
    cp /etc/nsswitch.conf /out/rootfs/etc/nsswitch.conf; \
    install -D -m 0644 /etc/ssl/certs/ca-certificates.crt /out/rootfs/etc/ssl/certs/ca-certificates.crt; \
    printf 'octacity:x:65532:65532:OctaCity:/nonexistent:/sbin/nologin\n' > /out/rootfs/etc/passwd; \
    printf 'octacity:x:65532:\n' > /out/rootfs/etc/group

FROM ${ALPINE_IMAGE} AS server
ARG OCTACITY_VERSION
ARG OCTACITY_REVISION
ARG OCTA_REVISION
ARG OCTA_SOURCE_SHA256
LABEL org.opencontainers.image.title="OctaCity server" \
      org.opencontainers.image.version="${OCTACITY_VERSION}" \
      org.opencontainers.image.revision="${OCTACITY_REVISION}" \
      dev.octacity.octa.revision="${OCTA_REVISION}" \
      dev.octacity.octa.source-sha256="${OCTA_SOURCE_SHA256}"
COPY --from=server-builder /out/rootfs /
COPY --chmod=0755 deployment/local-stand/server-entrypoint.sh /usr/local/bin/octacity-server-entrypoint
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/octacity-server-entrypoint"]

FROM rust-builder AS openapi-builder
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/octacity/target,sharing=locked \
    set -eu; \
    mkdir -p /out; \
    cargo run --quiet --locked -p octacity-server-api-rest --example export_openapi > /out/openapi.json

FROM ${NODE_IMAGE} AS ui-builder
WORKDIR /workspace/ui
COPY ui/package.json ui/pnpm-lock.yaml ui/.npmrc ui/.node-version ./
RUN --mount=type=cache,target=/root/.cache/node/corepack,sharing=locked \
    --mount=type=cache,target=/pnpm/store,sharing=locked \
    set -eu; \
    corepack enable; \
    pnpm_config_store_dir=/pnpm/store corepack pnpm install --frozen-lockfile --ignore-scripts
COPY ui/ ./
COPY --from=openapi-builder /out/openapi.json /tmp/octacity-openapi.json
RUN set -eu; \
    OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-openapi.json corepack pnpm api:generate; \
    corepack pnpm build

FROM ${NGINX_IMAGE} AS gateway
ARG OCTACITY_VERSION
ARG OCTACITY_REVISION
ARG NODE_VERSION
ARG PNPM_VERSION
LABEL org.opencontainers.image.title="OctaCity local gateway" \
      org.opencontainers.image.version="${OCTACITY_VERSION}" \
      org.opencontainers.image.revision="${OCTACITY_REVISION}" \
      dev.octacity.node.version="${NODE_VERSION}" \
      dev.octacity.pnpm.version="${PNPM_VERSION}"
COPY --from=ui-builder /workspace/ui/dist/ /srv/octacity-ui/
COPY deployment/console/nginx/cache-map.conf /etc/nginx/octacity-console/cache-map.conf
COPY deployment/console/nginx/routes.conf /etc/nginx/octacity-console/routes.conf
COPY deployment/console/nginx/security-headers.conf /etc/nginx/octacity-console/security-headers.conf
COPY deployment/local-stand/nginx.conf /etc/nginx/nginx.conf
USER 101:101
EXPOSE 8443
ENTRYPOINT ["nginx"]
CMD ["-g", "daemon off;"]
