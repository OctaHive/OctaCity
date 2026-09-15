# Agent operations

OctaCity packages are immutable agent bundles. The agent validates and runs an
operator-managed installation; it never enrolls, installs, or updates itself.
This separation keeps package and service-manager privileges outside job code.

## Verify a release

Download the platform archive and its `.sha256` file, then verify both the
checksum and GitHub build-provenance attestation before extracting it:

```shell
sha256sum --check octacity-agent-linux-amd64.tar.gz.sha256
gh attestation verify --repo OctaHive/OctaCity octacity-agent-linux-amd64.tar.gz
```

On macOS use `shasum -a 256 -c`; on Windows use `Get-FileHash` and compare the
lowercase SHA-256. After extraction, verify every line in the bundled
`SHA256SUMS` before copying files into their final locations.

Octa is a separate product and release. Verify its archive and provenance in
the same way, then install its complete runner, capability manifest, plugin
directory, and `Octa.lock` below the configured `octa_release_root`.

## Install and enroll

Create a dedicated `octacity` service account with no interactive login. Keep
the following roots separate and owned by that account:

- an ephemeral, quota-backed `work_root`;
- persistent `state_root` and cache root;
- read-only Octa and source-plugin release roots;
- a private configuration directory containing the enrollment token.

Each archive contains a platform-specific `share/agent.example.toml`. Copy it
to the platform configuration path, replace every sample identity, origin,
key, and filesystem path, then restrict the file and its parent directory to
the service identity. The macOS and Windows examples intentionally advertise
no runtime until an operator configures a supported backend. The procedures below
run validation with the same identity and isolation settings as the service;
validating as an unrelated administrator would test the wrong ownership model.

The enrollment token authenticates agent registration. Server signing keys
authenticate leased `JobSpec` documents and are independent. Never place
either value in a service environment variable or command line.

### Linux systemd

On a clean Linux host, create the service identity and private default roots
before copying the verified bundle and configuration:

```shell
sudo useradd --system --home-dir /var/lib/octacity --create-home --shell /usr/sbin/nologin octacity
sudo install -d -o octacity -g octacity -m 0700 /etc/octacity /var/lib/octacity/work /var/lib/octacity/state /var/lib/octacity/cache
sudo install -d -o root -g root -m 0755 /opt/octacity /opt/octacity/releases
```

Extract the verified archive into a root-owned version directory and create the
path used by the packaged service definition. Replace `VERSION` with the
verified release version:

```shell
VERSION=0.1.0
sudo install -d -o root -g root -m 0755 "/opt/octacity/releases/$VERSION"
sudo tar -xzf "octacity-agent-linux-amd64.tar.gz" -C "/opt/octacity/releases/$VERSION"
sudo ln -s "releases/$VERSION" /opt/octacity/.current-new
sudo mv -Tf /opt/octacity/.current-new /opt/octacity/current
```

Copy configuration and enrollment material into `/etc/octacity` as files owned
by `octacity:octacity` with mode `0600`, for example:

```shell
sudo install -o octacity -g octacity -m 0600 /opt/octacity/current/share/agent.example.toml /etc/octacity/agent.toml
sudo install -o octacity -g octacity -m 0600 ./enrollment-token /etc/octacity/enrollment-token
```

The systemd unit creates
`/var/lib/octacity` and `/run/octacity-identities` with mode `0700` on every
start; an external identity service may then populate the latter directory.

Install the packaged unit and exactly the runtime drop-ins that match the
validated configuration. For example, the default Linux Native configuration
uses:

```shell
sudo install -o root -g root -m 0644 /opt/octacity/current/share/systemd/octacity-agent.service /etc/systemd/system/octacity-agent.service
sudo install -d -o root -g root -m 0755 /etc/systemd/system/octacity-agent.service.d
sudo install -o root -g root -m 0644 /opt/octacity/current/share/systemd/native-runtime.conf /etc/systemd/system/octacity-agent.service.d/native-runtime.conf
```

Adjust only the declared writable paths when configured roots differ. Other
runtime-specific drop-ins belong below
`/etc/systemd/system/octacity-agent.service.d/`:

