[CmdletBinding()]
param(
  [string]$InstallRoot = "$env:ProgramFiles\OctaCity",
  [string]$ConfigPath = "$env:ProgramData\OctaCity\config\agent.toml",
  [string]$ServiceName = "OctaCityAgent"
)

$ErrorActionPreference = "Stop"

function Invoke-ServiceControl([string[]]$Arguments) {
  & sc.exe @Arguments | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "sc.exe failed with exit code $LASTEXITCODE: $($Arguments -join ' ')"
  }
}

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw "install-service.ps1 must run from an elevated PowerShell session"
}

$executable = Join-Path $InstallRoot "bin\octacity-agent.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
  throw "OctaCity agent executable does not exist at '$executable'"
}
if (-not [IO.Path]::IsPathFullyQualified($ConfigPath)) {
  throw "ConfigPath must be absolute"
}
if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
  throw "OctaCity agent configuration does not exist at '$ConfigPath'"
}
if ($ServiceName -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]{0,255}$') {
  throw "ServiceName must start with an ASCII letter or digit and contain at most 256 letters, digits, dots, underscores, or hyphens"
}
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
  throw "service '$ServiceName' already exists; use the documented upgrade procedure"
}
if ([Diagnostics.EventLog]::SourceExists($ServiceName)) {
  throw "Windows Event Log source '$ServiceName' already exists; remove the stale source before installing"
}

$binaryPath = ('"{0}" service --service-name "{1}" "{2}"' -f $executable, $ServiceName, $ConfigPath)
$serviceIdentity = "NT SERVICE\$ServiceName"
$eventSourceCreated = $false
try {
  # SCM does not preserve stdout/stderr. Register the same source consumed by
  # the agent's tracing layer before the service can be started.
  New-EventLog -LogName Application -Source $ServiceName
  $eventSourceCreated = $true
  # The operator must grant the newly created virtual account access to every
  # configured root before enabling automatic startup. Manual registration
  # closes the reboot race between service creation and ACL provisioning.
  New-Service -Name $ServiceName -BinaryPathName $binaryPath -DisplayName "OctaCity build agent" -StartupType Manual | Out-Null
  Invoke-ServiceControl @("config", $ServiceName, "obj=", $serviceIdentity, "password=", "")
  Invoke-ServiceControl @("sidtype", $ServiceName, "restricted")
  Invoke-ServiceControl @("failure", $ServiceName, "reset=", "86400", "actions=", "restart/5000/restart/15000/restart/60000")
  Invoke-ServiceControl @("failureflag", $ServiceName, "1")
}
catch {
  & sc.exe delete $ServiceName | Out-Null
  if ($eventSourceCreated) {
    Remove-EventLog -Source $ServiceName
  }
  throw
}

Write-Host "Installed $ServiceName as $serviceIdentity with manual startup. Grant its ACLs, start once to validate, then enable automatic startup."
