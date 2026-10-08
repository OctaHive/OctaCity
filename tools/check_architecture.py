#!/usr/bin/env python3
"""Enforce OctaCity product and server-layer dependency directions."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any, Sequence


SERVER_ROLES = frozenset({"composition", "api", "application", "core", "protocol", "infrastructure", "test"})
PRODUCT_AREAS = frozenset({"agent", "cli", "server"})
ALLOWED_TARGETS = {
    "agent": frozenset({"agent", "shared"}),
    "api": frozenset({"application", "shared"}),
    "application": frozenset({"core", "shared"}),
    "cli": frozenset({"shared"}),
    "composition": frozenset({"api", "application", "core", "protocol", "infrastructure", "shared"}),
    "core": frozenset({"core", "shared"}),
    "infrastructure": frozenset({"core", "protocol", "shared"}),
    "protocol": frozenset({"protocol", "shared"}),
    "shared": frozenset({"shared"}),
    "test": frozenset({
        "agent",
        "api",
        "application",
        "cli",
        "composition",
        "core",
        "infrastructure",
        "protocol",
        "shared",
        "test",
    }),
}

ROLE_ALLOWED_EXTERNAL_DEPENDENCIES = {
    # Transport adapters may use transport/runtime support, but every new
    # external dependency still requires an explicit architecture-policy edit.
    "api": frozenset({
        "async-trait",
        "axum",
        "http",
        "serde",
        "serde_json",
        "thiserror",
        "tokio",
        "tower",
        "tracing",
        "uuid",
    }),
    # Application and core crates are intentionally limited to foundational
    # libraries. Provider, persistence, transport, and configuration SDKs are
    # rejected without relying on a finite denylist of known implementations.
    "application": frozenset({"async-trait", "serde", "serde_json", "thiserror", "tracing", "uuid"}),
    "core": frozenset({
        "async-trait",
        "serde",
        "serde_json",
        "sha2",
        "thiserror",
        "tracing",
        "uuid",
        "zeroize",
    }),
    # Cross-product and process-boundary contracts stay dependency-light. Any
    # new library requires an explicit policy decision instead of evading a
    # finite list of known databases, brokers, or provider SDKs.
    "shared": frozenset({
        "base64",
        "ed25519-dalek",
        "octa-cache-protocol",
        "serde",
        "serde_json",
        "thiserror",
        "zeroize",
    }),
    "protocol": frozenset({"base64", "serde", "serde_json", "thiserror"}),
}

# Narrow package capabilities that are part of a core security boundary rather
# than transport, persistence, or provider integration. Keep these exceptions
# package-specific so another core crate cannot acquire private-key handling by
# depending on the same libraries.
PACKAGE_ALLOWED_EXTERNAL_DEPENDENCIES = {
    # Canonical Agent/server action bytes are hashed at their shared wire
    # contract boundary. Keep the cryptographic primitive package-specific so
    # other shared crates cannot acquire implementation dependencies by
    # implication.
    "octacity-protocol": frozenset({"sha2"}),
    # The shared observability contract emits through provider-neutral facades;
    # it owns no recorder, exporter, queue, or subscriber. tracing-subscriber
    # is test-only and verifies structured correlation at the facade boundary.
    "octacity-observability": frozenset({"metrics", "tracing", "tracing-subscriber"}),
    # URL syntax is validated before an endpoint enters an Agent capability.
    # The same boundary decodes shared runner output frames and keeps configured
    # redaction material in zeroing memory before any durable write.
    "octacity-server-application": frozenset({"base64", "url", "zeroize"}),
    # Cache namespace syntax follows Octa's public protocol while HMAC protects
    # the server-private credential derivation boundary.
    "octacity-server-cache": frozenset({"blake3", "hmac", "octa-cache-protocol", "zstd"}),
    "octacity-server-job": frozenset({"base64", "ed25519-dalek"}),
    "octacity-server-secrets": frozenset({"hmac"}),
    "octacity-server-trigger": frozenset({"chrono", "chrono-tz", "cron"}),
}

# Composition, infrastructure, Agent, CLI, and test packages intentionally
# select concrete implementations, so they cannot use the narrow layer
# allowlists above. Freeze their v1 external package vocabulary here instead:
# a new SDK requires an explicit architecture-policy review rather than being
# admitted by virtue of living near the composition root.
V1_APPROVED_EXTERNAL_DEPENDENCIES = frozenset({
    "async-trait",
    "aws-sdk-s3",
    "axum",
    "base64",
    "blake3",
    "chrono",
    "chrono-tz",
    "clap",
    "containerd-client",
    "cron",
    "ed25519-dalek",
    "fs4",
    "futures-util",
    "hex",
    "hmac",
    "http",
    "libc",
    "memchr",
    "metrics",
    "metrics-exporter-prometheus",
    "microsandbox",
    "octa-cache-protocol",
    "octa-plugin-lock",
    "octa-runner-protocol",
    "processkit",
    "prost",
    "prost-types",
    "rcgen",
    "reqwest",
    "rustix",
    "rustls",
    "seccompiler",
    "serde",
    "serde_json",
    "serde_yaml_ng",
    "sha2",
    "sqlx",
    "sysinfo",
    "tar",
    "tempfile",
    "thiserror",
    "tokio",
    "tokio-rustls",
    "tokio-util",
    "toml",
    "tower",
    "tracing",
    "tracing-layer-win-eventlog",
    "tracing-subscriber",
    "url",
    "uuid",
    "windows-service",
    "windows-sys",
    "zeroize",
    "zstd",
})

# These SDK families belong to explicitly deferred extension work. The broad
# v1 inventory above catches unknown package names in implementation-owning
# layers; these rules additionally keep named deferred families out of every
# layer even if somebody tries to add them to another allowlist.
DEFERRED_EXTENSION_SDK_RULES = (
    ("operator authentication", re.compile(r"(?:openidconnect|oauth2|jsonwebtoken)")),
    ("LDAP/TOTP", re.compile(r"(?:ldap3|oath|totp-rs)")),
    ("GraphQL", re.compile(r"(?:async-graphql(?:-.+)?|juniper(?:-.+)?)")),
    ("Kafka/NATS", re.compile(r"(?:rdkafka(?:-.+)?|kafka(?:-.+)?|async-nats|nats)")),
    ("managed GitHub/Gerrit", re.compile(r"(?:octocrab|hubcaps|github-api|github-rs|gerrit(?:-.+)?)")),
    (
        "secret stores",
        re.compile(
            r"(?:vaultrs|hashicorp-vault|aws-sdk-secretsmanager|azure-security-keyvault|google-cloud-secretmanager)"
        ),
    ),
    (
        "additional execution or infrastructure providers",
        re.compile(r"(?:proxmox(?:-.+)?|vsphere(?:-.+)?|vmware-vsphere|firecracker-sdk)"),
    ),
)

# Shared infrastructure modules contain reusable mechanics rather than a
# concrete adapter selection. Keep this exception package-specific so ordinary
# infrastructure crates cannot couple to one another.
INFRASTRUCTURE_SUPPORT_PACKAGES = frozenset({"octacity-server-adapter-host"})

SHARED_SOURCE_RULES = (
    (
        "transport DTO implementation",
        re.compile(r"\b(?:actix_web|async_graphql|axum|rocket|utoipa)::|\bpub\s+(?:struct|enum|type)\s+\w+(?:Rest|Http)(?:Dto|Request|Response)\b"),
    ),
    (
        "SQL row implementation",
        re.compile(r"\b(?:diesel|sea_orm|sqlx|tokio_postgres)::|#\s*\[\s*(?:diesel|sqlx)\b|\bFromRow\b"),
    ),
    (
        "provider-specific payload",
        re.compile(r"\bpub\s+(?:struct|enum|type)\s+(?:Bitbucket|Gerrit|GitHub|Github|GitLab|Gitlab)\w*"),
    ),
    (
        "server domain entity",
        re.compile(r"\bpub\s+(?:struct|enum|type)\s+(?:AgentPool|ArtifactRecord|Attempt|AuditFact|Build|BuildConfiguration|Pipeline|PipelineNode|Project|ProjectTree|TriggerOccurrence)\b"),
    ),
)

REST_API_SOURCE = Path("server/api/octacity-server-api-rest/src")
SERVER_COMPOSITION_SOURCE = Path("server/app/src")
SERVER_INFRASTRUCTURE_SOURCE = Path("server/infrastructure")
MANAGEMENT_SECURITY_SOURCE = Path("server/application/src/management_security")
POSTGRES_STORE_SOURCE = Path("server/infrastructure/octacity-server-store-postgres/src")
FACTORY_CORE_PACKAGE = "octacity-server-factory"
FACTORY_CORE_SOURCE = Path("server/core/octacity-server-factory/src")
FACTORY_INDEPENDENT_LIFECYCLE_PACKAGES = frozenset({
    "octacity-server-domain",
    "octacity-server-trigger",
    "octacity-server-pipeline",
    "octacity-server-job",
    "octacity-server-orchestrator",
    "octacity-server-scheduler",
})
FACTORY_CORE_SOURCE_RULES = (
    (
        "provider-specific import",
        re.compile(
            r"(?mi)^\s*(?:use|extern\s+crate)\s+"
            r"(?:anthropic|codex|github|gitlab|jev|openai)(?:_|::|\b)"
        ),
    ),
    (
        "provider-specific type",
        re.compile(
            r"\b(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|type)\s+"
            r"(?:Anthropic|Codex|GitHub|GitLab|JEV|Jev|OpenAI|OpenAi)\w*"
        ),
    ),
    (
        "SQL row implementation",
        re.compile(r"\b(?:diesel|sea_orm|sqlx|tokio_postgres)::|#\s*\[\s*(?:diesel|sqlx)\b|\bFromRow\b"),
    ),
    (
        "HTTP DTO implementation",
        re.compile(
            r"\b(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|type)\s+"
            r"\w*(?:Http|Rest)(?:Dto|Request|Response)\b"
        ),
    ),
    (
        "UI state implementation",
        re.compile(
            r"\b(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|type)\s+"
            r"\w*(?:Ui|UI)(?:Model|State|ViewModel)\b"
        ),
    ),
)
FACTORY_DUPLICATE_LIFECYCLE_TYPE = re.compile(
    r"\b(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|type|trait)\s+"
    r"(?:Attempt|AttemptState|Build|BuildState|Job|JobState|Lease|LeaseState|Pipeline|PipelineVersion)\b"
)
TYPED_MANAGEMENT_ROUTE_REGISTRY = Path("v1/adapter/routes.rs")
MANAGEMENT_PATH_DECLARATIONS = Path("v1/openapi/operations")
MANAGEMENT_PREFIX_OWNERS = frozenset({
    Path("v1/mod.rs"),
    Path("v1/openapi.rs"),
    TYPED_MANAGEMENT_ROUTE_REGISTRY,
})
OPERATIONAL_ROUTE_PATHS = frozenset({"/health/live", "/health/ready", "/metrics"})
ROUTE_LITERAL = re.compile(r"\.\s*route\s*\(\s*\"([^\"]+)\"")
STORE_SELECTED_MANAGEMENT_ACTOR = re.compile(
    r'"(?:un)?authenticated_management"|AuditActorKind::(?:Authenticated|Unauthenticated)Management'
)
UNTYPED_MUTATION_FACTS = re.compile(
    r"\bMutationFacts\s*\{[^}]*\bactor_kind\s*(?::|[,}])",
    re.DOTALL,
)
MANAGEMENT_AUTHORIZATION_IMPLEMENTATION = re.compile(
    r"\b(?:ManagementAuthorizationPolicy|TrustedNetworkManagementPolicy|"
    r"AuthorizedCommandHandler|AuthorizedQueryHandler)\b|\.\s*authorize\s*\("
)
UNDECORATED_MANAGEMENT_HANDLER = re.compile(
    r"\b(?:ManagementCommandUseCase|ManagementQueryUseCase)\b"
)
RAW_MANAGEMENT_CREDENTIAL_FIELD = re.compile(
    r"(?:^|[{,])\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"[A-Za-z0-9_]*(?:credential|password|token|cookie|header|certificate|claim)"
    r"[A-Za-z0-9_]*\s*:",
    re.MULTILINE,
)

SHARED_FORBIDDEN_SOURCE_DIRECTORIES = frozenset({
    "database",
    "domain",
    "gerrit",
    "github",
    "gitlab",
    "http",
    "postgres",
    "providers",
    "rest",
    "sql",
})

class MetadataError(ValueError):
    """Raised when Cargo metadata cannot be interpreted safely."""


@dataclass(frozen=True)
class Package:
    """One workspace package and its architecture role."""

    name: str
    manifest_path: Path
    role: str
    dependencies: tuple[str, ...]
    external_dependencies: tuple[str, ...]
    shared_contract: str | None
    shared_consumers: tuple[str, ...]
    shared_scaffold: bool
    shared_policy_valid: bool


@dataclass(frozen=True)
class Edge:
    """One dependency between workspace packages, including test-only edges."""

    source: Package
    target: Package
    kind: str


@dataclass(frozen=True)
class Graph:
    """Normalized workspace package graph."""

    packages: tuple[Package, ...]
    edges: tuple[Edge, ...]


@dataclass(frozen=True)
class Violation:
    """One dependency-policy violation."""

    code: str
    message: str


def _normalized_path(value: str | Path) -> Path:
    return Path(value).expanduser().absolute()


def classify(manifest_path: Path, workspace_root: Path) -> str:
    """Classify a workspace manifest by its product and server-layer path."""

    try:
        relative = manifest_path.relative_to(workspace_root)
    except ValueError as error:
        raise MetadataError(f"workspace manifest is outside workspace root: {manifest_path}") from error

    parts = relative.parts
    if not parts:
        return "unknown"
    if parts[0] in {"agent", "shared", "cli"}:
        return parts[0]
    if parts[0] != "server" or len(parts) < 2:
        return "unknown"
    return {
        "app": "composition",
        "api": "api",
        "application": "application",
        "core": "core",
        "infrastructure": "infrastructure",
        "protocols": "protocol",
        "tests": "test",
    }.get(parts[1], "unknown")


def graph_from_metadata(metadata: dict[str, Any]) -> Graph:
    """Normalize Cargo metadata or a metadata-shaped fixture into a graph."""

    try:
        workspace_root = _normalized_path(metadata["workspace_root"])
        raw_packages = metadata["packages"]
    except (KeyError, TypeError) as error:
        raise MetadataError("metadata must contain workspace_root and packages") from error
    if not isinstance(raw_packages, list):
        raise MetadataError("metadata packages must be a list")

    workspace_members = metadata.get("workspace_members")
    member_ids = set(workspace_members) if isinstance(workspace_members, list) else None
    packages_by_directory: dict[Path, Package] = {}
    raw_by_directory: dict[Path, dict[str, Any]] = {}

    for raw in raw_packages:
        if not isinstance(raw, dict):
            raise MetadataError("each metadata package must be an object")
        if member_ids is not None and raw.get("id") not in member_ids:
            continue
        try:
            manifest_path = _normalized_path(raw["manifest_path"])
            name = raw["name"]
        except (KeyError, TypeError) as error:
            raise MetadataError("each workspace package must contain name and manifest_path") from error
        if not isinstance(name, str) or not name:
            raise MetadataError("workspace package names must be non-empty strings")
        directory = manifest_path.parent
        if directory in packages_by_directory:
            raise MetadataError(f"multiple workspace packages use {directory}")
        dependencies = raw.get("dependencies", [])
        if not isinstance(dependencies, list):
            raise MetadataError(f"dependencies for {name} must be a list")
        dependency_names: list[str] = []
        for dependency in dependencies:
            if not isinstance(dependency, dict):
                raise MetadataError(f"dependency for {name} must be an object")
            dependency_name = dependency.get("name")
            if not isinstance(dependency_name, str) or not dependency_name:
                raise MetadataError(f"dependency for {name} must have a non-empty name")
            dependency_names.append(dependency_name)

        package_metadata = raw.get("metadata")
        octacity_metadata = package_metadata.get("octacity") if isinstance(package_metadata, dict) else None
        shared_metadata = octacity_metadata.get("shared") if isinstance(octacity_metadata, dict) else None
        shared_contract = shared_metadata.get("contract") if isinstance(shared_metadata, dict) else None
        shared_consumers = shared_metadata.get("consumers") if isinstance(shared_metadata, dict) else None
        shared_scaffold = shared_metadata.get("scaffold", False) if isinstance(shared_metadata, dict) else False
        shared_policy_valid = (
            isinstance(shared_contract, str)
            and bool(shared_contract.strip())
            and isinstance(shared_consumers, list)
            and all(isinstance(consumer, str) for consumer in shared_consumers)
            and isinstance(shared_scaffold, bool)
        )
        package = Package(
            name=name,
            manifest_path=manifest_path,
            role=classify(manifest_path, workspace_root),
            dependencies=tuple(sorted(set(dependency_names))),
            external_dependencies=(),
            shared_contract=shared_contract if isinstance(shared_contract, str) else None,
            shared_consumers=tuple(sorted(set(shared_consumers))) if shared_policy_valid else (),
            shared_scaffold=shared_scaffold if isinstance(shared_scaffold, bool) else False,
            shared_policy_valid=shared_policy_valid,
        )
        packages_by_directory[directory] = package
        raw_by_directory[directory] = raw

    workspace_directories = set(packages_by_directory)
    for directory, package in tuple(packages_by_directory.items()):
        dependencies = raw_by_directory[directory].get("dependencies", [])
        external_dependencies = {
            dependency["name"]
            for dependency in dependencies
            if dependency.get("path") is None
            or _normalized_path(dependency["path"]) not in workspace_directories
        }
        packages_by_directory[directory] = replace(
            package,
            external_dependencies=tuple(sorted(external_dependencies)),
        )

    edges: list[Edge] = []
    for directory, source in packages_by_directory.items():
        dependencies = raw_by_directory[directory].get("dependencies", [])
        if not isinstance(dependencies, list):
            raise MetadataError(f"dependencies for {source.name} must be a list")
        for dependency in dependencies:
            if not isinstance(dependency, dict):
                raise MetadataError(f"dependency for {source.name} must be an object")
            kind = dependency.get("kind") or "normal"
            dependency_path = dependency.get("path")
            if dependency_path is None:
                continue
            target = packages_by_directory.get(_normalized_path(dependency_path))
            if target is None:
                continue
            edges.append(Edge(source=source, target=target, kind=kind))

    return Graph(
        packages=tuple(sorted(packages_by_directory.values(), key=lambda package: package.name)),
        edges=tuple(sorted(edges, key=lambda edge: (edge.source.name, edge.target.name, edge.kind))),
    )


def check(graph: Graph) -> list[Violation]:
    """Return every architecture violation in deterministic order."""

    violations: list[Violation] = []
    reverse_edges: dict[str, set[Package]] = {}
    for edge in graph.edges:
        reverse_edges.setdefault(edge.target.name, set()).add(edge.source)

    def actual_product_consumers(shared_package: Package) -> set[str]:
        consumers: set[str] = set()
        pending = list(reverse_edges.get(shared_package.name, ()))
        visited: set[str] = set()
        while pending:
            dependent = pending.pop()
            if dependent.name in visited:
                continue
            visited.add(dependent.name)
            if dependent.role in {"agent", "cli"}:
                consumers.add(dependent.role)
            elif dependent.role in SERVER_ROLES:
                consumers.add("server")
            pending.extend(reverse_edges.get(dependent.name, ()))
        return consumers

    for package in graph.packages:
        if package.role == "unknown":
            violations.append(
                Violation(
                    "ARCH001_UNKNOWN_LAYER",
                    f"{package.name} has no recognized product/layer path: {package.manifest_path}",
                )
            )
        if package.role == "composition" and package.name != "octacity-server":
            violations.append(
                Violation(
                    "ARCH005_COMPOSITION_ROOT",
                    f"{package.name} is under server/app, but octacity-server is the only composition root",
                )
            )
        if package.role == "shared":
            declared_consumers = set(package.shared_consumers)
            actual_consumers = actual_product_consumers(package)
            if (
                not package.shared_policy_valid
                or len(declared_consumers) < 2
                or not declared_consumers.issubset(PRODUCT_AREAS)
            ):
                violations.append(
                    Violation(
                        "ARCH006_SHARED_OWNERSHIP",
                        f"{package.name} must declare a non-empty package.metadata.octacity.shared.contract "
                        "and at least two distinct consumers from agent, cli, and server",
                    )
                )
            elif package.shared_scaffold and (package.dependencies or actual_consumers):
                violations.append(
                    Violation(
                        "ARCH006_SHARED_OWNERSHIP",
                        f"{package.name} is marked as a scaffold but has dependencies or consumers; "
                        "remove the scaffold marker and establish real cross-product ownership",
                    )
                )
            elif not package.shared_scaffold and (
                len(actual_consumers) < 2 or declared_consumers != actual_consumers
            ):
                violations.append(
                    Violation(
                        "ARCH006_SHARED_OWNERSHIP",
                        f"{package.name} declares consumers {sorted(declared_consumers)}, but Cargo "
                        f"dependency edges show {sorted(actual_consumers)}; active shared contracts "
                        "must have at least two matching product consumers",
                    )
                )
        allowed_for_role = ROLE_ALLOWED_EXTERNAL_DEPENDENCIES.get(package.role)
        allowed_for_package = PACKAGE_ALLOWED_EXTERNAL_DEPENDENCIES.get(package.name, frozenset())
        deferred_dependencies: dict[str, list[str]] = {}
        for dependency in package.external_dependencies:
            for extension, pattern in DEFERRED_EXTENSION_SDK_RULES:
                if pattern.fullmatch(dependency):
                    deferred_dependencies.setdefault(extension, []).append(dependency)
                    break
        for extension, dependencies in sorted(deferred_dependencies.items()):
            violations.append(
                Violation(
                    "ARCH011_DEFERRED_EXTENSION_SDK",
                    f"{package.name} depends on deferred {extension} SDKs: "
                    f"{', '.join(sorted(dependencies))}",
                )
            )

        # Roles without a narrow implementation allowlist own concrete wiring.
        # Keep their complete external vocabulary frozen for v1 so an unknown
        # provider SDK cannot bypass the layer checks merely by moving here.
        if allowed_for_role is None:
            unreviewed_dependencies = sorted(
                set(package.external_dependencies)
                - V1_APPROVED_EXTERNAL_DEPENDENCIES
                - {
                    dependency
                    for dependencies in deferred_dependencies.values()
                    for dependency in dependencies
                }
            )
            if unreviewed_dependencies:
                violations.append(
                    Violation(
                        "ARCH012_UNREVIEWED_EXTERNAL_DEPENDENCY",
                        f"{package.name} ({package.role}) uses external dependencies outside the "
                        f"reviewed v1 inventory: {', '.join(unreviewed_dependencies)}",
                    )
                )
        unapproved_dependencies = (
            sorted(set(package.external_dependencies) - allowed_for_role - allowed_for_package)
            if allowed_for_role is not None
            else []
        )
        if unapproved_dependencies:
            if package.role == "shared":
                code = "ARCH007_SHARED_IMPLEMENTATION_DEPENDENCY"
                detail = "uses external dependencies not approved for shared contracts"
            elif package.role == "protocol":
                code = "ARCH009_PROTOCOL_IMPLEMENTATION_DEPENDENCY"
                detail = "uses external dependencies not approved for protocol crates"
            else:
                code = "ARCH010_LAYER_IMPLEMENTATION_DEPENDENCY"
                detail = f"uses external dependencies that are not approved for its {package.role} layer"
            violations.append(
                Violation(
                    code,
                    f"{package.name} ({package.role}) {detail}: {', '.join(unapproved_dependencies)}",
                )
            )

    for edge in graph.edges:
        source = edge.source
        target = edge.target
        if source.role == "agent" and target.role in SERVER_ROLES:
            violations.append(
                Violation(
                    "ARCH004_AGENT_SERVER",
                    f"{source.name} ({source.role}) must not depend on {target.name} ({target.role})",
                )
            )
            continue
        infrastructure_support = (
            source.role == "infrastructure" and target.name in INFRASTRUCTURE_SUPPORT_PACKAGES
        )
        if target.role == "infrastructure" and source.role not in {"composition", "test"} and not infrastructure_support:
            violations.append(
                Violation(
                    "ARCH003_ADAPTER_SELECTION",
                    f"{source.name} ({source.role}) selects concrete adapter {target.name}; only octacity-server may select adapters",
                )
            )
            continue
        allowed = ALLOWED_TARGETS.get(source.role, frozenset())
        if target.role not in allowed and not infrastructure_support:
            violations.append(
                Violation(
                    "ARCH002_INVALID_DIRECTION",
                    f"{source.name} ({source.role}) -> {target.name} ({target.role}) reverses the allowed dependency direction",
                )
            )

    return sorted(violations, key=lambda violation: (violation.code, violation.message))


def check_shared_sources(graph: Graph) -> list[Violation]:
    """Reject representation-specific source from shared contract crates."""

    violations: list[Violation] = []
    for package in graph.packages:
        if package.role != "shared":
            continue
        source_root = package.manifest_path.parent / "src"
        if not source_root.is_dir():
            continue
        for source_path in sorted(source_root.rglob("*.rs")):
            relative_path = source_path.relative_to(package.manifest_path.parent)
            forbidden_directories = sorted(
                set(relative_path.parts[:-1]) & SHARED_FORBIDDEN_SOURCE_DIRECTORIES
            )
            if forbidden_directories:
                violations.append(
                    Violation(
                        "ARCH008_SHARED_REPRESENTATION",
                        f"{package.name} has representation-specific source directory "
                        f"{forbidden_directories[0]}: {relative_path}",
                    )
                )
                continue
            try:
                source = source_path.read_text(encoding="utf-8")
            except OSError as error:
                raise MetadataError(f"cannot read shared source {source_path}: {error}") from error
            for representation, pattern in SHARED_SOURCE_RULES:
                if pattern.search(source):
                    violations.append(
                        Violation(
                            "ARCH008_SHARED_REPRESENTATION",
                            f"{package.name} contains {representation}: {relative_path}",
                        )
                    )
    return sorted(violations, key=lambda violation: (violation.code, violation.message))


def check_factory_graph(graph: Graph) -> list[Violation]:
    """Keep Factory above the existing execution core and inside one deep module."""

    violations: list[Violation] = []
    for package in graph.packages:
        if package.role == "core" and "factory" in package.name and package.name != FACTORY_CORE_PACKAGE:
            violations.append(
                Violation(
                    "ARCH020_FACTORY_MODULE_SHAPE",
                    f"{package.name} creates a second Factory core seam; extend {FACTORY_CORE_PACKAGE} instead",
                )
            )
        if package.name == FACTORY_CORE_PACKAGE:
            provider_dependencies = sorted(
                dependency
                for dependency in package.dependencies
                if re.search(
                    r"(?:^|[-_])(?:anthropic|codex|github|gitlab|jev|openai)(?:[-_]|$)",
                    dependency,
                    re.IGNORECASE,
                )
            )
            if provider_dependencies:
                violations.append(
                    Violation(
                        "ARCH018_FACTORY_CORE_COUPLING",
                        f"{FACTORY_CORE_PACKAGE} depends on provider-specific contracts: "
                        f"{', '.join(provider_dependencies)}",
                    )
                )

    for edge in graph.edges:
        if (
            edge.source.name in FACTORY_INDEPENDENT_LIFECYCLE_PACKAGES
            and edge.target.name == FACTORY_CORE_PACKAGE
        ):
            violations.append(
                Violation(
                    "ARCH019_FACTORY_EXECUTION_DEPENDENCY",
                    f"{edge.source.name} must remain independent of {FACTORY_CORE_PACKAGE}",
                )
            )

    return sorted(set(violations), key=lambda violation: (violation.code, violation.message))


def check_factory_core_sources(graph: Graph) -> list[Violation]:
    """Reject representation leakage and duplicate execution lifecycle types in Factory core."""

    package = next((candidate for candidate in graph.packages if candidate.name == FACTORY_CORE_PACKAGE), None)
    if package is None:
        return []
    source_root = package.manifest_path.parent / "src"
    source_paths = [
        source_path
        for source_path in sorted(source_root.rglob("*.rs"))
        if not _is_rust_test_source(source_path)
    ]
    if not source_paths:
        return [
            Violation(
                "ARCH020_FACTORY_MODULE_SHAPE",
                f"{FACTORY_CORE_PACKAGE} is an empty seam with no production Rust source",
            )
        ]

    violations: list[Violation] = []
    has_implementation = False
    for source_path in source_paths:
        source = _production_rust_source(source_path)
        relative_path = source_path.relative_to(package.manifest_path.parent)
        has_implementation = has_implementation or bool(
            re.search(
                r"(?m)^\s*(?:impl\b|(?:pub(?:\([^)]*\))?\s+)?"
                r"(?:const|enum|fn|struct|trait|type)\s+\w+)",
                source,
            )
        )
        for representation, pattern in FACTORY_CORE_SOURCE_RULES:
            if pattern.search(source):
                violations.append(
                    Violation(
                        "ARCH018_FACTORY_CORE_COUPLING",
                        f"{FACTORY_CORE_PACKAGE} contains {representation}: {relative_path}",
                    )
                )
        if FACTORY_DUPLICATE_LIFECYCLE_TYPE.search(source):
            violations.append(
                Violation(
                    "ARCH020_FACTORY_MODULE_SHAPE",
                    f"{FACTORY_CORE_PACKAGE} duplicates the existing Build/Job lifecycle: {relative_path}",
                )
            )

    if not has_implementation:
        violations.append(
            Violation(
                "ARCH020_FACTORY_MODULE_SHAPE",
                f"{FACTORY_CORE_PACKAGE} is an empty seam without domain behavior",
            )
        )
    return sorted(set(violations), key=lambda violation: (violation.code, violation.message))


def check_management_route_sources(workspace: Path) -> list[Violation]:
    """Keep every management route behind the typed authorization registry."""

    source_root = workspace / REST_API_SOURCE
    if not source_root.is_dir():
        return []

    violations: list[Violation] = []
    for source_path in sorted(source_root.rglob("*.rs")):
        relative_path = source_path.relative_to(source_root)
        try:
            source = source_path.read_text(encoding="utf-8")
        except OSError as error:
            raise MetadataError(f"cannot read REST API source {source_path}: {error}") from error

        if (
            relative_path.parts[:1] == ("v1",)
            and relative_path != TYPED_MANAGEMENT_ROUTE_REGISTRY
            and re.search(r"\.\s*route\s*\(", source)
        ):
            violations.append(
                Violation(
                    "ARCH013_UNTYPED_MANAGEMENT_ROUTE",
                    f"REST API source bypasses the typed management route registry: {relative_path}",
                )
            )

        if "API_PREFIX" in source and relative_path not in MANAGEMENT_PREFIX_OWNERS:
            violations.append(
                Violation(
                    "ARCH013_UNTYPED_MANAGEMENT_ROUTE",
                    f"REST API source uses the management prefix outside its route owners: {relative_path}",
                )
            )

        for route_path in ROUTE_LITERAL.findall(source):
            if route_path.startswith("/api/v1") and not relative_path.is_relative_to(MANAGEMENT_PATH_DECLARATIONS):
                violations.append(
                    Violation(
                        "ARCH013_UNTYPED_MANAGEMENT_ROUTE",
                        f"REST API source declares a management path outside OpenAPI operations: {relative_path}",
                    )
                )
            if relative_path == Path("lib.rs") and route_path not in OPERATIONAL_ROUTE_PATHS:
                violations.append(
                    Violation(
                        "ARCH013_UNTYPED_MANAGEMENT_ROUTE",
                        f"top-level REST router declares an unowned route {route_path!r}",
                    )
                )

    return sorted(set(violations), key=lambda violation: (violation.code, violation.message))


def _production_rust_source(source_path: Path) -> str:
    """Read source while excluding inline modules compiled only for tests."""

    try:
        source = source_path.read_text(encoding="utf-8")
    except OSError as error:
        raise MetadataError(f"cannot read Rust source {source_path}: {error}") from error
    test_module = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+\w+\s*\{")
    while match := test_module.search(source):
        depth = 0
        for index in range(match.end() - 1, len(source)):
            if source[index] == "{":
                depth += 1
            elif source[index] == "}":
                depth -= 1
                if depth == 0:
                    source = source[: match.start()] + source[index + 1 :]
                    break
        else:
            raise MetadataError(f"unclosed cfg(test) module in {source_path}")
    return source


def _is_rust_test_source(source_path: Path) -> bool:
    return source_path.name == "tests.rs" or source_path.name.endswith("_tests.rs")


def check_management_security_sources(workspace: Path) -> list[Violation]:
    """Keep management authorization and credential facts at their owning boundaries."""

    rest_sources = (workspace / REST_API_SOURCE).rglob("*.rs")
    infrastructure_sources = (workspace / SERVER_INFRASTRUCTURE_SOURCE).glob("*/src/**/*.rs")
    decision_sources = {
        source_path
        for source_path in set(rest_sources) | set(infrastructure_sources)
        if not _is_rust_test_source(source_path)
    }
    composition_sources = (workspace / SERVER_COMPOSITION_SOURCE).rglob("*.rs")
    assembly_sources = {
        source_path
        for source_path in set(decision_sources) | set(composition_sources)
        if not _is_rust_test_source(source_path)
    }
    context_sources = {
        source_path
        for source_path in (workspace / MANAGEMENT_SECURITY_SOURCE).rglob("*.rs")
        if not _is_rust_test_source(source_path)
    }

    violations: list[Violation] = []
    for source_path in sorted(assembly_sources | context_sources):
        source = _production_rust_source(source_path)
        relative_path = source_path.relative_to(workspace)
        if source_path in decision_sources and MANAGEMENT_AUTHORIZATION_IMPLEMENTATION.search(source):
            violations.append(
                Violation(
                    "ARCH015_AUTHORIZATION_OUTSIDE_APPLICATION",
                    f"management authorization decisions belong to the application layer: {relative_path}",
                )
            )
        if source_path in assembly_sources and UNDECORATED_MANAGEMENT_HANDLER.search(source):
            violations.append(
                Violation(
                    "ARCH016_UNDECORATED_MANAGEMENT_HANDLER",
                    f"management composition must expose only authorized handler seams: {relative_path}",
                )
            )
        if source_path in context_sources and RAW_MANAGEMENT_CREDENTIAL_FIELD.search(source):
            violations.append(
                Violation(
                    "ARCH017_RAW_MANAGEMENT_CREDENTIAL",
                    f"management context must contain only bounded credential-free facts: {relative_path}",
                )
            )

    return sorted(set(violations), key=lambda violation: (violation.code, violation.message))


def check_postgres_management_actor_sources(workspace: Path) -> list[Violation]:
    """Reject PostgreSQL adapters that select a management actor themselves."""

    source_root = workspace / POSTGRES_STORE_SOURCE
    if not source_root.is_dir():
        return []

    violations: list[Violation] = []
    for source_path in sorted(source_root.rglob("*.rs")):
        try:
            source = source_path.read_text(encoding="utf-8")
        except OSError as error:
            raise MetadataError(f"cannot read PostgreSQL store source {source_path}: {error}") from error
        if STORE_SELECTED_MANAGEMENT_ACTOR.search(source):
            violations.append(
                Violation(
                    "ARCH014_STORE_SELECTED_MANAGEMENT_ACTOR",
                    "PostgreSQL mutation adapters must persist the supplied management actor: "
                    f"{source_path.relative_to(source_root)}",
                )
            )
        if source_path.name != "mutation.rs" and UNTYPED_MUTATION_FACTS.search(source):
            violations.append(
                Violation(
                    "ARCH014_STORE_SELECTED_MANAGEMENT_ACTOR",
                    "PostgreSQL adapters must construct typed mutation evidence: "
                    f"{source_path.relative_to(source_root)}",
                )
            )
    return violations


def cargo_metadata(workspace: Path) -> dict[str, Any]:
    """Read locked workspace-only Cargo metadata without building packages."""

    completed = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=workspace,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip() or "cargo metadata failed"
        raise MetadataError(detail)
    try:
        parsed = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise MetadataError(f"cargo metadata returned invalid JSON: {error}") from error
    if not isinstance(parsed, dict):
        raise MetadataError("cargo metadata root must be an object")
    return parsed


def read_metadata(path: Path) -> dict[str, Any]:
    """Read a metadata-shaped JSON fixture."""

    try:
        parsed = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise MetadataError(f"cannot read metadata fixture {path}: {error}") from error
    if not isinstance(parsed, dict):
        raise MetadataError("metadata fixture root must be an object")
    return parsed


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workspace",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="workspace root used for cargo metadata",
    )
    parser.add_argument("--metadata", type=Path, help="read metadata-shaped JSON instead of invoking Cargo")
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    options = parse_arguments(arguments)
    try:
        metadata = read_metadata(options.metadata) if options.metadata else cargo_metadata(options.workspace)
        graph = graph_from_metadata(metadata)
        violations = (
            check(graph)
            + check_factory_graph(graph)
            + check_shared_sources(graph)
            + check_factory_core_sources(graph)
            + check_management_route_sources(options.workspace)
            + check_management_security_sources(options.workspace)
            + check_postgres_management_actor_sources(options.workspace)
        )
        violations.sort(key=lambda violation: (violation.code, violation.message))
    except MetadataError as error:
        print(f"architecture check failed: {error}", file=sys.stderr)
        return 2

    if violations:
        for violation in violations:
            print(f"{violation.code}: {violation.message}", file=sys.stderr)
        print(f"architecture check rejected {len(violations)} violation(s)", file=sys.stderr)
        return 1

    print(
        f"architecture check passed: {len(graph.packages)} workspace packages, "
        f"{len(graph.edges)} dependency edges"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