- `native-runtime.conf` delegates the service's own cgroup with the `cpu`,
  `io`, `memory`, and `pids` controllers while keeping the agent process in a
  non-delegated child subgroup;
- `microsandbox-runtime.conf` exposes KVM to the `octacity` account;
- `containerd-runtime.conf` grants the dedicated socket group access to the
  trusted system containerd daemon.

The packaged Native drop-in and `native_linux_cgroup_root` both use
`/sys/fs/cgroup/system.slice/octacity-agent.service`. systemd creates and owns
that path, delegates only the listed controllers, and places the agent process
in the `agent` subgroup so the parent can host per-job children. If the unit is
moved to another slice, change `native_linux_cgroup_root` and the drop-in's
`ReadWritePaths` together. Before enabling the containerd
drop-in, create the `octacity-containerd` group and configure the selected
containerd socket to be owned by that group with no access for other users.
The package deliberately does not alter daemon socket ownership.

Then run:

```shell
sudo systemctl daemon-reload
sudo systemctl enable --now octacity-agent.service
systemctl status octacity-agent.service
```

`ExecStartPre` runs `octacity-agent validate` as `octacity` inside the same
delegated service cgroup before the long-lived worker starts. This is required
for Native validation: running the command from an administrator shell would
neither have the service's cgroup nor test the service account's filesystem
access. Inspect `journalctl -u octacity-agent.service` if validation prevents
startup.

Native execution additionally requires the documented delegated cgroup v2 and
quota-backed work filesystem. OCI hypervisor execution requires access to its
hypervisor device. The currently tested OCI process backend uses a system
containerd daemon. Granting its dedicated socket group to the agent is an
explicit host-administrator trust decision because control of that daemon is
effectively host-root authority. Repository code receives neither that socket
nor the agent's host mount namespace. Rootless containerd is not claimed until
it passes the same retained backend contract with correct RootlessKit path and
cgroup mapping.

### macOS launchd

Choose an unused system UID and GID below 500, then create the non-login account
and private roots. The checks deliberately fail instead of reusing an existing
identifier:

```shell
OCTACITY_UID=399
OCTACITY_GID=399
! dscl . -search /Users UniqueID "$OCTACITY_UID" | grep -q .
! dscl . -search /Groups PrimaryGroupID "$OCTACITY_GID" | grep -q .
sudo dscl . -create /Groups/octacity
sudo dscl . -create /Groups/octacity PrimaryGroupID "$OCTACITY_GID"
sudo dscl . -create /Users/octacity
sudo dscl . -create /Users/octacity UniqueID "$OCTACITY_UID"
sudo dscl . -create /Users/octacity PrimaryGroupID "$OCTACITY_GID"
sudo dscl . -create /Users/octacity NFSHomeDirectory /var/empty
sudo dscl . -create /Users/octacity UserShell /usr/bin/false
sudo install -d -o octacity -g octacity -m 0700 /etc/octacity /var/lib/octacity/work /var/lib/octacity/state /var/lib/octacity/cache
sudo install -d -o root -g wheel -m 0755 /opt/octacity /opt/octacity/releases
VERSION=0.1.0
sudo install -d -o root -g wheel -m 0755 "/opt/octacity/releases/$VERSION"
sudo tar -xzf octacity-agent-macos-arm64.tar.gz -C "/opt/octacity/releases/$VERSION"
sudo ln -s "releases/$VERSION" /opt/octacity/.current-new
sudo mv -hf /opt/octacity/.current-new /opt/octacity/current
sudo install -o octacity -g octacity -m 0600 /opt/octacity/current/share/agent.example.toml /etc/octacity/agent.toml
sudo install -o octacity -g octacity -m 0600 ./enrollment-token /etc/octacity/enrollment-token
sudo install -o octacity -g octacity -m 0600 /dev/null /var/log/octacity-agent.log
sudo -u octacity /opt/octacity/current/bin/octacity-agent validate /etc/octacity/agent.toml
sudo install -o root -g wheel -m 0644 /opt/octacity/current/share/launchd/com.octahive.octacity-agent.plist /Library/LaunchDaemons/com.octahive.octacity-agent.plist
sudo launchctl bootstrap system /Library/LaunchDaemons/com.octahive.octacity-agent.plist
sudo launchctl print system/com.octahive.octacity-agent
```

