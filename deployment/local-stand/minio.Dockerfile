# check=skip=InvalidDefaultArgInFrom
# Digest-qualified bases are mandatory inputs supplied by build_pinned_minio.py.
ARG GO_IMAGE
ARG RUNTIME_IMAGE

FROM ${GO_IMAGE} AS minio-builder
ARG MINIO_REVISION
ARG MINIO_SHORT_REVISION
ARG MINIO_VERSION
ARG MINIO_RELEASE_TAG
ARG MINIO_COPYRIGHT_YEAR
WORKDIR /src/minio
COPY minio-source/ ./
RUN set -eu; \
    ldflags="-s -w \
      -X github.com/minio/minio/cmd.Version=${MINIO_VERSION} \
      -X github.com/minio/minio/cmd.CopyrightYear=${MINIO_COPYRIGHT_YEAR} \
      -X github.com/minio/minio/cmd.ReleaseTag=${MINIO_RELEASE_TAG} \
      -X github.com/minio/minio/cmd.CommitID=${MINIO_REVISION} \
      -X github.com/minio/minio/cmd.ShortCommitID=${MINIO_SHORT_REVISION} \
      -X github.com/minio/minio/cmd.GOPATH=/go \
      -X github.com/minio/minio/cmd.GOROOT=/usr/local/go"; \
    CGO_ENABLED=0 GOOS=linux go build \
      -tags kqueue \
      -trimpath \
      -ldflags "$ldflags" \
      -o /out/minio

FROM ${GO_IMAGE} AS mc-builder
ARG MC_REVISION
ARG MC_SHORT_REVISION
ARG MC_VERSION
ARG MC_RELEASE_TAG
ARG MC_COPYRIGHT_YEAR
WORKDIR /src/mc
COPY mc-source/ ./
RUN set -eu; \
    ldflags="-s -w \
      -X github.com/minio/mc/cmd.Version=${MC_VERSION} \
      -X github.com/minio/mc/cmd.CopyrightYear=${MC_COPYRIGHT_YEAR} \
      -X github.com/minio/mc/cmd.ReleaseTag=${MC_RELEASE_TAG} \
      -X github.com/minio/mc/cmd.CommitID=${MC_REVISION} \
      -X github.com/minio/mc/cmd.ShortCommitID=${MC_SHORT_REVISION}"; \
    CGO_ENABLED=0 GOOS=linux go build \
      -tags kqueue \
      -trimpath \
      -ldflags "$ldflags" \
      -o /out/mc

FROM ${RUNTIME_IMAGE} AS minio
COPY --from=minio-builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=minio-builder /out/minio /usr/local/bin/minio
COPY --from=mc-builder /out/mc /usr/local/bin/mc
RUN mkdir -p /data && chown 65534:65534 /data
USER 65534:65534
EXPOSE 9000 9001
ENTRYPOINT ["minio"]
CMD ["server", "/data", "--console-address", ":9001"]

FROM ${RUNTIME_IMAGE} AS mc
COPY --from=mc-builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=mc-builder /out/mc /usr/local/bin/mc
USER 65534:65534
ENTRYPOINT ["mc"]
