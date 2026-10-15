# Single GitHub provider plugin

`octacity-github` owns GitHub provider translation. Delivery is its first
capability. The separately planned Work Source, repository-read and Work Reporter
capabilities extend this same package, installation and SDK transport. They do
not create plugins for individual Flow stages. Their protocols and credential
classes remain independent; automatic intake and a concrete reporter remain in
task 16.9.

The plugin uses [Octocrab](https://github.com/XAMPPRocky/octocrab) 0.54.3 over the
existing reqwest transport, with default SDK features disabled. Automatic SDK
retry, redirect, tracing and JWT features are disabled. Raw SDK responses are
bounded before decoding. Scope, candidate verification and observe-before-retry
remain OctaCity responsibilities rather than SDK assurances.

## Installation and protected material

Install the binary with `adapter.toml` under the common operator adapter registry.
`octacity-server-delivery` validates the executable digest, Delivery version,
capabilities, `delivery_write` class and immutable configuration digest before
passing any handle. The common host clears the environment, bounds JSONL and
stdout/stderr, and terminates the process tree on timeout/cancellation. The
composition bridge also bounds calls by the current observer lease and original
attempt deadline. Cancellation addresses the active original operation before
acknowledging its stop.

The installed directory contains a private `github-plugin.toml`:

```toml
config_version = 1
api_base = "https://api.github.com/"
api_version = "2026-03-10"
git_path = "/absolute/operator-selected/git"
git_sha256 = "<verified-executable-sha256>"
credential_directory = "credentials"
input_directory = "inputs"

[repositories."repository:project-repository"]
repository_id = "<internal-repository-id>"
owner = "<github-owner>"
name = "<github-repository>"
remote = "https://github.com/<github-owner>/<github-repository>.git"
```

Optional `api_root_certificates_pem` adds at most eight individual PEM
certificates, at most 16 KiB each, to system trust for the HTTPS API client.
Hostname verification remains enabled. This setting is part of the immutable
configuration digest; changing it requires repinning the provider configuration.
It does not change Git's separate TLS trust or executable configuration.

The HTTPS tests load this operator configuration through `GithubPlugin::load`
and observe a real local TLS peer without mutating the private transport.
Untrusted certificates, hostname mismatch, malformed or oversized trust entries,
and stale configuration digests fail before an HTTP request is accepted.

Directories must be private (0700 on Unix); configuration/protected files must be
regular private files (0600). Caller-selected filesystem paths are never opened.
A handle selects `credentials/<sha256(handle)>.json` containing `class`, `token`
and permitted repository aliases. Only a stored `delivery_write` grant can
satisfy Delivery: read, repository Job and reporting grants cannot be reclassified
by changing a request. Tokens stay inside this trusted process, are zeroed in
owned buffers, and never appear in arguments, responses or diagnostics.

Artifact handles select `inputs/<sha256(handle)>.bin`. The deployment materializer
verifies Artifact ownership and atomically stages the exact accepted bundle and
canonical capture manifest privately. The adapter checks size and digest again.
`FactoryDeliveryMaterializer` is the explicit deployment integration seam, like
the existing ordinary Build compiler; it receives no forge token. Live deployment
materialization/qualification belongs to the separately planned installation work.

The SHA-256 of exact configuration bytes is advertised and frozen in each request.
Credential-file contents are excluded, allowing token rotation. Repository,
origin, executable or configuration changes cannot redirect an accepted request.
The server freezes the full target policy as `FactoryDeliveryPolicy::reference()`
and checks Project/Repository, plugin, network and secret-profile ceilings before
materialization or spawn. The frozen request carries the permitted exact hosts;
the provider compares its actual API and Git remote hosts with that ceiling before
reading a token or doing network activity. Both `api.github.com` and `github.com`
are required for the example installation above.

The reusable target policy declares `base_reference` and
`candidate_reference_prefix`. The server appends the stable operation digest to
the configured prefix to freeze one exact branch per logical publication. Different
Work operations use distinct branches under the same installed policy; replay
keeps the original branch. No Flow or node name is used to construct it.

## Publication and replay

Observe checks the candidate branch and matching review identity. Multiple
reviews, a changed head or another delivery marker are conflicts. New/open
publication additionally requires the original base to remain current. An
externally merged review may have an advanced base: its accepted candidate,
repository, refs and marker must still match, and its observed review base is
retained separately from the original accepted base. An unqualified paginated
list is rejected. An exact branch without a PR is a repairable partial publication.
An owned open PR with different title/body returns `repair_required`; the common
host sends that state through publication so it cannot silently skip repair.

Publish imports the retained bundle into a private ephemeral bare repository and
checks its canonical manifest, candidate, direct predecessor and original base
ancestry. It runs no checkout, repository hook, helper or user Git configuration.
An empty explicit push lease cannot overwrite a remote head. The adapter observes
again before creating a bounded PR. It repairs existing owned metadata under
the same review identity, then observes again. Merging is absent from the protocol;
an externally merged review is only an observation. Closed/conflicting reviews
cannot qualify publication.

Network Git failures, including lost push acknowledgments, are transient failures;
local integrity/ancestry checks remain conflicts. Replay observes the same stable delivery
identity before another write; it cannot choose another candidate, silently rebase
or create another PR. Accepted common node records retain branch identity,
verified remote head, review identity/state and receipt digest. These observations
never authorize a transition: configured gates, human responses and promotion
policies remain authoritative.

## Verification boundaries

Protocol tests use canonical fixtures and scope-substitution cases. Provider tests
use real Git capture/bundles/remotes and a controlled HTTP forge boundary; installed
configuration still requires HTTPS. Process tests invoke the actual binary, and
host tests exercise common verified process mechanics. Live HTTPS GitHub account
qualification is separate and never runs against a user's repository in this suite.