### Windows Service Control Manager

Extract below `%ProgramFiles%\OctaCity`, create separate protected roots below
`%ProgramData%\OctaCity`, and validate the administrator-owned installation
before registering the virtual service account. Substitute additional
workload-identity files if configured:

The initial Windows package has no execution backend. Its bundled example
already sets `enabled_runtime_modes = []`, `allow_native_execution = false`,
`native_environment = {}`, and `oci_engines = []`. The agent therefore
registers an empty runtime inventory and is
intentionally unschedulable; this exercises installation and service lifecycle
without silently running jobs outside the promised isolation boundary.

```powershell
New-Item -ItemType Directory -Force "$env:ProgramData\OctaCity\config", "$env:ProgramData\OctaCity\work", "$env:ProgramData\OctaCity\state", "$env:ProgramData\OctaCity\cache" | Out-Null
Copy-Item "$env:ProgramFiles\OctaCity\share\agent.example.toml" "$env:ProgramData\OctaCity\config\agent.toml"
Copy-Item .\enrollment-token "$env:ProgramData\OctaCity\config\enrollment-token"
icacls "$env:ProgramData\OctaCity" /inheritance:r /grant:r "SYSTEM:(OI)(CI)F" "BUILTIN\Administrators:(OI)(CI)F"
& "$env:ProgramFiles\OctaCity\bin\octacity-agent.exe" validate "$env:ProgramData\OctaCity\config\agent.toml"
& "$env:ProgramFiles\OctaCity\share\windows\install-service.ps1" -ConfigPath "$env:ProgramData\OctaCity\config\agent.toml"
icacls "$env:ProgramData\OctaCity" /setowner "NT SERVICE\OctaCityAgent" /t /c
icacls "$env:ProgramData\OctaCity" /inheritance:r /grant:r "NT SERVICE\OctaCityAgent:(OI)(CI)F" "SYSTEM:(OI)(CI)F" "BUILTIN\Administrators:(OI)(CI)F" /t /c
Start-Service OctaCityAgent
if ((Get-Service OctaCityAgent).Status -ne "Running") { throw "OctaCityAgent failed runtime validation" }
Set-Service OctaCityAgent -StartupType Automatic
Get-Service OctaCityAgent
Get-WinEvent -FilterHashtable @{LogName='Application'; ProviderName='OctaCityAgent'} -MaxEvents 100
```

The installer registers the service with manual startup so its
virtual account can first receive access to the configuration, credential,
state, cache, and work roots. Its first start performs the same validation as
normal operation; only a running service is switched to automatic startup. Do
not grant a job-writable directory the right to replace the executable or
configuration. The registered command uses the agent's private SCM entry point;
SCM Stop and Shutdown controls trigger the same fenced graceful cancellation as
SIGTERM on Unix. Service-mode tracing is written to the Windows Application
Event Log under the configured service name; foreground `validate` and `run`
commands continue to write compact or JSON logs to stderr.

## Drain, rotate, and upgrade

Drain is a control-plane operation, not a privileged local agent command.
Phase 8 implements and tests the agent-side `drain` directive; the released
server command that issues it belongs to Phase 9. Until that command exists,
stop a test deployment only when the coordinator shows that it has no active
lease. A draining agent
stops acquiring leases, finishes or cancels its active lease according to the
fenced directive, persists terminal delivery, and exits. Wait for the service
to stop before changing files.

To rotate the enrollment token, atomically replace the protected credential
file and restart the service. To rotate a server signing key, add the new key
beside the old one, restart and confirm registration, switch server signing,
then remove the old key in a second restart.

For Unix upgrades, verify and extract into a new directory below
`/opt/octacity/releases`, validate a configuration that points at the new Octa
release, drain and stop the old service, atomically replace the
`/opt/octacity/current` link, and start it. Both packaged Unix service
definitions execute through that link. Create `/opt/octacity/.current-new` as
a relative link to the new release and replace it with `mv -Tf` on Linux or
`mv -hf` on macOS; the macOS `-h` is required so BSD `mv` replaces rather than
follows the existing link. On Windows, drain and stop the service,
install the new versioned directory, update its `binPath` with `sc.exe config`,
validate, and restart. Keep the previous bundle until the new registration
and one job complete. Rollback uses the same drain/stop/switch/start sequence;
never replace binaries under a running agent.

The service-manager part of that sequence is explicit:

```shell
# Linux
sudo systemctl stop octacity-agent.service
sudo mv -Tf /opt/octacity/.current-new /opt/octacity/current
sudo systemctl start octacity-agent.service

# macOS
sudo launchctl bootout system/com.octahive.octacity-agent
sudo mv -hf /opt/octacity/.current-new /opt/octacity/current
sudo launchctl bootstrap system /Library/LaunchDaemons/com.octahive.octacity-agent.plist
```

On Windows, use a versioned `InstallRoot`; quote the complete SCM command so
paths containing spaces remain one argument:

```powershell
$ServiceName = "OctaCityAgent"
$InstallRoot = "$env:ProgramFiles\OctaCity-0.2.0"
$ConfigPath = "$env:ProgramData\OctaCity\config\agent.toml"
Stop-Service $ServiceName
(Get-Service $ServiceName).WaitForStatus("Stopped", [TimeSpan]::FromSeconds(300))
$BinaryPath = ('"{0}" --log-format json service --service-name "{1}" "{2}"' -f (Join-Path $InstallRoot "bin\octacity-agent.exe"), $ServiceName, $ConfigPath)
& sc.exe config $ServiceName binPath= $BinaryPath
if ($LASTEXITCODE -ne 0) { throw "failed to update the service command" }
& (Join-Path $InstallRoot "bin\octacity-agent.exe") validate $ConfigPath
Start-Service $ServiceName
```

## Restart, recovery, and removal

Restart an unchanged installation with the native service manager:

```shell
# Linux
sudo systemctl restart octacity-agent.service

# macOS
sudo launchctl bootout system/com.octahive.octacity-agent
sudo launchctl bootstrap system /Library/LaunchDaemons/com.octahive.octacity-agent.plist
```

On Windows run `Restart-Service OctaCityAgent` from an elevated PowerShell
session.

On every start the agent destroys backend-owned orphans before removing only
journal-proven interrupted workspaces. Unknown or corrupt state remains in
place and is reported for operator inspection. Repeated cleanup is safe.

When disk reserve policy is violated the agent reclaims inactive cache scopes
oldest-first, then advertises `accept_jobs = false` until the work and state
filesystems recover. It continues lease polling so coordinator drain and local
shutdown remain responsive, while rejecting any lease returned against that
admission state. Reservations are added when work, state, and cache roots
reside on the same mounted filesystem, so one pool of free bytes is never
counted more than once. The agent holds an exclusive process lock on the cache
root, so every simultaneously running agent needs a distinct configured root.
It does not delete unrecognized files or incomplete lifecycle state to
manufacture free space.

For removal, first apply the drain precondition above and archive diagnostics
required by policy. Then unregister the service with the platform command:

```shell
# Linux
sudo systemctl disable --now octacity-agent.service
sudo rm /etc/systemd/system/octacity-agent.service
sudo rm -r /etc/systemd/system/octacity-agent.service.d
sudo systemctl daemon-reload

# macOS
sudo launchctl bootout system/com.octahive.octacity-agent
sudo rm /Library/LaunchDaemons/com.octahive.octacity-agent.plist
```

On Windows run
`& "$env:ProgramFiles\OctaCity\share\windows\uninstall-service.ps1"
-StopTimeoutSeconds 300`. The timeout must exceed the configured graceful
cancel, cleanup, and durable completion budgets. systemd deliberately has no
second stop deadline because the validated agent policy already bounds those
operations. Remove work, state, cache, credentials, and installation roots
only after confirming no backend resources remain.
